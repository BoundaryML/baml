"""Open subcommand — starts the API server + Vite dev server and opens browser."""

import os
import subprocess
import sys
import time
from pathlib import Path

from speedtest.storage import results_dir


def _ui_dir():
    """Find the typescript2/app-speedtest directory."""
    d = Path.cwd()
    while d != d.parent:
        candidate = d / "typescript2" / "app-speedtest"
        if candidate.exists():
            return candidate
        sibling = d.parent / "typescript2" / "app-speedtest"
        if sibling.exists():
            return sibling
        d = d.parent
    return None


def cmd_open(args):
    rdir = results_dir(args)
    if not rdir.exists():
        print(f"No results found at {rdir}", file=sys.stderr)
        print("Run some benchmarks first: speedtest run --build", file=sys.stderr)
        sys.exit(1)

    ui_dir = _ui_dir()
    if ui_dir is None:
        print("ERROR: typescript2/app-speedtest not found.", file=sys.stderr)
        sys.exit(1)

    env = {**os.environ, "SPEEDTEST_RESULTS_DIR": str(rdir)}

    # Start API server in background
    api_proc = subprocess.Popen(
        ["node", str(ui_dir / "api-server.mjs")],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    # Start Vite dev server (foreground — ctrl-c kills both)
    print(f"Starting speedtest UI...", file=sys.stderr)
    print(f"  Results: {rdir}", file=sys.stderr)
    print(f"  UI:      {ui_dir}", file=sys.stderr)

    try:
        # Give API server a moment to start
        time.sleep(0.5)

        # Open browser
        subprocess.Popen(["open", "http://localhost:3333"])

        # Run Vite in foreground
        subprocess.run(["pnpm", "dev"], cwd=str(ui_dir), env=env)
    finally:
        api_proc.terminate()
        api_proc.wait()
