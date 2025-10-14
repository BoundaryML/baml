import pytest
import time
import asyncio
from baml_py.errors import BamlTimeoutError, BamlClientError
from baml_py import AbortController
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b

@pytest.mark.asyncio
async def test_connect_timeout():
    """Test that connect timeout raises BamlTimeoutError"""
    with pytest.raises(BamlTimeoutError) as exc_info:
        await b.TestTimeoutError("test input")

    error = exc_info.value
    assert "timeout" in str(error).lower()
    # Verify it's the right error type
    assert isinstance(error, BamlTimeoutError)
    assert isinstance(error, BamlClientError)  # Should inherit from BamlClientError

@pytest.mark.asyncio
async def test_request_timeout():
    """Test that request timeout raises BamlTimeoutError"""
    start_time = time.time()

    with pytest.raises(BamlTimeoutError) as exc_info:
        await b.TestRequestTimeout("climate change and its effects")

    elapsed = time.time() - start_time
    # Should fail quickly (within ~100ms accounting for overhead)
    assert elapsed < 0.2, f"Timeout took too long: {elapsed}s"

    error = exc_info.value
    assert "timeout" in str(error).lower()

@pytest.mark.asyncio
async def test_timeout_vs_abort_priority():
    """Test that abort signal takes priority over timeout"""
    abort_controller = AbortController()

    # Schedule abort after 25ms
    async def abort_after_delay():
        await asyncio.sleep(0.025)
        abort_controller.abort()

    asyncio.create_task(abort_after_delay())

    # Use a client with 100ms timeout
    with pytest.raises(Exception) as exc_info:
        await b.TestRequestTimeout(
            "test input",
            baml_options={"abort_controller": abort_controller}
        )

    # Should get abort error, not timeout error
    error_str = str(exc_info.value).lower()
    assert "abort" in error_str or "cancel" in error_str
    # Should NOT be a timeout error
    assert not isinstance(exc_info.value, BamlTimeoutError)

def test_sync_timeout():
    """Test timeout in synchronous context"""
    with pytest.raises(BamlTimeoutError) as exc_info:
        sync_b.TestTimeoutError("test input")

    error = exc_info.value
    assert "timeout" in str(error).lower()
    assert isinstance(error, BamlTimeoutError)

@pytest.mark.asyncio
async def test_streaming_timeout():
    """Test timeout with streaming (if streaming timeouts are implemented)"""
    # This test would be for Phase 4, but adding placeholder
    pytest.skip("Streaming timeouts not yet implemented")

    with pytest.raises(BamlTimeoutError):
        stream = b.stream.TestTimeoutError("test streaming timeout")
        async for _ in stream:
            pass
        await stream.get_final_response()

@pytest.mark.asyncio
async def test_fallback_with_timeout():
    """Test that timeout errors in fallback clients are handled correctly"""
    # The first client in fallback should timeout, but the second should succeed
    result = await b.TestTimeoutFallback("hello world")

    # Should have succeeded with the second client
    assert result is not None
    assert isinstance(result, str)
    assert len(result) > 10  # Should have gotten a reasonable response

@pytest.mark.asyncio
async def test_zero_timeout_means_infinite():
    """Test that timeout of 0 means no timeout"""
    # This should succeed despite having 0 timeout (infinite)
    result = await b.TestZeroTimeout("test infinite timeout")

    # Should succeed (no exception raised, got a valid response)
    assert result is not None
    assert isinstance(result, str)
    assert len(result) > 10  # Should have gotten a reasonable response

@pytest.mark.asyncio
async def test_timeout_error_includes_client_name():
    """Test that BamlTimeoutError includes the client name"""
    with pytest.raises(BamlTimeoutError) as exc_info:
        await b.TestTimeoutError("test")

    error = exc_info.value
    error_str = str(error)
    # Should mention the client name somewhere in the error
    assert "TestTimeoutClient" in error_str or "client" in error_str.lower()