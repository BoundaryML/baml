import base64
from pathlib import Path

import baml_py
import pytest

from baml_client import b


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
async def test_gpt_transcribe_non_streaming():
    transcript = await b.TestOpenAITranscription(transcription_audio())
    assert_friday_rocks(transcript)


@pytest.mark.asyncio
async def test_gpt_transcribe_streaming():
    stream = b.stream.TestOpenAITranscription(transcription_audio())
    partials = [partial async for partial in stream if partial]
    transcript = await stream.get_final_response()

    assert partials
    assert partials[-1] == transcript
    assert_friday_rocks(transcript)


@pytest.mark.asyncio
async def test_gpt_transcribe_non_streaming_multipart_chat_message():
    transcript = await b.TestOpenAITranscriptionMultipartChat(transcription_audio())
    assert_friday_rocks(transcript)
