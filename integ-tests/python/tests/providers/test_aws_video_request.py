import os
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
    request = await b.request.TestAwsVideoDescribe(
        video_input=baml_py.Video.from_url(s3_uri, media_type="video/mp4"),
    )

    body = request.body.json()
    video_block = _extract_video_block(body)

    assert video_block.get("format") == "mp4"
    source = video_block.get("source", {})
    assert "s3Location" in source, "Expected Bedrock video source to use s3Location"
    assert source["s3Location"].get("uri") == s3_uri
    assert "bytes" not in source, "Video request should not fall back to base64"


@pytest.mark.asyncio
async def test_bedrock_video_request_with_real_s3_upload(monkeypatch):
    resp = await b.TestAwsVideoDescribe(
        video_input=baml_py.Video.from_url(
            "s3://baml-integ-tests/sample-5s.mp4", media_type="video/mp4"
        ),
    )
    assert "park" in resp
