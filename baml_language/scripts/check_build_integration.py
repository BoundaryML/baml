#!/usr/bin/env python3
"""Exercise supplied protobuf tools, immutable schema imports, and Git overrides."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from check_reduced_build import check_graph

WORKSPACE = Path(__file__).resolve().parent.parent
SCHEMAS = {
    "bridge_ctypes": "types",
    "bex_events": "src/value/proto",
    "btel_bcs": "proto",
    "btel_recorder": "proto",
}
SDK_DIRS = [WORKSPACE / "sdks/rust/bridge_rust/src/wire", WORKSPACE / "sdks/python/src/baml_bridge/cffi/v1"]


def run(command, *, cwd=WORKSPACE, env=None, ok=True):
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True)
    assert (result.returncode == 0) == ok, f"{command}\n{result.stdout}\n{result.stderr}"
    return result


def cargo(*args, env):
    result = run(["cargo", *args, "--locked", "--message-format=json"], env=env)
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def check_default_graphs(metadata):
    # Check each root separately: a workspace-wide build can hide missing
    # feature forwarding by enabling the bundled compiler from another crate.
    consumers = {"baml_proto_codegen"}
    while True:
        expanded = consumers | {package["name"] for package in metadata["packages"] if any(dependency["kind"] != "dev" and dependency["name"] in consumers for dependency in package["dependencies"])}
        if expanded == consumers:
            break
        consumers = expanded
    for name in sorted(consumers):
        graph = run(["cargo", "tree", "--locked", "-p", name, "-e", "normal,build", "--prefix", "none", "-f", "{p}"]).stdout
        names = {line.split()[0] for line in graph.splitlines()}
        if "baml_proto_codegen" in names:
            assert "protoc-bin-vendored" in names, f"{name} lost the default bundled compiler"
    print(f"ok: {len(consumers)} individual default dependency graphs retain the bundled compiler", flush=True)


def sdk_snapshot():
    return {path: (path.read_bytes(), path.stat().st_mtime_ns) for directory in SDK_DIRS for path in directory.rglob("*") if path.is_file()}


def check_scripts(root, env, reduced, packages, wrapper, log):
    flags = ["--no-default-features"] if reduced else []
    before = sdk_snapshot()
    messages = cargo("check", *[arg for name in SCHEMAS for arg in ("-p", name)], *flags, env=env)
    assert sdk_snapshot() == before, "Ordinary compilation rewrote SDK source files"
    scripts = {}
    for message in messages:
        if message["reason"] == "compiler-artifact":
            assert message["target"]["name"] != "baml-generate-sdk-protos", "Ordinary compilation built the maintenance command"
            name = packages.get(message["package_id"])
            if name in SCHEMAS and message["target"]["kind"] == ["custom-build"]:
                scripts[name] = Path(message["filenames"][0]).resolve()
    assert scripts.keys() == SCHEMAS.keys(), scripts

    for name, schema in SCHEMAS.items():
        source = root / f"import-{reduced}" / "crates" / name
        shutil.copytree(WORKSPACE / "crates" / name / schema, source / schema)
        shutil.copy2(WORKSPACE / "crates" / name / "build.rs", source / "build.rs")
        paths = [source.parent.parent, source.parent, source, *source.rglob("*")]
        for path in paths:
            path.chmod(0o555 if path.is_dir() else 0o444)
        try:
            output = root / f"{name}-{reduced}-out"
            output.mkdir()
            script_env = dict(env, CARGO_MANIFEST_DIR=str(source), OUT_DIR=str(output))
            # This schema-only import has no sibling SDK trees. All writes must
            # go to OUT_DIR, even when the supplied compiler path contains spaces.
            log.unlink(missing_ok=True)
            supplied = dict(script_env, PROTOC=str(wrapper))
            run([str(scripts[name])], cwd=source, env=supplied)
            assert log.is_file(), f"{name} ignored PROTOC"
            assert list(output.glob("*.rs")), f"{name} produced no Rust bindings"

            invalid = dict(script_env, PROTOC=str(root / "missing-protoc"))
            run([str(scripts[name])], cwd=source, env=invalid, ok=False)
            empty = run([str(scripts[name])], cwd=source, env=dict(script_env, PROTOC=""), ok=False)
            assert "PROTOC must name" in empty.stderr, empty
            script_env.pop("PROTOC", None)
            result = run([str(scripts[name])], cwd=source, env=script_env, ok=not reduced)
            if reduced:
                assert "Set PROTOC" in result.stderr, result
        finally:
            for path in paths:
                path.chmod(0o755 if path.is_dir() else 0o644)
    print(f"ok: {'reduced' if reduced else 'default'} protobuf builds, compiler selection, and immutable schema imports", flush=True)


def check_sdk_command(root, env, wrapper, log):
    messages = cargo("build", "-p", "baml_proto_codegen", "--bin", "baml-generate-sdk-protos", env=env)
    executable = next(message["executable"] for message in messages if message["reason"] == "compiler-artifact" and message.get("executable"))
    command = [executable]
    assert "Usage:" in run(command + ["--help"], cwd=root, env=env).stdout
    for args in (["--unknown"], ["--rust-out"], ["--python-out", ""]):
        run(command + args, cwd=root, env=env, ok=False)
    rust = root / "sdk rust"
    python = root / "sdk python"
    args = ["--rust-out", str(rust), "--python-out", str(python)]
    before = sdk_snapshot()
    run(command + args, cwd=root, env=env)
    assert sdk_snapshot() == before, "Explicit output paths still rewrote checkout sources"
    expected = {
        **{Path("rust") / path.name: path.read_bytes() for path in SDK_DIRS[0].glob("baml_bridge*.rs")},
        **{Path("python/baml_bridge/cffi/v1") / path.name: path.read_bytes() for path in SDK_DIRS[1].glob("*_pb2.*")},
    }
    actual = {Path(label) / path.relative_to(directory): path.read_bytes() for label, directory in [("rust", rust), ("python", python)] for path in directory.rglob("*") if path.is_file()}
    assert actual.keys() == expected.keys(), (actual.keys(), expected.keys())
    for path in expected:
        assert actual[path] == expected[path], f"Generated SDK file differs: {path}"
    # The maintenance tool has the same reduced build contract as the scripts.
    messages = cargo("build", "-p", "baml_proto_codegen", "--bin", "baml-generate-sdk-protos", "--no-default-features", env=env)
    executable = next(message["executable"] for message in messages if message["reason"] == "compiler-artifact" and message.get("executable"))
    missing = run([executable, *args], cwd=root, env=env, ok=False)
    assert "Set PROTOC" in missing.stderr, missing
    log.unlink(missing_ok=True)
    run([executable, *args], cwd=root, env=dict(env, PROTOC=str(wrapper)))
    assert len(log.read_text().splitlines()) >= 2, "Rust and Python generation must both use PROTOC"
    print(f"ok: {len(expected)} SDK files match committed bytes; staged generation and reduced maintenance command work", flush=True)


def check_git_override(root, env):
    executable = root / "artifact-build"
    run(["rustc", "--edition=2024", str(WORKSPACE / "crates/baml_artifact/build.rs"), "-o", str(executable)], env=env)
    git_dir = root / "fake git"
    git_dir.mkdir()
    log = root / "git-invocations"
    git = git_dir / "git"
    git.write_text(f"#!{sys.executable}\nfrom pathlib import Path\nPath({str(log)!r}).touch()\nraise SystemExit(1)\n")
    git.chmod(0o755)
    git_env = dict(env, PATH=str(git_dir))
    for sha in ("a" * 40, "b" * 64, "", "not-a-commit"):
        valid = len(sha) in (40, 64)
        result = run([str(executable)], cwd=root, env=dict(git_env, BAML_GIT_SHA=sha), ok=valid)
        assert not log.exists(), "An explicit BAML_GIT_SHA still invoked Git"
        if valid:
            assert f"BAML_ARTIFACT_BUILD_COMMIT={sha}" in result.stdout, result
            assert result.stdout.count("rerun-if-changed=") == 1, "Override tracked checkout files"
    git_env.pop("BAML_GIT_SHA", None)
    run([str(executable)], cwd=root, env=git_env)
    assert log.exists(), "Missing override should retain Git discovery"
    print("ok: supplied source identity never invokes Git, including invalid overrides", flush=True)


def main():
    protoc = os.environ.get("PROTOC", shutil.which("protoc"))
    if not protoc:
        raise RuntimeError("Set PROTOC or install protoc to test reduced builds")
    protoc = shutil.which(protoc) or protoc
    env = dict(os.environ)
    env.pop("PROTOC", None)
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]).stdout)
    packages = {package["id"]: package["name"] for package in metadata["packages"]}
    check_graph()
    check_default_graphs(metadata)
    with tempfile.TemporaryDirectory(prefix="baml build integration ") as directory:
        root = Path(directory)
        wrapper = root / "supplied protoc"
        log = root / "protoc-invocations"
        wrapper.write_text(f"#!{sys.executable}\nimport subprocess, sys\nfrom pathlib import Path\nwith Path({str(log)!r}).open('a') as stream:\n    stream.write('invoked\\n')\nraise SystemExit(subprocess.call([{protoc!r}, *sys.argv[1:]]))\n")
        wrapper.chmod(0o755)
        check_scripts(root, env, False, packages, wrapper, log)
        check_scripts(root, dict(env, PROTOC=str(wrapper)), True, packages, wrapper, log)
        check_sdk_command(root, env, wrapper, log)
        check_git_override(root, env)


if __name__ == "__main__":
    main()
