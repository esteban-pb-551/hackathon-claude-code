# S3 Vectors Semantic Search

A serverless, event-driven semantic search engine built on AWS. Upload `.txt` files to S3, and they are automatically ingested with VoyageAI embeddings into S3 Vectors. Search your documents by meaning and get LLM-generated responses powered by GLM-5.

> [!IMPORTANT]
>
> This project uses **AWS S3 Vectors**, a relatively new AWS service for native vector storage.
> Make sure your AWS account has access to S3 Vectors in your target region (`us-east-1`) before
> deploying. All API keys are stored in **AWS Secrets Manager** — they are never exposed as
> plain-text environment variables.

## Prerequisites

- **Rust** toolchain (stable) + [Cargo Lambda](https://www.cargo-lambda.info/) for building Lambda functions targeting arm64:
  ```bash
  cargo install cargo-lambda
  ```
- **AWS SAM CLI** ([install guide](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html))
- **AWS credentials** configured (`aws configure`) with permissions for Lambda, S3, S3 Vectors, Secrets Manager, EventBridge, API Gateway, DynamoDB, IAM, and CloudFormation
- **Python 3.13** + [uv](https://github.com/astral-sh/uv) (for the CheckS3Vectors Lambda dependencies)
- **Node.js** (for building the Vue 3 frontends)
- **API keys:**
  - [VoyageAI](https://www.voyageai.com/) API key (for embedding generation with voyage-4-large)
  - [Friendli](https://friendli.ai/) API token (for GLM-5 LLM inference)

> [!TIP]
>
> You can verify that Cargo Lambda is correctly installed by running `cargo lambda --version`.
> If building on a non-ARM machine, the `--compiler cargo` flag in each Lambda's Makefile
> handles cross-compilation to `arm64` automatically.

## Setup

1. **Clone the repository:**
   ```bash
   git clone <repo-url>
   cd hackathon
   ```

2. **Configure deployment settings:**

   Copy the example configuration and fill in your API keys:
   ```bash
   cp '[example]samconfig.toml' samconfig.toml
   ```

   Edit `samconfig.toml` and set your values:
   ```toml
   [default.global.parameters]
   stack_name = "s3-vectors-search"
   region = "us-east-1"

   [default.build.parameters]
   cached = true
   parallel = true

   [default.deploy.parameters]
   capabilities = "CAPABILITY_IAM"
   confirm_changeset = false
   resolve_s3 = true
   parameter_overrides = "VoyageApiKey=\"your-voyageai-api-key\" FriendliToken=\"your-friendli-token\""
   tags = "application=\"s3-vectors-search\" environment=\"development\""
   ```

> [!IMPORTANT]
>
> The `parameter_overrides` line passes your API keys to the SAM template. They are stored
> in AWS Secrets Manager at deploy time (never as plain-text environment variables). Each
> Lambda fetches the secret values at runtime via their ARNs. **Never commit `samconfig.toml`
> with real API keys to version control.**

## Build and Deploy

```bash
# Validate the template
sam validate --lint

# Build all functions (Rust via Cargo Lambda, Python via pip)
sam build

# Deploy the stack
sam deploy
```

If this is your **first deploy** and you do not have a `samconfig.toml` yet, use the guided flow instead:
```bash
sam deploy --guided
```
This will prompt you for the stack name, region, API keys, and other parameters, then generate `samconfig.toml` for future deploys.

> [!TIP]
>
> After a successful deploy, the stack outputs are printed to the terminal. You can retrieve
> them at any time with:
> ```bash
> aws cloudformation describe-stacks \
>   --stack-name s3-vectors-search \
>   --query "Stacks[0].Outputs" \
>   --output table
> ```
> The key outputs you need are **UploadBucketName**, **SearchApiUrl**, and **UploadApiUrl**.

## Architecture

The system is split into three pipelines: **ingestion**, **search**, and **upload**.

```
                              ┌──────────────────┐
                              │   Upload Frontend │
                              │   (Vue 3 / Vite)  │
                              └────────┬─────────┘
                                       │ POST /upload
                                       ▼
                              ┌──────────────────┐    PutObject (.txt)     ┌──────────────┐
                              │  UploadS3 Lambda  │ ────────────────────→  │  S3 Bucket    │
                              │  (Rust)           │                        └──────┬───────┘
                              └──────────────────┘                                │
                                                                EventBridge rule  │
                                                                (suffix: .txt)    │
                                                                                  ▼
                                                             ┌────────────────────────┐
                                                             │  CheckS3Vectors (Py)   │
                                                             │  Durable Function       │
                                                             │  - get_index()          │
                                                             │  - create_index() if    │
                                                             │    missing (retry x3)   │
                                                             │  - invoke Embed Lambda  │
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

┌──────────────────┐                                         ┌──────────────────────────┐
│  Search Frontend │  POST /search                           │  SearchS3Vectors (Rust)  │
│  (Vue 3 / Vite)  │ ─────────────→  API Gateway  ────────→ │  - create DynamoDB record│
└──────────────────┘                                         │  - invoke SearchWorker   │
        │                                                    │  - return request_id     │
        │  GET /search/{id}                                  └──────────────────────────┘
        │  (poll every 4s)                                                │
        └──────────────────→  API Gateway  ─→  SearchS3Vectors  ─→  DynamoDB (read)
                                                                          │
                                                          ┌───────────────┘
                                                          ▼
                                            ┌──────────────────────────┐
                                            │  SearchWorker (Rust)     │
                                            │  - validate index exists │
                                            │  - embed query (voyage)  │
                                            │  - query_vectors()       │
                                            │  - POST to GLM-5 API    │
                                            │  - write result → DynamoDB│
                                            └──────────────────────────┘
```

> [!TIP]
>
> The search pipeline uses an **async polling pattern** to bypass API Gateway's 30-second
> timeout. The initial `POST /search` returns immediately with a `request_id` (HTTP 202),
> and the frontend polls `GET /search/{request_id}` every 4 seconds until the result is ready.
> This allows the full RAG pipeline (embed + vector search + LLM) to run for up to 5 minutes.

## Lambda Functions

The stack deploys **five** Lambda functions, all running on **arm64 (Graviton)**:

### CheckS3Vectors — Python 3.13, Durable Function

| Property | Value |
|----------|-------|
| Runtime | Python 3.13 |
| Timeout | 120s (execution: 600s) |
| Memory | 256 MB |
| Trigger | EventBridge rule (S3 Object Created, suffix `.txt`) |

**What it does:**
1. Receives an EventBridge event when a `.txt` file is uploaded to the S3 bucket
2. Derives the index name from the S3 key prefix (e.g., `movies/file.txt` → index `movies`)
3. Calls `get_index()` on S3 Vectors to check if the index exists
4. If the index does not exist, calls `create_index()` with Float32 / 1024 dimensions / Cosine distance
5. Retries up to 3 times if index creation fails (handles race conditions from concurrent uploads)
6. Invokes `EmbedS3Vectors` asynchronously, passing: bucket name, object key, and index name

**IAM permissions:** `s3vectors:GetIndex`, `s3vectors:CreateIndex`, `lambda:InvokeFunction`

> [!IMPORTANT]
>
> This Lambda uses the **AWS Lambda Durable Functions SDK** (`@durable_step` / `@durable_execution`),
> not a standard Lambda with manual retry logic. Durable Functions provide checkpointed execution,
> so if the function is interrupted, it resumes from the last completed step.

### EmbedS3Vectors — Rust

| Property | Value |
|----------|-------|
| Runtime | provided.al2023 (Rust via Cargo Lambda) |
| Timeout | 300s |
| Memory | 512 MB |
| Trigger | Invoked asynchronously by CheckS3Vectors |

**What it does:**
1. Receives a payload with `bucket_name`, `object_key`, and `index_name`
2. Reads the file content from S3 via `get_object()`
3. Reads the S3 object metadata key `filter` (defaults to `"none"` if absent)
4. Validates the file content is not empty
5. Normalizes and chunks the text using `mongodb-voyageai` (`normalize` + `chunk_recursive`, chunk size 500, overlap 80)
6. Generates embeddings via VoyageAI (voyage-4-large, 1024 dimensions, batch call)
7. Inserts vectors into S3 Vectors with metadata: `filter` (filterable) and `source_text` (non-filterable, contains the chunk content)

**IAM permissions:** `secretsmanager:GetSecretValue`, `s3:GetObject`, `s3vectors:PutVectors`

> [!TIP]
>
> The VoyageAI API key is fetched from Secrets Manager during **cold start** and set as the
> `VOYAGEAI_API_KEY` environment variable so that `VoyageClient::from_env()` picks it up
> automatically. This avoids fetching the secret on every invocation.

### SearchS3Vectors — Rust (API Handler)

| Property | Value |
|----------|-------|
| Runtime | provided.al2023 (Rust via Cargo Lambda) |
| Timeout | 29s |
| Memory | 256 MB |
| Trigger | API Gateway HTTP: `POST /search`, `GET /search/{requestId}` |

**What it does:**

**`POST /search`** — Initiates an async search request:
1. Parses the request body: `index_name` (required), `query` (required), `filter` (optional)
2. Generates a UUID `request_id`
3. Creates a DynamoDB record with: `request_id`, `status: "pending"`, `query`, `created_at`, `ttl` (1 hour)
4. Invokes `SearchWorker` Lambda asynchronously (fire-and-forget) with the full search payload
5. Returns HTTP 202 with `{"request_id": "..."}`

**`GET /search/{requestId}`** — Polls for the search result:
1. Reads the DynamoDB record by `request_id`
2. If `status` is `"pending"`, returns `{"status": "pending"}`
3. If `status` is `"complete"` or `"error"`, returns the status merged with the `response_body` JSON

**IAM permissions:** `dynamodb:PutItem`, `dynamodb:GetItem`, `lambda:InvokeFunction`

### SearchWorker — Rust (RAG Pipeline)

| Property | Value |
|----------|-------|
| Runtime | provided.al2023 (Rust via Cargo Lambda) |
| Timeout | 300s |
| Memory | 512 MB |
| Trigger | Invoked asynchronously by SearchS3Vectors |

**What it does:**
1. Validates the target index exists in S3 Vectors via `get_index()`
2. Generates the query embedding via VoyageAI (voyage-4-large, 1024 dimensions, `input_type: "query"`)
3. Queries S3 Vectors for the top 5 most similar vectors (with optional metadata filter)
4. If no vectors are found, writes `"No relevant information found for the query"` to DynamoDB and returns
5. Extracts `source_text` metadata from each result and builds a RAG prompt
6. Calls GLM-5 via the Friendli API (`POST https://api.friendli.ai/serverless/v1/chat/completions`) with retry logic (3 attempts, backoff: 1s, 2s, 4s)
7. Writes the final result (`complete` or `error`) to DynamoDB via `UpdateItem`

**IAM permissions:** `secretsmanager:GetSecretValue`, `s3vectors:GetIndex`, `s3vectors:QueryVectors`, `s3vectors:GetVectors`, `dynamodb:UpdateItem`

> [!IMPORTANT]
>
> The SearchWorker always writes its result to DynamoDB, even on failure. This ensures the
> frontend polling loop always resolves (either with a response or an error message) instead
> of timing out silently.

### UploadS3 — Rust

| Property | Value |
|----------|-------|
| Runtime | provided.al2023 (Rust via Cargo Lambda) |
| Timeout | 29s |
| Memory | 256 MB |
| Trigger | API Gateway HTTP: `POST /upload` |

**What it does:**
1. Parses the request body: `index_name` (required), `filename` (required), `filter` (optional), `content` (required, base64-encoded)
2. Validates that `filename` ends with `.txt`
3. Validates that `index_name` and `content` are not empty
4. Decodes the base64 content
5. Uploads the file to S3 as `{index_name}/{filename}` with `Content-Type: text/plain`
6. If a `filter` value is provided, sets it as S3 object metadata (`x-amz-meta-filter`)
7. Returns the S3 key, index name, and filter value on success

**IAM permissions:** `s3:PutObject`

> [!TIP]
>
> After UploadS3 places the file in the S3 bucket, the existing ingestion pipeline kicks in
> automatically: EventBridge detects the `.txt` upload, triggers CheckS3Vectors, which then
> invokes EmbedS3Vectors. No additional wiring is needed — the upload frontend triggers the
> full ingestion pipeline end-to-end.

## Other AWS Resources

| Resource | Type | Purpose |
|----------|------|---------|
| **VoyageApiKeySecret** | Secrets Manager | Stores the VoyageAI API key |
| **FriendliTokenSecret** | Secrets Manager | Stores the Friendli API token |
| **VectorsBucket** | S3 Vectors Bucket | Stores vector indexes and vector data |
| **UploadBucket** | S3 Bucket | Receives uploaded `.txt` files; EventBridge-enabled |
| **SearchResultsTable** | DynamoDB Table | Async search state store (PAY_PER_REQUEST, TTL: 1 hour) |
| **SearchHttpApi** | API Gateway HTTP | Routes: `POST /search`, `GET /search/{id}`, `POST /upload` |

## Uploading Documents

### Via CLI

Upload a `.txt` file to the S3 upload bucket. The S3 key prefix (first path segment) determines the index name in S3 Vectors.

```bash
aws s3 cp events/test-data/back_to_the_future.txt \
  s3://<UploadBucketName>/movies/back_to_the_future.txt \
  --metadata filter=scifi
```

### Via the Upload Frontend

Use the upload web app (see [Frontend](#frontend) below). Select a `.txt` file, enter the index name, optionally set a filter, and click Upload.

### Conventions

- The S3 prefix becomes the index name: `movies/back_to_the_future.txt` creates (or reuses) the index `movies`.
- The optional `filter` metadata key lets you tag documents for filtered search. If omitted, the filter defaults to `none`.
- Only `.txt` files trigger the ingestion pipeline. Other file types are ignored.

> [!TIP]
>
> You can upload multiple files to the same index (prefix). Each file is chunked and embedded
> independently, but all vectors share the same index. For example, uploading three movie
> summaries to `movies/` makes all three searchable under the `movies` index.

## Running a Search

### Via CLI

Send a POST request to the search endpoint:

```bash
# Start a search (returns request_id)
curl -s -X POST https://<SearchApiUrl>/search \
  -H "Content-Type: application/json" \
  -d '{"index_name":"movies","query":"time travel adventures","filter":"scifi"}'

# Poll for the result
curl -s https://<SearchApiUrl>/search/<request_id>
```

### Via the Search Frontend

Use the search web app (see [Frontend](#frontend) below). Enter an index name, a query, optionally a filter, and click Search. The app polls automatically until the result is ready.

### Request Body

| Field        | Required | Description                                       |
|--------------|----------|---------------------------------------------------|
| `index_name` | Yes      | Name of the S3 Vectors index to search            |
| `query`      | Yes      | Natural language search query                     |
| `filter`     | No       | Filter results by the `filter` metadata value     |

### Response

**Success (after polling):**
```json
{
  "status": "complete",
  "response": "Based on the documents, Back to the Future follows Marty McFly..."
}
```

**Error:**
```json
{
  "status": "error",
  "error": "Index 'nonexistent' not found or inaccessible: ..."
}
```

**No results:**
```json
{
  "status": "complete",
  "response": "No relevant information found for the query"
}
```

> [!IMPORTANT]
>
> The initial `POST /search` returns HTTP **202** with a `request_id`, not the final result.
> You must poll `GET /search/{request_id}` until the status changes from `"pending"` to
> `"complete"` or `"error"`. The search frontend handles this automatically.

## Frontend

The project includes two single-page applications built with **Vue 3** (Composition API, `<script setup>`, Vite). Both share the same visual design: dark/light theme toggle, responsive layout, and consistent styling.

### Search App — `frontend/search/`

Semantic search interface for querying indexed documents.

```
frontend/search/
├── index.html
├── package.json
├── vite.config.js              # Dev server: port 52193
└── src/
    ├── main.js
    ├── App.vue
    ├── config.js               # API endpoint URL (SearchApiUrl)
    ├── style.css               # Global styles + dark/light theme variables
    ├── composables/
    │   ├── useTheme.js         # Dark/light theme (localStorage persistence)
    │   └── useSearch.js        # Async polling search (POST → poll GET)
    └── components/
        ├── AppHeader.vue       # Title, badges, theme toggle
        ├── ThemeToggle.vue     # Dark/light switch with accessibility
        ├── SearchForm.vue      # Index name, query, filter fields
        ├── SearchResult.vue    # Result card (success/error states)
        └── AppFooter.vue
```

**Features:**
- Search form with index name (required), query (required), and filter (optional)
- Async polling with animated progress bar and elapsed time counter
- Cancel button to abort in-progress searches
- Form locks after success; "New Question" button to reset
- Error display for network failures, timeouts, and backend errors

### Upload App — `frontend/upload/`

File upload interface for ingesting `.txt` documents into the pipeline.

```
frontend/upload/
├── index.html
├── package.json
├── vite.config.js              # Dev server: port 52194
└── src/
    ├── main.js
    ├── App.vue
    ├── config.js               # API endpoint URL (UploadApiUrl)
    ├── style.css               # Global styles + dark/light theme variables
    ├── composables/
    │   ├── useTheme.js         # Dark/light theme (localStorage persistence)
    │   └── useUpload.js        # Upload flow (POST with base64 content)
    └── components/
        ├── AppHeader.vue       # Title, badges, theme toggle
        ├── ThemeToggle.vue     # Dark/light switch with accessibility
        ├── UploadForm.vue      # Index name, filter, file picker
        ├── UploadResult.vue    # Result card (success/error states)
        └── AppFooter.vue
```

**Features:**
- File picker restricted to `.txt` files with client-side validation
- Styled drop zone showing file name and size after selection
- Index name (required) and filter (optional) fields
- Loading indicator with progress bar and elapsed time
- Success card showing the S3 key where the file was stored

### Configuration

Before running either frontend, update the API endpoint in the respective `config.js`:

```javascript
// frontend/search/src/config.js
export const API_URL = 'https://<your-api-id>.execute-api.us-east-1.amazonaws.com/search'

// frontend/upload/src/config.js
export const API_URL = 'https://<your-api-id>.execute-api.us-east-1.amazonaws.com/upload'
```

> [!TIP]
>
> Both frontends share the same API Gateway (`SearchHttpApi`). The API ID is the same for
> both — only the path differs (`/search` vs `/upload`). You can find the full URLs in the
> CloudFormation stack outputs (`SearchApiUrl` and `UploadApiUrl`).

### Local Development

```bash
# Search frontend (port 52193)
cd frontend/search
npm install
npm run dev

# Upload frontend (port 52194)
cd frontend/upload
npm install
npm run dev
```

### Production Build

```bash
cd frontend/search && npm run build   # outputs to frontend/search/dist/
cd frontend/upload && npm run build   # outputs to frontend/upload/dist/
```

### Deploy on EC2 with nginx

To serve both frontends from an EC2 instance:

1. **Launch an EC2 instance** (Amazon Linux 2023, t3.micro is sufficient) with a security group allowing inbound HTTP (port 80) and SSH (port 22).

2. **Install nginx and Node.js:**
   ```bash
   sudo dnf install -y nginx nodejs
   sudo systemctl enable nginx
   ```

3. **Clone the repo and build both frontends:**
   ```bash
   git clone <repo-url>
   cd hackathon/frontend/search && npm install && npm run build
   cd ../upload && npm install && npm run build
   ```

4. **Copy the build outputs to nginx:**
   ```bash
   sudo mkdir -p /usr/share/nginx/html/search /usr/share/nginx/html/upload
   sudo cp -r frontend/search/dist/* /usr/share/nginx/html/search/
   sudo cp -r frontend/upload/dist/* /usr/share/nginx/html/upload/
   ```

5. **Start nginx:**
   ```bash
   sudo systemctl start nginx
   ```

6. The frontends are now accessible at:
   - `http://<ec2-public-ip>/search/`
   - `http://<ec2-public-ip>/upload/`

> [!IMPORTANT]
>
> The API Gateway returns CORS headers (`access-control-allow-origin: *`), so cross-origin
> requests from the EC2-hosted frontends work without additional nginx proxy configuration.
> If you restrict CORS origins in production, update the `AllowOrigins` list in the
> `SearchHttpApi` resource in `template.yaml`.

## Test Scripts

End-to-end test scripts are provided in the `scripts/` directory:

```bash
# Test the ingestion pipeline (Etapa 1)
bash scripts/test-etapa1.sh

# Test the search API (Etapa 2)
bash scripts/test-etapa2.sh
```

> [!TIP]
>
> These scripts run against **real AWS services** using your deployed stack. Make sure the
> stack is deployed and the outputs (bucket name, API URL) are correct before running them.
> The ingestion test uploads a file and waits for vectors to appear; the search test runs
> four scenarios covering success, filters, invalid index, and missing body.
