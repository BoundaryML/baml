import pytest
import time
import asyncio
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from baml_py.errors import BamlTimeoutError, BamlClientError
from baml_py import AbortController, ClientRegistry
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
            "test input", baml_options={"abort_controller": abort_controller}
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


# Mock OpenAI-compatible streaming server that sends many chunks with delays
# This will send 200 chunks with 10ms between each, taking ~2.5 seconds total
# BUT chunk 3 has a 500ms delay to trigger the idle timeout
class MockStreamingHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        # Suppress server logs during tests
        pass

    def do_OPTIONS(self):
        # Handle CORS preflight
        self.send_response(200)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        self.end_headers()

    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_response(404)
            self.end_headers()
            return

        # Set up SSE headers
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "keep-alive")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()

        # Send first chunk immediately with role
        chunk = 'data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":"Chunk 0 "},"finish_reason":null}]}\n\n'
        self.wfile.write(chunk.encode("utf-8"))
        self.wfile.flush()

        # Send 199 more chunks with 10ms delay between each
        # BUT chunk 3 has a 500ms delay to trigger the idle timeout
        # Total time would be 198 * 10ms + 500ms = 2480ms (~2.5 seconds)
        for i in range(1, 200):
            # Special case: 500ms delay before chunk 3 to trigger idle timeout
            if i == 3:
                time.sleep(0.5)
            else:
                time.sleep(0.01)

            chunk = f'data: {{"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{{"index":0,"delta":{{"content":"Chunk {i} "}},"finish_reason":null}}]}}\n\n'
            try:
                self.wfile.write(chunk.encode("utf-8"))
                self.wfile.flush()
            except BrokenPipeError:
                # Client disconnected (timeout triggered), which is expected
                return

        # Send final chunk
        final = 'data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n'
        done = "data: [DONE]\n\n"
        try:
            self.wfile.write(final.encode("utf-8"))
            self.wfile.write(done.encode("utf-8"))
            self.wfile.flush()
        except BrokenPipeError:
            # Client disconnected
            pass


@pytest.mark.asyncio
async def test_timeout_on_idle_with_mock_server():
    """Test that idle timeout short-circuits a long streaming response"""
    # Start mock server in a background thread
    server = HTTPServer(("localhost", 0), MockStreamingHandler)
    port = server.server_port
    base_url = f"http://localhost:{port}/v1"

    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    try:
        # Create a client registry with our mock server
        registry = ClientRegistry()
        registry.add_llm_client(
            "MockIdleClient",
            "openai",
            {
                "base_url": base_url,
                "api_key": "mock-key",
                "model": "gpt-4",
                "http": {
                    "idle_timeout_ms": 200,  # 200ms idle timeout - should trigger during the 500ms delay before chunk 3
                },
            },
        )
        registry.set_primary("MockIdleClient")

        start_time = time.time()

        with pytest.raises(BamlTimeoutError) as exc_info:
            stream = b.stream.TestStreamingTimeout(
                "test with mock server", {"client_registry": registry}
            )

            chunk_count = 0
            async for chunk in stream:
                chunk_count += 1
                # Don't log every chunk to avoid spam with 200 chunks
                if chunk_count <= 5 or chunk_count % 50 == 0:
                    print(f"Received chunk {chunk_count}: {str(chunk)[:50]}")

            await stream.get_final_response()

        elapsed = time.time() - start_time

        error = exc_info.value
        print(f"Mock server timeout took {elapsed:.3f} seconds")

        # The mock server would take ~2.5 seconds to send all 200 chunks without timeout
        # (chunks 0-2: ~20ms, 500ms delay, chunks 3-199: ~1970ms = ~2.5s total)
        # With the 200ms idle timeout, it should short-circuit during the 500ms delay.
        # Expected time: ~20ms (chunks 0-2) + 200ms (timeout) = ~220ms
        # Allow up to 1 second for safety to ensure it's much less than the full 2.5s
        assert elapsed < 1.0, f"Timeout took too long: {elapsed}s (should be < 1.0s)"

        # Verify it's a timeout error
        assert "timeout" in str(error).lower()
        assert isinstance(error, BamlTimeoutError)

    finally:
        # Clean up server
        server.shutdown()
        server_thread.join(timeout=1)


@pytest.mark.asyncio
async def test_streaming_timeout_3_seconds():
    """Test that streaming with a timeout of 3 seconds triggers a timeout error.

    This test uses a real streaming LLM function with a client that specifies a 3-second timeout.
    """

    start_time = time.time()

    with pytest.raises(BamlTimeoutError) as exc_info:
        stream = b.stream.TestDefaultStreamingTimeout(
            "test default timeout", {}
        )

        chunk_count = 0
        async for chunk in stream:
            chunk_count += 1
            print(f"Received chunk {chunk_count}: {str(chunk)[:50]}")

        await stream.get_final_response()

    elapsed = time.time() - start_time

    error = exc_info.value
    print(f"Default timeout test took {elapsed:.3f} seconds")
    print(f"Error type: {type(error).__name__}")
    print(f"Error message: {str(error)}")

    # Should timeout around 3 seconds (default timeout)
    assert 2 < elapsed < 4, f"Expected ~3s timeout, but took {elapsed}s"

    # Verify it's a timeout error
    assert "timeout" in str(error).lower()
    assert isinstance(error, BamlTimeoutError)
