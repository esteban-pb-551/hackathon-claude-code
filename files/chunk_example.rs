//! Example: Text normalization and chunking for RAG pipelines
//!
//! This example demonstrates how to:
//! 1. Normalize raw text (remove extra whitespace, fix formatting)
//! 2. Split text into chunks using recursive strategy
//! 3. Generate embeddings for each chunk
//! 4. Use embeddings for semantic search
//!
//! Run with: cargo run --example chunk-normalize

use mongodb_voyageai::{
    Client,
    chunk::{
        chunking::{ChunkConfig, chunk_recursive},
        normalizer::{NormalizerConfig, normalize},
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env();

    // ── Long document example ──────────────────────────────────────────
    let long_document = r#"
        The Mediterranean diet emphasizes fish, olive oil, and vegetables,
        believed to reduce chronic diseases. Studies show a significant reduction
        in cardiovascular risk among adherents.

        Photosynthesis in plants converts light energy into glucose and produces
        essential oxygen. Chlorophyll absorbs sunlight primarily in the red and
        blue wavelengths, reflecting green light back to our eyes.

        Rivers provide water, irrigation, and habitat for aquatic species, vital
        for ecosystems. The Amazon River alone discharges about 20% of all
        freshwater entering the world's oceans.
    "#;

    println!("=== Text Normalization and Chunking Example ===\n");
    println!("Original document length: {} chars\n", long_document.len());

    // ── 1. Recursive chunking (best strategy for RAG) ──────────────────
    let config = ChunkConfig {
        chunk_size: 500, // ~150 tokens — adjusted to model
        chunk_overlap: 80,
    };

    // Normalize text: remove extra whitespace, fix line breaks
    let clean = normalize(long_document, &NormalizerConfig::prose());
    println!("Normalized text length: {} chars\n", clean.len());

    // Split into chunks respecting paragraph boundaries
    let chunks = chunk_recursive(&clean, &config);
    println!("Number of chunks created: {}\n", chunks.len());

    // Display each chunk
    for (i, chunk) in chunks.iter().enumerate() {
        println!("--- Chunk {} ({} chars) ---", i + 1, chunk.len());
        println!("{}\n", chunk);
    }

    // ── 2. Generate embeddings for each chunk ──────────────────────────
    println!("=== Generating Embeddings ===\n");

    let embed_response = client
        .embed(chunks.clone())
        .model("voyage-3-lite")
        .input_type("document")
        .send()
        .await?;

    println!("Model used: {}", embed_response.model);
    println!("Total embeddings: {}", embed_response.embeddings.len());
    println!(
        "Embedding dimensions: {}",
        embed_response.embedding(0).unwrap().len()
    );
    println!("Total tokens used: {}", embed_response.usage.total_tokens);

    // ── 3. Example: Find most relevant chunk for a query ───────────────
    println!("\n=== Semantic Search Example ===\n");

    let query = "What are the benefits of the Mediterranean diet?";
    println!("Query: \"{}\"\n", query);

    // Generate query embedding
    let query_embed = client
        .embed(vec![query])
        .model("voyage-3-lite")
        .input_type("query")
        .send()
        .await?;

    let query_vector = query_embed.embedding(0).unwrap();

    // Calculate cosine similarity with each chunk
    let mut similarities: Vec<(usize, f64)> = embed_response
        .embeddings
        .iter()
        .enumerate()
        .map(|(idx, chunk_embedding)| {
            let similarity = cosine_similarity_simple(query_vector, chunk_embedding);
            (idx, similarity)
        })
        .collect();

    // Sort by similarity (highest first)
    similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("Top 2 most relevant chunks:\n");
    for (rank, (chunk_idx, similarity)) in similarities.iter().take(2).enumerate() {
        println!(
            "Rank {}: Chunk {} (similarity: {:.4})",
            rank + 1,
            chunk_idx + 1,
            similarity
        );
        println!("{}\n", chunks[*chunk_idx]);
    }

    Ok(())
}

/// Simple cosine similarity calculation between two vectors
fn cosine_similarity_simple(a: &[f64], b: &[f64]) -> f64 {
    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}