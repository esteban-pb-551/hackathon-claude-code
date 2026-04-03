# S3 Vectors Semantic Search — Product Requirements

## Problem Statement
Developers who want semantic search over document collections today must manage dedicated vector database infrastructure or rely on Python-heavy toolchains like LlamaIndex and LangChain. There is no native Rust + S3 Vectors solution that lets teams drop text files into an S3 bucket and immediately have them searchable by meaning — serverless, event-driven, and zero-maintenance.

## User Stories

### Document Ingestion

- As a developer who wants semantic search over a document collection, I want to upload a `.txt` file to an S3 bucket with a specific prefix so that the system automatically processes, embeds, and stores the content — without any manual intervention beyond the upload.
  - [ ] Uploading a `.txt` file to a prefixed path (e.g., `movies/back_to_the_future.txt`) triggers an EventBridge rule
  - [ ] EventBridge invokes CheckS3Vectors (Python, Lambda Durable Function)
  - [ ] CheckS3Vectors verifies whether the S3 Vectors index (named after the prefix, e.g., `movies`) exists; if not, creates it
  - [ ] If index creation fails due to concurrent creation (race condition), CheckS3Vectors retries up to 3 times. On retry, if the index now exists, it proceeds normally. If it still doesn't exist after 3 attempts, it terminates with an error
  - [ ] CheckS3Vectors invokes EmbedS3Vectors (Rust) passing: bucket name, full file key, and index name
  - [ ] EmbedS3Vectors reads the file content and S3 object metadata from the bucket
  - [ ] EmbedS3Vectors checks that the file content is not null; if null, returns an error that CheckS3Vectors captures and reports as a failed job
  - [ ] If the S3 object has a `filter` metadata key, its value is stored as filterable metadata; if absent, defaults to `none`
  - [ ] Text is chunked using `mongodb-voyageai` crate (`normalize` with `NormalizerConfig::prose()`, then `chunk_recursive`)
  - [ ] Embeddings are generated via VoyageAI (Voyage 4 Lite, 1024 dimensions)
  - [ ] Vectors are inserted into S3 Vectors with: `filter` (filterable metadata) and `source_text` (non-filterable metadata containing the chunk content)
  - [ ] EventBridge rule only triggers for `.txt` files — other file types are ignored

### Semantic Search

- As a developer with indexed documents in S3 Vectors, I want to send a text query specifying an index and optionally a filter so that I receive an LLM-generated response based on the most relevant document fragments.
  - [ ] SearchS3Vectors (Rust) receives: index name (required), query text (required), filter value (optional)
  - [ ] If the specified index does not exist, returns an error message without invoking the LLM
  - [ ] Generates the query embedding via VoyageAI
  - [ ] Searches for the most similar vectors in the specified S3 Vectors index
  - [ ] If a filter is provided, only returns results matching that filterable metadata
  - [ ] If no relevant results are found, returns "No relevant information found for the query" without invoking the LLM
  - [ ] Passes retrieved fragments to GLM-5 (via zai-org API) to generate a response
  - [ ] Returns the LLM-generated response

### Search Frontend

- As a user who wants to search indexed documents, I want a professional-looking web page with a simple form so that I can enter an index name, a query, and optionally a filter, and see the LLM-generated response.
  - [ ] Page displays a form with: index name (required), query (required), filter (optional)
  - [ ] Professional, clean design
  - [ ] Form submits to API Gateway HTTP which invokes SearchS3Vectors
  - [ ] Displays the LLM-generated response on screen
  - [ ] Displays error messages appropriately (index not found, no results)

### Infrastructure & Deployment

- As a developer deploying this system, I want a SAM template and clear documentation so that I can deploy the entire stack with minimal effort.
  - [ ] SAM template defines: Secrets Manager secrets, S3 Vectors bucket (`AWS::S3Vectors::VectorBucket`), S3 upload bucket, EventBridge rule (filtered to `.txt`), CheckS3Vectors Lambda (Python 3.13, arm64, Durable Function), EmbedS3Vectors Lambda (Rust, arm64 via Cargo Lambda)
  - [ ] API keys stored in Secrets Manager (not as plain text env vars); Lambdas fetch at runtime via `aws-sdk-secretsmanager`
  - [ ] IAM policies scoped to specific resource ARNs (not `*`)
  - [ ] SAM template is extended in Etapa 2 to include SearchS3Vectors Lambda
  - [ ] `sam validate --lint` passes before every deploy
  - [ ] Code documentation follows Python best practices for CheckS3Vectors and Rust doc comments for EmbedS3Vectors
  - [ ] Python dependencies managed with `uv` (`pyproject.toml` + `uv.lock`), `requirements.txt` generated for SAM
  - [ ] README.md provides complete step-by-step deployment instructions using SAM
  - [ ] README.md does not include a project tree

## What We're Building

**Etapa 1 — Ingestion backend:**
- CheckS3Vectors (Python, Durable Function): index verification/creation with retry logic for concurrent access
- EmbedS3Vectors (Rust): file reading, metadata extraction, chunking, embedding, vector storage
- EventBridge rule filtering for `.txt` files
- SAM template for full deployment
- Complete documentation (Python docstrings, Rust doc comments, README with deployment steps)

**Etapa 2 — Semantic search:**
- SearchS3Vectors (Rust): query embedding, vector search with optional filtering, LLM response generation via GLM-5
- Updated SAM template and documentation

Etapa 1 and 2 must both be fully functional for the Devpost submission. The demo story: upload a text file, then search by meaning and get an intelligent response.

## What We'd Add With More Time

- **Search frontend (Etapa 3):** Node.js web page behind API Gateway HTTP with a professional form UI. The long response time (potentially 60+ seconds) needs a UX solution — loading indicator, streaming, or async pattern. This is defined but not built.
- **PDF and CSV ingestion:** Extend EventBridge rules and EmbedS3Vectors to handle additional file formats.
- **Authentication/authorization:** API keys or IAM-based access control for the search endpoint.

## Non-Goals

- **Multi-format ingestion** — Only `.txt` files. PDF, CSV, and other formats are future work, not a hackathon deliverable.
- **Authentication/authorization** — No user management, API keys, or access control for this version.
- **Multi-region support** — Single region (us-east-1) only.
- **Custom chunking logic** — The `mongodb-voyageai` crate handles chunking; no custom implementation.
- **MongoDB usage** — The `mongodb-voyageai` crate is used purely as a VoyageAI embedding client.
- **Source fragment display** — SearchS3Vectors returns only the LLM-generated response, not the underlying chunks used to generate it.

## Open Questions

- **Long response times for search (Etapa 3):** The full search + LLM pipeline could take over 60 seconds. How should the frontend handle this? Loading indicator, streaming response, async polling? Needs resolution before building Etapa 3.
- **GLM-5 API rate limits and error handling:** What happens if the GLM-5 API is unavailable or rate-limited during a search? Needs resolution before building Etapa 2.
- **S3 Vectors index configuration details:** Cosine distance, Float32, 1024 dimensions are set in the scope — are these configurable per index or hardcoded? Can wait until build time.
