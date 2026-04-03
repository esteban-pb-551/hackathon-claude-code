use std::collections::HashMap;

use anyhow::{Context, Result};
use aws_sdk_s3vectors::types::{DataType, DistanceMetric, MetadataConfiguration, PutInputVector, VectorData};
use aws_smithy_types::Document;
use mongodb_voyageai::{Client as VoyageClient, model};

const VECTOR_BUCKET: &str = "test-vector-bucket";
const INDEX_NAME: &str = "movies";
const REGION: &str = "us-east-1";

#[tokio::main]
async fn main() -> Result<()> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(REGION)
        .load()
        .await;

    let s3vectors = aws_sdk_s3vectors::Client::new(&config);
    let voyage = VoyageClient::from_env();

    println!("Using provider: VoyageAI (Voyage 4 Lite)\n");

    // Step 2: Ensure vector index exists
    ensure_vector_index(&s3vectors).await?;

    // Step 3: Generate embeddings and insert vectors
    insert_vectors(&voyage, &s3vectors).await?;

    // Step 4: Query vectors by similarity
    query_vectors(&voyage, &s3vectors).await?;

    // Summary: List stored vectors and estimate storage
    list_stored_vectors(&s3vectors).await?;

    Ok(())
}

async fn ensure_vector_index(client: &aws_sdk_s3vectors::Client) -> Result<()> {
    let result = client
        .get_index()
        .vector_bucket_name(VECTOR_BUCKET)
        .index_name(INDEX_NAME)
        .send()
        .await;

    match result {
        Ok(output) => {
            let index = output.index().context("Empty index in response")?;
            println!("Vector index '{}' already exists (ARN: {})", INDEX_NAME, index.index_arn());
            return Ok(());
        }
        Err(err) => {
            if err.as_service_error().is_some_and(|e| e.is_not_found_exception()) {
                println!("Vector index '{INDEX_NAME}' not found, creating...");
            } else {
                return Err(err).context("Failed to check vector index");
            }
        }
    }

    let metadata_config = MetadataConfiguration::builder()
        .non_filterable_metadata_keys("source_text".to_string())
        .build()
        .context("Failed to build metadata configuration")?;

    let output = client
        .create_index()
        .vector_bucket_name(VECTOR_BUCKET)
        .index_name(INDEX_NAME)
        .data_type(DataType::Float32)
        .dimension(1024)
        .distance_metric(DistanceMetric::Cosine)
        .metadata_configuration(metadata_config)
        .send()
        .await
        .context("Failed to create vector index")?;

    println!("Vector index created! ARN: {:?}", output.index_arn());
    Ok(())
}

async fn generate_embedding(client: &VoyageClient, text: &str) -> Result<Vec<f32>> {
    let response = client
        .embed(vec![text])
        .model(model::VOYAGE_4_LITE)
        .input_type("document")
        .output_dimension(1024)
        .send()
        .await
        .with_context(|| format!("Failed to generate VoyageAI embedding for: {text}"))?;

    let embedding = response
        .embeddings
        .first()
        .context("No embeddings returned from VoyageAI")?;

    Ok(embedding.iter().map(|v| *v as f32).collect())
}

async fn insert_vectors(
    voyage: &VoyageClient,
    s3vectors: &aws_sdk_s3vectors::Client,
) -> Result<()> {
    let movies = vec![
        ("Star Wars", "A farm boy joins rebels to fight an evil empire in space", "scifi"),
        ("Jurassic Park", "Scientists create dinosaurs in a theme park that goes wrong", "scifi"),
        ("Finding Nemo", "A father fish searches the ocean to find his lost son", "family"),
        ("The Matrix", "A hacker discovers reality is a simulation controlled by machines", "scifi"),
        ("Inception", "A thief enters people's dreams to steal secrets from their subconscious", "scifi"),
        ("Toy Story", "A cowboy toy feels threatened when a new spaceman toy arrives", "family"),
        ("The Lion King", "A young lion prince flees his kingdom only to return as the true king", "family"),
        ("Interstellar", "Astronauts travel through a wormhole to find a new home for humanity", "scifi"),
        ("The Godfather", "The aging patriarch of a crime dynasty transfers control to his son", "drama"),
        ("Titanic", "A young couple from different social classes fall in love aboard a doomed ship", "drama"),
        ("The Shawshank Redemption", "A wrongly convicted banker befriends a smuggler while planning his escape from prison", "drama"),
        ("Avengers Endgame", "Superheroes team up for a final battle to reverse a catastrophic event", "action"),
    ];

    let mut vectors: Vec<PutInputVector> = Vec::new();

    for (title, description, genre) in &movies {
        println!("Generating embedding for: {title}");
        let embedding = generate_embedding(voyage, description).await?;

        let metadata = Document::from(HashMap::from([
            ("source_text".to_string(), Document::from(*description)),
            ("genre".to_string(), Document::from(*genre)),
        ]));

        let vector = PutInputVector::builder()
            .key(*title)
            .data(VectorData::Float32(embedding))
            .metadata(metadata)
            .build()
            .with_context(|| format!("Failed to build vector for: {title}"))?;

        vectors.push(vector);
    }

    s3vectors
        .put_vectors()
        .vector_bucket_name(VECTOR_BUCKET)
        .index_name(INDEX_NAME)
        .set_vectors(Some(vectors))
        .send()
        .await
        .context("Failed to insert vectors")?;

    println!("Successfully inserted {} vectors into '{INDEX_NAME}'", movies.len());
    Ok(())
}

