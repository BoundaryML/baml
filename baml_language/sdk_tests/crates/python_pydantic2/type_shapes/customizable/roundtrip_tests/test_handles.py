"""Coverage for handle-backed stdlib types returned from BAML to Python.

The non-media cases are encode-back tests through user types that wrap
stdlib handles (`stdlib_wrappers.baml`): Python receives a user class with
an embedded stdlib handle, passes that same instance back to user
functions, and the engine must see the original handle state. No external
dependency: the HTTP test binds an ephemeral localhost server and the FS
test uses a temp file.
"""

import os
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import (
    HttpExchange,
    OpenFile,
    close_open_file,
    fetch_http_exchange,
    http_exchange_text,
    make_http_exchange,
    open_read_only,
    read_open_file,
    seek_open_file,
)
from baml_sdk.baml.media import Image

# 1x1 transparent PNG.
PNG_B64 = (
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
    "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC"
)


# --- media: Image.from_base64 ---------------------------------------------


def test_handles_image_from_base64_roundtrips_payload():
    img = Image.from_base64(PNG_B64, "image/png")
    assert img.mime_type() == "image/png"
    assert img.base64() == PNG_B64


# --- baml.http.Response wrapped in a user type ----------------------------


# SDK_PARITY_LINT(skip): stdlib handles reach the host only through user types in SDKs that omit stdlib functions
def test_handles_user_type_wraps_http_response():
    exchange = make_http_exchange("wrapped", "hello from BAML")
    assert isinstance(exchange, HttpExchange)
    assert exchange.label == "wrapped"
    assert exchange.response.status_code == 200
    assert exchange.response.headers["x-label"] == "wrapped"
    assert http_exchange_text(exchange) == "hello from BAML"


_HTTP_BODY = b"hello from localhost"


class _Handler(BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(_HTTP_BODY)))
        self.end_headers()
        self.wfile.write(_HTTP_BODY)

    def log_message(self, *args):  # silence per-request stderr logging
        pass


@pytest.fixture
def http_server():
    srv = HTTPServer(("127.0.0.1", 0), _Handler)
    thread = threading.Thread(target=srv.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{srv.server_address[1]}/"
    finally:
        srv.shutdown()
        thread.join()
        srv.server_close()


# SDK_PARITY_LINT(skip): stdlib handles reach the host only through user types in SDKs that omit stdlib functions
def test_handles_user_type_wraps_fetched_http_response(http_server):
    exchange = fetch_http_exchange(http_server)
    assert exchange.label == http_server
    assert exchange.response.status_code == 200
    assert http_exchange_text(exchange) == _HTTP_BODY.decode()


# --- baml.fs.File wrapped in a user type: cursor state across calls ------


@pytest.fixture
def temp_file():
    d = tempfile.mkdtemp()
    path = os.path.join(d, "digits.txt")
    with open(path, "w") as fh:
        fh.write("0123456789")
    try:
        yield path
    finally:
        os.remove(path)
        os.rmdir(d)


# SDK_PARITY_LINT(skip): stdlib handles reach the host only through user types in SDKs that omit stdlib functions
def test_handles_user_type_wraps_file_handle(temp_file):
    opened = open_read_only(temp_file)
    assert isinstance(opened, OpenFile)
    assert opened.path == temp_file
    assert type(opened.file).__name__ == "File"
    assert close_open_file(opened) is None


def test_handles_file_cursor_state_persists_across_calls(temp_file):
    opened = open_read_only(temp_file)

    # Relative seeks verify that separate calls share one engine-side handle.
    assert seek_open_file(opened, "current", 3) == 3
    assert seek_open_file(opened, "current", 3) == 6

    # Seek back to the start and confirm the cursor actually moved.
    assert seek_open_file(opened, "start", 0) == 0
    assert seek_open_file(opened, "current", 2) == 2

    # Reading picks up from the current cursor (now at 2) to EOF.
    assert read_open_file(opened) == "23456789"

    assert close_open_file(opened) is None
