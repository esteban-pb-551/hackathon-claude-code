# Security Checklist

Security review of the S3 Vectors Semantic Search deployment. This document covers the current security posture, known risks, potential threats, and recommended improvements.

---

## Current Security Posture

### What is already in place

| Control | Status | Details |
|---------|--------|---------|
| Secrets in Secrets Manager | Implemented | VoyageAI and Friendli API keys stored in AWS Secrets Manager, fetched at runtime via ARN |
| IAM least privilege | Implemented | Each Lambda has scoped IAM policies to specific resource ARNs (not `*`) |
| DynamoDB TTL | Implemented | Search results auto-expire after 1 hour, preventing unbounded data growth |
| EventBridge filtering | Implemented | Rule only triggers for `.txt` suffix in the specific upload bucket |
| arm64 / Graviton | Implemented | Reduced attack surface compared to x86 (smaller instruction set, fewer side-channel vectors) |
| Input validation | Partial | UploadS3 validates `.txt` extension and non-empty content; SearchS3Vectors validates required fields |
| CORS configured | Partial | CORS headers present but configured as `AllowOrigins: "*"` (see threats below) |

---

## Threat Model

### T1 — Unauthenticated API Access

**Severity:** Critical
**Status:** Open

The API Gateway (`SearchHttpApi`) has **no authentication or authorization**. All three endpoints (`POST /search`, `GET /search/{id}`, `POST /upload`) are publicly accessible to anyone with the URL.

**Attack scenarios:**
- An attacker discovers the API URL and floods the search endpoint, consuming VoyageAI/Friendli API credits
- An attacker uploads arbitrary `.txt` files to the S3 bucket, triggering the ingestion pipeline and consuming compute/storage resources
- An attacker polls `GET /search/{id}` with brute-forced UUIDs to read other users' search results

**Recommended mitigations:**
- Add API Gateway usage plans with API keys and throttling
- Implement IAM or Cognito authorization on the API Gateway
- Add a Lambda authorizer for token-based authentication
- At minimum, add rate limiting per IP via AWS WAF

---

### T2 — Unrestricted CORS Policy

**Severity:** High
**Status:** Open

```yaml
CorsConfiguration:
  AllowOrigins:
    - "*"
```

The wildcard CORS policy allows **any website** to make requests to the API. A malicious site could make cross-origin requests to the API on behalf of a user's browser.

**Recommended mitigation:**
- Replace `"*"` with the specific frontend origin(s):
  ```yaml
  AllowOrigins:
    - "https://your-domain.com"
    - "http://<ec2-public-ip>"
  ```

---

### T3 — Denial of Wallet (API Credit Exhaustion)

**Severity:** High
**Status:** Open

Each search request triggers:
1. One VoyageAI API call (embedding generation)
2. One Friendli API call (GLM-5 inference)

With no rate limiting or authentication, an attacker can rapidly exhaust API credits by sending thousands of search requests.

**Recommended mitigations:**
- API Gateway throttling (`ThrottlingBurstLimit`, `ThrottlingRateLimit`)
- AWS WAF with rate-based rules (e.g., max 100 requests/5 min per IP)
- Set billing alerts on VoyageAI and Friendli accounts
- Add a Lambda-level concurrency limit (`ReservedConcurrentExecutions`) to cap parallel executions

---

### T4 — Unrestricted File Upload Size

**Severity:** Medium
**Status:** Open

The `UploadS3` Lambda accepts base64-encoded file content in the request body. API Gateway HTTP APIs have a **10 MB payload limit**, but there is no application-level size restriction.

**Attack scenario:** An attacker uploads a 10 MB text file that produces thousands of chunks, generating a large number of VoyageAI API calls and S3 Vectors writes.

**Recommended mitigations:**
- Add a content size check in the `UploadS3` Lambda (e.g., reject files over 1 MB)
- Set `MaximumBatchingWindowInSeconds` or payload size limits at the API Gateway level
- Monitor `put_vectors()` call volume per index with CloudWatch alarms

---

### T5 — S3 Bucket Public Access

**Severity:** Medium
**Status:** Needs verification

The `UploadBucket` does not explicitly configure `PublicAccessBlockConfiguration`. While S3 buckets created after April 2023 have public access blocked by default, it is a best practice to explicitly declare it.

**Recommended mitigation:**
```yaml
UploadBucket:
  Type: AWS::S3::Bucket
  Properties:
    PublicAccessBlockConfiguration:
      BlockPublicAcls: true
      BlockPublicPolicy: true
      IgnorePublicAcls: true
      RestrictPublicBuckets: true
    NotificationConfiguration:
      EventBridgeConfiguration:
        EventBridgeEnabled: true
```

---

### T6 — Secrets Passed as CloudFormation Parameters

**Severity:** Medium
**Status:** Open

API keys are passed as `samconfig.toml` parameter overrides:
```toml
parameter_overrides = "VoyageApiKey=\"sk-...\" FriendliToken=\"ft-...\""
```

These values are visible in:
- CloudFormation parameter history (console and API)
- CloudTrail logs for `CreateStack` / `UpdateStack` events
- The `samconfig.toml` file itself (risk of accidental git commit)

