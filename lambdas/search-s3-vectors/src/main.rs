use lambda_http::{run, service_fn, tracing, Error};
mod http_handler;
use http_handler::function_handler;

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

    run(service_fn(function_handler)).await
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
