"""AI failure values remain distinct through the Python bridge."""

import pytest

from baml_bridge import BamlError
from baml_sdk.ai.errors import InvalidResponse, ParseFailed, ProviderError
from baml_sdk.ai_error_test import (
    ExtractInt_stream,
    ExtractInt_stream_async,
    ThrowInvalidResponse,
    ThrowProviderError,
)


def test_invalid_response_preserves_wire_diagnostics():
    with pytest.raises(BamlError) as raised:
        ThrowInvalidResponse()
    assert raised.value.class_name == "ai.errors.InvalidResponse"
    failure = raised.value.value
    assert isinstance(failure, InvalidResponse)
    assert failure.provider == "test-provider"
    assert failure.message == "invalid provider envelope"
    assert failure.raw_body == "not JSON"
    assert failure.status_code == 200


def test_provider_error_preserves_native_error_metadata():
    with pytest.raises(BamlError) as raised:
        ThrowProviderError()
    assert raised.value.class_name == "ai.errors.ProviderError"
    failure = raised.value.value
    assert isinstance(failure, ProviderError)
    assert failure.error_type == "overloaded_error"
    assert failure.error_code == "server_error"
    assert failure.message == "provider overloaded"
    assert failure.status_code is None
    assert failure.raw_body == '{"error":{"type":"overloaded_error"}}'


def test_stream_schema_failure_is_parse_failed():
    stream = ExtractInt_stream()
    with pytest.raises(BamlError) as raised:
        stream.final()
    assert raised.value.class_name == "ai.errors.ParseFailed"
    failure = raised.value.value
    assert isinstance(failure, ParseFailed)
    assert failure.provider == "test-provider"
    assert failure.raw_output == "not a number"
    assert failure.message


async def test_async_stream_schema_failure_is_parse_failed():
    stream = await ExtractInt_stream_async()
    with pytest.raises(BamlError) as raised:
        await stream.final_async()
    assert isinstance(raised.value.value, ParseFailed)
    assert raised.value.value.raw_output == "not a number"
    assert raised.value.value.message
