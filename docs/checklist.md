# Build Checklist

## Build Preferences

- **Build mode:** Autonomous
- **Comprehension checks:** N/A (autonomous mode)
- **Git:** Commit after each item with message: "Complete step N: [title]"
- **Verification:** Yes. Checkpoints every 3-4 items — agent pauses, gives summary, learner confirms before continuing.
- **Check-in cadence:** N/A (autonomous mode)

## Checklist

### Etapa 1 — Ingestion Backend

- [ ] **1. SAM template — Etapa 1 infrastructure**
  Spec ref: `spec.md > Infrastructure (SAM Template) > Resources — Etapa 1`
  What to build: Create `template.yaml` with: S3 bucket for file uploads, S3 Vectors bucket (`AWS::S3Tables::TableBucket` or equivalent), EventBridge rule (source `aws.s3`, detail-type `Object Created`, suffix filter `.txt`), CheckS3Vectors Lambda (Python 3.12, Durable Function, triggered by EventBridge), EmbedS3Vectors Lambda (Rust, `provided.al2023` via Cargo Lambda, invoked by CheckS3Vectors). Define IAM permissions per spec: CheckS3Vectors gets `s3vectors:GetIndex`, `s3vectors:CreateIndex`, `lambda:InvokeFunction`; EmbedS3Vectors gets `s3:GetObject`, `s3vectors:PutVectors`. Environment variables: `VECTOR_BUCKET_NAME` on all Lambdas, `VOYAGE_API_KEY` on EmbedS3Vectors.
  Acceptance: `sam validate` passes. Template defines all Etapa 1 resources with correct permissions and environment variables.
  Verify: Run `sam validate` and confirm no errors. Review the template and confirm all resources, permissions, and env vars match the spec.

- [ ] **2. CheckS3Vectors — Python Durable Function**
  Spec ref: `spec.md > CheckS3Vectors (Python, Durable Function)`
  What to build: Create `lambdas/check-s3-vectors/check_s3_vectors.py`. Handler receives EventBridge event, extracts bucket name and object key, derives index name from S3 prefix (first path segment). Calls `get_index()` — if index exists, proceeds; if not, calls `create_index()` with Float32, 1024 dimensions, cosine distance, metadata config (filter filterable, source_text non-filterable). Retry logic: up to 3 attempts, each retry checks `get_index()` first. After successful index verification/creation, invokes EmbedS3Vectors Lambda asynchronously passing bucket name, object key, index name. Captures and reports errors from invocation.
  Acceptance: Handler parses EventBridge event correctly. Index check/create logic with retry is implemented. Async Lambda invocation passes correct payload. Error handling covers race conditions and failed invocations.
  Verify: Read the code and trace the flow: event → derive index name → get_index → create_index (if needed, with retry) → invoke Embed. Confirm all three retry paths are handled.

- [ ] **3. EmbedS3Vectors — Rust Lambda**
  Spec ref: `spec.md > EmbedS3Vectors (Rust)`
  What to build: Create `lambdas/embed-s3-vectors/` with `Cargo.toml` and `src/main.rs`. Cargo.toml includes: `aws-sdk-s3vectors`, `aws-sdk-s3`, `mongodb-voyageai`, `lambda_runtime`, `aws_lambda_events`, `aws-config`, `aws-smithy-types`, `tokio`, `anyhow`, `serde`, `serde_json`. Handler receives JSON payload (bucket_name, object_key, index_name). Pipeline: (1) `get_object()` from S3, read as UTF-8, read `filter` metadata (default `"none"`), validate non-empty; (2) normalize with `NormalizerConfig::prose()`, chunk with `chunk_recursive` (size 500, overlap 80); (3) embed all chunks via VoyageAI (`voyage-4-large`, 1024d, input_type `"document"`); (4) build `PutInputVector` for each chunk (key: `{object_key}#{chunk_index}`, metadata: filter + source_text), batch into calls of max 500 vectors.
  Acceptance: Compiles with `cargo lambda build --release`. All four pipeline stages implemented per spec. Handles >500 chunks with batched put_vectors calls. Empty file returns error.
  Verify: Run `cargo lambda build --release` in the embed-s3-vectors directory and confirm successful compilation. Read main.rs and trace the full pipeline.

- [ ] **4. Deploy and test Etapa 1 end-to-end**
  Spec ref: `spec.md > Runtime & Deployment`, `prd.md > Document Ingestion`
  What to build: Run `sam build && sam deploy` for Etapa 1. Upload a test `.txt` file (e.g., `movies/back_to_the_future.txt` with movie description content and `filter=scifi` metadata) to the S3 bucket. Verify the full pipeline executes: EventBridge triggers CheckS3Vectors, index is created, EmbedS3Vectors processes the file, vectors appear in S3 Vectors.
  Acceptance: All EventBridge → CheckS3Vectors → EmbedS3Vectors acceptance criteria from prd.md pass. Vectors exist in S3 Vectors index `movies` with correct metadata.
  Verify: Check CloudWatch logs for both Lambdas — confirm no errors. Use AWS CLI or SDK to list vectors in the `movies` index and confirm they exist with `filter` and `source_text` metadata.

### Etapa 2 — Semantic Search

