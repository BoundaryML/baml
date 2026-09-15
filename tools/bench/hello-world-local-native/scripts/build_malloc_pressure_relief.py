#!/usr/bin/env python3
"""Build the opt-in macOS malloc pressure-relief packed-BAML diagnostic."""

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

import build as native_build


DIAGNOSTIC_BUILD = native_build.BUILD / "malloc-pressure-relief"
BASE_ARTIFACTS = ("toolchain/baml-cli", "toolchain/baml-pack-host", "apps/baml-only-explicit-gc/hello")


def verify_base_artifacts(manifest):
    for relative in BASE_ARTIFACTS:
        path = native_build.BUILD / relative
        expected = manifest.get("artifacts", {}).get(relative)
        if not path.is_file() or not expected or native_build.digest(path) != expected:
            raise RuntimeError(f"base artifact does not match .build/manifest.json: {relative}; rebuild with scripts/build.py --explicit-gc-diagnostic")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baml-source", type=Path, required=True, help="Path to a baml_language workspace containing the pressure-relief pack-host probe")
    args = parser.parse_args()
    source = args.baml_source.expanduser().resolve()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("this diagnostic currently requires native macOS arm64")
    if not (source / "Cargo.toml").is_file():
        parser.error(f"not a baml_language workspace: {source}")

    base_manifest_path = native_build.BUILD / "manifest.json"
    if not base_manifest_path.exists():
        parser.error("missing base .build/manifest.json; run scripts/build.py --explicit-gc-diagnostic first")
    base_manifest = json.loads(base_manifest_path.read_text())
    if not base_manifest.get("explicit_gc_diagnostic"):
        parser.error("the base build lacks explicit-GC diagnostics")
    if base_manifest.get("source_dirty") is not False:
        parser.error("the base build must use clean source; rebuild it before continuing")
    try:
        verify_base_artifacts(base_manifest)
    except RuntimeError as error:
        parser.error(str(error))
    base_manifest_sha256 = native_build.digest(base_manifest_path)

    revision, dirty, source_status, source_snapshot = native_build.source_identity(source)
    if dirty:
        parser.error(f"pressure-relief source must be clean:\n{source_status}")
    base_revision = base_manifest["revision"]
    ancestor = subprocess.run(["git", "merge-base", "--is-ancestor", base_revision, revision], cwd=source)
    if ancestor.returncode != 0:
        parser.error(f"pressure-relief revision {revision[:12]} is not a descendant of base build {base_revision[:12]}")

    rust_toolchain = native_build.output("rustup", "show", "active-toolchain", cwd=source).split()[0]
    env = dict(
        os.environ,
        BAML_GIT_SHA=base_revision,
        BAML_PROFILE="0",
        BAML_TELEMETRY_DISABLED="1",
        BAML_LOG="off",
        CARGO_TARGET_DIR=str(native_build.BUILD / "cargo-target"),
        RUSTUP_TOOLCHAIN=rust_toolchain,
    )
    native_build.run(
        "mise",
        "x",
        "-C",
        source,
        "--",
        "cargo",
        "build",
        "--release",
        "--locked",
        "-p",
        "baml_pack_host",
        "--bin",
        "baml-pack-host",
        "--features",
        "gc_profiling",
        cwd=source,
        env=env,
    )

    toolchain = DIAGNOSTIC_BUILD / "toolchain"
    toolchain.mkdir(parents=True, exist_ok=True)
    cli = toolchain / "baml-cli"
    pack_host = toolchain / "baml-pack-host"
    shutil.copy2(native_build.BUILD / "toolchain/baml-cli", cli)
    shutil.copy2(native_build.BUILD / "cargo-target/release/baml-pack-host", pack_host)
    app = DIAGNOSTIC_BUILD / "apps/baml-only-explicit-gc"
    native_build.copy_tree(native_build.ROOT / "diagnostics/baml-only-explicit-gc", app)
    executable = app / "hello"
    native_build.run(cli, "pack", "main", "--target", native_build.TARGET, "--output", executable, "--no-progress", "--agent-skill-check", "off", cwd=app, env=env)

    manifest = {
        "source": str(source),
        "revision": revision,
        "source_dirty": dirty,
        "source_status": source_status,
        "source_snapshot": source_snapshot,
        "base_build_revision": base_revision,
        "embedded_toolchain_revision": base_revision,
        "base_build_manifest_sha256": base_manifest_sha256,
        "artifacts": {
            "toolchain/baml-cli": native_build.digest(cli),
            "toolchain/baml-pack-host": native_build.digest(pack_host),
            "apps/baml-only-explicit-gc/hello": native_build.digest(executable),
        },
    }
    verify_base_artifacts(base_manifest)
    if native_build.digest(base_manifest_path) != base_manifest_sha256:
        raise RuntimeError("base .build/manifest.json changed during focused build")
    output = DIAGNOSTIC_BUILD / "manifest.json"
    output.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Malloc pressure-relief diagnostic ready for {revision[:12]}. Run python3 scripts/run_malloc_pressure_relief.py")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
