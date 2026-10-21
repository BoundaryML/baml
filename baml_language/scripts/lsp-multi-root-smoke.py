#!/usr/bin/env python3
"""Live multi-root smoke for `baml lsp`, over real stdio against real files.

The protocol suite in `baml_lsp` covers this logic against an in-memory
filesystem. This drives the actual binary instead, so it also exercises the
stdio transport, project discovery on a real disk, and the two ways a root
reaches the server: the `--workspace` flags the host process is started with,
and the folders an editor announces in `initialize`.

    scripts/lsp-multi-root-smoke.py                     # all three modes
    scripts/lsp-multi-root-smoke.py --mode client       # one mode
    scripts/lsp-multi-root-smoke.py --binary path/to/baml-cli --keep

Exits nonzero on the first failed expectation and leaves the server log and
fixtures behind when `--keep` is passed.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# Two projects declaring the same class with different fields: if the roots
# were one world, `Point` would be declared twice and both would fail.
PROJECT_A = {
    "point.baml": "class Point {\n    x int\n}\nfunction use_a(p: Point) -> int {\n    p.x\n}\n",
}
PROJECT_B = {
    "point.baml": "class Point {\n    y string\n}\nfunction use_b(p: Point) -> string {\n    p.y\n}\n",
    "bad.baml": "class Bad {\n  x Undefined\n}\n",
}
# Announced after startup, to check that a folder added later is discovered.
PROJECT_C = {"c.baml": "class Bad {\n  x Undefined\n}\n"}

# How long to wait for a publication, and how long a "nothing more is coming"
# window is. Diagnostics are debounced and the first compile pulls in the
# standard library, so the first wait is the slow one.
PUBLISH_TIMEOUT = 90.0
SETTLE_WINDOW = 5.0

MODES = ("host", "client", "both")


class Server:
    """One `baml lsp` process, spoken to over stdio."""

    def __init__(self, binary: Path, flag_roots: list[Path], log: Path) -> None:
        argv = [str(binary), "lsp"]
        for root in flag_roots:
            argv += ["--workspace", str(root)]
        self.log = log
        self._log_file = log.open("wb")
        # Unbuffered on this side, so `select` on the pipe is the whole truth
        # about whether a message is waiting. A buffered reader would hold a
        # decoded message the kernel no longer has, and `settle` would call
        # that quiet and move on before reading it.
        self.proc = subprocess.Popen(
            argv,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self._log_file,
            bufsize=0,
        )
        # Every publication seen so far, newest last, keyed by URI.
        self.published: dict[str, list[list[dict]]] = {}
        # Ids of the requests the server has answered.
        self.answered: set[int] = set()

    def send(self, message: dict) -> None:
        body = json.dumps(message).encode()
        assert self.proc.stdin is not None
        self.proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        self.proc.stdin.flush()

    def notify(self, method: str, params: dict) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def receive(self) -> dict | None:
        """The next message, or None once the server's stdout closes."""
        assert self.proc.stdout is not None
        headers: dict[str, str] = {}
        while True:
            line = self.proc.stdout.readline()
            if not line:
                return None
            if line in (b"\r\n", b"\n"):
                break
            name, _, value = line.decode().partition(":")
            headers[name.strip().lower()] = value.strip()
        message = json.loads(self._read_exactly(int(headers["content-length"])))
        if message.get("method") == "textDocument/publishDiagnostics":
            params = message["params"]
            self.published.setdefault(params["uri"], []).append(params["diagnostics"])
        elif "id" in message and "method" not in message:
            self.answered.add(message["id"])
        return message

    def _read_exactly(self, count: int) -> bytes:
        """Exactly `count` bytes. An unbuffered read returns what is there."""
        assert self.proc.stdout is not None
        chunks: list[bytes] = []
        remaining = count
        while remaining > 0:
            chunk = self.proc.stdout.read(remaining)
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        return b"".join(chunks)

    def pump_until(self, done, timeout: float) -> bool:
        """Read messages until `done()` holds. False if the timeout wins."""
        deadline = time.monotonic() + timeout
        while not done():
            if time.monotonic() > deadline or self.receive() is None:
                return done()
        return True

    def settle(self, window: float = SETTLE_WINDOW) -> None:
        """Read until the server has been quiet for `window` seconds."""
        assert self.proc.stdout is not None
        deadline = time.monotonic() + window
        while time.monotonic() < deadline:
            remaining = deadline - time.monotonic()
            if not _readable(self.proc.stdout, remaining):
                return
            if self.receive() is None:
                return
            deadline = time.monotonic() + window

    def latest(self, path: Path) -> list[dict] | None:
        """The most recent diagnostics published for `path`, if any."""
        seen = self.published.get(uri_of(path))
        return seen[-1] if seen else None

    def shutdown(self) -> int:
        self.send({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": None})
        self.pump_until(lambda: 99 in self.answered, timeout=30.0)
        self.notify("exit", {})
        try:
            code = self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            code = self.proc.wait()
        self._log_file.close()
        return code


def _readable(stream, timeout: float) -> bool:
    import selectors

    with selectors.DefaultSelector() as selector:
        selector.register(stream, selectors.EVENT_READ)
        return bool(selector.select(max(0.0, timeout)))


def uri_of(path: Path) -> str:
    return path.resolve().as_uri()


def write_project(root: Path, files: dict[str, str]) -> None:
    source = root / "baml_src"
    source.mkdir(parents=True)
    for name, text in files.items():
        (source / name).write_text(text)


def has_error(diagnostics: list[dict] | None) -> bool:
    return bool(diagnostics) and any(d.get("severity") == 1 for d in diagnostics)


class Failed(Exception):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise Failed(message)


def run_mode(mode: str, binary: Path, workspace: Path, keep: bool) -> None:
    """Drive one server through the multi-root lifecycle for `mode`.

    The fixtures and the server's stderr survive a failed run, which is when
    they are worth reading.
    """
    succeeded = False
    fixtures = Path(tempfile.mkdtemp(prefix=f"baml-lsp-smoke-{mode}-", dir=workspace))
    a, b, c = fixtures / "a", fixtures / "b", fixtures / "c"
    write_project(a, PROJECT_A)
    write_project(b, PROJECT_B)
    write_project(c, PROJECT_C)

    flag_roots = [a, b] if mode in ("host", "both") else []
    announced = [a, b] if mode in ("client", "both") else []
    server = Server(binary, flag_roots, fixtures / "server.log")

    try:
        server.send(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "processId": None,
                    "capabilities": {"general": {"positionEncodings": ["utf-16"]}},
                    "workspaceFolders": [
                        {"uri": uri_of(root), "name": root.name} for root in announced
                    ],
                },
            }
        )
        expect(
            server.pump_until(lambda: 1 in server.answered, timeout=60.0),
            "the server never answered initialize",
        )
        server.notify("initialized", {})

        opened = [
            a / "baml_src/point.baml",
            b / "baml_src/point.baml",
            b / "baml_src/bad.baml",
        ]
        for path in opened:
            server.notify(
                "textDocument/didOpen",
                {
                    "textDocument": {
                        "uri": uri_of(path),
                        "languageId": "baml",
                        "version": 1,
                        "text": path.read_text(),
                    }
                },
            )

        # Each project checks against its own `Point`, and the error in `b`
        # stays in `b`.
        expect(
            server.pump_until(
                lambda: all(server.latest(p) is not None for p in opened),
                PUBLISH_TIMEOUT,
            ),
            "timed out waiting for the first diagnostics of every open file",
        )
        expect(
            not server.latest(a / "baml_src/point.baml"),
            "project a should check clean against its own Point",
        )
        expect(
            not server.latest(b / "baml_src/point.baml"),
            "project b should check clean against its own Point",
        )
        expect(
            has_error(server.latest(b / "baml_src/bad.baml")),
            "the bad file in project b should report an error",
        )

        # A folder announced after startup is discovered like the first ones.
        server.notify(
            "workspace/didChangeWorkspaceFolders",
            {"event": {"added": [{"uri": uri_of(c), "name": "c"}], "removed": []}},
        )
        expect(
            server.pump_until(
                lambda: server.latest(c / "baml_src/c.baml") is not None,
                PUBLISH_TIMEOUT,
            ),
            "a folder added after initialize was not discovered",
        )
        expect(
            has_error(server.latest(c / "baml_src/c.baml")),
            "the folder added later should be checked like any other",
        )

        # Withdraw b's folder with nothing open under it. A folder the host
        # process announced still covers its project; a folder only the client
        # announced does not, so the project goes and its markers clear.
        for path in (b / "baml_src/point.baml", b / "baml_src/bad.baml"):
            server.notify("textDocument/didClose", {"textDocument": {"uri": uri_of(path)}})
        server.notify(
            "workspace/didChangeWorkspaceFolders",
            {"event": {"added": [], "removed": [{"uri": uri_of(b), "name": "b"}]}},
        )
        server.settle()
        bad_after = server.latest(b / "baml_src/bad.baml")
        if flag_roots:
            expect(
                has_error(bad_after),
                "a --workspace root must survive the client withdrawing the same folder",
            )
        else:
            expect(
                not bad_after,
                "withdrawing the only folder covering a project should clear its markers",
            )

        code = server.shutdown()
        expect(code == 0, f"the server exited with {code}, expected a clean shutdown")
        succeeded = True
    finally:
        if server.proc.poll() is None:
            server.proc.kill()
            server.proc.wait()
        if keep or not succeeded:
            print(f"  fixtures kept in {fixtures}")
            print(f"  server log: {server.log}")
        else:
            shutil.rmtree(fixtures, ignore_errors=True)


