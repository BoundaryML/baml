"""The controller side: turn a BAML checkout into a stage directory the machine can run."""
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import time
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent
ROOT = PACKAGE.parent
GLUE = PACKAGE / "native_glue.rs"

# `baml pack --native` writes the generated crate and then runs `cargo build`.
# The bench builds on the machine, from a clean target, so this stands in for
# cargo on PATH while packing: it records the environment pack gave cargo and
# leaves an empty file where pack expects the binary.
CARGO_SHIM = """#!/bin/sh
set -e
target=""
while [ $# -gt 0 ]; do
  [ "$1" = "--target-dir" ] && target="$2"
  shift
done
mkdir -p "$target/release"
: > "$target/release/baml_native_program"
printf '%s' "${BAML_GIT_SHA:-}" > "$target/../BAML_GIT_SHA"
"""

# The generated crate's own dependencies plus what the glue calls; paths are
# the machine's checkout. baml_pack_host stays so features unify as in a
# shipped `baml pack --native` binary. The cases share a target directory, so
# each package has its own name: cargo would otherwise take one case's build
# as fresh for the next.
MANIFEST = """[package]
name = "bench_{case}"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[lib]
name = "baml_native_program"
path = "src/lib.rs"

[[bin]]
name = "baml_native_program"
path = "src/main.rs"

[dependencies]
bex_vm = {{ path = "{crates}/bex_vm" }}
bex_vm_types = {{ path = "{crates}/bex_vm_types" }}
baml_pack_host = {{ path = "{crates}/baml_pack_host" }}
baml_artifact = {{ path = "{crates}/baml_artifact" }}
baml_exec = {{ path = "{crates}/baml_exec" }}
bex_engine = {{ path = "{crates}/bex_engine", default-features = false, features = ["ring-crypto"] }}
sys_native = {{ path = "{crates}/sys_native", default-features = false, features = ["bundle-http", "ring-crypto"] }}
tokio = {{ version = "1", features = ["rt", "rt-multi-thread"] }}

[profile.release]
lto = "fat"
codegen-units = 1
opt-level = 3
panic = "unwind"
"""


def git(worktree, *args):
    return subprocess.run(["git", "-C", str(worktree), *args], capture_output=True, text=True, check=True).stdout.strip()


def emit_case(cli, worktree, case, dest, guest_crates, shim_dir):
    """Pack `prepare` and `run` with --native; keep the generated crate, swap in the glue and the machine's paths."""
    project = dest / "project"
    (project / "baml_src").mkdir(parents=True)
    (project / "baml.toml").write_text('[package]\nname = "workload"\n')
    shutil.copy(ROOT / "cases" / case / "workload.baml", project / "baml_src" / "workload.baml")
    emit = dest / "emit"
    env = {**os.environ, "BAML_AGENT_SKILL_CHECK": "off", "PATH": f"{shim_dir}{os.pathsep}{os.environ['PATH']}"}
    r = subprocess.run([str(cli), "--agent-skill-check", "off", "--no-progress", "--color", "never", "pack",
                        "--project", str(project), "-f", "prepare", "-f", "run", "--native", "--native-report",
                        "--native-workspace", str(Path(worktree) / "baml_language"), "--native-build-dir", str(emit),
                        "--output", str(dest / "packed")], capture_output=True, text=True, env=env)
    (dest / "emit.log").write_text(r.stdout + r.stderr)
    report = [line for line in r.stderr.splitlines() if line.startswith(("native ", "bytecode "))]
    (dest / "report.txt").write_text("\n".join(report) + "\n")
    (dest / "packed").unlink(missing_ok=True)
    if r.returncode != 0 or not (emit / "src" / "lib.rs").exists():
        shutil.rmtree(emit, ignore_errors=True)
        return {"status": "rejected", "reason": (r.stderr.strip().splitlines() or ["?"])[-1][:300]}
    shutil.rmtree(emit / "target", ignore_errors=True)
    # The machine builds the host from `git archive` of this commit, which has
    # no .git for the build script to read the commit from. Stamp it with that
    # commit: the one the emitting CLI was built from (pack checks this), so
    # the host accepts the envelope.
    stamp = emit / "BAML_GIT_SHA"
    if not stamp.exists() or not stamp.read_text().strip():
        head = subprocess.run(["git", "-C", str(worktree), "rev-parse", "HEAD"],
                              capture_output=True, text=True, check=True).stdout.strip()
        stamp.write_text(head)
    shutil.copy(GLUE, emit / "src" / "main.rs")
    (emit / "Cargo.toml").write_text(MANIFEST.format(case=case, crates=guest_crates))
    # Resolve against the workspace's lock rather than whatever the registry has today.
    shutil.copy(Path(worktree) / "baml_language" / "Cargo.lock", emit / "Cargo.lock")
    toolchain = Path(worktree) / "baml_language" / "rust-toolchain.toml"
    if toolchain.exists():
        shutil.copy(toolchain, emit / "rust-toolchain.toml")
    totals = [line for line in r.stderr.splitlines() if line.endswith(" bytecode") and " native, " in line]
    return {"status": "emitted", "report": totals[-1] if totals else ""}


