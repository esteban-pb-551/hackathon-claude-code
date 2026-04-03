# S3 Vectors Semantic Search — Technical Spec

## Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Ingestion orchestrator | Python 3.13 (Lambda Durable Function) | Durable execution SDK for checkpointed retry on concurrent index creation |
| Embedding + vector storage | Rust (Cargo Lambda) | Learner's core expertise; no native Rust + S3 Vectors tool exists yet |
| Search + LLM | Rust (Cargo Lambda) | Same stack as ingestion; `reqwest` for LLM HTTP call |
| Infrastructure | AWS SAM | Single template for full stack deployment |
| Embedding model | VoyageAI voyage-4-large, 1024 dimensions | High quality embeddings with configurable dimension output |
| LLM | GLM-5 via Friendli API | Serverless inference, OpenAI-compatible chat completions |
| Vector store | AWS S3 Vectors | Native S3 service, serverless, no infra to manage |

### Key Dependencies — Rust Lambdas

| Crate | Version | Purpose | Docs |
|-------|---------|---------|------|
| `aws-sdk-s3vectors` | 1.23.0 | S3 Vectors CRUD operations | [docs.rs](https://docs.rs/aws-sdk-s3vectors/latest/aws_sdk_s3vectors/) |
| `aws-sdk-s3` | latest | Read uploaded files from S3 | [docs.rs](https://docs.rs/aws-sdk-s3/latest/aws_sdk_s3/) |
| `mongodb-voyageai` | 0.1.2 | VoyageAI embedding client + text normalization/chunking | [docs.rs](https://docs.rs/mongodb-voyageai/0.1.2/mongodb_voyageai/) |
| `reqwest` | 0.13.2 [json, native-tls] | HTTP client for GLM-5 API call (SearchS3Vectors only) | [docs.rs](https://docs.rs/reqwest/0.13.2/reqwest/) |
| `aws-sdk-secretsmanager` | 1 | Fetch API keys from Secrets Manager at runtime | [docs.rs](https://docs.rs/aws-sdk-secretsmanager/latest/aws_sdk_secretsmanager/) |
| `lambda_runtime` | 1.1.2 | Lambda handler runtime | [docs.rs](https://docs.rs/lambda_runtime/latest/lambda_runtime/) |
| `aws_lambda_events` | 1.1.2 | Lambda event type definitions | [docs.rs](https://docs.rs/aws_lambda_events/latest/aws_lambda_events/) |
| `aws-config` | latest | AWS SDK shared configuration | [docs.rs](https://docs.rs/aws-config/latest/aws_config/) |
| `aws-smithy-types` | latest | `Document` type for S3 Vectors metadata | [docs.rs](https://docs.rs/aws-smithy-types/latest/aws_smithy_types/) |
| `tokio` | latest [full] | Async runtime | [docs.rs](https://docs.rs/tokio/latest/tokio/) |
| `anyhow` | latest | Error handling | [docs.rs](https://docs.rs/anyhow/latest/anyhow/) |
| `serde` + `serde_json` | latest | JSON serialization/deserialization | [docs.rs](https://docs.rs/serde/latest/serde/) |

### Key Dependencies — Python Lambda

| Package | Version | Purpose |
|---------|---------|---------|
| `boto3` | 1.42.82 | AWS SDK (S3 Vectors, Lambda invocation) |
| `aws-durable-execution-sdk-python` | 1.3.0 | Durable execution SDK for checkpointed steps |

Dependencies managed with `uv` (`pyproject.toml` + `uv.lock`). `requirements.txt` generated via `uv export` for SAM build compatibility.

## Runtime & Deployment

- **Runtime:** AWS Lambda (all three functions)
- **Deployment:** AWS SAM CLI (`sam validate --lint && sam build && sam deploy`)
- **Rust build:** Cargo Lambda (already installed), `BuildMethod: makefile` with `--compiler cargo` for native aarch64
- **Architecture:** arm64 (Graviton) for all Lambdas
- **Region:** us-east-1
- **Demo:** Live in AWS. Tests run against real AWS services — no mocks. Test script: `scripts/test-etapa1.sh`
- **Secrets management:** API keys stored in AWS Secrets Manager (created by SAM template). Lambdas receive secret ARNs as env vars and fetch values at runtime via `aws-sdk-secretsmanager`.
- **Environment variables:**
  - `VOYAGE_SECRET_ARN` — ARN of VoyageAI API key secret (EmbedS3Vectors, SearchS3Vectors)
  - `FRIENDLI_SECRET_ARN` — ARN of Friendli token secret (SearchS3Vectors only)
  - `VECTOR_BUCKET_NAME` — S3 Vectors bucket name, extracted from ARN via `!Select [1, !Split ["/", !GetAtt VectorsBucket.VectorBucketArn]]` (all Lambdas)

## Architecture Overview

```
┌─────────────┐    PutObject (.txt)     ┌──────────────┐
│   Developer  │ ─────────────────────→  │  S3 Bucket   │
└─────────────┘                          └──────┬───────┘
                                                │ EventBridge rule
                                                │ (suffix: .txt)
                                                ▼
                                   ┌────────────────────────┐
                                   │   CheckS3Vectors (Py)  │
                                   │  Durable Function      │
                                   │  - get_index()         │
                                   │  - create_index() if   │
                                   │    missing (retry x3)  │
                                   │  - invoke EmbedLambda  │
                                   └────────────┬───────────┘
                                                │ async invoke
                                                ▼
                                   ┌────────────────────────┐
                                   │  EmbedS3Vectors (Rust) │
                                   │  - get_object() from S3│
                                   │  - read filter metadata│
                                   │  - normalize + chunk   │
                                   │  - embed (voyage-4-lg) │
                                   │  - put_vectors()       │
                                   └────────────────────────┘

                 ┌─────────────┐   POST /search    ┌───────────────┐
                 │   Developer  │ ───────────────→  │  API Gateway  │
                 └─────────────┘                    └───────┬───────┘
                                                           │
                                                           ▼
                                            ┌──────────────────────────┐
                                            │  SearchS3Vectors (Rust)  │
                                            │  - validate index exists │
                                            │  - embed query (voyage)  │
                                            │  - query_vectors()       │
                                            │  - POST to GLM-5 API    │
                                            │  - return LLM response   │
                                            └──────────────────────────┘
```

---

## CheckS3Vectors (Python, Durable Function)

Implements `prd.md > Document Ingestion` — index verification and creation with retry logic.

### Handler

- **Trigger:** EventBridge rule (S3 PutObject, suffix `.txt`)
- **Input event fields:** bucket name, object key (e.g., `movies/back_to_the_future.txt`)
- **Derives index name:** S3 prefix (first path segment). `movies/back_to_the_future.txt` → index name `movies`

### Index Verification Flow

1. Call `get_index()` on S3 Vectors with derived index name
2. If index exists → proceed to step 4
3. If index does not exist → call `create_index()` with:
   - `data_type`: Float32
   - `dimension`: 1024
   - `distance_metric`: Cosine
   - `metadata_configuration`: `source_text` as non-filterable (note: `filterableMetadataKeys` is not a valid boto3 parameter — filterable keys like `filter` are inferred automatically)
4. Invoke EmbedS3Vectors Lambda asynchronously, passing: bucket name, full object key, index name

### Retry Logic (Durable Function)

If `create_index()` fails (race condition from concurrent uploads to the same prefix):
- Retry up to 3 times
- On each retry: call `get_index()` first — if the index now exists, proceed normally
- After 3 failed attempts: terminate with error

### Error Reporting

- Captures and reports errors from EmbedS3Vectors invocation as failed jobs
- Logs all steps for observability

---

## EmbedS3Vectors (Rust)

Implements `prd.md > Document Ingestion` — file reading, chunking, embedding, and vector storage.

### Handler

- **Trigger:** Invoked by CheckS3Vectors (async Lambda invocation)
- **Input:** JSON payload with `bucket_name`, `object_key`, `index_name`

### Processing Pipeline

#### 1. Read File from S3

- `get_object()` using `bucket_name` and `object_key`
- Read the response body as UTF-8 string
- Read S3 object metadata: if key `filter` exists, use its value; otherwise default to `"none"`
- **Validation:** if file content is empty/null, return error (CheckS3Vectors captures this as failed job)

#### 2. Normalize and Chunk Text

Using `mongodb-voyageai::chunk` module (see `files/chunk_example.rs`):

```rust
let clean = normalize(&raw_text, &NormalizerConfig::prose());
let chunks = chunk_recursive(&clean, &ChunkConfig {
    chunk_size: 500,
    chunk_overlap: 80,
});
```

#### 3. Generate Embeddings

Using `mongodb-voyageai::Client` (see `files/use_voyage.rs`):

```rust
// At cold start (in main()), fetch secret from Secrets Manager and set as env var:
// load_secret(&secrets_client, &env::var("VOYAGE_SECRET_ARN")?, "VOYAGEAI_API_KEY").await?;
let voyage = VoyageClient::from_env(); // reads VOYAGEAI_API_KEY
let response = voyage
    .embed(chunks.clone())
    .model(model::VOYAGE_4_LARGE)
    .input_type("document")
    .output_dimension(1024)
    .send()
    .await?;
```

- Embeds all chunks in a single call (batch)
- Model: `voyage-4-large`, 1024 dimensions

#### 4. Insert Vectors into S3 Vectors

For each chunk + embedding pair, build a `PutInputVector`:

```rust
let metadata = Document::from(HashMap::from([
    ("source_text".to_string(), Document::from(chunk_text)),
    ("filter".to_string(), Document::from(filter_value)),
]));

PutInputVector::builder()
    .key(format!("{}#{}", object_key, chunk_index))
    .data(VectorData::Float32(embedding))
    .metadata(metadata)
    .build()?;
```

- **Vector key format:** `{object_key}#{chunk_index}` — unique per chunk, traceable to source file
- **Metadata:** `filter` (filterable) + `source_text` (non-filterable, contains chunk content)
- **S3 Vectors limit:** max 500 vectors per `put_vectors()` call. If chunks > 500, batch into multiple calls.

---

## SearchS3Vectors (Rust)

Implements `prd.md > Semantic Search` — query embedding, vector search, and LLM response generation.

### Handler

- **Trigger:** API Gateway HTTP POST `/search`
- **Input JSON body:**
  ```json
  {
    "index_name": "movies",
    "query": "adventures in space",
    "filter": "scifi"
  }
  ```
  - `index_name`: required
  - `query`: required
  - `filter`: optional

### Search Pipeline

#### 1. Validate Index Exists

- Call `get_index()` with the provided `index_name`
- If not found → return error response immediately (no LLM call)

#### 2. Generate Query Embedding

```rust
let response = voyage
    .embed(vec![&query])
    .model(model::VOYAGE_4_LARGE)
    .input_type("query")
    .output_dimension(1024)
    .send()
    .await?;
```

Note: `input_type` is `"query"` (not `"document"`) for search queries.

#### 3. Query S3 Vectors

```rust
let mut request = s3vectors
    .query_vectors()
    .vector_bucket_name(bucket_name)
    .index_name(index_name)
    .query_vector(VectorData::Float32(embedding))
    .top_k(5)
    .return_metadata(true);

if let Some(filter_value) = filter {
    let filter_doc = Document::from(HashMap::from([
        ("filter".to_string(), Document::from(filter_value)),
    ]));
    request = request.filter(filter_doc);
}

let response = request.send().await?;
```

- Returns top 5 results with metadata
- If filter provided, only matching vectors are returned

#### 4. Check Results

- If `response.vectors` is empty → return `"No relevant information found for the query"` (no LLM call)

#### 5. Build LLM Prompt

Extract `source_text` from each result's metadata. Construct the prompt:

```
System: You are a helpful assistant. Answer the user's question based only on the following context fragments. If the context doesn't contain enough information, say so.

Context:
[Fragment 1]
[Fragment 2]
...

User: {query}
```

#### 6. Call GLM-5 via Friendli API

Using `reqwest` 0.13.2:

```rust
let client = reqwest::Client::new();
let response = client
    .post("https://api.friendli.ai/serverless/v1/chat/completions")
    .header("Authorization", format!("Bearer {}", friendli_token))
    .json(&json!({
        "model": "zai-org/GLM-5",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": query}
        ]
    }))
    .send()
    .await?;
```

- Parse response: `choices[0].message.content`
- **Retry logic:** 3 attempts with incremental backoff (e.g., 1s, 2s, 4s). On each failure, wait then retry. After 3 failed attempts, return an error response.
- Return only the LLM-generated text to the caller

### Response Format

**Success:**
```json
{
  "response": "Based on the documents, adventures in space..."
}
```

**Error (index not found):**
```json
{
  "error": "Index 'movies' not found"
}
```

**No results:**
```json
{
  "response": "No relevant information found for the query"
}
```

---

## Infrastructure (SAM Template)

Implements `prd.md > Infrastructure & Deployment`.

### template.yaml

#### Resources — Etapa 1

| Resource | Type | Configuration |
|----------|------|---------------|
| VoyageApiKeySecret | `AWS::SecretsManager::Secret` | VoyageAI API key, `DeletionPolicy: Delete` |
| FriendliTokenSecret | `AWS::SecretsManager::Secret` | Friendli API token, `DeletionPolicy: Delete` |
| VectorsBucket | `AWS::S3Vectors::VectorBucket` | Created by stack, tagged `voyageModel=voyage-4-large` |
| UploadBucket | `AWS::S3::Bucket` | Standard bucket for file uploads, EventBridge enabled |
| EventBridge Rule | `AWS::Events::Rule` | Source: `aws.s3`, detail-type: `Object Created`, filter: suffix `.txt` |
| CheckS3Vectors | `AWS::Serverless::Function` | Python 3.13, arm64, Durable Function, triggered by EventBridge |
| EmbedS3Vectors | `AWS::Serverless::Function` | Rust (provided.al2023, arm64, BuildMethod: makefile), invoked by CheckS3Vectors |

#### Resources — Etapa 2

| Resource | Type | Configuration |
|----------|------|---------------|
| SearchS3Vectors | `AWS::Serverless::Function` | Rust (provided.al2023, arm64, BuildMethod: makefile), API Gateway HTTP POST `/search` |
| API Gateway | `AWS::Serverless::HttpApi` | HTTP API, single route: `POST /search` |

#### Environment Variables

All Lambdas receive `VECTOR_BUCKET_NAME` (bucket name extracted from ARN). Rust Lambdas additionally receive `VOYAGE_SECRET_ARN` (Secrets Manager ARN). SearchS3Vectors additionally receives `FRIENDLI_SECRET_ARN`.

#### Tags

- Stack-level (via `samconfig.toml`): `application=s3-vectors-search`, `environment=development`
- VectorsBucket: `voyageModel=voyage-4-large`

#### IAM Permissions

- CheckS3Vectors: `s3vectors:GetIndex`, `s3vectors:CreateIndex` (scoped to VectorsBucket ARN + `/*`), `lambda:InvokeFunction` (EmbedS3Vectors ARN)
- EmbedS3Vectors: `secretsmanager:GetSecretValue` (scoped to VoyageApiKeySecret), `s3:GetObject` (scoped to UploadBucket), `s3vectors:PutVectors` (scoped to VectorsBucket ARN + `/*`)
- SearchS3Vectors: `secretsmanager:GetSecretValue` (scoped to VoyageApiKeySecret + FriendliTokenSecret), `s3vectors:GetIndex`, `s3vectors:QueryVectors` (scoped to VectorsBucket ARN + `/*`)

---

## Data Model

### S3 Vectors Index Configuration

| Parameter | Value |
|-----------|-------|
| Data type | Float32 |
| Dimensions | 1024 |
| Distance metric | Cosine |
| Filterable metadata keys | `filter` |
| Non-filterable metadata keys | `source_text` |

### Vector Record

| Field | Type | Description |
|-------|------|-------------|
| `key` | String | `{object_key}#{chunk_index}` — unique identifier per chunk |
| `data` | Float32[1024] | Embedding vector from VoyageAI |
| `metadata.filter` | String (filterable) | From S3 object metadata `filter` key, defaults to `"none"` |
| `metadata.source_text` | String (non-filterable) | Raw chunk content for RAG context |

### S3 Vectors Limits (relevant)

- Max 500 vectors per `put_vectors()` call
- Max 40 KB total metadata per vector
- Max 2 KB filterable metadata per vector
- Max 100 top-K results per query
- Max 4,096 dimensions (we use 1,024)

### Index Naming Convention

S3 prefix (first path segment) becomes the index name:
- `movies/back_to_the_future.txt` → index `movies`
- `papers/attention_is_all_you_need.txt` → index `papers`

---

## File Structure

```
hackathon/
├── lambdas/
│   ├── check-s3-vectors/              # CheckS3Vectors (Python, Durable Function)
│   │   ├── check_s3_vectors.py        # Handler: index verify/create, invoke Embed
│   │   ├── pyproject.toml             # Dependencies managed with uv
│   │   ├── uv.lock                    # Pinned dependency versions
│   │   └── requirements.txt           # Generated via `uv export` for SAM build
│   ├── embed-s3-vectors/              # EmbedS3Vectors (Rust)
│   │   ├── src/
│   │   │   └── main.rs                # Handler: fetch secret, read S3, chunk, embed, store
│   │   ├── Cargo.toml                 # aws-sdk-s3vectors, aws-sdk-s3, aws-sdk-secretsmanager, etc.
│   │   └── Makefile                   # SAM build target (cargo lambda --compiler cargo)
│   └── search-s3-vectors/             # SearchS3Vectors (Rust)
│       ├── src/
│       │   └── main.rs                # Handler: embed query, search vectors, call GLM-5
│       ├── Cargo.toml                 # aws-sdk-s3vectors, mongodb-voyageai, reqwest, etc.
│       └── Makefile                   # SAM build target
├── template.yaml                      # SAM template: Secrets, S3 Vectors Bucket, S3, EventBridge, Lambdas, API Gateway
├── samconfig.toml                     # SAM deploy config with stack-level tags
├── scripts/
│   └── test-etapa1.sh                 # End-to-end test for ingestion pipeline
├── events/                            # Test event payloads and data
│   └── test-data/
│       └── back_to_the_future.txt
├── docs/                              # Hackathon artifacts
│   ├── learner-profile.md
│   ├── scope.md
│   ├── prd.md
│   └── spec.md
├── files/                             # Reference implementations
│   ├── use_voyage.rs                  # S3 Vectors + VoyageAI full flow example
│   └── chunk_example.rs               # Text normalization + chunking example
├── process-notes.md
└── README.md                          # Deployment instructions (SAM), no project tree
```

---

## Key Technical Decisions

### 1. Two-Lambda ingestion instead of one

**Decision:** Split ingestion into CheckS3Vectors (Python) + EmbedS3Vectors (Rust).
**Why:** The index check/create step needs Durable Function retry logic for race conditions. Python is natural for Step Functions orchestration. The heavy lifting (chunking, embedding, vector insertion) runs in Rust for performance.
**Tradeoff:** Two Lambdas to maintain instead of one, but cleaner separation of concerns.

### 2. `mongodb-voyageai` as embedding + chunking client

**Decision:** Use `mongodb-voyageai` crate for VoyageAI API calls and text chunking — not for MongoDB.
**Why:** Provides a clean Rust client for VoyageAI embeddings plus built-in `normalize()` and `chunk_recursive()`. No need to build a custom HTTP client or chunking logic.
**Tradeoff:** Crate name is misleading (implies MongoDB dependency), but the functionality is exactly what's needed.

### 3. `reqwest` for GLM-5 only

**Decision:** Use `reqwest` 0.13.2 exclusively for the Friendli/GLM-5 API call in SearchS3Vectors. All AWS and VoyageAI calls use their native SDKs.
**Why:** GLM-5 has a standard OpenAI-compatible REST API. `reqwest` with `json` + `native-tls` features is the lightest way to make that single HTTP call.
**Tradeoff:** Adds a dependency to SearchS3Vectors that EmbedS3Vectors doesn't need — but it's scoped to one Lambda.

---

## Dependencies & External Services

| Service | Usage | Auth | Limits | Docs |
|---------|-------|------|--------|------|
| AWS S3 | File uploads (`.txt`) | IAM role | Standard S3 limits | [S3 docs](https://docs.aws.amazon.com/s3/) |
| AWS S3 Vectors | Vector storage and similarity search (bucket created by SAM stack) | IAM role | 500 vectors/put, 1K writes/s/index, 100 top-K | [S3 Vectors docs](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-vectors.html) |
| AWS Secrets Manager | Store VoyageAI and Friendli API keys | IAM role | — | [Secrets Manager docs](https://docs.aws.amazon.com/secretsmanager/) |
| AWS EventBridge | S3 → Lambda trigger | IAM role | — | [EventBridge docs](https://docs.aws.amazon.com/eventbridge/) |
| AWS Lambda | All three functions (arm64/Graviton) | IAM role | 15 min timeout, 10 GB memory | [Lambda docs](https://docs.aws.amazon.com/lambda/) |
| AWS API Gateway | HTTP API for search | Public | — | [API Gateway docs](https://docs.aws.amazon.com/apigateway/) |
| VoyageAI | Embedding generation (voyage-4-large) | Secrets Manager (`VOYAGE_SECRET_ARN`) | 200M free tokens/mo, 2K RPM (Tier 1) | [VoyageAI docs](https://docs.voyageai.com/) |
| Friendli (GLM-5) | LLM response generation | Secrets Manager (`FRIENDLI_SECRET_ARN`) | Per-plan rate limits | [Friendli docs](https://docs.friendli.ai/) |

---

## Open Issues

1. **Chunking configuration tuning:** `chunk_size: 500` and `chunk_overlap: 80` are starting values from the reference implementation. May need adjustment after testing with real documents to balance retrieval quality vs. vector count.
