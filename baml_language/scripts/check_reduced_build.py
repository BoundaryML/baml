#!/usr/bin/env python3
"""Verify the dependency contract of builds without `baml-defaults`."""

import json
from pathlib import Path
import queue
import re
import subprocess
import threading

WORKSPACE = Path(__file__).resolve().parent.parent
FORBIDDEN = {
    "baml_query", "baml_release", "libsui", "mimalloc", "libmimalloc-sys",
    "pyo3-stub-gen", "pyo3-stub-gen-derive", "rustpython-parser", "axum",
}
FORBIDDEN_FEATURES = {
    "serde_json": {"arbitrary_precision"},
    "parking_lot": {"deadlock_detection"},
    "jiff": {"tzdb-bundle-platform", "tzdb-bundle-always"},
    "tar": {"xattr"},
    "clap": {"cargo"},
    "tower-http": {"fs"},
}


def check_graph(workspace=WORKSPACE, replacement=False):
    for package in ("baml_cli", "bridge_python"):
        command = [
            "cargo", "tree", "--locked", "-p", package, "--no-default-features",
            "--target", "all", "-e", "normal,build", "--prefix", "none", "-f", "{p}|{f}",
        ]
        if package == "bridge_python":
            command += ["--features", "bundle-http"]
        output = subprocess.check_output(command, cwd=workspace, text=True)
        forbidden = set(FORBIDDEN)
        if replacement:
            forbidden |= {"reqwest", "tokio-tungstenite", "sha1"}
        failures = []
        for line in output.splitlines():
            identity, features = line.split("|", 1)
            name = identity.split()[0]
            if name in forbidden or name.startswith(("datafusion", "arrow", "sqlparser")):
                failures.append(identity)
            enabled = set(features.removesuffix(" (*)").split(","))
            unwanted = enabled & FORBIDDEN_FEATURES.get(name, set())
            if unwanted:
                failures.append(f"{name}: {', '.join(sorted(unwanted))}")
        if failures:
            raise RuntimeError(f"{package} reduced build includes:\n" + "\n".join(sorted(set(failures))))
        print(f"ok: {package} reduced dependency graph across all targets", flush=True)


def check_cli(cli, workspace, env):
    help_text = subprocess.check_output([cli, "--help"], cwd=workspace, env=env, text=True)
    for command in ("pack", "playground"):
        assert not re.search(rf"^\s+{command}\s", help_text, re.MULTILINE), help_text
        result = subprocess.run([cli, command, "--help"], cwd=workspace, env=env, capture_output=True)
        assert result.returncode != 0, f"{command} unexpectedly available"
    for command in ("check", "run", "lsp"):
        assert re.search(rf"^\s+{command}\s", help_text, re.MULTILINE), help_text

    # Exercise an actual editor handshake and clean shutdown. The reduced LSP
    # must stay usable and must not advertise the disabled playground handler.
    process = subprocess.Popen([cli, "lsp"], cwd=workspace, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    responses = queue.Queue()

    def read_frames():
        try:
            while True:
                header = process.stdout.readline()
                if not header:
                    return
                length = int(header.decode().split(":", 1)[1])
                assert process.stdout.readline() == b"\r\n"
                responses.put(json.loads(process.stdout.read(length)))
        except Exception as error:
            responses.put(error)

    def send(message):
        body = json.dumps({"jsonrpc": "2.0", **message}).encode()
        process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        process.stdin.flush()

    def response(request_id):
        while True:
            message = responses.get(timeout=30)
            if isinstance(message, Exception):
                raise message
            if message.get("id") == request_id:
                assert "error" not in message, message
                return message["result"]

    threading.Thread(target=read_frames, daemon=True).start()
    try:
        send({"id": 1, "method": "initialize", "params": {"processId": None, "rootUri": None, "capabilities": {}}})
        initialized = response(1)
        assert initialized["capabilities"].get("codeLensProvider") is None, initialized
        send({"method": "initialized", "params": {}})
        send({"id": 2, "method": "shutdown", "params": None})
        response(2)
        send({"method": "exit", "params": None})
        assert process.wait(timeout=30) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdin.close()
        process.stdout.close()
    print("ok: reduced CLI commands and stdio LSP", flush=True)


if __name__ == "__main__":
    check_graph()
