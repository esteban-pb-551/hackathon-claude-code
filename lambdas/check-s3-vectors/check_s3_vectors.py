"""CheckS3Vectors Lambda — Durable Function.

Handles S3 Object Created events from EventBridge. Uses Lambda durable execution
to checkpoint each step: index verification/creation, then EmbedS3Vectors invocation.
If the function is interrupted, it replays from the last checkpoint without
re-executing completed steps.

Determinism contract:
  - Code outside context.step() / context.invoke() is deterministic (pure event parsing).
  - All external API calls (S3 Vectors, Lambda invoke) happen inside durable operations.
  - No global mutable state; module-level constants are read-only env vars.

Idempotency:
  - ensure-index step: check-before-create pattern. Safe to retry — either the index
    exists (no-op) or gets created. Race conditions handled with retry loop.
  - invoke-embed step: context.invoke() is checkpointed. If the function replays after
    the invoke completes, the cached result is returned without re-invoking.
  - EmbedS3Vectors itself is idempotent: vectors are keyed by {object_key}#{chunk_index},
    so reprocessing overwrites the same keys.
"""

import os
import time

import boto3
from botocore.exceptions import ClientError
from aws_durable_execution_sdk_python import (
    DurableContext,
    StepContext,
    durable_execution,
    durable_step,
)

# Read-only constants from environment — deterministic across replays.
VECTOR_BUCKET_NAME = os.environ.get("VECTOR_BUCKET_NAME", "")
EMBED_FUNCTION_ARN = os.environ.get("EMBED_FUNCTION_ARN", "")

MAX_RETRIES = 3

# Module-level boto3 clients — immutable, safe across replays.
s3vectors_client = boto3.client("s3vectors")


@durable_step
def ensure_index(step_ctx: StepContext, index_name: str) -> dict:
    """Check whether the S3 Vectors index exists; create it if not.

    Uses a check-before-create pattern with retry logic to handle race
    conditions when multiple files are uploaded to the same prefix
    concurrently.
    """
    # Check if index already exists
    try:
        s3vectors_client.get_index(
            vectorBucketName=VECTOR_BUCKET_NAME,
            indexName=index_name,
        )
        step_ctx.logger.info("Index already exists", extra={"index_name": index_name})
        return {"created": False}
    except ClientError as exc:
        error_code = exc.response["Error"]["Code"]
        if error_code not in ("NotFoundException", "404"):
            raise
        step_ctx.logger.info("Index does not exist, will create", extra={"index_name": index_name})

    # Create index with retry logic for concurrent creation race conditions
    for attempt in range(1, MAX_RETRIES + 1):
        try:
            step_ctx.logger.info(
                "Creating index",
                extra={"index_name": index_name, "attempt": attempt, "max_retries": MAX_RETRIES},
            )
            s3vectors_client.create_index(
                vectorBucketName=VECTOR_BUCKET_NAME,
                indexName=index_name,
                dataType="float32",
                dimension=1024,
                distanceMetric="cosine",
                metadataConfiguration={
                    "filterableMetadataKeys": ["filter"],
                    "nonFilterableMetadataKeys": ["source_text"],
                },
            )
            step_ctx.logger.info("Index created successfully", extra={"index_name": index_name})
            return {"created": True}
        except ClientError as exc:
            step_ctx.logger.warning(
                "create_index failed",
                extra={"attempt": attempt, "error": str(exc)},
            )
            # Another invocation may have created the index concurrently
            try:
                s3vectors_client.get_index(
                    vectorBucketName=VECTOR_BUCKET_NAME,
                    indexName=index_name,
                )
                step_ctx.logger.info("Index now exists (concurrent creation)", extra={"index_name": index_name})
                return {"created": False}
            except ClientError:
                pass
            if attempt < MAX_RETRIES:
                time.sleep(1)

    raise RuntimeError(
        f"Failed to create index '{index_name}' after {MAX_RETRIES} attempts."
    )


@durable_execution
def handler(event: dict, context: DurableContext) -> dict:
    """Lambda entry point — durable function for S3 Object Created events.

    Execution flow (two checkpointed operations):
      1. ensure-index: verify/create the S3 Vectors index for this prefix.
      2. invoke-embed: invoke EmbedS3Vectors to chunk, embed, and store vectors.

    On replay, completed steps return cached results instantly.
    """
    # --- Deterministic event parsing (safe outside steps) ---
    detail = event["detail"]
    bucket_name = detail["bucket"]["name"]
    object_key = detail["object"]["key"]
    index_name = object_key.split("/")[0]

    context.logger.info(
        "Processing upload",
        extra={"object_key": object_key, "bucket": bucket_name, "index_name": index_name},
    )

    # Step 1: Ensure the S3 Vectors index exists (checkpointed).
    context.step(ensure_index(index_name), name="ensure-index")

    # Step 2: Invoke EmbedS3Vectors to process the file (checkpointed).
    result = context.invoke(
        EMBED_FUNCTION_ARN,
        {
            "bucket_name": bucket_name,
            "object_key": object_key,
            "index_name": index_name,
        },
        name="invoke-embed",
    )

    return {
        "status": "success",
        "index_name": index_name,
        "object_key": object_key,
        "embed_result": result,
    }
