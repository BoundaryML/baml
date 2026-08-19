#!/usr/bin/env python3
"""Prove a published BAML release works from a clean external Go module."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


GO_MODULE = "github.com/boundaryml/baml-go"
MAX_DOWNLOAD_BYTES = 1 << 30


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def target_triple() -> str:
    machine = platform.machine().lower()
    architecture = {
        "amd64": "x86_64",
        "x86_64": "x86_64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }.get(machine)
    if architecture is None:
        fail(f"unsupported architecture {machine!r}")
    system = platform.system()
    if system == "Darwin":
        return f"{architecture}-apple-darwin"
    if system == "Linux":
        return f"{architecture}-unknown-linux-gnu"
    if system == "Windows":
        return f"{architecture}-pc-windows-msvc"
    fail(f"unsupported operating system {system!r}")


def download(url: str, destination: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "baml-go-release-smoke/1"})
    with urllib.request.urlopen(request, timeout=60) as response, destination.open("wb") as output:
        remaining = MAX_DOWNLOAD_BYTES
        while chunk := response.read(min(1024 * 1024, remaining + 1)):
            remaining -= len(chunk)
            if remaining < 0:
                fail(f"download exceeds {MAX_DOWNLOAD_BYTES} bytes: {url}")
            output.write(chunk)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_member_path(name: str) -> Path:
    normalized = PurePosixPath(name.replace("\\", "/"))
    if normalized.is_absolute() or ".." in normalized.parts:
        fail(f"unsafe archive member {name!r}")
    return Path(*normalized.parts)


def extract_archive(archive: Path, destination: Path) -> None:
    destination.mkdir(parents=True)
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as bundle:
            for member in bundle.infolist():
                relative = safe_member_path(member.filename)
                mode = member.external_attr >> 16
                if mode & 0o170000 == 0o120000:
                    fail(f"archive symlink is not allowed: {member.filename}")
                target = destination / relative
                if member.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                with bundle.open(member) as source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)
        return
    if tarfile.is_tarfile(archive):
        with tarfile.open(archive, "r:*") as bundle:
            for member in bundle.getmembers():
                relative = safe_member_path(member.name)
                if member.issym() or member.islnk() or not (member.isdir() or member.isfile()):
                    fail(f"unsupported archive member: {member.name}")
                target = destination / relative
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                source = bundle.extractfile(member)
                if source is None:
                    fail(f"could not read archive member: {member.name}")
                with source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(member.mode & 0o777)
        return
    fail(f"unsupported release archive: {archive}")


def run(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    attempts: int = 1,
    timeout_seconds: int = 300,
) -> str:
    for attempt in range(1, attempts + 1):
        try:
            result = subprocess.run(
                command,
                cwd=cwd,
                env=env,
                text=True,
                capture_output=True,
                timeout=timeout_seconds,
            )
        except subprocess.TimeoutExpired as exc:
            stdout = (
                exc.stdout.decode(errors="replace")
                if isinstance(exc.stdout, bytes)
                else (exc.stdout or "")
            )
            stderr = (
                exc.stderr.decode(errors="replace")
                if isinstance(exc.stderr, bytes)
                else (exc.stderr or "")
            )
            if attempt == attempts:
                print(stdout, end="")
                print(stderr, end="", file=sys.stderr)
                fail(f"command timed out after {timeout_seconds}s: {' '.join(command)}")
            time.sleep(2 ** attempt)
            continue
        if result.returncode == 0:
            if result.stdout:
                print(result.stdout, end="")
            if result.stderr:
                print(result.stderr, end="", file=sys.stderr)
            return result.stdout
        if attempt == attempts:
            print(result.stdout, end="")
            print(result.stderr, end="", file=sys.stderr)
            fail(f"command failed ({result.returncode}): {' '.join(command)}")
        time.sleep(2 ** attempt)
    raise AssertionError("unreachable")


def write_consumer(root: Path, module_version: str) -> None:
    (root / "baml_src").mkdir(parents=True)
    (root / "baml.toml").write_text(
        """[package]
name = "go-release-smoke"

[generator.go_client]
output_type = "go"
output_dir = "."
naming_convention = "language"
sdk_import_path = "example.com/baml-go-release-smoke/baml_sdk"
""",
        encoding="utf-8",
    )
    (root / "baml_src" / "main.baml").write_text(
        'function echo(value: string) -> string { value }\n', encoding="utf-8"
    )
    (root / "go.mod").write_text(
        f"""module example.com/baml-go-release-smoke

go 1.23

require {GO_MODULE} {module_version}
""",
        encoding="utf-8",
    )
    (root / "main_test.go").write_text(
        """package release_smoke

import (
    "context"
    "testing"

    "example.com/baml-go-release-smoke/baml_sdk"
)

