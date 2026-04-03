# Build Checklist

## Build Preferences

- **Build mode:** Autonomous
- **Comprehension checks:** N/A (autonomous mode)
- **Git:** Commit after each item with message: "Complete step N: [title]"
- **Verification:** Yes. Checkpoints every 3-4 items — agent pauses, gives summary, learner confirms before continuing.
- **Check-in cadence:** N/A (autonomous mode)

## Checklist

### Etapa 1 — Ingestion Backend

- [x] **1. SAM template — Etapa 1 infrastructure**
  Spec ref: `spec.md > Infrastructure (SAM Template) > Resources — Etapa 1`
  What was built: `template.yaml` with: Secrets Manager secrets (VoyageAI, Friendli), `AWS::S3Vectors::VectorBucket` (created by stack, tagged `voyageModel=voyage-4-large`), S3 upload bucket, EventBridge rule (suffix `.txt`), CheckS3Vectors (Python 3.13, arm64, Durable Function), EmbedS3Vectors (Rust, arm64, BuildMethod: makefile). IAM policies scoped to specific ARNs. Environment variables: `VECTOR_BUCKET_NAME` (extracted from ARN), `VOYAGE_SECRET_ARN` (Secrets Manager). Stack-level tags via `samconfig.toml`.
  Acceptance: `sam validate --lint` passes. Template defines all Etapa 1 resources with scoped permissions and secret-based env vars.
  Verify: Run `sam validate --lint` and confirm no errors.

- [x] **2. CheckS3Vectors — Python Durable Function**
  Spec ref: `spec.md > CheckS3Vectors (Python, Durable Function)`
  What was built: `lambdas/check-s3-vectors/check_s3_vectors.py` using `@durable_step` / `@durable_execution` SDK. `create_index()` uses `nonFilterableMetadataKeys` only (`filterableMetadataKeys` not valid in boto3). Dependencies managed with `uv` (`pyproject.toml` + `uv.lock`), `requirements.txt` generated via `uv export`.
  Acceptance: Handler parses EventBridge event correctly. Index check/create logic with retry is implemented. Async Lambda invocation passes correct payload.
  Verify: Read the code and trace the flow: event → derive index name → get_index → create_index (if needed, with retry) → invoke Embed.

- [x] **3. EmbedS3Vectors — Rust Lambda**
  Spec ref: `spec.md > EmbedS3Vectors (Rust)`
  What was built: `lambdas/embed-s3-vectors/` with Cargo.toml, Makefile, and `src/main.rs`. Added `aws-sdk-secretsmanager` for fetching VoyageAI API key from Secrets Manager at cold start. Updated `lambda_runtime` to 1.1.2 and `aws_lambda_events` to 1.1.2. `load_secret()` helper fetches secret and sets env var before handler loop so `VoyageClient::from_env()` picks it up.
  Acceptance: Compiles with `cargo lambda build --release --arm64 --compiler cargo`. All four pipeline stages implemented. Secrets fetched from Secrets Manager.
  Verify: Run `cargo lambda build --release` and confirm successful compilation.

- [x] **4. Deploy and test Etapa 1 end-to-end**
  Spec ref: `spec.md > Runtime & Deployment`, `prd.md > Document Ingestion`
  What was built: `sam validate --lint && sam build && sam deploy`. Test script `scripts/test-etapa1.sh` automates: upload test file, wait, check logs, verify vectors. VectorsBucket created by stack, secrets in Secrets Manager, IAM scoped to ARNs.
  Acceptance: All EventBridge → CheckS3Vectors → EmbedS3Vectors acceptance criteria pass. Vectors exist in S3 Vectors index `movies`.
  Verify: Run `./scripts/test-etapa1.sh` and confirm 3 vectors in `movies` index.

### Etapa 2 — Semantic Search

- [x] **5. SAM template — extend for Etapa 2**
  Spec ref: `spec.md > Infrastructure (SAM Template) > Resources — Etapa 2`
  What was built: Added to `template.yaml`: SearchS3VectorsFunction (Rust, `provided.al2023`, arm64, BuildMethod: makefile, 300s timeout, 512MB), `AWS::Serverless::HttpApi` with `POST /search` route. IAM: `secretsmanager:GetSecretValue` (both secrets), `s3vectors:GetIndex`, `s3vectors:QueryVectors`, `s3vectors:GetVectors` (VectorsBucket ARN + `/*`). Env vars: `VECTOR_BUCKET_NAME`, `VOYAGE_SECRET_ARN`, `FRIENDLI_SECRET_ARN`. Outputs: `SearchApiUrl`, `SearchS3VectorsFunctionArn`. Note: `s3vectors:GetVectors` was not in original spec but required by `query_vectors()` at runtime.
  Acceptance: `sam validate --lint` passes. SearchS3Vectors and API Gateway resources defined with scoped permissions, secret ARNs, and route.
  Verify: Run `sam validate --lint`. Review the new resources in the template.

- [x] **6. SearchS3Vectors — Rust Lambda**
  Spec ref: `spec.md > SearchS3Vectors (Rust)`
  What was built: `lambdas/search-s3-vectors/` with `Cargo.toml`, `Makefile`, `src/main.rs`, and `src/http_handler.rs`. Uses `lambda_http` crate (1.0.0) instead of `lambda_runtime`/`aws_lambda_events` — cleaner HTTP request/response handling via `Request`/`Response<Body>` with `Response::builder()`. Cargo.toml includes: `aws-sdk-s3vectors`, `aws-sdk-secretsmanager`, `mongodb-voyageai`, `reqwest` (0.13.2, json + native-tls), `lambda_http`, `aws-config`, `aws-smithy-types`, `tokio`, `anyhow`, `serde`, `serde_json`. Cold start: fetches VoyageAI + Friendli secrets from Secrets Manager (`load_secret()` pattern). Handler: full 6-stage RAG pipeline with detailed `tracing` at each step. Response codes: 200 (success/no results), 400 (bad request), 404 (index not found), 500 (LLM error).
  Acceptance: Compiles with `cargo lambda build --release --arm64 --compiler cargo`. All six pipeline stages implemented. Secrets fetched from Secrets Manager. Error responses match spec format.
  Verify: Run `cargo lambda build --release --arm64 --compiler cargo` and confirm successful compilation. Read main.rs and trace the full pipeline.

- [x] **7. Deploy and test Etapa 2 end-to-end**
  Spec ref: `spec.md > Runtime & Deployment`, `prd.md > Semantic Search`
  What was built: Full stack deployed via `sam validate --lint && sam build && sam deploy`. API Gateway URL: `https://rlwozruimc.execute-api.us-east-1.amazonaws.com/search`. Test script `scripts/test-etapa2.sh` runs 4 scenarios: (1) valid query with filter → GLM-5 returns coherent Back to the Future response, (2) valid query without filter → detailed time travel response, (3) invalid index → `{"error": "Index 'nonexistent' not found"}` with 404, (4) missing body → `{"error": "Missing request body"}` with 400. Issues resolved: missing `s3vectors:GetVectors` IAM permission (required by `query_vectors()` internally), initial API Gateway creation failed due to IAM tagging permissions (learner fixed externally).
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
