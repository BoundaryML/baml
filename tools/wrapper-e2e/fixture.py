#!/usr/bin/env python3
"""OS fixtures only. Tests, wrapper invocations and assertions live in BAML."""

from dataclasses import dataclass
from enum import Enum
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import socketserver
import subprocess
import sys
import tarfile
import tempfile
import time
from urllib.parse import unquote, urlsplit
import zipfile

TARGETS = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]
VERSIONS = ["0.11.0", "0.12.0", "0.13.0"]
SUFFIX = ".exe" if os.name == "nt" else ""


class FixtureHTTPServer(http.server.ThreadingHTTPServer):
    def server_bind(self):
        # This server only binds loopback. HTTPServer's reverse-DNS lookup is
        # unnecessary and can stall every fixture on macOS CI runners.
        socketserver.TCPServer.server_bind(self)
        self.server_name = "localhost"
        self.server_port = self.server_address[1]


def write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def stub(path, identity, source):
    path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, path)
    path.chmod(0o755)
    write(path.with_suffix(".identity"), identity)


def init():
    root = Path(tempfile.mkdtemp(prefix="baml-wrapper-")).resolve()
    for directory in [
        "home",
        "baml",
        "project/nested/deep",
        "outside",
        "behaviors",
        "server/version",
    ]:
        (root / directory).mkdir(parents=True)
    write(root / "invocations.jsonl", "")
    write(root / "requests.jsonl", "")
    write(root / "telemetry.jsonl", "")
    for identity in VERSIONS + [
        "project-local-toolchain",
        "working-directory-toolchain",
        "reinstalled-toolchain",
        "replacement",
    ]:
        write(
            root / "behaviors" / (identity + ".json"),
            json.dumps(
                {
                    "args": [["check"]],
                    "stdout": identity + "\n",
                    "stderr": "",
                    "exit_code": 0,
                }
            ),
        )
    for version in VERSIONS:
        stub(
            root / "baml/toolchains" / version / "bin" / ("baml-cli" + SUFFIX),
            version,
            os.environ["WRAPPER_STUB"],
        )
        write(root / "baml/toolchains" / version / "VERSION", version + "\n")
        FixtureFiles(root).release(version)
    for sub, identity in [
        ("project/build", "project-local-toolchain"),
        ("project/nested/deep/build", "working-directory-toolchain"),
        ("outside/build", "working-directory-toolchain"),
        ("home/build", "project-local-toolchain"),
        ("project/space ünicode", "project-local-toolchain"),
    ]:
        stub(root / sub / ("baml-cli" + SUFFIX), identity, os.environ["WRAPPER_STUB"])
    write(root / "project/baml.toml", '[package]\nname = "fixture"\n')
    write(
        root / "baml/config.toml",
        '[default]\nselector = "0.11.0"\n[update]\nauto_check = false\n',
    )
    write(
        root / "baml/state.toml",
        '[channels.canary]\nactive_version = "0.11.0"\nresolved_at = "fixture"\nmanifest_path = "fixture"\n[channels.nightly]\nactive_version = "0.12.0"\nresolved_at = "fixture"\nmanifest_path = "fixture"\n',
    )
    environment = {
        key: value
        for (key, value) in os.environ.items()
        if not key.startswith(("BAML_", "WRAPPER_"))
    }
    environment.update(
        HOME=str(root / "home"),
        USERPROFILE=str(root / "home"),
        BAML_HOME=str(root / "baml"),
        WRAPPER_FIXTURE_ROOT=str(root),
        BAML_TELEMETRY_ENABLED="false",
        BAML_WRAPPER_BCS_TELEMETRY="off",
        DO_NOT_TRACK="1",
    )
    return {
        "root": str(root),
        "cwd": str(root / "project"),
        "env": environment,
        "suffix": SUFFIX,
        "windows": os.name == "nt",
        "host": json.loads(
            subprocess.check_output(
                [os.environ["WRAPPER_STUB"], "--fixture-host-info"], text=True
            )
        ),
    }


class ReleaseMode(str, Enum):
    VALID = "valid"
    MISSING_CLI = "missing-cli"
    INVALID_LAYOUT = "layout"
    TRAVERSAL = "traversal"
    LINK = "link"
    CHECKSUM = "checksum"
    UNSUPPORTED_SCHEMA = "schema"
    MISSING_TARGET = "target"


class Channel(str, Enum):
    CANARY = "canary"
    NIGHTLY = "nightly"


class FixtureCommand(str, Enum):
    CANONICALIZE = "canonicalize"
    INIT = "init"
    SERVE = "serve"
    REMOVE = "remove"
    RELEASE = "release"
    CHANNEL = "channel"
    SYMLINK = "symlink"
    COPY = "copy"
    WRAPPER = "wrapper"
    CHMOD = "chmod"
    AGE = "age"


