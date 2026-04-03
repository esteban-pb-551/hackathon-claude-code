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
    // Parse request body
    let body_bytes = event.body().as_ref();
    let body_str = std::str::from_utf8(body_bytes)
        .map_err(|e| Error::from(format!("Invalid UTF-8 in body: {e}")))?;

    if body_str.is_empty() {
        return json_response(400, serde_json::json!({"error": "Missing request body"}));
    }

    let search_request: SearchRequest = match serde_json::from_str(body_str) {
        Ok(req) => req,
        Err(e) => {
            return json_response(
                400,
                serde_json::json!({"error": format!("Invalid request body: {e}. Required fields: index_name, query")}),
            );
        }
    };

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;

    let vector_bucket = std::env::var("VECTOR_BUCKET_NAME")
        .map_err(|e| Error::from(format!("Missing VECTOR_BUCKET_NAME: {e}")))?;

    let s3vectors = aws_sdk_s3vectors::Client::new(&config);

    // 1. Validate index exists
    let index_result = s3vectors
        .get_index()
        .vector_bucket_name(&vector_bucket)
        .index_name(&search_request.index_name)
        .send()
        .await;

    if let Err(err) = &index_result {
        if err.as_service_error().is_some_and(|e| e.is_not_found_exception()) {
            return json_response(
                404,
                serde_json::json!({"error": format!("Index '{}' not found", search_request.index_name)}),
            );
        }
        let msg = format!("Failed to check index: {err}");
        tracing::error!("{msg}");
        return json_response(500, serde_json::json!({"error": msg}));
    }

    // 2. Embed query via VoyageAI
    let voyage = VoyageClient::from_env();

    let embed_response = voyage
        .embed(vec![&search_request.query])
        .model(model::VOYAGE_4_LARGE)
        .input_type("query")
        .output_dimension(1024)
        .send()
        .await
        .map_err(|e| Error::from(format!("Failed to generate VoyageAI embedding: {e}")))?;

    let query_embedding: Vec<f32> = embed_response
        .embeddings
        .first()
        .ok_or_else(|| Error::from("No embeddings returned from VoyageAI"))?
        .iter()
        .map(|v| *v as f32)
        .collect();

    // 3. Query vectors
    let mut query_builder = s3vectors
        .query_vectors()
        .vector_bucket_name(&vector_bucket)
        .index_name(&search_request.index_name)
        .query_vector(VectorData::Float32(query_embedding))
        .top_k(5)
        .return_metadata(true);

    if let Some(ref filter_value) = search_request.filter {
        let filter_doc = Document::from(HashMap::from([
            ("filter".to_string(), Document::from(filter_value.as_str())),
        ]));
        query_builder = query_builder.filter(filter_doc);
    }

    let query_response = query_builder
        .send()
        .await
        .map_err(|e| Error::from(format!("Failed to query vectors: {e}")))?;

    // 4. Check results
    if query_response.vectors.is_empty() {
        return json_response(
            200,
            serde_json::json!({"response": "No relevant information found for the query"}),
        );
    }

    // 5. Build RAG prompt from source_text metadata
    let mut fragments: Vec<String> = Vec::new();
    for vector in &query_response.vectors {
        if let Some(metadata) = vector.metadata() {
            if let Some(obj) = metadata.as_object() {
                if let Some(source_text) = obj.get("source_text") {
                    if let Some(text) = source_text.as_string() {
                        fragments.push(text.to_string());
                    }
                }
            }
        }
    }

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
    let friendli_token = std::env::var("FRIENDLI_TOKEN")
        .map_err(|e| Error::from(format!("Missing FRIENDLI_TOKEN: {e}")))?;

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
        let result = http_client
            .post("https://api.friendli.ai/serverless/v1/chat/completions")
            .header("Authorization", format!("Bearer {friendli_token}"))
            .json(&llm_body)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                let json: serde_json::Value = resp.json().await
                    .map_err(|e| Error::from(format!("Failed to parse Friendli response: {e}")))?;

                let content = json["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("No response generated")
                    .to_string();

                tracing::info!("Successfully generated response via GLM-5");
                return json_response(200, serde_json::json!({"response": content}));
            }
            Ok(resp) => {
                last_error = format!("HTTP {} on attempt {}", resp.status(), attempt + 1);
                tracing::warn!("{last_error}");
            }
            Err(e) => {
                last_error = format!("Request error on attempt {}: {e}", attempt + 1);
                tracing::warn!("{last_error}");
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
    }

    tracing::error!("All LLM attempts failed: {last_error}");
    json_response(
        500,
        serde_json::json!({"error": format!("LLM call failed after 3 attempts: {last_error}")}),
    )
}
