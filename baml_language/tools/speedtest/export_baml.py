#!/usr/bin/env python3
"""Emit every speedtest workload's *expanded* BAML source as JSON.

This is the bridge that lets the Rust benchmark suite (crates/baml_tests)
reuse the workloads under tools/speedtest/workloads/ as CodSpeed benches
without duplicating the .md parsing / eval-setup / $$-templating logic that
already lives in speedtest.loader.

Output (stdout): a JSON array of {"name", "category", "baml"} objects.
Anything that can't be parsed is skipped (loader.py prints a WARN to stderr).

Usage:
    python3 export_baml.py [workloads_dir]

If workloads_dir is omitted, loader.py's default (../workloads) is used.
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "src"))

from speedtest.loader import load_workloads  # noqa: E402


def main() -> int:
    workloads_dir = sys.argv[1] if len(sys.argv) > 1 else None
    workloads = load_workloads(workloads_dir)
    out = [
        {"name": w["name"], "category": w["category"], "baml": w["baml"]}
        for w in workloads
    ]
    json.dump(out, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
