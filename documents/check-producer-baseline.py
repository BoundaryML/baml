#!/usr/bin/env python3
"""Check frozen producer files against the post-cloud-integration baseline.

Run from the worktree root. Optionally pass a different manifest path.
Exits 1 and lists every changed or missing file. The original phase-A manifest
is retained as historical evidence for the pre-#4958 implementation.
"""
import hashlib
import json
import sys
from pathlib import Path

manifest = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(
    "documents/btel-query-cloud-base-producer-baseline.json"
)
baseline = json.loads(manifest.read_text())
changed = []
for path, digest in sorted(baseline["files"].items()):
    file = Path(path)
    if not file.exists():
        changed.append(f"missing  {path}")
    elif hashlib.sha256(file.read_bytes()).hexdigest() != digest:
        changed.append(f"changed  {path}")
print(f"{len(baseline['files'])} frozen producer files, {len(changed)} differ")
for line in changed:
    print(line)
sys.exit(1 if changed else 0)