func TestPublishedGoSDKCallsBAML(t *testing.T) {
    const want = "release-smoke"
    got, err := baml_sdk.Echo(context.Background(), want)
    if err != nil {
        t.Fatal(err)
    }
    if got != want {
        t.Fatalf("Echo() = %q, want %q", got, want)
    }
}
""",
        encoding="utf-8",
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument(
        "--manifest-base-url",
        default="https://pkg.boundaryml.com/manifest/v1",
    )
    parser.add_argument("--work-dir", required=True, type=Path)
    args = parser.parse_args()

    version = args.version.removeprefix("v")
    manifest_base_url = args.manifest_base_url.rstrip("/")
    root = args.work_dir.resolve()
    repository = Path(__file__).resolve().parents[1]
    if root == repository or repository in root.parents:
        fail("the release consumer must live outside the repository")
    if root.exists():
        shutil.rmtree(root)
    root.mkdir(parents=True)

    manifest_url = f"{manifest_base_url}/version/{version}.json"
    manifest_path = root / "release-manifest.json"
    download(manifest_url, manifest_path)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("version") != version:
        fail(f"manifest version is {manifest.get('version')!r}, expected {version!r}")

    go_release = manifest.get("baml_bridge_go")
    expected_go_release = {"module": GO_MODULE, "version": f"v{version}"}
    if go_release != expected_go_release:
        fail(f"manifest Go release is {go_release!r}, expected {expected_go_release!r}")

    target = target_triple()
    try:
        toolchain = manifest["artifacts"][target]
        runtime_artifact = manifest["cffi"][target]
    except KeyError as error:
        fail(f"manifest lacks {error.args[0]!r} for {target}")

    archive = root / "toolchain.archive"
    download(toolchain["url"], archive)
    if sha256(archive) != toolchain["sha256"]:
        fail("toolchain archive checksum mismatch")
    toolchain_root = root / "toolchain"
    extract_archive(archive, toolchain_root)
    executable = ".exe" if platform.system() == "Windows" else ""
    cli = toolchain_root / "bin" / f"baml-cli{executable}"
    if not cli.is_file():
        fail(f"released CLI is missing: {cli}")

    consumer = root / "consumer"
    consumer.mkdir()
    module_version = go_release["version"]
    write_consumer(consumer, module_version)

    runtime_cache = root / "runtime-cache"
    env = os.environ.copy()
    for name in list(env):
        if name.startswith("BAML_RUNTIME_") or name in {
            "BAML_CACHE_DIR",
            "BAML_DISABLE_DOWNLOAD",
            "GONOSUMDB",
            "GONOPROXY",
            "GOPRIVATE",
        }:
            env.pop(name)
    env.update(
        {
            "BAML_CACHE_DIR": str(runtime_cache),
            "BAML_RUNTIME_MANIFEST_BASE_URL": manifest_base_url,
            "CARGO": str(root / "forbidden-cargo"),
            "RUSTC": str(root / "forbidden-rustc"),
            "CGO_ENABLED": "1",
            "GOCACHE": str(root / "go-build-cache"),
            "GOMODCACHE": str(root / "go-module-cache"),
            # No `direct` fallback: channel promotion must prove the immutable
            # mirror tag has propagated through the public Go module proxy.
            "GOPROXY": "https://proxy.golang.org",
            "GOSUMDB": "sum.golang.org",
            "GOWORK": "off",
        }
    )

    run([str(cli), "--version"], cwd=consumer, env=env)
    run([str(cli), "generate", "--project", "."], cwd=consumer, env=env)
    if "replace " in (consumer / "go.mod").read_text(encoding="utf-8"):
        fail("external consumer unexpectedly contains a Go replace directive")
    run(
        # Reconcile the generated imports into the consumer's go.mod and go.sum.
        # A named download does not add transitive requirements to the main module.
        ["go", "mod", "tidy"],
        cwd=consumer,
        env=env,
        attempts=6,
    )
    module_json = json.loads(
        run(
            ["go", "list", "-m", "-json", GO_MODULE],
            cwd=consumer,
            env=env,
            attempts=4,
        )
    )
    if module_json.get("Version") != module_version or "Replace" in module_json:
        fail(f"resolved unexpected Go module: {module_json!r}")
    module_dir = Path(module_json["Dir"]).resolve()
    if module_dir == repository or repository in module_dir.parents:
        fail(f"Go resolved the repository checkout instead of the published module: {module_dir}")

    run(
        ["go", "test", "./...", "-count=1"],
        cwd=consumer,
        env=env,
        attempts=4,
    )
    expected_runtime_sha = runtime_artifact["sha256"].lower()
    cached_runtimes = [path for path in runtime_cache.rglob("*") if path.is_file()]
    if not any(sha256(path) == expected_runtime_sha for path in cached_runtimes):
        fail("the clean consumer did not cache the manifest-selected native runtime")

    print(
        f"ok: external Go consumer used {module_version}, released CLI {version}, "
        f"and native runtime {target}"
    )


if __name__ == "__main__":
    main()
