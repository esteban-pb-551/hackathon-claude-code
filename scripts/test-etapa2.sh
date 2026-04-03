#!/usr/bin/env bash
# Test script for Etapa 2 — Semantic Search
# Sends POST requests to the SearchS3Vectors API and verifies responses:
# valid query, filtered query, invalid index, and missing body.
set -euo pipefail

STACK_NAME="s3-vectors-search"
REGION="us-east-1"

# --- Resolve stack outputs ---
echo "==> Resolving stack outputs..."
SEARCH_URL=$(aws cloudformation describe-stacks \
  --stack-name "$STACK_NAME" --region "$REGION" \
  --query "Stacks[0].Outputs[?OutputKey=='SearchApiUrl'].OutputValue" \
  --output text)

echo "  Search API URL: $SEARCH_URL"

# --- Test 1: Valid query with filter ---
echo ""
echo "==> Test 1: Valid query with filter (index=movies, filter=scifi)"
echo '{"index_name": "movies", "query": "adventures in space", "filter": "scifi"}' | \
  curl -s -w "\n  HTTP status: %{http_code}\n" -X POST "$SEARCH_URL" \
    -H "Content-Type: application/json" \
    -d @- | tee /dev/null
echo ""

# --- Test 2: Valid query without filter ---
echo "==> Test 2: Valid query without filter (index=movies)"
echo '{"index_name": "movies", "query": "time travel"}' | \
  curl -s -w "\n  HTTP status: %{http_code}\n" -X POST "$SEARCH_URL" \
    -H "Content-Type: application/json" \
    -d @- | tee /dev/null
echo ""

# --- Test 3: Invalid index (should return 404 error) ---
echo "==> Test 3: Invalid index (expect error)"
echo '{"index_name": "nonexistent", "query": "test"}' | \
  curl -s -w "\n  HTTP status: %{http_code}\n" -X POST "$SEARCH_URL" \
    -H "Content-Type: application/json" \
    -d @- | tee /dev/null
echo ""

# --- Test 4: Missing body (should return 400 error) ---
echo "==> Test 4: Missing body (expect error)"
curl -s -w "\n  HTTP status: %{http_code}\n" -X POST "$SEARCH_URL" \
  -H "Content-Type: application/json" | tee /dev/null
echo ""

echo "==> Test complete."
