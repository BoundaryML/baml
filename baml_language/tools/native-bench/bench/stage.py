"""The controller side: turn a BAML checkout into a stage directory the machine can run."""
import json
import os
import shutil
import subprocess
import tarfile
import time
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent
ROOT = PACKAGE.parent
GLUE = PACKAGE / "native_glue.rs"


def git(worktree, *args):
    return subprocess.run(["git", "-C", str(worktree), *args], capture_output=True, text=True, check=True).stdout.strip()


def emit_case(cli, worktree, case_src, dest, guest_runtime_path):
    """Emit `run` with `prepare` for one case; the Cargo manifest points at the machine's runtime path."""
    project = dest / "project"
    (project / "baml_src").mkdir(parents=True)
    (project / "baml.toml").write_text('[package]\nname = "workload"\n')
    shutil.copy(case_src, project / "baml_src" / "workload.baml")
    env = {**os.environ, "BAML_AGENT_SKILL_CHECK": "off"}
    base = [str(cli), "--agent-skill-check", "off", "--no-progress", "--color", "never", "__emit-rust", "--project", str(project)]
    report = subprocess.run([*base, "--report"], capture_output=True, text=True, env=env)
    (dest / "report.txt").write_text(report.stdout + report.stderr)
    local_runtime = Path(worktree) / "baml_language" / "crates" / "bex_aot"
    r = subprocess.run([*base, "--function", "run", "--function", "prepare", "--crate-name", "baml_native",
                        "--out", str(dest / "emit"), "--runtime-path", str(local_runtime)], capture_output=True, text=True, env=env)
    (dest / "emit.log").write_text(r.stdout + r.stderr)
    if r.returncode != 0:
        shutil.rmtree(dest / "emit", ignore_errors=True)
        return {"status": "rejected", "reason": (r.stderr.strip().splitlines() or ["?"])[-1][:300]}
    shutil.copy(GLUE, dest / "emit" / "src" / "main.rs")
    toolchain = Path(worktree) / "baml_language" / "rust-toolchain.toml"
    if toolchain.exists():
        shutil.copy(toolchain, dest / "emit" / "rust-toolchain.toml")
    manifest = dest / "emit" / "Cargo.toml"
    text = manifest.read_text().replace(str(local_runtime.resolve()), guest_runtime_path).replace(str(local_runtime), guest_runtime_path)
    manifest.write_text(text)
    return {"status": "emitted", "report": report.stdout.strip().splitlines()[-1] if report.stdout.strip() else ""}


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
    guest_runtime = str(machine.bench_root / "src" / "baml_language" / "crates" / "bex_aot")
    emitted = {}
    for case in protocol.cases:
        (stage / "cases" / case).mkdir(parents=True)
        shutil.copy(ROOT / "cases" / case / "workload.baml", stage / "cases" / case / "workload.baml")
        emitted[case] = emit_case(cli, worktree, ROOT / "cases" / case / "workload.baml", stage / "native" / case, guest_runtime)
        print(f"  {case}: {emitted[case]['status']} {emitted[case].get('report') or emitted[case].get('reason', '')}")
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
