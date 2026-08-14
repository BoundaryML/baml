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
import re
import shutil
import subprocess
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

    path = os.path.abspath(path)
    cwd = path if os.path.isdir(path) else os.path.dirname(path)

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


def _jj_info_for_path(path):
    """Return jj info for the repo containing the given file path."""
    if not path or not os.path.exists(path):
        return {"commit": None, "branch": None, "message": None, "in_repo": False}

    path = os.path.abspath(path)
    cwd = path if os.path.isdir(path) else os.path.dirname(path)

    def _run(args):
        try:
            return subprocess.run(
                ["jj", "--no-pager", *args],
                capture_output=True,
                text=True,
                cwd=cwd,
                timeout=5,
            )
        except Exception:
            return None

    root = _run(["root"])
    if not root or root.returncode != 0:
        return {"commit": None, "branch": None, "message": None, "in_repo": False}

    def _template(rev, expr):
        result = _run(["log", "-r", rev, "--no-graph", "--template", expr])
        if not result or result.returncode != 0:
            return ""
        return result.stdout.strip()

    def _is_empty_change(rev):
        result = _run(["diff", "-r", rev, "--summary"])
        return bool(result and result.returncode == 0 and not result.stdout.strip())

    rev = "@"
    description = _template(rev, "description.first_line()")
    parent_exists = _run(["log", "-r", "@-", "--no-graph", "--template", "commit_id.short()"])
    if not description and _is_empty_change(rev) and parent_exists and parent_exists.returncode == 0:
        rev = "@-"
        description = _template(rev, "description.first_line()")

    commit = _template(rev, "commit_id.short()")
    commit_full = _template(rev, "commit_id")
    change = _template(rev, "change_id.short()")
    change_full = _template(rev, "change_id")
    bookmarks = _template(rev, 'bookmarks.join(" ")')
    author = _template(rev, "author.name()")
    commit_date = _template(rev, "committer.timestamp()")

    # Prefer a bookmark on the current change. Anonymous jj changes fall back to
    # their change id so every run still gets a stable, human-usable baseline ref.
    bookmark = bookmarks.split()[0] if bookmarks else ""
    branch = bookmark or change or "unknown"

    return {
        "system": "jj",
        "commit": commit or "unknown",
        "commit_full": commit_full or "unknown",
        "change": change or "unknown",
        "change_full": change_full or "unknown",
        "bookmark": bookmark,
        "bookmarks": bookmarks.split() if bookmarks else [],
        "branch": branch,
        "message": description,
        "commit_date": commit_date,
        "author": author,
        "root": root.stdout.strip(),
        "in_repo": True,
    }


def _vcs_info_for_path(path):
    jj = _jj_info_for_path(path)
    if jj.get("in_repo"):
        return jj

    git = _git_info_for_path(path)
    if git.get("in_repo"):
        return {"system": "git", **git}

    return {"system": None, "commit": None, "branch": None, "message": None, "in_repo": False}


def _cli_info(cli_path):
    """Get version, build time, and VCS info for a baml-cli binary."""
    if not cli_path or not os.path.isfile(cli_path):
        return {"path": None, "version": None, "built_at": None, "git": None, "vcs": None}
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
    vcs = _vcs_info_for_path(cli_path)
    return {
        "path": os.path.abspath(cli_path),
        "version": version,
        "built_at": built_at,
        "git": vcs,
        "vcs": vcs,
    }


def _branch_slug(cli_info):
    """Get the current VCS ref name for baseline storage."""
    vcs = cli_info.get("vcs") or cli_info.get("git") or {}
    if not vcs or not vcs.get("in_repo"):
        return "NO_BRANCH"
    branch = vcs.get("branch") or vcs.get("bookmark") or vcs.get("change") or "unknown"
    return re.sub(r"[^A-Za-z0-9_.@-]+", "_", branch).strip("_") or "unknown"


def generate_run_id():
    now = datetime.now(timezone.utc)
    vcs = _vcs_info_for_path(os.getcwd())
    commit = vcs.get("commit", "unknown")
    return now.strftime("%Y%m%d-%H%M%S") + f"-{commit}"


def build_run_data(args, results, runners_used, *, baml_cli=None):
    """Build the complete meta.json — metadata + workload timings."""
    baml_cli = baml_cli or args.baml or ""

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
        vcs = cli.get("vcs") or cli.get("git") or {}
        commit = vcs.get("commit", "?") if vcs else "?"
        ts = data.get("timestamp", "?")[:19]
        n_workloads = len(data.get("workloads", []))
        print(f"{typ:<10s} {ref:<40s} {commit:<10s} {n_workloads:>9d} {ts:<22s}")