def archive_source(worktree, sha, dest):
    """baml_language at HEAD plus a SHA file, as the machine's `src/`."""
    tar = dest / "src.tar"
    subprocess.run(["git", "-C", str(worktree), "archive", "--format=tar", "-o", str(tar), "HEAD", "baml_language"], check=True)
    (dest / "SHA").write_text(sha + "\n")
    with tarfile.open(tar, "a") as t:
        t.add(dest / "SHA", arcname="SHA")
    subprocess.run(["gzip", "-1", "-f", str(tar)], check=True)
    (dest / "src.tar.gz").rename(dest / "src.tgz")
    (dest / "SHA").unlink()


def build_stage(worktree, cli, protocol, machine, stage):
    worktree = Path(worktree).resolve()
    sha = git(worktree, "rev-parse", "HEAD")
    dirty = len(git(worktree, "status", "--porcelain", "--", "baml_language").splitlines())
    stage.mkdir(parents=True)
    guest_crates = str(machine.bench_root / "src" / "baml_language" / "crates")
    shim_dir = Path(tempfile.mkdtemp(prefix="bench-cargo-shim."))
    (shim_dir / "cargo").write_text(CARGO_SHIM)
    (shim_dir / "cargo").chmod(0o755)
    emitted = {}
    try:
        for case in protocol.cases:
            (stage / "cases" / case).mkdir(parents=True)
            shutil.copy(ROOT / "cases" / case / "workload.baml", stage / "cases" / case / "workload.baml")
            emitted[case] = emit_case(cli, worktree, case, stage / "native" / case, guest_crates, shim_dir)
            print(f"  {case}: {emitted[case]['status']} {emitted[case].get('report') or emitted[case].get('reason', '')}")
    finally:
        shutil.rmtree(shim_dir, ignore_errors=True)
    archive_source(worktree, sha, stage)
    shutil.copytree(PACKAGE, stage / "bench", ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy(protocol.path, stage / "protocol.json")
    shutil.copy(machine.path, stage / "machine.json")
    meta = {"candidate_sha": sha, "candidate_dirty_files": dirty, "candidate_branch": git(worktree, "rev-parse", "--abbrev-ref", "HEAD"),
            "candidate_subject": git(worktree, "log", "-1", "--pretty=%s"), "emitter": str(cli),
            "emitter_version": subprocess.run([str(cli), "--version"], capture_output=True, text=True).stdout.strip(),
            "protocol": protocol.name, "protocol_sha256": protocol.sha256, "machine": machine.name, "machine_sha256": machine.sha256,
            "emitted": emitted, "staged_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    (stage / "stage.json").write_text(json.dumps(meta, indent=1))
    return meta
