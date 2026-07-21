"""Coverage for handle-backed stdlib types returned from BAML to Python.

The non-media cases are encode-back tests: Python receives a generated
class instance with an embedded handle, calls generated stdlib methods
with that same instance, and the engine must see the original handle
state. No external dependency: the HTTP test binds an ephemeral localhost
server and the FS test uses a temp file.
"""

import os
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.baml.fs import open as baml_open
from baml_sdk.baml.http import fetch
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


# --- baml.http.Response ---------------------------------------------------

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


def test_handles_http_get_response_fields_and_methods(http_server):
    resp = fetch(http_server)
    assert resp.status_code == 200
    assert resp.ok() is True
    assert resp.text() == _HTTP_BODY.decode()


# --- baml.fs.File: cursor state preserved across calls --------------------


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


def test_handles_open_file_returns_file_handle(temp_file):
    f = baml_open(temp_file, "r")
    assert type(f).__name__ == "File"
    assert f.close() is None


def test_handles_file_cursor_state_persists_across_calls(temp_file):
    f = baml_open(temp_file, "r")

    # Two successive reads on the *same* handle must advance the cursor —
    # the second read continues where the first stopped. This is the
    # load-bearing assertion: engine-side file state survives across
    # separate host→engine FFI calls.
    assert f.read(3) == "012"
    assert f.read(3) == "345"

    # Seek back to the start and confirm the cursor actually moved.
    assert f.seek_from("start", 0) == 0
    assert f.read(2) == "01"

    # text() reads from the current cursor (now at 2) to EOF.
    assert f.text() == "23456789"

    assert f.close() is None