- [ ] **5. SAM template — extend for Etapa 2**
  Spec ref: `spec.md > Infrastructure (SAM Template) > Resources — Etapa 2`
  What to build: Add to `template.yaml`: SearchS3Vectors Lambda (Rust, `provided.al2023`, API Gateway trigger), HTTP API Gateway with single route `POST /search`. IAM permissions: `s3vectors:GetIndex`, `s3vectors:QueryVectors`. Environment variables: `VECTOR_BUCKET_NAME`, `VOYAGE_API_KEY`, `FRIENDLI_TOKEN`.
  Acceptance: `sam validate` passes. SearchS3Vectors and API Gateway resources defined with correct permissions, env vars, and route.
  Verify: Run `sam validate`. Review the new resources in the template.

- [ ] **6. SearchS3Vectors — Rust Lambda**
  Spec ref: `spec.md > SearchS3Vectors (Rust)`
  What to build: Create `lambdas/search-s3-vectors/` with `Cargo.toml` and `src/main.rs`. Cargo.toml includes: `aws-sdk-s3vectors`, `mongodb-voyageai`, `reqwest` (0.13.2, json + native-tls), `lambda_runtime`, `aws_lambda_events`, `aws-config`, `aws-smithy-types`, `tokio`, `anyhow`, `serde`, `serde_json`. Handler receives API Gateway event with JSON body (index_name, query, optional filter). Pipeline: (1) validate index exists via `get_index()` — if not found, return error; (2) embed query via VoyageAI (voyage-4-large, 1024d, input_type `"query"`); (3) `query_vectors()` with top_k=5, return_metadata=true, optional filter; (4) if no results, return "No relevant information found" without LLM call; (5) build RAG prompt from source_text metadata; (6) POST to Friendli API (GLM-5) with retry (3 attempts, incremental backoff). Return LLM response.
  Acceptance: Compiles with `cargo lambda build --release`. All six pipeline stages implemented. Error responses match spec format. Retry logic on LLM call.
  Verify: Run `cargo lambda build --release` in the search-s3-vectors directory and confirm successful compilation. Read main.rs and trace the full pipeline.

- [ ] **7. Deploy and test Etapa 2 end-to-end**
  Spec ref: `spec.md > Runtime & Deployment`, `prd.md > Semantic Search`
  What to build: Run `sam build && sam deploy` for the full stack. Test SearchS3Vectors against the vectors ingested in step 4. Send POST to API Gateway `/search` with `{"index_name": "movies", "query": "adventures in space", "filter": "scifi"}`. Also test: missing index (error response), no results scenario, query without filter.
  Acceptance: All Semantic Search acceptance criteria from prd.md pass. LLM returns a coherent response based on the ingested document. Error cases return correct response format.
  Verify: Use `curl` to POST to the API Gateway URL. Confirm: (1) valid query returns LLM response, (2) invalid index returns error JSON, (3) filter works correctly.

### Etapa 3 — Frontend (Stretch)

- [ ] **8. Search frontend web page**
  Spec ref: `prd.md > Search Frontend`
  What to build: Create a static HTML page with professional, clean design. Form with three fields: index name (required), query (required), filter (optional). On submit, POST to API Gateway `/search` endpoint. Display LLM response on screen. Handle and display error messages (index not found, no results). Include a loading indicator for the potentially long response time.
  Acceptance: Form submits to API Gateway and displays the response. Error messages display appropriately. Design is clean and professional.
  Verify: Open the page in a browser. Submit a search query and confirm the response appears. Test with an invalid index name and confirm the error displays.

- [ ] **9. Deploy and test frontend**
  Spec ref: `prd.md > Search Frontend`, `prd.md > Infrastructure & Deployment`
  What to build: Add the frontend to the SAM template or deploy separately (S3 static hosting, or simply serve locally for demo). Test the full flow: upload a file → search from the frontend → see LLM response.
  Acceptance: Frontend is accessible and the full demo flow works end-to-end.
  Verify: Open the frontend, run a search against ingested documents, confirm the LLM response displays correctly.

### Cierre

- [ ] **10. README with deployment instructions**
  Spec ref: `spec.md > File Structure`, `prd.md > Infrastructure & Deployment`
  What to build: Write `README.md` with complete step-by-step deployment instructions using SAM. Cover: prerequisites (Cargo Lambda, SAM CLI, AWS credentials, API keys), environment variable setup, `sam build && sam deploy`, how to upload a test file, how to run a search query. No project tree per prd.md requirement.
  Acceptance: A developer can follow the README from zero and deploy the full stack. All required environment variables and steps are documented.
  Verify: Read the README end-to-end. Could someone unfamiliar with the project deploy it by following the instructions?

- [ ] **11. Submit project to Devpost**
  Spec ref: `prd.md > What We're Building` (the core submission story)
  What to build: Walk through the Devpost submission form. Write a project name and tagline. Draft the project story using scope.md and prd.md as source material — explain what you built, why, and what you learned. Add "built with" tags for your tech stack. Take screenshots of the working app for the image gallery. Upload your docs/ folder artifacts (scope, PRD, spec, checklist). Link your code repository. Optionally, link a deployed app or a demo video (YouTube/Vimeo). Review everything and submit.
  Acceptance: Submission is live on Devpost with project name, tagline, description, built-with tags, screenshots, docs artifacts, and repo link. All required fields are complete.
  Verify: Open your Devpost submission page and confirm the green "Submitted" badge appears. Read the project description — would someone who knows nothing about your project understand what it does and why it matters?
