use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_lambda::primitives::Blob;
use aws_sdk_lambda::types::InvocationType;
use lambda_http::{Body, Error, Request, RequestExt, Response, tracing};
use serde::Deserialize;

use crate::AppState;

#[derive(Deserialize)]
struct SearchRequest {
    index_name: String,
    query: String,
    filter: Option<String>,
}

/// CORS JSON response helper.
fn json_response(status_code: u16, body: serde_json::Value) -> Result<Response<Body>, Error> {
    let resp = Response::builder()
        .status(status_code)
        .header("content-type", "application/json")
        .header("access-control-allow-origin", "*")
        .header("access-control-allow-methods", "POST, GET, OPTIONS")
        .header("access-control-allow-headers", "Content-Type")
        .body(body.to_string().into())
        .map_err(Box::new)?;
    Ok(resp)
}

/// Route requests based on method and path.
pub async fn router(event: Request, state: &AppState) -> Result<Response<Body>, Error> {
    let method = event.method().as_str();
    let path = event.raw_http_path();

    tracing::info!("Request: {} {}", method, path);

    match method {
        "POST" if path == "/search" => handle_post(event, state).await,
        "GET" if path.starts_with("/search/") => {
            // Extract requestId from path: /search/{requestId}
            let request_id = path.strip_prefix("/search/").unwrap_or("");
            if request_id.is_empty() {
                json_response(400, serde_json::json!({"error": "Missing request ID"}))
            } else {
                handle_get(request_id, state).await
            }
        }
        _ => json_response(404, serde_json::json!({"error": "Not found"})),
    }
}

/// POST /search — validate, create DynamoDB record, invoke worker, return request_id.
async fn handle_post(event: Request, state: &AppState) -> Result<Response<Body>, Error> {
    // Parse body
    let body_bytes = event.body().as_ref();
    let body_str = std::str::from_utf8(body_bytes)
        .map_err(|e| Error::from(format!("Invalid UTF-8: {e}")))?;

    if body_str.is_empty() {
        return json_response(400, serde_json::json!({"error": "Missing request body"}));
    }

    let req: SearchRequest = match serde_json::from_str(body_str) {
        Ok(r) => r,
        Err(e) => {
            return json_response(400, serde_json::json!({
                "error": format!("Invalid request body: {e}. Required fields: index_name, query")
            }));
        }
    };

    // Generate request ID
    let request_id = uuid::Uuid::new_v4().to_string();
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ttl = created_at + 3600; // 1 hour TTL

    tracing::info!("Creating search request: {}", request_id);

    // Store in DynamoDB as pending
    state.dynamodb.put_item()
        .table_name(&state.table_name)
        .item("request_id", AttributeValue::S(request_id.clone()))
        .item("status", AttributeValue::S("pending".into()))
        .item("query", AttributeValue::S(req.query.clone()))
        .item("created_at", AttributeValue::N(created_at.to_string()))
        .item("ttl", AttributeValue::N(ttl.to_string()))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("DynamoDB put_item failed: {e:?}");
            Error::from(format!("Failed to create search request: {e}"))
        })?;

    // Build worker payload
    let worker_payload = serde_json::json!({
        "request_id": request_id,
        "index_name": req.index_name,
        "query": req.query,
        "filter": req.filter,
    });

    // Invoke worker Lambda asynchronously (InvocationType::Event = fire-and-forget)
    state.lambda.invoke()
        .function_name(&state.worker_arn)
        .invocation_type(InvocationType::Event)
        .payload(Blob::new(serde_json::to_vec(&worker_payload)?))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Lambda invoke failed: {e:?}");
            Error::from(format!("Failed to invoke worker: {e}"))
        })?;

    tracing::info!("Worker invoked for request: {}", request_id);

    json_response(202, serde_json::json!({"request_id": request_id}))
}

/// GET /search/{requestId} — poll DynamoDB for result.
async fn handle_get(request_id: &str, state: &AppState) -> Result<Response<Body>, Error> {
    tracing::info!("Polling request: {}", request_id);

    let result = state.dynamodb.get_item()
        .table_name(&state.table_name)
        .key("request_id", AttributeValue::S(request_id.to_string()))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("DynamoDB get_item failed: {e:?}");
            Error::from(format!("Failed to fetch result: {e}"))
        })?;

    let item = match result.item() {
        Some(item) => item,
        None => {
            return json_response(404, serde_json::json!({"error": "Request not found"}));
        }
    };

    let default_status = String::from("unknown");
    let status = item
        .get("status")
        .and_then(|v| v.as_s().ok())
        .unwrap_or(&default_status);

    match status.as_str() {
        "pending" => {
            json_response(200, serde_json::json!({"status": "pending"}))
        }
        "complete" | "error" => {
            let default_body = String::from("{}");
            let response_body = item
                .get("response_body")
                .and_then(|v| v.as_s().ok())
                .unwrap_or(&default_body);

            // Parse the stored JSON and merge with status
            let body: serde_json::Value = serde_json::from_str(response_body)
                .unwrap_or(serde_json::json!({}));

            let mut result = serde_json::json!({"status": status});
            if let Some(obj) = body.as_object() {
                for (k, v) in obj {
                    result[k] = v.clone();
                }
            }

            json_response(200, result)
        }
        _ => {
            json_response(200, serde_json::json!({"status": status}))
        }
    }
}
