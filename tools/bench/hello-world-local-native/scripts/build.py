#!/usr/bin/env python3
"""Build the native macOS benchmark from a selected BAML source worktree."""

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / ".build"
TARGET = "aarch64-apple-darwin"
VEGETA_VERSION = "12.13.0"


def run(*args, cwd=ROOT, env=None):
    print("+", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)


def run_with_mise(source, cwd, *args, env=None):
    run("mise", "x", "-C", source, "--", "bash", "-c", 'cd "$1" && shift && exec "$@"', "bash", cwd, *args, env=env)


def output(*args, cwd=ROOT):
    return subprocess.check_output(list(map(str, args)), cwd=cwd, text=True).strip()


def output_bytes(*args, cwd=ROOT):
    return subprocess.check_output(list(map(str, args)), cwd=cwd)


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def copy_tree(source, destination):
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns("node_modules", "dist", "target", ".baml", "__pycache__", "*.pyc", "*.so", "*.dylib", "*.node"))


def source_identity(source):
    jj = subprocess.run(["jj", "--ignore-working-copy", "log", "-r", "@", "--no-graph", "-T", "commit_id"], cwd=source, text=True, capture_output=True)
    if jj.returncode == 0:
        revision = jj.stdout.strip()
        status = subprocess.run(["jj", "status", "--no-pager"], cwd=source, text=True, capture_output=True).stdout
        dirty = "The working copy has no changes." not in status
        snapshot = hashlib.sha256(output_bytes("jj", "diff", "--git", cwd=source)).hexdigest()
        return revision, dirty, status, snapshot
    revision = output("git", "rev-parse", "HEAD", cwd=source)
    status = output("git", "status", "--short", cwd=source)
    snapshot = hashlib.sha256()
    snapshot.update(output_bytes("git", "diff", "--binary", "HEAD", cwd=source))
    for relative in output("git", "ls-files", "--others", "--exclude-standard", cwd=source).splitlines():
        path = source / relative
        snapshot.update(relative.encode())
        if path.is_file():
            snapshot.update(path.read_bytes())
    return revision, bool(status), status, snapshot.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baml-source", type=Path, default=ROOT.parents[2] / "baml_language", help="Path to a baml_language workspace")
    parser.add_argument("--skip-rust", action="store_true", help="Reuse the CLI, pack host, Python bridge, and Node addon for the same source revision")
    args = parser.parse_args()
    source = args.baml_source.expanduser().resolve()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("this harness currently builds and measures native macOS arm64 binaries")
    if not (source / "Cargo.toml").is_file() or not (source / "sdks/python/pyproject.toml").is_file():
        parser.error(f"not a baml_language workspace: {source}")

    revision, dirty, source_status, source_snapshot = source_identity(source)
    BUILD.mkdir(parents=True, exist_ok=True)
    manifest_path = BUILD / "manifest.json"
    previous = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    if args.skip_rust and (previous.get("source") != str(source) or previous.get("revision") != revision or previous.get("source_snapshot") != source_snapshot):
        parser.error("--skip-rust requires a completed build from the same source path and exact source snapshot")

    rust_toolchain = output("rustup", "show", "active-toolchain", cwd=source).split()[0]
    common_env = dict(os.environ, BAML_GIT_SHA=revision, BAML_PROFILE="0", BAML_TELEMETRY_DISABLED="1", BAML_LOG="off", CARGO_TARGET_DIR=str(BUILD / "cargo-target"), RUSTUP_TOOLCHAIN=rust_toolchain)
    toolchain = BUILD / "toolchain"
    toolchain.mkdir(exist_ok=True)
    cli = toolchain / "baml-cli"
    pack_host = toolchain / "baml-pack-host"
    pyvenv = BUILD / "pyvenv"
    if not pyvenv.exists():
        run("uv", "venv", "--python", "3.10", pyvenv)
    run("uv", "pip", "install", "--python", pyvenv / "bin/python", "maturin>=1.10,<2.0", "starlette==0.47.3", "uvicorn==0.35.0", "pydantic==2.11.9", "protobuf>=6.31.1", "typing-extensions>=4.14.0")

    if not args.skip_rust:
        run("mise", "x", "-C", source, "--", "cargo", "build", "--release", "--locked", "-p", "baml_cli", "--bin", "baml-cli", "-p", "baml_pack_host", "--bin", "baml-pack-host", cwd=source, env=common_env)
        shutil.copy2(BUILD / "cargo-target/release/baml-cli", cli)
        shutil.copy2(BUILD / "cargo-target/release/baml-pack-host", pack_host)
        wheels = BUILD / "wheels"
        if wheels.exists():
            shutil.rmtree(wheels)
        wheels.mkdir()
        run(pyvenv / "bin/maturin", "build", "--release", "--locked", "--interpreter", pyvenv / "bin/python", "--out", wheels, cwd=source / "sdks/python", env=common_env)
        wheel = next(wheels.glob("baml_bridge-*.whl"))
        run("uv", "pip", "install", "--reinstall", "--python", pyvenv / "bin/python", wheel)

    apps = BUILD / "apps"
    for name in ("node-baseline", "node-baml", "python-baseline", "python-baml", "baml-only"):
        copy_tree(ROOT / "apps" / name, apps / name)

    for name in ("node-baml", "python-baml"):
        run(cli, "generate", "--agent-skill-check", "off", cwd=apps / name, env=common_env)
    run(cli, "pack", "main", "--target", TARGET, "--output", apps / "baml-only/hello", "--no-progress", "--agent-skill-check", "off", cwd=apps / "baml-only", env=common_env)

    source_node = Path(output("mise", "x", "-C", source, "--", "which", "node"))
    source_node_env = dict(common_env, PATH=str(source_node.parent) + os.pathsep + os.environ["PATH"])
    bridge = BUILD / "node-bridge"
    if not args.skip_rust:
        copy_tree(source / "sdks/typescript/bridge_typescript", bridge)
        run(source_node.parent / "npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", cwd=bridge, env=source_node_env)
        proto = source / "crates/bridge_ctypes/types"
        run(source_node, bridge / "node_modules/protobufjs-cli/bin/pbjs", "-t", "static-module", "-w", "es6", "-p", proto, "-o", bridge / "typescript_src/proto/baml_cffi.js", proto / "baml_bridge/cffi/v1/baml_inbound.proto", proto / "baml_bridge/cffi/v1/baml_outbound.proto", cwd=bridge, env=source_node_env)
        run(source_node, bridge / "node_modules/protobufjs-cli/bin/pbts", "-o", bridge / "typescript_src/proto/baml_cffi.d.ts", bridge / "typescript_src/proto/baml_cffi.js", cwd=bridge, env=source_node_env)
        run_with_mise(source, bridge, bridge / "node_modules/.bin/napi", "build", "--manifest-path", source / "sdks/typescript/bridge_typescript/Cargo.toml", "--package-json-path", bridge / "package.json", "--output-dir", bridge / "dist", "--js", "native.js", "--dts", "native.d.ts", "--platform", "--esm", "--release", "--target", TARGET, env=source_node_env)
        shutil.copy2(bridge / "dist/native.d.ts", bridge / "typescript_src/native.d.ts")
        run(source_node, bridge / "node_modules/typescript/bin/tsc", "-p", bridge / "tsconfig.json", "--noCheck", cwd=bridge, env=source_node_env)
        run(source_node, bridge / "typescript_src/copy-proto.js", cwd=bridge, env=source_node_env)
        run(source_node, bridge / "typescript_src/tag-generated-files.js", cwd=bridge, env=source_node_env)

    node20 = Path(output("mise", "x", "node@20", "--", "which", "node"))
    node_env = dict(os.environ, PATH=str(node20.parent) + os.pathsep + os.environ["PATH"])
    for name in ("node-baseline", "node-baml"):
        run(node20.parent / "npm", "install", "--install-links", "--ignore-scripts", "--no-audit", "--no-fund", cwd=apps / name, env=node_env)
    run(apps / "node-baml/node_modules/.bin/tsc", "-p", apps / "node-baml/tsconfig.json", cwd=apps / "node-baml", env=node_env)

    tools = BUILD / "tools"
    tools.mkdir(exist_ok=True)
    vegeta = tools / "vegeta"
    filename = f"vegeta_{VEGETA_VERSION}_darwin_arm64.tar.gz"
    base = f"https://github.com/tsenart/vegeta/releases/download/v{VEGETA_VERSION}/"
    archive = tools / filename
    if not archive.exists():
        urllib.request.urlretrieve(base + filename, archive)
    checksums = urllib.request.urlopen(base + f"vegeta_{VEGETA_VERSION}_checksums.txt").read().decode()
    expected = next(line.split()[0] for line in checksums.splitlines() if line.split()[-1].lstrip("*") == filename)
    if digest(archive) != expected:
        raise RuntimeError(f"checksum mismatch for {filename}")
    with tarfile.open(archive) as tar:
        vegeta.write_bytes(tar.extractfile("vegeta").read())
    vegeta.chmod(0o755)

    artifacts = [cli, pack_host, next((BUILD / "wheels").glob("baml_bridge-*.whl")), bridge / "dist/baml_node.darwin-arm64.node", apps / "baml-only/hello", vegeta]
    manifest = {
        "source": str(source),
        "revision": revision,
        "source_dirty": dirty,
        "source_status": source_status,
        "source_snapshot": source_snapshot,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": output(pyvenv / "bin/python", "--version"),
        "node": output(node20, "--version"),
        "node_binary": str(node20),
        "vegeta": VEGETA_VERSION,
        "artifacts": {str(path.relative_to(BUILD)): digest(path) for path in artifacts},
    }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Native artifacts ready for {revision[:12]}. Run python3 scripts/run.py --rate 300 --duration 300")


if __name__ == "__main__":
    main()
