"""Workload loader.

A workload is a directory under ``workloads/<category>/<slug>/``:

    bench.toml   name, description, expected output, jobs and warmup per trial
    main.baml    the BAML program; ``main()`` performs one job
    main.py      the same program in Python; ``main()`` returns the same value
    main.js      the same program in JavaScript (node and bun)

``main()`` must be deterministic: every implementation returns the value in
``bench.toml``'s ``expect``, and every trial is checked against it.
"""

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SOURCES = {"baml": "main.baml", "python": "main.py", "js": "main.js"}
REQUIRED = ("name", "description", "expect")


def workloads_dir():
    return ROOT / "workloads"


def load_workload(path):
    """Load one workload directory. Raises ValueError on a malformed workload."""
    path = Path(path)
    meta_path = path / "bench.toml"
    with open(meta_path, "rb") as f:
        meta = tomllib.load(f)
    missing = [k for k in REQUIRED if k not in meta]
    if missing:
        raise ValueError(f"{meta_path}: missing {', '.join(missing)}")
    sources = {}
    for key, filename in SOURCES.items():
        src = path / filename
        if not src.exists():
            raise ValueError(f"{path}: missing {filename}")
        sources[key] = src.read_text()
    category = path.parent.name
    return {
        "id": f"{category}/{path.name}",
        # `category::name` is the stable identity: CodSpeed bench names and
        # saved baselines are derived from it, so renaming a workload breaks
        # its history.
        "name": f"{category}::{meta['name']}",
        "category": category,
        "description": meta["description"],
        "origin": meta.get("origin"),
        "expect": str(meta["expect"]),
        "jobs": int(meta.get("jobs", 1)),
        "warmup": int(meta.get("warmup", 1)),
        "path": path,
        "files": {key: path / filename for key, filename in SOURCES.items()},
        **sources,
    }


def load_workloads(directory=None):
    """Every workload under ``directory``, ordered by category then slug."""
    wdir = Path(directory) if directory else workloads_dir()
    if not wdir.is_dir():
        sys.exit(f"workloads directory not found: {wdir}")
    workloads = []
    for meta in sorted(wdir.glob("*/*/bench.toml")):
        try:
            workloads.append(load_workload(meta.parent))
        except (ValueError, tomllib.TOMLDecodeError) as e:
            sys.exit(f"invalid workload: {e}")
    names = [w["name"] for w in workloads]
    dupes = sorted({n for n in names if names.count(n) > 1})
    if dupes:
        sys.exit(f"duplicate workload names: {', '.join(dupes)}")
    return workloads


def select(workloads, filters):
    """Workloads whose id or name contains any of ``filters`` (case-insensitive)."""
    if not filters:
        return workloads
    fs = [f.lower() for f in filters]
    return [w for w in workloads if any(f in w["id"].lower() or f in w["name"].lower() for f in fs)]


def cmd_list(args=None):
    for w in load_workloads():
        print(f"{w['id']:45s} {w['description']}")
