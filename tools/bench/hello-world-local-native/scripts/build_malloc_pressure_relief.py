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

    pack_host = native_build.BUILD / "toolchain/baml-pack-host"
    shutil.copy2(native_build.BUILD / "cargo-target/release/baml-pack-host", pack_host)
    app = native_build.BUILD / "apps/baml-only-explicit-gc"
    native_build.copy_tree(native_build.ROOT / "diagnostics/baml-only-explicit-gc", app)
    executable = app / "hello"
    cli = native_build.BUILD / "toolchain/baml-cli"
    native_build.run(cli, "pack", "main", "--target", native_build.TARGET, "--output", executable, "--no-progress", "--agent-skill-check", "off", cwd=app, env=env)

    manifest = {
        "source": str(source),
        "revision": revision,
        "source_dirty": dirty,
        "source_status": source_status,
        "source_snapshot": source_snapshot,
        "base_build_revision": base_revision,
        "embedded_toolchain_revision": base_revision,
        "base_build_manifest_sha256": native_build.digest(base_manifest_path),
        "artifacts": {
            "toolchain/baml-pack-host": native_build.digest(pack_host),
            "apps/baml-only-explicit-gc/hello": native_build.digest(executable),
        },
    }
    output = native_build.BUILD / "malloc-pressure-relief-manifest.json"
    output.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Malloc pressure-relief diagnostic ready for {revision[:12]}. Run python3 scripts/run_malloc_pressure_relief.py")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
