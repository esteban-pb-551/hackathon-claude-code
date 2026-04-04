use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use base64::Engine;
use lambda_http::{run, service_fn, Body, Request, Response, Error};
use serde::Deserialize;
use sha2::{Sha256, Digest};
use std::env;
use std::collections::HashMap;

struct AppState {
    s3: S3Client,
    bucket_name: String,
}

#[derive(Deserialize)]
struct UploadRequest {
    index_name: String,
    filename: String,
    filter: Option<String>,
    content: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let state = AppState {
        s3: S3Client::new(&config),
        bucket_name: env::var("UPLOAD_BUCKET_NAME").expect("UPLOAD_BUCKET_NAME must be set"),
    };

    let state = &*Box::leak(Box::new(state));

    run(service_fn(move |req: Request| async move {
        handle_upload(req, state).await
    }))
    .await
}

fn json_response(status: u16, body: serde_json::Value) -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::Text(body.to_string()))?)
}

async fn handle_upload(req: Request, state: &AppState) -> Result<Response<Body>, Error> {
    let body_str = match req.body() {
        Body::Text(s) => s.clone(),
        Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
        Body::Empty => {
            return json_response(400, serde_json::json!({"error": "Missing request body"}));
        }
        _ => {
            return json_response(400, serde_json::json!({"error": "Unsupported body type"}));
        }
    };

    let upload_req: UploadRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(e) => {
            return json_response(400, serde_json::json!({"error": format!("Invalid JSON: {e}")}));
        }
    };

    // Validate .txt extension
    if !upload_req.filename.ends_with(".txt") {
        return json_response(400, serde_json::json!({"error": "Only .txt files are allowed"}));
    }

    // Validate required fields
    if upload_req.index_name.trim().is_empty() {
        return json_response(400, serde_json::json!({"error": "index_name is required"}));
    }
    if upload_req.content.is_empty() {
        return json_response(400, serde_json::json!({"error": "content is required"}));
    }

    // Decode base64 content
    let decoded = match base64::engine::general_purpose::STANDARD.decode(&upload_req.content) {
        Ok(bytes) => bytes,
        Err(e) => {
            return json_response(400, serde_json::json!({"error": format!("Invalid base64 content: {e}")}));
        }
    };

    // Compute SHA-256 content hash for deduplication
    let hash = Sha256::digest(&decoded);
    let hash_short = format!("{:x}", hash).chars().take(16).collect::<String>();
    let s3_key = format!("{}/file-{}.txt", upload_req.index_name.trim(), hash_short);

    tracing::info!(
        key = %s3_key,
        hash = %hash_short,
        filter = upload_req.filter.as_deref().unwrap_or("(none)"),
        size = decoded.len(),
        "Checking for duplicate before upload"
    );

    // Check if file with same content already exists
    match state.s3.head_object().bucket(&state.bucket_name).key(&s3_key).send().await {
        Ok(_) => {
            tracing::info!(key = %s3_key, "Duplicate detected, rejecting upload");
            return json_response(409, serde_json::json!({
                "error": "File with identical content already exists in this index"
            }));
        }
        Err(e) => {
            tracing::info!(key = %s3_key, error = %e, "head_object result: object not found, proceeding");
        }
    }

    // Build S3 metadata
    let mut metadata = HashMap::new();
    if let Some(ref filter) = upload_req.filter {
        let f = filter.trim();
        if !f.is_empty() {
            metadata.insert("filter".to_string(), f.to_string());
        }
    }

    let mut put = state
        .s3
        .put_object()
        .bucket(&state.bucket_name)
        .key(&s3_key)
        .content_type("text/plain")
        .body(ByteStream::from(decoded));

    if !metadata.is_empty() {
        for (k, v) in &metadata {
            put = put.metadata(k, v);
        }
    }

    put.send().await.map_err(|e| {
        tracing::error!("S3 PutObject failed: {e:?}");
        Error::from(format!("Failed to upload file: {e}"))
    })?;

    tracing::info!(key = %s3_key, "Upload successful");

    json_response(200, serde_json::json!({
        "message": "File uploaded successfully",
        "key": s3_key,
        "index_name": upload_req.index_name.trim(),
        "filter": upload_req.filter.as_deref().unwrap_or("none")
    }))
}
