"""Run persistence + baseline management.

Storage layout:
    ~/.speedtest/
      runs/<YYYYMMDD-HHMMSS-commit>/meta.json       # every run
      baselines/<branch>/latest/meta.json            # auto: most recent on branch
      baselines/<branch>/last/meta.json              # auto: previous on branch
      baselines/<branch>/<tag>/meta.json             # user: --tag <name>
      baselines/NO_BRANCH/latest/meta.json           # CLI not in a git repo
"""

import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def default_results_dir():
    return Path.home() / ".speedtest"


def results_dir(args):
    if hasattr(args, 'results_dir') and args.results_dir:
        return Path(args.results_dir).resolve()
    return default_results_dir()


def _git_info_for_path(path):
    """Return git info for the repo containing the given file path."""
    if not path or not os.path.exists(path):
        return {"commit": None, "branch": None, "message": None, "in_repo": False}

    cwd = os.path.dirname(os.path.abspath(path))

    def _run(cmd):
        try:
            return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd).stdout.strip()
        except Exception:
            return None

    if not _run(["git", "rev-parse", "--git-dir"]):
        return {"commit": None, "branch": None, "message": None, "in_repo": False}

    return {
        "commit": _run(["git", "rev-parse", "--short", "HEAD"]) or "unknown",
        "commit_full": _run(["git", "rev-parse", "HEAD"]) or "unknown",
        "branch": _run(["git", "rev-parse", "--abbrev-ref", "HEAD"]) or "unknown",
        "message": _run(["git", "log", "-1", "--pretty=%s"]) or "",
        "commit_date": _run(["git", "log", "-1", "--pretty=%aI"]) or "",
        "author": _run(["git", "log", "-1", "--pretty=%an"]) or "",
        "in_repo": True,
    }


def _cli_info(cli_path):
    """Get version, build time, and git info for a baml-cli binary."""
    if not cli_path or not os.path.isfile(cli_path):
        return {"path": None, "version": None, "built_at": None, "git": None}
    version = None
    try:
        r = subprocess.run([cli_path, "--version"], capture_output=True, text=True, timeout=5)
        if r.returncode == 0:
            version = r.stdout.strip()
    except Exception:
        pass
    try:
        mtime = os.path.getmtime(cli_path)
        built_at = datetime.fromtimestamp(mtime, tz=timezone.utc).isoformat()
    except Exception:
        built_at = None
    git = _git_info_for_path(cli_path)
    return {
        "path": os.path.abspath(cli_path),
        "version": version,
        "built_at": built_at,
        "git": git,
    }


def _branch_slug(cli_info):
    """Get the branch name for baseline storage. Returns 'NO_BRANCH' if not in a repo."""
    git = cli_info.get("git", {})
    if not git or not git.get("in_repo"):
        return "NO_BRANCH"
    branch = git.get("branch", "unknown")
    # Sanitize for filesystem
    return branch.replace("/", "_").replace(" ", "_")


def generate_run_id():
    now = datetime.now(timezone.utc)
    git = _git_info_for_path(os.getcwd())
    commit = git.get("commit", "unknown")
    return now.strftime("%Y%m%d-%H%M%S") + f"-{commit}"


def build_run_data(args, results, runners_used):
    """Build the complete meta.json — metadata + workload timings."""
    baml_cli = args.baml or ""

    workloads = []
    for row in results:
        entry = {
            "name": row["name"],
            "category": row["category"],
            "source": row.get("source", {}),
            "results": {},
        }
        for key in ("baml", "python", "node", "bun"):
            entry["results"][key] = row.get(key)
        workloads.append(entry)

    return {
        "id": generate_run_id(),
        "tag": getattr(args, 'tag', None),
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "filter": args.filter if args.filter else None,
        "runs_per_workload": args.runs if args.runs else f"adaptive ~{args.measurement_time}s",
        "runners": runners_used,
        "cli": _cli_info(baml_cli),
        "workloads": workloads,
    }


def _write_run(path, data):
    """Write meta.json + profiles to a directory."""
    path.mkdir(parents=True, exist_ok=True)
    with open(path / "meta.json", "w") as f:
        json.dump(data, f, indent=2)


def save_run(rdir, data, profile_files=None):
    """Save a run to runs/<id>/. Returns the path."""
    run_id = data["id"]
    run_path = rdir / "runs" / run_id
    _write_run(run_path, data)

    if profile_files:
        prof_dir = run_path / "profiles"
        prof_dir.mkdir(exist_ok=True)
        for src in profile_files:
            shutil.copy2(src, prof_dir / Path(src).name)

    return run_path


