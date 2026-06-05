#!/usr/bin/env python3
"""Push a local Claude Code run into agent-tries-baml's pipeline.

Reads a Claude Code session `.jsonl` (the newest under ~/.claude/projects by
default) and POSTs it to the API's /ingest/run endpoint, which creates a task +
queued trophy so the full dedup -> issues -> Notion pipeline runs over it.

Usage:
    SERVICE_URL=http://localhost:8080 SERVICE_TOKEN=... \
        python scripts/upload_local_run.py --prompt "what I asked the agent"

Options:
    --prompt TEXT       The prompt/description for this run (required).
    --session PATH      Specific session .jsonl (default: newest under ~/.claude).
    --baml-version SHA  baml version the run used.
    --trophy-json PATH  Optional agent self-report (summary/findings/filesCreated).
    --source NAME       Task source label (default: local).
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.request
from pathlib import Path
from typing import Optional


def _newest_session() -> Optional[Path]:
    """Find the most recently modified Claude Code session log.

    Returns:
        The newest ``*.jsonl`` under ~/.claude/projects, or None when none exist.
    """
    root = Path.home() / ".claude" / "projects"
    if not root.is_dir():
        return None
    sessions = sorted(root.rglob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)
    return sessions[0] if sessions else None


def main() -> int:
    """Parse args, build the ingest payload, POST it, and print the run URL.

    Returns:
        Process exit code (0 on success, non-zero on a usage/config error).
    """
    ap = argparse.ArgumentParser(description="Upload a local Claude Code run to agent-tries-baml.")
    ap.add_argument("--prompt", required=True, help="Prompt/description for this run.")
    ap.add_argument("--session", help="Path to a session .jsonl (default: newest).")
    ap.add_argument("--baml-version", dest="baml_version", help="baml version sha.")
    ap.add_argument("--trophy-json", dest="trophy_json", help="Path to an agent self-report JSON.")
    ap.add_argument("--source", default="local", help="Task source label (default: local).")
    args = ap.parse_args()

    base = os.environ.get("SERVICE_URL")
    token = os.environ.get("SERVICE_TOKEN", "")
    if not base:
        print("error: SERVICE_URL is not set", file=sys.stderr)
        return 2

    session_path = Path(args.session) if args.session else _newest_session()
    if not session_path or not session_path.exists():
        print("error: no session .jsonl found (pass --session)", file=sys.stderr)
        return 2

    trophy_json = None
    if args.trophy_json:
        trophy_json = json.loads(Path(args.trophy_json).read_text())

    payload = {
        "prompt": args.prompt,
        "source": args.source,
        "bamlVersion": args.baml_version,
        "transcript": session_path.read_text(),
        "trophyJson": trophy_json,
    }
    req = urllib.request.Request(
        base.rstrip("/") + "/ingest/run",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"},
        method="POST",
    )
    print(f"uploading {session_path} …", file=sys.stderr)
    with urllib.request.urlopen(req) as resp:
        out = json.loads(resp.read().decode())
    print(out.get("runUrl") or json.dumps(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
