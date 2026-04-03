#!/usr/bin/env bash
# Test script for Etapa 1 — Ingestion pipeline
# Uploads a test file and verifies the full pipeline:
# S3 upload → EventBridge → CheckS3Vectors → EmbedS3Vectors → S3 Vectors
set -euo pipefail

STACK_NAME="s3-vectors-search"
REGION="us-east-1"
TEST_FILE="events/test-data/back_to_the_future.txt"
S3_KEY="movies/back_to_the_future.txt"
INDEX_NAME="movies"
FILTER="scifi"
WAIT_SECONDS=30

# --- Resolve stack outputs ---
echo "==> Resolving stack outputs..."
UPLOAD_BUCKET=$(aws cloudformation describe-stacks \
  --stack-name "$STACK_NAME" --region "$REGION" \
  --query "Stacks[0].Outputs[?OutputKey=='UploadBucketName'].OutputValue" \
  --output text)

VECTORS_BUCKET=$(aws cloudformation describe-stacks \
  --stack-name "$STACK_NAME" --region "$REGION" \
  --query "Stacks[0].Outputs[?OutputKey=='VectorsBucketName'].OutputValue" \
  --output text)

CHECK_FN_ARN=$(aws cloudformation describe-stacks \
  --stack-name "$STACK_NAME" --region "$REGION" \
  --query "Stacks[0].Outputs[?OutputKey=='CheckS3VectorsFunctionArn'].OutputValue" \
  --output text)
CHECK_FN_NAME=$(echo "$CHECK_FN_ARN" | awk -F: '{print $NF}')

EMBED_FN_ARN=$(aws cloudformation describe-stacks \
  --stack-name "$STACK_NAME" --region "$REGION" \
  --query "Stacks[0].Outputs[?OutputKey=='EmbedS3VectorsFunctionArn'].OutputValue" \
  --output text)
EMBED_FN_NAME=$(echo "$EMBED_FN_ARN" | awk -F: '{print $NF}')

echo "  Upload bucket:  $UPLOAD_BUCKET"
echo "  Vectors bucket: $VECTORS_BUCKET"
echo "  CheckS3Vectors: $CHECK_FN_NAME"
echo "  EmbedS3Vectors: $EMBED_FN_NAME"

# --- Upload test file ---
echo ""
echo "==> Uploading $TEST_FILE to s3://$UPLOAD_BUCKET/$S3_KEY (filter=$FILTER)..."
aws s3 cp "$TEST_FILE" "s3://$UPLOAD_BUCKET/$S3_KEY" \
  --metadata "filter=$FILTER" --region "$REGION"

# --- Wait for pipeline ---
echo ""
echo "==> Waiting ${WAIT_SECONDS}s for pipeline to complete..."
sleep "$WAIT_SECONDS"

# --- Check CheckS3Vectors logs ---
echo ""
echo "==> CheckS3Vectors logs (last 3 min):"
aws logs tail "/aws/lambda/$CHECK_FN_NAME" \
  --since 3m --format short --region "$REGION" 2>&1 | \
  grep -v '"type":"platform\.' | tail -20 || echo "  (no log lines found)"

# --- Check EmbedS3Vectors logs ---
echo ""
echo "==> EmbedS3Vectors logs (last 3 min):"
aws logs tail "/aws/lambda/$EMBED_FN_NAME" \
  --since 3m --format short --region "$REGION" 2>&1 | \
  grep -v '"type":"platform\.' | tail -20 || echo "  (no log lines found)"

# --- Verify vectors in S3 Vectors ---
echo ""
echo "==> Listing vectors in index '$INDEX_NAME'..."
aws s3vectors list-vectors \
  --vector-bucket-name "$VECTORS_BUCKET" \
  --index-name "$INDEX_NAME" \
  --region "$REGION" 2>&1 | head -30

echo ""
echo "==> Test complete."