**Recommended mitigations:**
- Create secrets manually in Secrets Manager before deployment, and reference them by ARN in the template instead of passing raw values
- Add `samconfig.toml` to `.gitignore`
- Use `NoEcho: true` on parameters (already in place — this prevents console display but not CloudTrail)

---

### T7 — No Encryption at Rest Configuration

**Severity:** Low
**Status:** Acceptable for hackathon

- **DynamoDB:** Uses default AWS-owned encryption (AES-256). Acceptable, but for sensitive data consider CMK (customer-managed key)
- **S3 Bucket:** Uses default SSE-S3 encryption. Acceptable for text files
- **Secrets Manager:** Encrypted by default with AWS-managed key
- **S3 Vectors Bucket:** Encryption managed by the service

**Recommended mitigation (production):**
- Enable SSE-KMS with a customer-managed key on the S3 bucket and DynamoDB table for audit trail via CloudTrail

---

### T8 — No VPC Isolation

**Severity:** Low
**Status:** Acceptable for hackathon

All Lambda functions run outside a VPC, with direct internet access for external API calls (VoyageAI, Friendli). This is simpler but means:
- Lambdas communicate with AWS services over public endpoints
- No network-level isolation between functions
- No ability to restrict outbound traffic

**Recommended mitigation (production):**
- Deploy Lambdas inside a VPC with private subnets
- Use VPC endpoints for S3, DynamoDB, Secrets Manager, and S3 Vectors
- Use a NAT Gateway for outbound calls to VoyageAI and Friendli APIs
- Apply security groups to restrict traffic

---

### T9 — Search Result Data Leakage via UUID Guessing

**Severity:** Low
**Status:** Acceptable

Search results are stored in DynamoDB keyed by UUID v4 (`request_id`). While UUIDs have 122 bits of randomness (making brute-force impractical), the `GET /search/{id}` endpoint returns the full result without any authorization check.

**Recommended mitigation (production):**
- Associate each request with a session or user ID
- Validate ownership before returning results
- Reduce TTL from 1 hour to 10 minutes

---

### T10 — Injection via User Query to LLM

**Severity:** Low
**Status:** Inherent to RAG

The user's `query` is passed directly into the LLM prompt as the user message. While the system prompt constrains the LLM to answer based on context fragments only, prompt injection is an inherent risk in any RAG system.

**Attack scenario:** A user crafts a query that instructs the LLM to ignore the system prompt and reveal the context fragments or behave unexpectedly.

**Recommended mitigations:**
- Sanitize or truncate user queries before passing to the LLM
- Add output filtering to detect and block sensitive information leakage
- Monitor LLM responses for anomalous patterns
- This is a defense-in-depth issue — no single mitigation eliminates the risk entirely

---

### T11 — No Logging or Monitoring Alerts

**Severity:** Medium
**Status:** Open

Lambda functions log to CloudWatch, but there are no:
- CloudWatch Alarms for error rates or throttling
- AWS X-Ray tracing for end-to-end request visibility
- Alerts for unusual API usage patterns (spike in requests, unusual IPs)

**Recommended mitigations:**
- Enable X-Ray tracing on API Gateway and all Lambda functions
- Create CloudWatch Alarms for: Lambda errors > 5/min, API Gateway 4xx/5xx rates, DynamoDB throttled reads/writes
- Enable API Gateway access logging
- Consider AWS GuardDuty for S3 bucket anomaly detection

---

## Security Improvement Priority

| Priority | Threat | Effort | Impact |
|----------|--------|--------|--------|
| 1 | T1 — API Authentication | Medium | Blocks all unauthenticated abuse |
| 2 | T3 — Rate Limiting / WAF | Low | Prevents credit exhaustion and DDoS |
| 3 | T2 — CORS Restriction | Low | Prevents cross-origin abuse |
| 4 | T4 — Upload Size Limit | Low | Prevents resource exhaustion via large files |
| 5 | T6 — Secrets via Parameter Store | Medium | Removes secrets from CloudFormation history |
| 6 | T5 — S3 Public Access Block | Low | Explicit defense-in-depth |
| 7 | T11 — Monitoring and Alerts | Medium | Enables detection and incident response |
| 8 | T8 — VPC Isolation | High | Network-level security (production only) |
| 9 | T7 — CMK Encryption | Medium | Audit trail and key rotation (production only) |
| 10 | T9 — Result Authorization | Medium | Per-user result isolation (production only) |
| 11 | T10 — Prompt Injection | Ongoing | Defense-in-depth, no complete fix |

---

## Quick Wins

Changes that can be applied immediately with minimal effort:

1. **Add `.gitignore` entry for `samconfig.toml`** to prevent accidental credential commits
2. **Restrict CORS origins** to the actual frontend URL(s)
3. **Add `PublicAccessBlockConfiguration`** to the S3 bucket in `template.yaml`
4. **Add file size validation** in `UploadS3` Lambda (reject content > 1 MB after base64 decode)
5. **Add `ReservedConcurrentExecutions: 10`** to SearchWorker to cap parallel LLM calls
