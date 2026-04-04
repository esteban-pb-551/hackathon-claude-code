use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_s3vectors::types::VectorData;
use aws_smithy_types::Document;
use lambda_runtime::{service_fn, tracing, Error, LambdaEvent};
use mongodb_voyageai::{model, Client as VoyageClient};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct SearchWorkerEvent {
    request_id: String,
    index_name: String,
    query: String,
    filter: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    // Load secrets from Secrets Manager during cold start
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;
    let secrets = aws_sdk_secretsmanager::Client::new(&config);

    let voyage_secret_arn =
        std::env::var("VOYAGE_SECRET_ARN").map_err(|e| Error::from(e.to_string()))?;
    load_secret(&secrets, &voyage_secret_arn, "VOYAGEAI_API_KEY").await?;

    let friendli_secret_arn =
        std::env::var("FRIENDLI_SECRET_ARN").map_err(|e| Error::from(e.to_string()))?;
    load_secret(&secrets, &friendli_secret_arn, "FRIENDLI_TOKEN").await?;

    lambda_runtime::run(service_fn(handler)).await
}

/// Fetch a secret value from AWS Secrets Manager and set it as an environment variable.
async fn load_secret(
    client: &aws_sdk_secretsmanager::Client,
    secret_arn: &str,
    env_var: &str,
) -> Result<(), Error> {
    let response = client
        .get_secret_value()
        .secret_id(secret_arn)
        .send()
        .await
        .map_err(|e| Error::from(format!("Failed to fetch secret {secret_arn}: {e}")))?;

    let value = response
        .secret_string()
        .ok_or_else(|| Error::from("Secret has no string value"))?;

    std::env::set_var(env_var, value);
    Ok(())
}

/// Top-level handler: always writes result to DynamoDB, even on error.
async fn handler(event: LambdaEvent<SearchWorkerEvent>) -> Result<serde_json::Value, Error> {
    let (payload, _context) = event.into_parts();
    let request_id = payload.request_id.clone();

    tracing::info!(
        request_id = %request_id,
        index_name = %payload.index_name,
        query = %payload.query,
        filter = ?payload.filter,
        "SearchWorker invoked"
    );

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;
    let dynamodb = aws_sdk_dynamodb::Client::new(&config);
    let table_name = std::env::var("SEARCH_RESULTS_TABLE")
        .map_err(|e| Error::from(format!("Missing SEARCH_RESULTS_TABLE: {e}")))?;

    // Wrap the entire pipeline so any error is captured and written to DynamoDB
    match run_rag_pipeline(&config, &payload).await {
        Ok(response_content) => {
            tracing::info!(request_id = %request_id, "RAG pipeline succeeded, writing result to DynamoDB");
            write_dynamo_result(
                &dynamodb,
                &table_name,
                &request_id,
                "complete",
                &json!({"response": response_content}).to_string(),
            )
            .await?;
            Ok(json!({"status": "complete", "request_id": request_id}))
        }
        Err(err) => {
            let error_msg = format!("{err:#}");
            tracing::error!(request_id = %request_id, error = %error_msg, "RAG pipeline failed");
            write_dynamo_result(
                &dynamodb,
                &table_name,
                &request_id,
                "error",
                &json!({"error": error_msg}).to_string(),
            )
            .await?;
            Ok(json!({"status": "error", "request_id": request_id}))
        }
    }
}

/// Write status and response_body to the existing DynamoDB record.
async fn write_dynamo_result(
    dynamodb: &aws_sdk_dynamodb::Client,
    table_name: &str,
    request_id: &str,
    status: &str,
    response_body: &str,
) -> Result<(), Error> {
    tracing::info!(
        request_id = %request_id,
        status = %status,
        "Updating DynamoDB record"
    );

    dynamodb
        .update_item()
        .table_name(table_name)
        .key("request_id", AttributeValue::S(request_id.to_string()))
        .update_expression("SET #s = :status, response_body = :body")
        .expression_attribute_names("#s", "status")
        .expression_attribute_values(":status", AttributeValue::S(status.to_string()))
        .expression_attribute_values(":body", AttributeValue::S(response_body.to_string()))
        .send()
        .await
        .map_err(|e| {
            tracing::error!(request_id = %request_id, "DynamoDB update failed: {e}");
            Error::from(format!("DynamoDB update failed: {e}"))
        })?;

    tracing::info!(request_id = %request_id, "DynamoDB record updated successfully");
    Ok(())
}

