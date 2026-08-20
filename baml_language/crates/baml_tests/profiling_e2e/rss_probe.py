#!/usr/bin/env python3
"""Measure one packed process's peak RSS without polluting wall benchmarks."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", default="")
    parser.add_argument("--cwd", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    if arguments.command[:1] == ["--"]:
        arguments.command = arguments.command[1:]
    if not arguments.command:
        parser.error("a command is required after --")
    return arguments


def main() -> int:
    arguments = parse_arguments()
    environment = dict(os.environ)
    for assignment in arguments.env:
        name, separator, value = assignment.partition("=")
        if not separator or not name:
            raise ValueError(f"invalid environment assignment: {assignment!r}")
        environment[name] = value

    started = time.perf_counter_ns()
    process = subprocess.Popen(
        arguments.command,
        cwd=arguments.cwd,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=arguments.timeout_seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
        process.kill()
        stdout, stderr = process.communicate()
    elapsed_seconds = (time.perf_counter_ns() - started) / 1_000_000_000

    # This probe launches exactly one child. On macOS ru_maxrss is bytes; on
    # Linux and the other supported Unix CI hosts it is KiB.
    peak_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    peak_rss_bytes = int(peak_rss if sys.platform == "darwin" else peak_rss * 1024)
    result = {
        "command": arguments.command,
        "elapsed_s": elapsed_seconds,
        "exit_code": process.returncode,
        "label": arguments.label,
        "peak_rss_bytes": peak_rss_bytes,
        "stderr_bytes": len(stderr),
        "stdout_bytes": len(stdout),
        "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
        "timed_out": timed_out,
    }
    print(json.dumps(result, sort_keys=True))
    if timed_out:
        return 124
    return process.returncode


if __name__ == "__main__":
    raise SystemExit(main())