async fn query_vectors(
    voyage: &VoyageClient,
    s3vectors: &aws_sdk_s3vectors::Client,
) -> Result<()> {
    let input_text = "adventures in space";
    println!("\n--- Query: \"{input_text}\" ---");

    let embedding = generate_embedding(voyage, input_text).await?;

    // Query without filter
    let response = s3vectors
        .query_vectors()
        .vector_bucket_name(VECTOR_BUCKET)
        .index_name(INDEX_NAME)
        .query_vector(VectorData::Float32(embedding.clone()))
        .top_k(3)
        .return_distance(true)
        .return_metadata(true)
        .send()
        .await
        .context("Failed to query vectors")?;

    println!("\nAll results:");
    print_results(&response.vectors);

    // Query with metadata filter: genre = "scifi"
    let filter = Document::from(HashMap::from([
        ("genre".to_string(), Document::from("scifi")),
    ]));

    let filtered_response = s3vectors
        .query_vectors()
        .vector_bucket_name(VECTOR_BUCKET)
        .index_name(INDEX_NAME)
        .query_vector(VectorData::Float32(embedding))
        .top_k(3)
        .filter(filter)
        .return_distance(true)
        .return_metadata(true)
        .send()
        .await
        .context("Failed to query vectors with filter")?;

    println!("\nFiltered results (genre: scifi):");
    print_results(&filtered_response.vectors);

    Ok(())
}

async fn list_stored_vectors(s3vectors: &aws_sdk_s3vectors::Client) -> Result<()> {
    println!("\n--- Stored vectors in '{INDEX_NAME}' ---");

    let mut all_keys: Vec<String> = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut request = s3vectors
            .list_vectors()
            .vector_bucket_name(VECTOR_BUCKET)
            .index_name(INDEX_NAME)
            .return_metadata(true);

        if let Some(token) = &next_token {
            request = request.next_token(token);
        }

        let response = request.send().await.context("Failed to list vectors")?;

        for vector in &response.vectors {
            let genre = vector
                .metadata()
                .and_then(|m| m.as_object())
                .and_then(|obj| obj.get("genre"))
                .and_then(|v| v.as_string())
                .unwrap_or("unknown");
            println!("  - {} [{}]", vector.key(), genre);
            all_keys.push(vector.key().to_string());
        }

        next_token = response.next_token().map(|t| t.to_string());
        if next_token.is_none() {
            break;
        }
    }

    let count = all_keys.len();
    let dimension = 1024;
    let bytes_per_float = 4;
    let estimated_vector_bytes = count * dimension * bytes_per_float;
    let estimated_kb = estimated_vector_bytes as f64 / 1024.0;

    println!("\nTotal vectors: {count}");
    println!("Estimated vector storage: {estimated_kb:.2} KB ({dimension} dimensions x {bytes_per_float} bytes × {count} vectors)");

    Ok(())
}

fn print_results(vectors: &[aws_sdk_s3vectors::types::QueryOutputVector]) {
    for vector in vectors {
        if let Some(distance) = vector.distance() {
            let similarity = (1.0 - distance) * 100.0;
            println!("  - {} (similarity: {similarity:.2}%, distance: {distance:.4})", vector.key());
        } else {
            println!("  - {}", vector.key());
        }
        if let Some(metadata) = vector.metadata() {
            println!("    metadata: {:?}", metadata);
        }
    }
}