@dataclass
class FixtureFiles:
    """Own fixture files and OS resources; wrapper decisions stay in BAML tests."""

    root: Path

    def release(self, version, mode="valid", identity=None):
        mode = ReleaseMode(mode)
        source = Path(os.environ["WRAPPER_STUB"])
        cache = Path(os.environ.get("WRAPPER_E2E_ARCHIVES", str(self.root / "cache")))
        cache.mkdir(parents=True, exist_ok=True)
        cache_key = mode + "-" + (identity or "default")
        if all(((cache / (cache_key + ext)).exists() for ext in [".zip", ".tar.gz"])):
            for ext in [".zip", ".tar.gz"]:
                shutil.copyfile(
                    cache / (cache_key + ext), self.root / "server" / (version + ext)
                )
            return
        files = {
            "bin/baml-cli" + SUFFIX: source.read_bytes(),
            "bin/baml-pack-host" + SUFFIX: b"pack host",
            "assets/baml-vscode.vsix": b"vsix",
            "assets/playground/index.html": b"html",
            "assets/playground/assets/index.js": b"js",
            "assets/playground/assets/index.css": b"css",
        }
        if identity:
            files["bin/baml-cli.identity"] = identity.encode()
        if mode == "missing-cli":
            del files["bin/baml-cli" + SUFFIX]
        if mode == "layout":
            del files["assets/baml-vscode.vsix"]
        if mode == "traversal":
            files["../escaped"] = b"bad"
        if mode == "link":
            files["bin/link"] = b"link"
        for extension in [".tar.gz", ".zip"]:
            buffer = io.BytesIO()
            if extension == ".zip":
                with zipfile.ZipFile(buffer, "w") as archive:
                    for name, data in files.items():
                        entry = zipfile.ZipInfo(name)
                        entry.create_system = 3
                        entry.external_attr = (
                            0o120777 if name == "bin/link" else 0o100755
                        ) << 16
                        archive.writestr(entry, data)
            else:
                with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
                    for name, data in files.items():
                        entry = tarfile.TarInfo(name)
                        entry.mode = 0o755
                        entry.size = len(data)
                        if name == "bin/link":
                            entry.type = tarfile.SYMTYPE
                            entry.linkname = "../../escaped"
                            entry.size = 0
                        archive.addfile(entry, io.BytesIO(data))
            data = buffer.getvalue()
            (self.root / "server" / f"{version}{extension}").write_bytes(data)
            (cache / (cache_key + extension)).write_bytes(data)
        return files

    def manifest(self, version, channel="canary", mode="valid"):
        mode = ReleaseMode(mode)
        channel = Channel(channel)
        base = (self.root / "url").read_text()
        artifacts = {}
        for target in TARGETS:
            extension = ".zip" if "windows" in target else ".tar.gz"
            archive = self.root / "server" / f"{version}{extension}"
            artifacts[target] = {
                "url": f"{base}/{archive.name}",
                "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()
                if mode != "checksum"
                else "0" * 64,
            }
        result = {
            "schema": 99 if mode == "schema" else 1,
            "version": version,
            "channel": channel,
            "released_at": "2026-01-01T00:00:00Z",
            "artifacts": {} if mode == "target" else artifacts,
        }
        return json.dumps(result)

    def serve(self):
        fixture_root = self.root

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                with (fixture_root / "telemetry.jsonl").open("a") as output:
                    output.write(json.dumps(json.loads(body)) + "\n")
                delay = fixture_root / "telemetry.delay"
                if delay.exists():
                    time.sleep(float(delay.read_text()))
                status = fixture_root / "telemetry.status"
                rejected = fixture_root / "telemetry.reject-event"
                code = int(status.read_text()) if status.exists() else 204
                if rejected.exists() and json.loads(body).get("event") == rejected.read_text():
                    code = 503
                self.send_response(code)
                self.send_header("Content-Length", "0")
                self.end_headers()

            def do_GET(self):
                with (fixture_root / "requests.jsonl").open("a") as output:
                    output.write(json.dumps(self.path) + "\n")
                server_root = (fixture_root / "server").resolve()
                path = (server_root / unquote(urlsplit(self.path).path).lstrip("/")).resolve()
                if not path.is_relative_to(server_root):
                    self.send_error(404)
                    return
                if path.with_name(path.name + ".status").exists():
                    self.send_error(
                        int(path.with_name(path.name + ".status").read_text())
                    )
                    return
                if path.with_name(path.name + ".delay").exists():
                    time.sleep(float(path.with_name(path.name + ".delay").read_text()))
                if not path.is_file():
                    self.send_error(404)
                    return
                data = path.read_bytes()
                self.send_response(200)
                self.send_header(
                    "Content-Length",
                    str(
                        len(data)
                        + (
                            1000
                            if path.with_name(path.name + ".disconnect").exists()
                            else 0
                        )
                    ),
                )
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass

        server = FixtureHTTPServer(("127.0.0.1", 0), Handler)
        server.daemon_threads = True
        url = f"http://127.0.0.1:{server.server_port}"
        write(self.root / "url", url)
        for version in VERSIONS:
            write(
                self.root / "server/version" / (version + ".json"),
                self.manifest(version),
            )
        write(self.root / "server/canary.json", self.manifest(VERSIONS[0]))
        write(self.root / "server/nightly.json", self.manifest(VERSIONS[1], "nightly"))
        print(url, flush=True)
        server.serve_forever()

    def remove(self, args):
        path = self.root / args[0]
        if path.is_dir() and (not path.is_symlink()):

            def writable(function, failing_path, error):
                Path(failing_path).chmod(0o755)
                function(failing_path)

            for attempt in range(40):
                try:
                    shutil.rmtree(path, onerror=writable)
                    break
                except PermissionError:
                    if attempt == 39:
                        raise
                    time.sleep(0.05)
        else:
            path.unlink(missing_ok=True)

    def publish_release(self, args):
        (version, mode, identity) = args[0:3]
        self.release(version, mode, identity or None)
        write(
            self.root / "server/version" / (version + ".json"),
            self.manifest(version, mode=mode),
        )

    def set_channel(self, args):
        (channel, version) = args[0:2]
        write(
            self.root / "server" / (channel + ".json"), self.manifest(version, channel)
        )

    def symlink(self, args):
        (path, target) = (self.root / p for p in args[0:2])
        path.unlink(missing_ok=True)
        path.symlink_to(target)

    def copy(self, args):
        (source, destination) = args[0:2]
        shutil.copyfile(source, self.root / destination)
        (self.root / destination).chmod(0o755)

    def wrapper_archive(self, args):
        data = io.BytesIO()
        extension = ".zip" if os.name == "nt" else ".tar.gz"
        source = Path(os.environ["WRAPPER_STUB"]).read_bytes()
        name = "bin/baml" + SUFFIX
        if os.name == "nt":
            with zipfile.ZipFile(data, "w") as archive:
                archive.writestr(name, source)
        else:
            with tarfile.open(fileobj=data, mode="w:gz") as archive:
                entry = tarfile.TarInfo(name)
                (entry.mode, entry.size) = (0o644 if args and args[0] == "not-executable" else 0o755, len(source))
                archive.addfile(entry, io.BytesIO(source))
        body = data.getvalue()
        write(
            self.root / "server/wrapper.json",
            json.dumps(
                {
                    "schema": 1,
                    "version": "9.9.9",
                    "released_at": "fixture",
                    "artifacts": {
                        target: {
                            "url": (self.root / "url").read_text()
                            + "/wrapper"
                            + extension,
                            "sha256": "0" * 64
                            if len(args) > 0 and args[0] == "checksum"
                            else hashlib.sha256(body).hexdigest(),
                        }
                        for target in TARGETS
                    },
                }
            ),
        )
        (self.root / "server" / ("wrapper" + extension)).write_bytes(body)

    def chmod(self, args):
        (self.root / args[0]).chmod(int(args[1], 8))

    def age(self, args):
        path = self.root / args[0]
        stamp = time.time() - int(args[1])
        os.utime(path, (stamp, stamp))


def main():
    command = FixtureCommand(sys.argv[1])
    if command is FixtureCommand.INIT:
        print(json.dumps(init()))
        return
    files = FixtureFiles(Path(sys.argv[2]))
    operations = {
        FixtureCommand.CANONICALIZE: lambda args: print(
            os.path.normcase(str(Path(args[0]).resolve()).removeprefix("\\\\?\\"))
        ),
        FixtureCommand.SERVE: lambda args: files.serve(),
        FixtureCommand.REMOVE: files.remove,
        FixtureCommand.RELEASE: files.publish_release,
        FixtureCommand.CHANNEL: files.set_channel,
        FixtureCommand.SYMLINK: files.symlink,
        FixtureCommand.COPY: files.copy,
        FixtureCommand.WRAPPER: files.wrapper_archive,
        FixtureCommand.CHMOD: files.chmod,
        FixtureCommand.AGE: files.age,
    }
    operations[command](sys.argv[3:])


if __name__ == "__main__":
    main()
