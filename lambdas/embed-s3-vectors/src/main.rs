use std::collections::HashMap;

use anyhow::Context;
use aws_sdk_s3vectors::types::{PutInputVector, VectorData};
use aws_smithy_types::Document;
use lambda_runtime::{service_fn, tracing, LambdaEvent, Error};
use mongodb_voyageai::{
    Client as VoyageClient,
    chunk::{
        chunking::{ChunkConfig, chunk_recursive},
        normalizer::{NormalizerConfig, normalize},
    },
    model,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct EmbedRequest {
    bucket_name: String,
    object_key: String,
    index_name: String,
}

////////////////////////////////////////////////////////////////////////////////////////////////////////
// Fetch a secret value from AWS Secrets Manager and set it as an environment variable.
//
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
        .with_context(|| format!("Failed to fetch secret: {secret_arn}"))
        .map_err(|e| Error::from(e.to_string()))?;

    let value = response
        .secret_string()
        .context("Secret has no string value")
        .map_err(|e| Error::from(e.to_string()))?;

    std::env::set_var(env_var, value);
    Ok(())
}

////////////////////////////////////////////////////////////////////////////////////////////////////////
// Lambda handler function
//
async fn handler(event: LambdaEvent<EmbedRequest>) -> Result<serde_json::Value, Error> {
    let (request, _context) = event.into_parts();

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;

    // 1. Read file from S3
    let s3 = aws_sdk_s3::Client::new(&config);

    let resp = s3
        .get_object()
        .bucket(&request.bucket_name)
        .key(&request.object_key)
        .send()
        .await
        .with_context(|| format!("Failed to get S3 object: {}", request.object_key))
        .map_err(|e| {
            tracing::error!("Failed to get S3 object: {}", request.object_key);
            Error::from(e.to_string())
        })?;

    let filter = resp
        .metadata()
        .and_then(|m| m.get("filter"))
        .cloned()
        .unwrap_or_else(|| "none".to_string());

    let body = resp
        .body
        .collect()
        .await
        .context("Failed to read S3 object body")
        .map_err(|e| {
            tracing::error!("Failed to read S3 object body: {}", request.object_key);
            Error::from(e.to_string())
        })?
        .into_bytes();

    let content = String::from_utf8(body.to_vec())
        .context("S3 object is not valid UTF-8")
        .map_err(|e| {
            tracing::error!("S3 object is not valid UTF-8: {}", request.object_key);
            Error::from(e.to_string())
        })?;

    if content.is_empty() {
        tracing::error!("File content is empty: {}", request.object_key);
        return Err(Error::from(format!(
            "File content is empty: {}",
            request.object_key
        )));
    }

    // 2. Normalize and chunk text
    let clean = normalize(&content, &NormalizerConfig::prose());
    let chunk_config = ChunkConfig {
        chunk_size: 500,
        chunk_overlap: 80,
    };
    let chunks = chunk_recursive(&clean, &chunk_config);

    // 3. Generate embeddings via VoyageAI
    let voyage = VoyageClient::from_env();

    let response = voyage
        .embed(chunks.clone())
        .model(model::VOYAGE_4_LARGE)
        .input_type("document")
        .output_dimension(1024)
        .send()
        .await
        .context("Failed to generate VoyageAI embeddings")
        .map_err(|e| {
            tracing::error!("Failed to generate VoyageAI embeddings");
            Error::from(e.to_string())
        })?;

    // 4. Insert vectors into S3 Vectors
    let vector_bucket =
        std::env::var("VECTOR_BUCKET_NAME")
        .map_err(|e| {
            tracing::error!("Failed to get VECTOR_BUCKET_NAME from environment");
            Error::from(e.to_string())
        })?;
    let s3vectors = aws_sdk_s3vectors::Client::new(&config);

    let mut vectors: Vec<PutInputVector> = Vec::new();

    for (i, (chunk, embedding_data)) in chunks.iter().zip(response.embeddings.iter()).enumerate() {
        let embedding: Vec<f32> = embedding_data.iter().map(|v| *v as f32).collect();

        let metadata = Document::from(HashMap::from([
            (
                "source_text".to_string(),
                Document::from(chunk.as_str()),
            ),
            ("filter".to_string(), Document::from(filter.as_str())),
        ]));

        let vector = PutInputVector::builder()
            .key(format!("{}#{}", request.object_key, i))
            .data(VectorData::Float32(embedding))
            .metadata(metadata)
            .build()
            .with_context(|| format!("Failed to build vector #{}", i))
            .map_err(|e| {
                tracing::error!("Failed to build vector #{}", i);
                Error::from(e.to_string())
            })?;

        vectors.push(vector);
    }

    // Batch into calls of max 500 vectors
    for batch in vectors.chunks(500) {
        s3vectors
            .put_vectors()
            .vector_bucket_name(&vector_bucket)
            .index_name(&request.index_name)
            .set_vectors(Some(batch.to_vec()))
            .send()
            .await
            .context("Failed to put vectors into S3 Vectors")
            .map_err(|e| {
                tracing::error!("Failed to put vectors into S3 Vectors");
                Error::from(e.to_string())
            })?;
    }

    tracing::info!("Successfully processed {} chunks", chunks.len());
    Ok(serde_json::json!({
        "status": "success",
        "chunks_processed": chunks.len()
    }))
}

////////////////////////////////////////////////////////////////////////////////////////////////////////
// Main function
//
#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;
    
    // VoyageClient::from_env() reads VOYAGEAI_API_KEY at construction time.
    // Load VoyageAI API key from Secrets Manager before starting the handler.
    let secrets = aws_sdk_secretsmanager::Client::new(&config);
    let voyage_secret_arn =
        std::env::var("VOYAGE_SECRET_ARN").map_err(|e| Error::from(e.to_string()))?;
    load_secret(&secrets, &voyage_secret_arn, "VOYAGEAI_API_KEY").await?;

    lambda_runtime::run(service_fn(handler)).await
}