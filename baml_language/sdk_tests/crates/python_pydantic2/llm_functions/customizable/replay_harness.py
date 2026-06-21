"""Keyless replay harness shared by the streaming tests.

`@replay_server(recording_path=...)` is a decorator that runs the wrapped test
(sync or async) against an in-process BAML server replaying a checked-in SSE
recording, with the env-driven `StreamStub` client pointed at it — so the test
exercises the full streaming path with **no `OPENAI_API_KEY`**. Inside a
decorated test the server address is available as
`os.environ["BAML_REPLAY_BASE_URL"]`.
"""
import contextlib
import functools
import inspect
import os
import threading
import time
import urllib.request
from pathlib import Path


def recording_path(name: str) -> Path:
    """Absolute path to a checked-in SSE recording under sdk_tests/fixtures."""
    for parent in Path(__file__).resolve().parents:
        if parent.name == "sdk_tests":
            rec = parent / "fixtures" / "llm_functions" / "recordings" / f"{name}.snap.sse"
            assert rec.exists(), f"missing recording {rec}"
            return rec
    raise RuntimeError("could not locate the sdk_tests/ ancestor directory")


@contextlib.contextmanager
def _running_server(recording: str):
    """Serve `recording` on a background thread, set the replay client env, and
    tear it down on exit. Yields the bound `host:port`."""
    from baml_sdk import replay

    rec = recording_path(recording)
    addr_file = Path(os.environ.get("TMPDIR", "/tmp")) / (
        f"baml_replay_{os.getpid()}_{threading.get_ident()}_{recording}"
    )
    with contextlib.suppress(FileNotFoundError):
        addr_file.unlink()

    result: list = []
    error: list = []

    def _serve():
        try:
            result.append(replay.replay_serve_until_shutdown(str(rec), str(addr_file)))
        except Exception as e:  # surface bridge/engine failures to the poller
            error.append(e)

    thread = threading.Thread(target=_serve, daemon=True)
    thread.start()

    addr = None
    deadline = time.time() + 10.0
    while time.time() < deadline:
        if error:
            raise RuntimeError(f"replay server thread failed: {error[0]!r}")
        if addr_file.exists() and (text := addr_file.read_text().strip()):
            addr = text
            break
        time.sleep(0.02)
    if addr is None:
        raise TimeoutError("replay server did not bind within 10s")

    os.environ["BAML_REPLAY_BASE_URL"] = f"http://{addr}"
    os.environ["BAML_REPLAY_API_KEY"] = "replay-test-key"
    try:
        yield addr
    finally:
        with contextlib.suppress(Exception):
            urllib.request.urlopen(
                f"http://{addr}/__replay__/shutdown", data=b"", timeout=5
            )
        thread.join(timeout=10)
        with contextlib.suppress(FileNotFoundError):
            addr_file.unlink()
        for key in ("BAML_REPLAY_BASE_URL", "BAML_REPLAY_API_KEY"):
            os.environ.pop(key, None)


def replay_server(*, recording_path: str):
    """Decorator: run the wrapped test against the keyless replay server for the
    `recording_path` recording (a name under `recordings/`, resolved by
    `recording_path()`). Keyword-only. Works on both sync and `async` tests."""

    def decorator(fn):
        if inspect.iscoroutinefunction(fn):

            @functools.wraps(fn)
            async def awrapper(*args, **kwargs):
                with _running_server(recording_path):
                    return await fn(*args, **kwargs)

            return awrapper

        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            with _running_server(recording_path):
                return fn(*args, **kwargs)

        return wrapper

    return decorator
