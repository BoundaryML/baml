#!/usr/bin/env python3
"""Short, paired comparison of two already-built btel_record_bench binaries.

Includes telemetry-off controls. Builds are deliberately separate. Raw results
stay in the requested local report; temporary recordings are removed. This is
a regression smoke check on a shared machine, not a significance test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tempfile
import time

ROOTS = {"tiny": 50000, "dense": 4096, "spawn": 2048,
         "capture-repeat": 25000, "capture-unique": 2048,
         "throw": 2048, "dictionary": 256}
METRICS = ("execution_s", "cpu_s", "cpu_including_drain_s", "call_p95_us",
           "drain_ms", "peak_rss_bytes", "recording_bytes", "cas_bytes",
           "startup_ms", "rss_before_execution_bytes")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def measure(args):
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists():
        raise RuntimeError("Output exists; choose a new report path")
    started = time.monotonic()
    deadline = started + args.budget_seconds
    rng = random.Random(args.seed)
    report = {"version": 1, "status": "running", "platform": platform.platform(),
              "load_start": os.getloadavg(), "repeats": args.repeats,
              "seed": args.seed, "roots": {w: ROOTS[w] for w in args.workloads},
              "payload_bytes": args.payload_bytes, "target_bytes": 1024 * 1024,
              "binaries": {name: {"path": str(binary), "sha256": digest(binary)}
                           for name, binary in (("before", args.before), ("after", args.after))},
              "samples": []}
    failure = None
    try:
        # Put scratch on the report's filesystem, not possibly-tmpfs /tmp.
        with tempfile.TemporaryDirectory(prefix="record-smoke-", dir=args.output.parent) as scratch:
            for workload in args.workloads:
                # Untimed warmups precede measured, interleaved pairs. The
                # same fixed work runs in both binaries and both modes.
                print(f"measuring {workload}", flush=True)
                for repetition in range(-1, args.repeats):
                    modes = ["off", "btel"]
                    rng.shuffle(modes)
                    for mode in modes:
                        versions = ["before", "after"]
                        rng.shuffle(versions)
                        for version in versions:
                            remaining = deadline - time.monotonic()
                            if remaining <= 0:
                                raise RuntimeError("Benchmark exhausted its time budget")
                            with tempfile.TemporaryDirectory(dir=scratch) as project:
                                command = [str(getattr(args, version)), mode, workload,
                                           str(ROOTS[workload]), project, str(args.payload_bytes),
                                           str(report["target_bytes"])]
                                child = subprocess.run(command, text=True, capture_output=True,
                                                       timeout=min(20, remaining), check=False,
                                                       env=dict(os.environ, BAML_TELEMETRY=(
                                                           "off" if mode == "off" else "medium")))
                                if child.returncode != 0:
                                    raise RuntimeError(f"Recorder failed: {command}\n{child.stderr}")
                                result = json.loads(child.stdout)
                            if mode == "off" and (result["recording_bytes"] or result["cas_bytes"]):
                                raise RuntimeError("Telemetry-off control wrote telemetry")
                            if mode == "btel" and not result["recording_files"]:
                                raise RuntimeError("Recording produced no sealed files")
                            report["samples"].append({"version": version, "repetition": repetition,
                                                      "warmup": repetition < 0, **result})
        report["status"] = "complete"
    except (Exception, KeyboardInterrupt) as error:
        failure = error
        report["status"] = "failed"
        report["failure"] = f"{type(error).__name__}: {error}"
    report["elapsed_seconds"] = time.monotonic() - started
    report["load_end"] = os.getloadavg()
    report["summary"] = []
    for workload in args.workloads:
        for mode in ("off", "btel"):
            row = {"workload": workload, "mode": mode, "metrics": {}}
            for metric in METRICS:
                values = {}
                for version in ("before", "after"):
                    samples = [s[metric] for s in report["samples"] if not s["warmup"]
                               and (s["workload"], s["mode"], s["version"]) == (workload, mode, version)]
                    if samples:
                        values[version] = {"median": statistics.median(samples),
                                           "min": min(samples), "max": max(samples), "n": len(samples)}
                if len(values) == 2 and values["before"]["median"]:
                    values["after_over_before"] = values["after"]["median"] / values["before"]["median"]
                row["metrics"][metric] = values
            report["summary"].append(row)
    with args.output.open("x") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    if failure:
        raise RuntimeError(f"{report['failure']}\nPartial results: {args.output}") from failure
    print("workload         mode    before ms [range]        after ms [range]         change")
    for row in report["summary"]:
        values = row["metrics"]["execution_s"]
        before, after = values["before"], values["after"]
        print(f"{row['workload']:<16} {row['mode']:<5} "
              f"{before['median'] * 1000:7.1f} [{before['min'] * 1000:.1f}, {before['max'] * 1000:.1f}] "
              f"{after['median'] * 1000:7.1f} [{after['min'] * 1000:.1f}, {after['max'] * 1000:.1f}] "
              f"{(values['after_over_before'] - 1) * 100:+.1f}%")
    print(f"Completed in {report['elapsed_seconds']:.1f}s; report: {args.output}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--workloads", nargs="+", choices=list(ROOTS), default=list(ROOTS))
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--payload-bytes", type=int, default=4096)
    parser.add_argument("--budget-seconds", type=float, default=120)
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()
    for name in ("before", "after", "output"):
        setattr(args, name, getattr(args, name).resolve())
    if args.repeats < 1 or args.budget_seconds <= 0 or args.payload_bytes < 0:
        parser.error("repeats/budget must be positive and payload-bytes nonnegative")
    for binary in (args.before, args.after):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            parser.error(f"Missing executable {binary}; build both release binaries first")
    try:
        measure(args)
    except RuntimeError as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
