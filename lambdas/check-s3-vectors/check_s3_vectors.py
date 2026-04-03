"""CheckS3Vectors Lambda function.

Handles S3 Object Created events from EventBridge. Ensures an S3 Vectors
index exists for the object's prefix, then asynchronously invokes the
EmbedS3Vectors Lambda to generate and store embeddings.
"""

import os
import json
import logging
import time

import boto3
from botocore.exceptions import ClientError

logger = logging.getLogger()
logger.setLevel(logging.INFO)

s3vectors = boto3.client("s3vectors")
lambda_client = boto3.client("lambda")

VECTOR_BUCKET_NAME = os.environ.get("VECTOR_BUCKET_NAME", "")
EMBED_FUNCTION_NAME = os.environ.get("EMBED_FUNCTION_NAME", "")

MAX_RETRIES = 3


def _get_index_name(object_key: str) -> str:
    """Derive the index name from the first path segment of the S3 key."""
    return object_key.split("/")[0]


def _index_exists(index_name: str) -> bool:
    """Check whether the S3 Vectors index already exists."""
    try:
        s3vectors.get_index(
            vectorBucketName=VECTOR_BUCKET_NAME,
            indexName=index_name,
        )
        logger.info("Index '%s' already exists.", index_name)
        return True
    except ClientError as exc:
        error_code = exc.response["Error"]["Code"]
        if error_code in ("NotFoundException", "404"):
            logger.info("Index '%s' does not exist.", index_name)
            return False
        raise


def _ensure_index(index_name: str) -> None:
    """Create the index if it does not exist, with retry logic."""
    if _index_exists(index_name):
        return

    for attempt in range(1, MAX_RETRIES + 1):
        try:
            logger.info(
                "Creating index '%s' (attempt %d/%d).",
                index_name,
                attempt,
                MAX_RETRIES,
            )
            s3vectors.create_index(
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
            logger.info("Index '%s' created successfully.", index_name)
            return
        except ClientError as exc:
            logger.warning(
                "create_index failed on attempt %d: %s", attempt, exc
            )
            # On retry, check if another invocation already created the index.
            if _index_exists(index_name):
                return
            if attempt < MAX_RETRIES:
                time.sleep(1)

    raise RuntimeError(
        f"Failed to create index '{index_name}' after {MAX_RETRIES} attempts."
    )


def handler(event, context):
    """Lambda entry point for S3 Object Created EventBridge events."""
    logger.info("Received event: %s", json.dumps(event))

    detail = event["detail"]
    bucket_name = detail["bucket"]["name"]
    object_key = detail["object"]["key"]
    index_name = _get_index_name(object_key)

    logger.info(
        "Processing object '%s' from bucket '%s' with index '%s'.",
        object_key,
        bucket_name,
        index_name,
    )

    _ensure_index(index_name)

    payload = {
        "bucket_name": bucket_name,
        "object_key": object_key,
        "index_name": index_name,
    }

    try:
        logger.info(
            "Invoking '%s' asynchronously with payload: %s",
            EMBED_FUNCTION_NAME,
            json.dumps(payload),
        )
        response = lambda_client.invoke(
            FunctionName=EMBED_FUNCTION_NAME,
            InvocationType="Event",
            Payload=json.dumps(payload),
        )
        logger.info(
            "Invoke response status: %d", response["StatusCode"]
        )
    except ClientError as exc:
        logger.error("Failed to invoke '%s': %s", EMBED_FUNCTION_NAME, exc)
        raise

    return {
        "statusCode": 200,
        "body": json.dumps({
            "message": "EmbedS3Vectors invoked successfully.",
            "index_name": index_name,
            "object_key": object_key,
        }),
    }
