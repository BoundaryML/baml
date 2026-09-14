import base64
import os
from pathlib import Path

import baml_py
import pytest

from baml_client import b


requires_openai_api_key = pytest.mark.skipif(
    not os.environ.get("OPENAI_API_KEY"), reason="OPENAI_API_KEY not set"
)


def transcription_audio() -> baml_py.Audio:
    fixture = (
        Path(__file__).resolve().parents[3]
        / "baml_src"
        / "fiddle-examples"
        / "audio"
        / "friday-rocks.wav"
    )
    encoded = base64.b64encode(fixture.read_bytes()).decode("ascii")
    return baml_py.Audio.from_base64("audio/wav", encoded)


def assert_friday_rocks(transcript: str) -> None:
    normalized = transcript.lower()
    assert "friday" in normalized
    assert "rock" in normalized


@pytest.mark.asyncio
async def test_build_request_openai_transcription(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("OPENAI_API_KEY", "test-openai-api-key")
    audio = baml_py.Audio.from_base64("audio/wav", base64.b64encode(b"RIFF").decode())

    request = await b.request.TestOpenAITranscription(audio)
    headers = {key.lower(): value for key, value in request.headers.items()}
    content_type = headers["content-type"]
    boundary = content_type.removeprefix("multipart/form-data; boundary=")
    body = bytes(request.body.raw()).replace(boundary.encode(), b"<BOUNDARY>")

    assert request.method == "POST"
    assert request.url == "https://api.openai.com/v1/audio/transcriptions"
    assert headers == {
        "authorization": "Bearer test-openai-api-key",
        "baml-original-url": "https://api.openai.com/v1",
        "content-type": f"multipart/form-data; boundary={boundary}",
    }
    assert body == (
        b"--<BOUNDARY>\r\n"
        b'Content-Disposition: form-data; name="model"\r\n\r\n'
        b"gpt-transcribe\r\n"
        b"--<BOUNDARY>\r\n"
        b'Content-Disposition: form-data; name="file"; filename="audio.wav"\r\n'
        b"Content-Type: audio/wav\r\n\r\n"
        b"RIFF\r\n"
        b"--<BOUNDARY>--\r\n"
    )


@requires_openai_api_key
@pytest.mark.asyncio
async def test_gpt_transcribe_non_streaming():
    transcript = await b.TestOpenAITranscription(transcription_audio())
    assert_friday_rocks(transcript)


@requires_openai_api_key
@pytest.mark.asyncio
async def test_gpt_transcribe_streaming():
    stream = b.stream.TestOpenAITranscription(transcription_audio())
    partials = [partial async for partial in stream if partial]
    transcript = await stream.get_final_response()

    assert partials
    assert partials[-1] == transcript
    assert_friday_rocks(transcript)


@requires_openai_api_key
@pytest.mark.asyncio
async def test_gpt_transcribe_non_streaming_multipart_chat_message():
    transcript = await b.TestOpenAITranscriptionMultipartChat(transcription_audio())
    assert_friday_rocks(transcript)
