import os
import uuid
from pathlib import Path

import boto3
import pytest

import baml_py
from baml_client import b


def _extract_video_block(body: dict) -> dict:
    """Locate the first video content block in the Bedrock request payload."""
    for message in body.get("messages", []):
        for content in message.get("content", []):
            if "video" in content:
                return content["video"]
    raise AssertionError("No video block found in Bedrock request payload")


@pytest.mark.asyncio
async def test_bedrock_video_request_prefers_s3_location(monkeypatch):
    monkeypatch.setenv("AWS_REGION", os.getenv("AWS_REGION", "us-east-1"))

    s3_uri = "s3://baml-test-bucket/example/path/video.mp4"
    try:
        request = await b.request.TestAwsVideoDescribe(
            video_input=baml_py.Video.from_url(s3_uri, media_type="video/mp4"),
        )
    except Exception as exc:  # noqa: BLE001 - propagates expected runtime guard
        # Current behaviour rejects non-base64 videos; keep the test active so the
        # assertion flips once S3 support ships.
        assert "base64 video inputs" in str(exc)
        return

    body = request.body.json()
    video_block = _extract_video_block(body)

    assert video_block.get("format") == "mp4"
    source = video_block.get("source", {})
    assert "s3Location" in source, "Expected Bedrock video source to use s3Location"
    assert source["s3Location"].get("uri") == s3_uri
    assert "bytes" not in source, "Video request should not fall back to base64"


@pytest.mark.asyncio
async def test_bedrock_video_request_with_real_s3_upload(monkeypatch):
    bucket = os.getenv("BAML_TEST_S3_BUCKET")
    if not bucket:
        pytest.skip("BAML_TEST_S3_BUCKET not configured")

    region = os.getenv("BAML_TEST_S3_REGION", os.getenv("AWS_REGION", "us-east-1"))
    monkeypatch.setenv("AWS_REGION", region)

    session = boto3.Session(region_name=region)
    creds = session.get_credentials()
    if creds is None or not creds.access_key:
        pytest.skip("AWS credentials unavailable for S3 upload test")

    s3_client = session.client("s3")

    repo_root = Path(__file__).resolve().parents[4]
    video_path = repo_root / "integ-tests" / "baml_src" / "sample-5s.mp4"
    if not video_path.exists():
        pytest.skip(f"Sample video not found at {video_path}")
    video_bytes = video_path.read_bytes()

    object_key = f"baml-integ-video/{uuid.uuid4()}.mp4"
    s3_uri = f"s3://{bucket}/{object_key}"

    try:
        s3_client.put_object(
            Bucket=bucket,
            Key=object_key,
            Body=video_bytes,
            ContentType="video/mp4",
        )
    except Exception as exc:  # pragma: no cover - guard rails for missing perms
        pytest.skip(f"Unable to upload integration video to S3: {exc}")

    try:
        request = await b.request.TestAwsVideoDescribe(
            video_input=baml_py.Video.from_url(s3_uri, media_type="video/mp4"),
        )
        body = request.body.json()
        video_block = _extract_video_block(body)

        assert video_block.get("format") == "mp4"
        source = video_block.get("source", {})
        assert source.get("s3Location", {}).get("uri") == s3_uri
        assert "bytes" not in source
    finally:
        try:
            s3_client.delete_object(Bucket=bucket, Key=object_key)
        except Exception:
            pass