def main() -> int:
    default_binary = Path(__file__).resolve().parents[1] / "target/debug/baml-cli"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        type=Path,
        default=default_binary,
        help="the baml-cli to drive (default: the debug build)",
    )
    parser.add_argument(
        "--mode",
        choices=MODES,
        action="append",
        help="how roots are announced; repeatable, defaults to all three",
    )
    parser.add_argument(
        "--keep", action="store_true", help="keep the fixtures and the server log"
    )
    args = parser.parse_args()

    if not args.binary.exists():
        print(f"no such binary: {args.binary}", file=sys.stderr)
        print("build it with: cargo build -p baml_cli", file=sys.stderr)
        return 2

    workspace = Path(tempfile.mkdtemp(prefix="baml-lsp-smoke-"))
    failures = 0
    try:
        for mode in args.mode or MODES:
            label = {
                "host": "--workspace flags only",
                "client": "initialize workspaceFolders only",
                "both": "both",
            }[mode]
            print(f"mode {mode}: {label}")
            started = time.monotonic()
            try:
                run_mode(mode, args.binary, workspace, args.keep)
            except Failed as failure:
                failures += 1
                print(f"  FAIL {failure}")
            else:
                print(f"  ok in {time.monotonic() - started:.1f}s")
    finally:
        if not args.keep and not failures:
            shutil.rmtree(workspace, ignore_errors=True)

    if failures:
        print(f"\n{failures} of {len(args.mode or MODES)} modes failed")
        return 1
    print("\nevery mode served both projects independently")
    return 0


if __name__ == "__main__":
    # The wrapper is not on this path: the binary is invoked directly.
    os.environ.setdefault("BAML_CLI_ALLOW_DIRECT", "1")
    sys.exit(main())
