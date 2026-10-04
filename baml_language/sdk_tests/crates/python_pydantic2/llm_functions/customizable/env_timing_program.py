"""A host program whose environment is set at a chosen point relative to
`import baml_sdk`. `test_env_timing.py` runs it in a fresh interpreter per
scenario, since import order cannot be replayed inside the pytest process.

usage: env_timing_program.py <scenario>

Every LLM call goes through the env-driven `StreamStub` client
(`api_key = env.BAML_REPLAY_API_KEY`, `base_url = env.BAML_REPLAY_BASE_URL`),
pointed at a local server that records each request's path and `Authorization`
header and answers LLM posts with a checked-in SSE recording. The last line of
stdout is a JSON report: `{"calls": [...], "requests": [...]}`.
"""
import http.server
import json
import os
import sys
import threading

from replay_harness import recording_path

SCENARIO = sys.argv[1]
RECORDING = recording_path("replay_extract_string").read_bytes()
REQUESTS: list[dict] = []


class _Capture(http.server.BaseHTTPRequestHandler):
    def _handle(self):
        self.rfile.read(int(self.headers.get("content-length") or 0))
        REQUESTS.append({"path": self.path, "authorization": self.headers.get("authorization")})
        is_llm = self.path == "/responses"
        body = RECORDING if is_llm else b""
        self.send_response(200 if is_llm else 404)
        self.send_header("content-type", "text/event-stream")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_POST = _handle
    do_PUT = _handle

    def log_message(self, format, *args):
        pass


_server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Capture)
threading.Thread(target=_server.serve_forever, daemon=True).start()
URL = f"http://127.0.0.1:{_server.server_address[1]}"


def set_llm_env(api_key: str) -> None:
    os.environ["BAML_REPLAY_BASE_URL"] = URL
    os.environ["BAML_REPLAY_API_KEY"] = api_key


def set_boundary_env() -> None:
    os.environ["BOUNDARY_URL"] = URL
    os.environ["BOUNDARY_API_KEY"] = "boundary-key"


if SCENARIO == "set_then_import":
    set_llm_env("key-set-before-import")
elif SCENARIO == "boundary_set_then_import":
    set_boundary_env()

# The imports under test: everything above ran before them, `main()` after.
import baml_sdk
from baml_bridge import shutdown_runtime
from baml_sdk import lorem
from baml_sdk.ai.stream import Done
from baml_sdk.baml import env as baml_env

CALLS: list[str] = []


def call_llm() -> None:
    """Run one LLM function to completion, recording how it ended."""
    try:
        stream = lorem.stream_e2e_extract_stream("ignored-by-capture-server")
        while not isinstance(stream.next(), Done):
            pass
        assert isinstance(stream.final(), str)
        CALLS.append("ok")
    except Exception as error:  # the test asserts on how each call ended
        CALLS.append(f"error: {error}")


def main() -> None:
    if SCENARIO == "set_then_import":
        call_llm()
    elif SCENARIO == "import_then_set":
        set_llm_env("key-set-after-import")
        call_llm()
    elif SCENARIO == "change_between_calls":
        call_llm()
        for api_key in ("key-1", "key-2"):
            set_llm_env(api_key)
            CALLS.append(f"env.get: {baml_env.get('BAML_REPLAY_API_KEY')}")
            call_llm()
        del os.environ["BAML_REPLAY_API_KEY"]
        CALLS.append(f"env.get: {baml_env.get('BAML_REPLAY_API_KEY')}")
        call_llm()
    elif SCENARIO in ("boundary_set_then_import", "boundary_import_then_set"):
        set_boundary_env()
        set_llm_env("llm-key")
        call_llm()
        # Trace delivery completes during shutdown, which a real program gets
        # from baml_bridge's `atexit` hook.
        shutdown_runtime()
    else:
        raise SystemExit(f"unknown scenario {SCENARIO!r}")


if __name__ == "__main__":
    main()
    print(json.dumps({"calls": CALLS, "requests": REQUESTS}))
