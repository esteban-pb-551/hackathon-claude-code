# S3 Vectors Semantic Search

A serverless, event-driven semantic search engine built on AWS. Upload `.txt` files to S3, and they are automatically ingested with VoyageAI embeddings into S3 Vectors. Search your documents by meaning and get LLM-generated responses powered by GLM-5.

## Prerequisites

- **Rust** toolchain (stable) + [Cargo Lambda](https://www.cargo-lambda.info/) for building Lambda functions targeting arm64:
  ```bash
  cargo install cargo-lambda
  ```
- **AWS SAM CLI** ([install guide](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html))
- **AWS credentials** configured (`aws configure`) with permissions for Lambda, S3, S3 Vectors, Secrets Manager, EventBridge, API Gateway, IAM, and CloudFormation
- **Python 3.13** + [uv](https://github.com/astral-sh/uv) (for the CheckS3Vectors Lambda dependencies)
- **API keys:**
  - [VoyageAI](https://www.voyageai.com/) API key (for embedding generation with voyage-4-large)
  - [Friendli](https://friendli.ai/) API token (for GLM-5 LLM inference)

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

   The `parameter_overrides` line passes your API keys to the SAM template. They are stored in AWS Secrets Manager at deploy time (never as plain-text environment variables). Lambdas fetch the secrets at runtime via their ARNs.

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

After a successful deploy, the stack outputs are printed to the terminal. Take note of:
- **UploadBucketName** -- the S3 bucket where you upload `.txt` files
- **SearchApiUrl** -- the API Gateway endpoint for semantic search

You can retrieve these at any time with:
```bash
aws cloudformation describe-stacks \
  --stack-name s3-vectors-search \
  --query "Stacks[0].Outputs" \
  --output table
```

## Uploading Documents

Upload a `.txt` file to the S3 upload bucket. The S3 key prefix (first path segment) determines the index name in S3 Vectors.

```bash
aws s3 cp events/test-data/back_to_the_future.txt \
  s3://<UploadBucketName>/movies/back_to_the_future.txt \
  --metadata filter=scifi
```

**Conventions:**
- The S3 prefix becomes the index name: `movies/back_to_the_future.txt` creates (or reuses) the index `movies`.
- The optional `filter` metadata key lets you tag documents for filtered search. If omitted, the filter defaults to `none`.
- Only `.txt` files trigger the ingestion pipeline. Other file types are ignored.

**What happens behind the scenes:** The upload triggers an EventBridge rule, which invokes the CheckS3Vectors Lambda. It verifies (or creates) the S3 Vectors index, then invokes the EmbedS3Vectors Lambda. That function reads the file, chunks the text, generates embeddings via VoyageAI, and stores the vectors in S3 Vectors.

## Running a Search

Send a POST request to the search endpoint:

```bash
curl -X POST https://<SearchApiUrl>/search \
  -H "Content-Type: application/json" \
  -d '{"index_name":"movies","query":"time travel adventures","filter":"scifi"}'
```

**Request body:**
| Field        | Required | Description                                       |
|--------------|----------|---------------------------------------------------|
| `index_name` | Yes      | Name of the S3 Vectors index to search            |
| `query`      | Yes      | Natural language search query                     |
| `filter`     | No       | Filter results by the `filter` metadata value     |

**Response:** The API returns a JSON object with the LLM-generated answer based on the most relevant document fragments:
```json
{
  "response": "Based on the documents, Back to the Future follows Marty McFly..."
}
```

If the index does not exist, the API returns an error. If no relevant results are found, it returns a message indicating so -- without invoking the LLM.

## Architecture

The system is split into two pipelines:

**Ingestion pipeline:**
S3 upload (.txt) --> EventBridge rule --> CheckS3Vectors (Python, Durable Function) --> EmbedS3Vectors (Rust) --> S3 Vectors

1. A `.txt` file lands in the S3 upload bucket.
2. EventBridge matches the Object Created event (suffix `.txt`) and invokes CheckS3Vectors.
3. CheckS3Vectors (Python 3.13, Lambda Durable Function) verifies the target index exists in S3 Vectors. If not, it creates one with Float32 / 1024 dimensions / Cosine distance. Retry logic handles race conditions from concurrent uploads.
4. CheckS3Vectors invokes EmbedS3Vectors asynchronously.
5. EmbedS3Vectors (Rust) reads the file from S3, normalizes and chunks the text, generates embeddings via VoyageAI (voyage-4-large, 1024 dimensions), and inserts the vectors into S3 Vectors with `source_text` and `filter` metadata.

**Search pipeline:**
API Gateway POST /search --> SearchS3Vectors (Rust) --> VoyageAI + S3 Vectors + GLM-5

1. A POST request hits the API Gateway `/search` endpoint.
2. SearchS3Vectors (Rust) validates the index exists, embeds the query via VoyageAI, queries S3 Vectors for the top 5 similar vectors (with optional filter), and sends the retrieved context fragments to GLM-5 via the Friendli API.
3. The LLM-generated response is returned to the caller.

All three Lambda functions run on arm64 (Graviton). API keys are stored in AWS Secrets Manager and fetched at runtime.

## Frontend

A single-page search UI built with Vue 3 (Composition API) is available at `frontend/index.html`. It includes:

- **Theme toggle:** Switch between dark and light modes (persisted in localStorage)
- **Search form:** Index name, query, and optional filter fields
- **Loading state:** Progress indicator with elapsed time counter
- **Response display:** Success and error states with request metadata

### Running the frontend

Open `frontend/index.html` directly in your browser -- no build step or server required. The page loads Vue 3 via CDN (ES module import map) and posts directly to the API Gateway endpoint.

Before using it, update the `API_URL` constant in the `<script>` section at the bottom of the file to match your deployed `SearchApiUrl`:

```javascript
const API_URL = 'https://<your-api-id>.execute-api.us-east-1.amazonaws.com/search'
```

The API returns CORS headers (`access-control-allow-origin: *`), so the frontend works from `file://` or any static host.

## Test Scripts

End-to-end test scripts are provided in the `scripts/` directory:

```bash
# Test the ingestion pipeline (Etapa 1)
bash scripts/test-etapa1.sh

# Test the search API (Etapa 2)
bash scripts/test-etapa2.sh
```

These scripts run against real AWS services using your deployed stack.
