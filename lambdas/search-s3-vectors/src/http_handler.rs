use std::collections::HashMap;

use aws_sdk_s3vectors::types::VectorData;
use aws_smithy_types::Document;
use lambda_http::{Body, Error, Request, Response, tracing};
use mongodb_voyageai::{Client as VoyageClient, model};
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchRequest {
    index_name: String,
    query: String,
    filter: Option<String>,
}

/// Build a JSON response with the given status code and body.
fn json_response(status_code: u16, body: serde_json::Value) -> Result<Response<Body>, Error> {
    let resp = Response::builder()
        .status(status_code)
        .header("content-type", "application/json")
        .body(body.to_string().into())
        .map_err(Box::new)?;
    Ok(resp)
}

/// Lambda handler: receives HTTP request, runs RAG pipeline, returns response.
pub(crate) async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    tracing::info!("SearchS3Vectors handler invoked");

    // Parse request body
    let body_bytes = event.body().as_ref();
    let body_str = std::str::from_utf8(body_bytes)
        .map_err(|e| Error::from(format!("Invalid UTF-8 in body: {e}")))?;

    tracing::info!("Request body: {}", body_str);

    if body_str.is_empty() {
        tracing::warn!("Empty request body");
        return json_response(400, serde_json::json!({"error": "Missing request body"}));
    }

    let search_request: SearchRequest = match serde_json::from_str(body_str) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Failed to parse request body: {e}");
            return json_response(
                400,
                serde_json::json!({"error": format!("Invalid request body: {e}. Required fields: index_name, query")}),
            );
        }
    };

    tracing::info!(
        "Parsed request: index_name={}, query={}, filter={:?}",
        search_request.index_name, search_request.query, search_request.filter
    );

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;

    let vector_bucket = std::env::var("VECTOR_BUCKET_NAME")
        .map_err(|e| Error::from(format!("Missing VECTOR_BUCKET_NAME: {e}")))?;

    tracing::info!("Using vector bucket: {vector_bucket}");

    let s3vectors = aws_sdk_s3vectors::Client::new(&config);

    // 1. Validate index exists
    tracing::info!("Step 1: Validating index '{}' exists", search_request.index_name);
    let index_result = s3vectors
        .get_index()
        .vector_bucket_name(&vector_bucket)
        .index_name(&search_request.index_name)
        .send()
        .await;

    match &index_result {
        Ok(output) => {
            tracing::info!("Index '{}' found: {:?}", search_request.index_name, output.index());
        }
        Err(err) => {
            if err.as_service_error().is_some_and(|e| e.is_not_found_exception()) {
                tracing::warn!("Index '{}' not found", search_request.index_name);
                return json_response(
                    404,
                    serde_json::json!({"error": format!("Index '{}' not found", search_request.index_name)}),
                );
            }
            tracing::error!("get_index failed: {:?}", err);
            tracing::error!("get_index raw error: {}", err);
            if let Some(svc_err) = err.as_service_error() {
                tracing::error!("get_index service error: {:?}", svc_err);
            }
            return json_response(500, serde_json::json!({"error": format!("Failed to check index: {err:?}")}));
        }
    }

    // 2. Embed query via VoyageAI
    tracing::info!("Step 2: Embedding query via VoyageAI");
    let voyage = VoyageClient::from_env();

    let embed_response = voyage
        .embed(vec![&search_request.query])
        .model(model::VOYAGE_4_LARGE)
        .input_type("query")
        .output_dimension(1024)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("VoyageAI embedding failed: {e:?}");
            Error::from(format!("Failed to generate VoyageAI embedding: {e}"))
        })?;

    let query_embedding: Vec<f32> = embed_response
        .embeddings
        .first()
        .ok_or_else(|| {
            tracing::error!("No embeddings returned from VoyageAI");
            Error::from("No embeddings returned from VoyageAI")
        })?
        .iter()
        .map(|v| *v as f32)
        .collect();

    tracing::info!("Embedding generated: {} dimensions", query_embedding.len());

    // 3. Query vectors
    tracing::info!(
        "Step 3: Querying vectors (bucket={}, index={}, top_k=5, filter={:?})",
        vector_bucket, search_request.index_name, search_request.filter
    );

    let mut query_builder = s3vectors
        .query_vectors()
        .vector_bucket_name(&vector_bucket)
        .index_name(&search_request.index_name)
        .query_vector(VectorData::Float32(query_embedding))
        .top_k(5)
        .return_metadata(true);

    if let Some(ref filter_value) = search_request.filter {
        tracing::info!("Applying filter: {filter_value}");
        let filter_doc = Document::from(HashMap::from([
            ("filter".to_string(), Document::from(filter_value.as_str())),
        ]));
        query_builder = query_builder.filter(filter_doc);
    }

    let query_response = match query_builder.send().await {
        Ok(resp) => {
            tracing::info!("query_vectors succeeded: {} vectors returned", resp.vectors.len());
            resp
        }
        Err(e) => {
            tracing::error!("query_vectors failed: {:?}", e);
            tracing::error!("query_vectors raw error: {}", e);
            if let Some(svc_err) = e.as_service_error() {
                tracing::error!("query_vectors service error: {:?}", svc_err);
                tracing::error!("query_vectors service error message: {}", svc_err);
            }
            return json_response(500, serde_json::json!({"error": format!("Failed to query vectors: {e:?}")}));
        }
    };

    // 4. Check results
    if query_response.vectors.is_empty() {
        tracing::info!("No vectors found, returning early without LLM call");
        return json_response(
            200,
            serde_json::json!({"response": "No relevant information found for the query"}),
        );
    }

    // 5. Build RAG prompt from source_text metadata
    tracing::info!("Step 5: Building RAG prompt from {} vectors", query_response.vectors.len());
    let mut fragments: Vec<String> = Vec::new();
    for (i, vector) in query_response.vectors.iter().enumerate() {
        tracing::info!("Vector {}: key={}", i, vector.key());
        if let Some(metadata) = vector.metadata() {
            if let Some(obj) = metadata.as_object() {
                if let Some(source_text) = obj.get("source_text") {
                    if let Some(text) = source_text.as_string() {
                        tracing::info!("Vector {} source_text length: {} chars", i, text.len());
                        fragments.push(text.to_string());
                    }
                }
            }
        }
    }

    tracing::info!("Collected {} fragments for RAG prompt", fragments.len());

    let context_text = fragments
        .iter()
        .enumerate()
        .map(|(i, f)| format!("[Fragment {}]\n{}", i + 1, f))
        .collect::<Vec<_>>()
        .join("\n\n");

    let system_prompt = format!(
        "You are a helpful assistant. Answer the user's question based only on the following context fragments. \
         If the context doesn't contain enough information, say so.\n\nContext:\n{context_text}"
    );

    // 6. Call GLM-5 via Friendli API
    tracing::info!("Step 6: Calling GLM-5 via Friendli API");
    let friendli_token = std::env::var("FRIENDLI_TOKEN")
        .map_err(|e| {
            tracing::error!("Missing FRIENDLI_TOKEN: {e}");
            Error::from(format!("Missing FRIENDLI_TOKEN: {e}"))
        })?;

    let llm_body = serde_json::json!({
        "model": "zai-org/GLM-5",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": search_request.query}
        ]
    });

    let http_client = reqwest::Client::new();
    let mut last_error = String::new();
    let backoff_secs = [1u64, 2, 4];

    for (attempt, delay) in backoff_secs.iter().enumerate() {
        tracing::info!("LLM attempt {} of 3", attempt + 1);

        let result = http_client
            .post("https://api.friendli.ai/serverless/v1/chat/completions")
            .header("Authorization", format!("Bearer {friendli_token}"))
            .json(&llm_body)
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status();
                tracing::info!("Friendli response status: {status}");

                if status.is_success() {
                    let json: serde_json::Value = resp.json().await
                        .map_err(|e| {
                            tracing::error!("Failed to parse Friendli JSON response: {e}");
                            Error::from(format!("Failed to parse Friendli response: {e}"))
                        })?;

                    let content = json["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("No response generated")
                        .to_string();

                    tracing::info!("GLM-5 response length: {} chars", content.len());
                    return json_response(200, serde_json::json!({"response": content}));
                } else {
                    let body_text = resp.text().await.unwrap_or_default();
                    last_error = format!("HTTP {status} on attempt {}: {body_text}", attempt + 1);
                    tracing::warn!("{last_error}");
                }
            }
            Err(e) => {
                last_error = format!("Request error on attempt {}: {e}", attempt + 1);
                tracing::warn!("{last_error}");
            }
        }

        tracing::info!("Backing off {}s before retry", delay);
        tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
    }

    tracing::error!("All LLM attempts failed: {last_error}");
    json_response(
        500,
        serde_json::json!({"error": format!("LLM call failed after 3 attempts: {last_error}")}),
    )
}
