use lambda_http::{run, service_fn, tracing, Error, Request};
use std::sync::Arc;

mod http_handler;

struct AppState {
    dynamodb: aws_sdk_dynamodb::Client,
    lambda: aws_sdk_lambda::Client,
    table_name: String,
    worker_arn: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;

    let state = Arc::new(AppState {
        dynamodb: aws_sdk_dynamodb::Client::new(&config),
        lambda: aws_sdk_lambda::Client::new(&config),
        table_name: std::env::var("SEARCH_RESULTS_TABLE")
            .map_err(|e| Error::from(format!("Missing SEARCH_RESULTS_TABLE: {e}")))?,
        worker_arn: std::env::var("SEARCH_WORKER_ARN")
            .map_err(|e| Error::from(format!("Missing SEARCH_WORKER_ARN: {e}")))?,
    });

    run(service_fn(move |event: Request| {
        let state = Arc::clone(&state);
        async move { http_handler::router(event, &state).await }
    }))
    .await
}
