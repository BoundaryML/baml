"""
Pytest configuration for bridge_python tests.

Before running these tests, build and install the native module:

    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/ -v
"""

import http.server
import json

import pytest


# ============================================================================
# Shared mock LLM handler
# ============================================================================


class MockLLMHandler(http.server.BaseHTTPRequestHandler):
    """Mock HTTP handler returning OpenAI-compatible chat completion responses."""

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        self.rfile.read(content_length)

        body = json.dumps(
            {
                "model": "mock-model",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": "mocked response"},
                        "finish_reason": "stop",
                    }
                ],
            }
        ).encode()

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        pass  # suppress request logging


# ============================================================================
# Cleanup fixture
# ============================================================================
@pytest.fixture(scope="session", autouse=True)
def flush_traces():
    """Ensure traces are flushed when pytest exits."""
    yield
    from baml_py import flush_events

    flush_events()
