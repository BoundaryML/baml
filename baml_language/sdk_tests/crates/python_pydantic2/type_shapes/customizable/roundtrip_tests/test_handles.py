"""Coverage for handle-backed stdlib types returned from BAML to Python.

Backs the `ns_handles` fixture. Three handle shapes are exercised:

* **media** (`image_from_base64`) — returns a `BamlImage` PyO3 object;
  its instance methods (`base64()`, `mime_type()`) are callable directly
  on the host object.
* **`baml.http.Response`** (`http_get`) — driven against a localhost
  server. `status_code` is a plain decoded field and `ok()` is a
  pure-expression method, so both are callable host-side. `text()` is a
  `$rust_io_function` (a SysOp) and is *not* host-invokable as an entry
  point, so it's reached through the `response_text` BAML wrapper.
* **`baml.fs.File`** (`open_file_read` + `file_*` wrappers) — the point
  of interest: the File handle must preserve engine-side cursor state
  across separate FFI calls. Because every File method is a SysOp (not
  host-invokable directly), each method is wrapped in a BAML function
  that takes the File as a parameter; passing the same File into
  successive wrapper calls is what proves the cursor persists.

No external dependency: the HTTP test binds an ephemeral localhost
server and the FS test uses a temp file.
"""

import os
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.handles import (
    image_from_base64,
    http_get,
    response_text,
    open_file_read,
    file_read,
    file_text,
    file_seek,
    file_close,
)

# 1x1 transparent PNG.
PNG_B64 = (
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
    "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC"
)


# --- media: Image.from_base64 ---------------------------------------------


def test_image_from_base64_roundtrips_payload():
    img = image_from_base64(data=PNG_B64, mime="image/png")
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


def test_http_get_response_fields_and_methods(http_server):
    resp = http_get(url=http_server)
    # status_code is a decoded field; ok() is a pure-expression method —
    # both work host-side.
    assert resp.status_code == 200
    assert resp.ok() is True
    # text() is a SysOp, reached via the BAML wrapper.
    assert response_text(r=resp) == _HTTP_BODY.decode()


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


def test_open_file_returns_file_handle(temp_file):
    f = open_file_read(path=temp_file)
    assert type(f).__name__ == "File"


def test_file_cursor_state_persists_across_calls(temp_file):
    f = open_file_read(path=temp_file)

    # Two successive reads on the *same* handle must advance the cursor —
    # the second read continues where the first stopped. This is the
    # load-bearing assertion: engine-side file state survives across
    # separate host→engine FFI calls.
    assert file_read(f=f, n=3) == "012"
    assert file_read(f=f, n=3) == "345"

    # Seek back to the start and confirm the cursor actually moved.
    assert file_seek(f=f, whence="start", offset=0) == 0
    assert file_read(f=f, n=2) == "01"

    # text() reads from the current cursor (now at 2) to EOF.
    assert file_text(f=f) == "23456789"

    assert file_close(f=f) is None
