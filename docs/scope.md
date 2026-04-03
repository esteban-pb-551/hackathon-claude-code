# S3 Vectors Semantic Search

## Idea
A serverless, event-driven semantic search engine built in Rust that automatically ingests plain text files uploaded to S3, generates vector embeddings via VoyageAI, and stores them in AWS S3 Vectors for similarity search — all triggered without user intervention beyond a file upload.

## Who It's For
Developers and teams who need to build semantic search over document collections without managing vector database infrastructure. The immediate user is someone who wants to drop text files into an S3 bucket and instantly have them searchable by meaning, not just keywords.

## Inspiration & References
- **AWS S3 Vectors** — first cloud object store with native vector support, cost-optimized, serverless: https://aws.amazon.com/s3/features/vectors/
- **VoyageAI embeddings** via `mongodb-voyageai` Rust crate: https://docs.rs/mongodb-voyageai/0.1.2/mongodb_voyageai/
- **Cargo Lambda** for Rust Lambda deployment: https://www.cargo-lambda.info/
- Reference implementation in `files/use_voyage.rs` — working example of the full S3 Vectors + VoyageAI flow (create index, embed, insert, query, list)
- Tools like LlamaIndex and LangChain solve similar problems in Python, but nothing exists yet for this stack natively in Rust + S3 Vectors — a real differentiator

## Goals
- Demonstrate a production-quality serverless architecture in Rust using cutting-edge AWS services (S3 Vectors)
- Build a complete, working pipeline: upload → auto-ingest → semantic search
- Explore spec-driven development as a practical workflow with Claude Code
- Deliver something substantive and well-crafted — not a demo, a real tool

## What "Done" Looks Like
After 29 hours:
1. **Ingestion Lambda** — upload a `.txt` file to an S3 bucket, EventBridge triggers a Lambda that reads the file, chunks the text, generates embeddings via VoyageAI, creates the S3 Vectors index if needed (named after the S3 prefix/folder), and inserts the vectors
2. **Search Lambda** — an API endpoint (Lambda behind API Gateway) that accepts a text query, generates its embedding, queries S3 Vectors, and returns ranked results with similarity scores
3. **Infrastructure** — EventBridge rule, S3 bucket, Lambda functions deployed via Cargo Lambda, API Gateway for the search endpoint
4. **Stretch goal** — a basic web frontend for search

## What's Explicitly Cut
- **PDF and CSV ingestion** — future enhancement. Only plain text (`.txt`) for this version.
- **Authentication/authorization** — no user management or API keys for now.
- **Multi-region support** — single region (us-east-1).
- **Custom chunking implementation** — the `mongodb-voyageai` crate provides built-in recursive chunking (`chunk_recursive`) and text normalization (`normalize` with `NormalizerConfig::prose()`), so no need to build this from scratch. Configuration (chunk size, overlap) will be tuned but the strategy itself is handled by the crate.
- **MongoDB** — the `mongodb-voyageai` crate is used purely as a VoyageAI embedding client; no MongoDB involved.

## Loose Implementation Notes
- **Language:** Rust throughout (both Lambdas)
- **Deployment:** Cargo Lambda (already installed), SAM CLI, arm64/Graviton
- **Secrets:** AWS Secrets Manager (VoyageAI API key, Friendli token)
- **Embedding model:** VoyageAI Voyage 4 Large, 1024 dimensions, cosine distance
- **S3 Vectors config:** Float32 data type, cosine distance metric, metadata for source text and file origin
- **Index naming convention:** S3 prefix (folder name) becomes the S3 Vectors index name
- **Event flow:** S3 PutObject → EventBridge rule → async Lambda invocation
- **Chunking:** Use `mongodb-voyageai::chunk` module — `normalize()` with `NormalizerConfig::prose()` for text cleaning, `chunk_recursive()` with configurable `ChunkConfig` (chunk_size, chunk_overlap) for splitting. Reference implementation in `files/chunk_example.rs`.
- **Key crates:** `aws-sdk-s3vectors`, `aws-sdk-s3`, `mongodb-voyageai`, `aws_lambda_events`, `lambda_runtime`, `tokio`