/// Execute the full RAG pipeline: embed, search vectors, call LLM.
async fn run_rag_pipeline(
    config: &aws_config::SdkConfig,
    payload: &SearchWorkerEvent,
) -> anyhow::Result<String> {
    let vector_bucket = std::env::var("VECTOR_BUCKET_NAME")
        .map_err(|e| anyhow::anyhow!("Missing VECTOR_BUCKET_NAME: {e}"))?;

    tracing::info!("Using vector bucket: {vector_bucket}");

    let s3vectors = aws_sdk_s3vectors::Client::new(config);

    // 1. Validate index exists
    tracing::info!("Step 1: Validating index '{}' exists", payload.index_name);
    s3vectors
        .get_index()
        .vector_bucket_name(&vector_bucket)
        .index_name(&payload.index_name)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Index '{}' not found or inaccessible: {e}", payload.index_name))?;

    tracing::info!("Index '{}' validated", payload.index_name);

    // 2. Embed query via VoyageAI
    tracing::info!("Step 2: Embedding query via VoyageAI");
    let voyage = VoyageClient::from_env();

    let embed_response = voyage
        .embed(vec![&payload.query])
        .model(model::VOYAGE_4_LARGE)
        .input_type("query")
        .output_dimension(1024)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to generate VoyageAI embedding: {e}"))?;

    let query_embedding: Vec<f32> = embed_response
        .embeddings
        .first()
        .ok_or_else(|| anyhow::anyhow!("No embeddings returned from VoyageAI"))?
        .iter()
        .map(|v| *v as f32)
        .collect();

    tracing::info!("Embedding generated: {} dimensions", query_embedding.len());

    // 3. Query vectors
    tracing::info!(
        "Step 3: Querying vectors (bucket={}, index={}, top_k=5, filter={:?})",
        vector_bucket,
        payload.index_name,
        payload.filter
    );

    let mut query_builder = s3vectors
        .query_vectors()
        .vector_bucket_name(&vector_bucket)
        .index_name(&payload.index_name)
        .query_vector(VectorData::Float32(query_embedding))
        .top_k(5)
        .return_metadata(true);

    if let Some(ref filter_value) = payload.filter {
        tracing::info!("Applying filter: {filter_value}");
        let filter_doc = Document::from(HashMap::from([(
            "filter".to_string(),
            Document::from(filter_value.as_str()),
        )]));
        query_builder = query_builder.filter(filter_doc);
    }

    let query_response = query_builder
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query vectors: {e:?}"))?;

    tracing::info!(
        "query_vectors succeeded: {} vectors returned",
        query_response.vectors.len()
    );

    // 4. Check results — empty means no relevant info
    if query_response.vectors.is_empty() {
        tracing::info!("No vectors found, returning early without LLM call");
        return Ok("No relevant information found for the query".to_string());
    }

    // 5. Build RAG prompt from source_text metadata
    tracing::info!(
        "Step 5: Building RAG prompt from {} vectors",
        query_response.vectors.len()
    );
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
        .map_err(|e| anyhow::anyhow!("Missing FRIENDLI_TOKEN: {e}"))?;

    let llm_body = json!({
        "model": "zai-org/GLM-5",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": payload.query}
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
                    let json_resp: serde_json::Value = resp
                        .json()
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to parse Friendli response: {e}"))?;

                    let content = json_resp["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("No response generated")
                        .to_string();

                    tracing::info!("GLM-5 response length: {} chars", content.len());
                    return Ok(content);
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

    Err(anyhow::anyhow!(
        "LLM call failed after 3 attempts: {last_error}"
    ))
}