def save_baseline(rdir, run_path, data, tag=None):
    """Create baseline symlinks under baselines/<branch>/ pointing to the run.

    Always rotates latest -> last, symlinks current run as latest.
    If tag is provided, also creates baselines/<branch>/<tag> -> run.
    """
    cli_info = data.get("cli", {})
    branch = _branch_slug(cli_info)
    bl_dir = rdir / "baselines" / branch
    bl_dir.mkdir(parents=True, exist_ok=True)

    # Use relative symlink target so it works if ~/.speedtest moves
    run_rel = os.path.relpath(run_path, bl_dir)

    # Rotate: latest -> last
    latest_path = bl_dir / "latest"
    last_path = bl_dir / "last"
    if latest_path.exists() or latest_path.is_symlink():
        # Capture where latest points before removing
        if last_path.exists() or last_path.is_symlink():
            last_path.unlink()
        if latest_path.is_symlink():
            # Re-point last to where latest was pointing
            old_target = os.readlink(latest_path)
            last_path.symlink_to(old_target)
        latest_path.unlink()

    # Symlink latest -> run
    latest_path.symlink_to(run_rel)

    # Symlink tag -> run
    if tag:
        tag_path = bl_dir / tag
        if tag_path.exists() or tag_path.is_symlink():
            if tag_path.is_symlink():
                tag_path.unlink()
            else:
                shutil.rmtree(tag_path)
        tag_path.symlink_to(run_rel)

    return bl_dir


def resolve_ref(rdir, ref):
    """Resolve a reference like 'canary', 'canary/latest', 'canary/v1', or a run ID.

    Resolution order:
      1. baselines/<ref>/latest  (branch name -> latest on that branch)
      2. baselines/*/<ref>       (tag name -> search all branches)
      3. runs/<ref>              (exact run ID)
    Returns the loaded meta.json dict, or None.
    """
    # 1. Branch name -> latest
    p = rdir / "baselines" / ref / "latest" / "meta.json"
    if p.exists():
        with open(p) as f:
            return json.load(f)

    # 1b. Explicit branch/tag path
    if "/" in ref:
        p = rdir / "baselines" / ref / "meta.json"
        if p.exists():
            with open(p) as f:
                return json.load(f)

    # 2. Tag name -> search all branches
    bl_dir = rdir / "baselines"
    if bl_dir.exists():
        for branch_dir in sorted(bl_dir.iterdir()):
            if not branch_dir.is_dir():
                continue
            p = branch_dir / ref / "meta.json"
            if p.exists():
                with open(p) as f:
                    return json.load(f)

    # 3. Exact run ID
    p = rdir / "runs" / ref / "meta.json"
    if p.exists():
        with open(p) as f:
            return json.load(f)

    return None


def list_all(rdir):
    """Return list of (display_path, type, data) for all baselines + runs."""
    entries = []

    # Baselines: baselines/<branch>/<tag>/
    bl_dir = rdir / "baselines"
    if bl_dir.exists():
        for branch_dir in sorted(bl_dir.iterdir()):
            if not branch_dir.is_dir():
                continue
            for tag_dir in sorted(branch_dir.iterdir()):
                meta_path = tag_dir / "meta.json"
                if tag_dir.is_dir() and meta_path.exists():
                    with open(meta_path) as f:
                        data = json.load(f)
                    display = f"{branch_dir.name}/{tag_dir.name}"
                    entries.append((display, "baseline", data))

    # Runs
    runs_dir = rdir / "runs"
    if runs_dir.exists():
        for d in sorted(runs_dir.iterdir(), reverse=True):
            meta_path = d / "meta.json"
            if d.is_dir() and meta_path.exists():
                with open(meta_path) as f:
                    data = json.load(f)
                entries.append((d.name, "run", data))

    return entries


def cmd_baselines(args):
    """List all saved baselines and runs."""
    rdir = results_dir(args)
    entries = list_all(rdir)
    if not entries:
        print("No runs saved yet.")
        print(f"  (results dir: {rdir})")
        return

    print(f"{'type':<10s} {'ref':<40s} {'commit':<10s} {'workloads':>9s} {'timestamp':<22s}")
    print("-" * 91)
    for ref, typ, data in entries:
        cli = data.get("cli", {})
        git = cli.get("git", {})
        commit = git.get("commit", "?") if git else "?"
        ts = data.get("timestamp", "?")[:19]
        n_workloads = len(data.get("workloads", []))
        print(f"{typ:<10s} {ref:<40s} {commit:<10s} {n_workloads:>9d} {ts:<22s}")
