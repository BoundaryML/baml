#!/usr/bin/env python3
"""Measure packed BAML before and after GC and macOS malloc pressure relief."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

import run as harness
import run_explicit_gc as explicit_gc


DIAGNOSTIC_BUILD = harness.BUILD / "malloc-pressure-relief"


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def capture(command, output_path, timeout=180):
    started = time.monotonic()
    with output_path.open("w") as output:
        try:
            completed = subprocess.run(list(map(str, command)), stdout=output, stderr=subprocess.STDOUT, text=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            output.write(f"\ncommand timed out after {timeout} seconds\n")
            return {
                "command": list(map(str, command)),
                "output": output_path.name,
                "timed_out": True,
                "elapsed_seconds": time.monotonic() - started,
            }
    return {
        "command": list(map(str, command)),
        "output": output_path.name,
        "returncode": completed.returncode,
        "elapsed_seconds": time.monotonic() - started,
    }


def invoke_pressure_relief(pid, result_path, timeout=10):
    before = result_path.read_text().splitlines()
    started = time.monotonic()
    os.kill(pid, signal.SIGUSR2)
    deadline = started + timeout
    while time.monotonic() < deadline:
        lines = result_path.read_text().splitlines()
        if len(lines) > len(before):
            match = re.fullmatch(r"relieved=(\d+)", lines[-1])
            if not match:
                raise RuntimeError(f"invalid pressure-relief result: {lines[-1]!r}")
            return {
                "signal": "SIGUSR2",
                "reported_bytes_relieved": int(match.group(1)),
                "elapsed_seconds": time.monotonic() - started,
            }
        time.sleep(0.01)
    raise RuntimeError(f"pressure-relief probe did not respond within {timeout} seconds")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rate", type=int, default=4000, help="Requests per second")
    parser.add_argument("--duration", type=float, default=300, help="Continuous loaded seconds")
    parser.add_argument("--interval", type=float, default=1, help="Heap and RSS sampling interval during load")
    parser.add_argument("--final-delay", type=float, default=30, help="Seconds to observe after pressure relief")
    parser.add_argument("--results-dir", type=Path, help="Result directory; defaults beneath results")
    args = parser.parse_args()
    if args.rate <= 0 or args.duration <= 0 or args.interval <= 0 or args.final_delay < 1:
        parser.error("rate, duration, and interval must be positive and final-delay must be at least one second")

    manifest_path = DIAGNOSTIC_BUILD / "manifest.json"
    if not manifest_path.exists():
        parser.error("missing .build/malloc-pressure-relief/manifest.json; run scripts/build_malloc_pressure_relief.py first")
    manifest = json.loads(manifest_path.read_text())

    source_executable = DIAGNOSTIC_BUILD / "apps/baml-only-explicit-gc/hello"
    vegeta = harness.BUILD / "tools/vegeta"
    for required in (source_executable, vegeta):
        if not required.exists():
            parser.error(f"missing required file: {required}")
    expected_executable = manifest.get("artifacts", {}).get("apps/baml-only-explicit-gc/hello")
    if digest(source_executable) != expected_executable:
        parser.error("packed diagnostic hash does not match .build/malloc-pressure-relief-manifest.json; rebuild it")

    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_dir = (args.results_dir or harness.ROOT / "results" / f"{timestamp}-{manifest['revision'][:12]}-pure-baml-malloc-pressure-relief-{args.rate}rps").resolve()
    result_dir.mkdir(parents=True, exist_ok=False)
    for directory in ("attacks", "logs", "targets"):
        (result_dir / directory).mkdir()
    (result_dir / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    config = {
        "rate": args.rate,
        "duration_seconds": args.duration,
        "interval_seconds": args.interval,
        "final_delay_seconds": args.final_delay,
        "target": "baml-only-explicit-gc",
        "port": harness.PORTS["baml-only"],
        "continuous_load": True,
        "explicit_gc_after_load": True,
        "malloc_pressure_relief_after_gc": True,
        "relief_signal": "SIGUSR2",
    }
    (result_dir / "config.json").write_text(json.dumps(config, indent=2) + "\n")

    probe_result = result_dir / "malloc-pressure-relief.txt"
    binary = {"executable_sha256": digest(source_executable)}

    port = harness.PORTS["baml-only"]
    target = result_dir / "targets/baml-only.txt"
    target.write_text(f"GET http://127.0.0.1:{port}/\n")
    attack = result_dir / "attacks/baml-only.gob"
    app_log = (result_dir / "logs/baml-only.log").open("wb")
    load_log = (result_dir / "logs/load-baml-only.log").open("wb")
    env = dict(
        os.environ,
        PORT=str(port),
        BAML_PROFILE="0",
        BAML_TELEMETRY_DISABLED="1",
        BAML_LOG="off",
        BAML_MALLOC_PRESSURE_RELIEF_RESULT=str(probe_result),
    )
    process = subprocess.Popen([source_executable], cwd=source_executable.parent, env=env, stdout=app_log, stderr=subprocess.STDOUT, start_new_session=True)
    processes = [process]
    samples = []
    captures = []
    checkpoint = {}
    load = None
    started = time.monotonic()

    with (result_dir / "samples.jsonl").open("w") as sample_file:
        def sample(phase, include_heap=True):
            stats = harness.process_stats(process.pid)
            heap = explicit_gc.sample_heap_stats(port) if include_heap and stats.get("alive") else None
            value = {
                "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                "elapsed_seconds": time.monotonic() - started,
                "phase": phase,
                "process": stats,
                "heap": heap,
            }
            samples.append(value)
            sample_file.write(json.dumps(value) + "\n")
            sample_file.flush()
            print(
                f"t={value['elapsed_seconds']:.2f}s phase={phase} rss={stats.get('rss_bytes', 0) / 1048576:.2f}MiB "
                + (
                    f"malloc_live={heap.get('allocator_bytes_in_use', 0) / 1048576:.2f}MiB "
                    f"malloc_reserved={heap.get('allocator_bytes_reserved', 0) / 1048576:.2f}MiB "
                    f"runtime_objects={heap.get('runtime_objects', 0)}"
                    if heap
                    else "heap=not-sampled"
                ),
                flush=True,
            )
            if not stats.get("alive"):
                raise RuntimeError("packed BAML process exited")
            return value

        def vmmap(label):
            output_path = result_dir / f"vmmap-{label}.txt"
            result = capture(["vmmap", "-summary", str(process.pid)], output_path)
            result["label"] = label
            captures.append(result)
            if result.get("returncode") != 0:
                raise RuntimeError(f"vmmap failed; inspect {output_path}")
            return result

        try:
            deadline = time.monotonic() + 30
            state = harness.probe(port)
            while not state["ok"] and time.monotonic() < deadline:
                time.sleep(0.25)
                state = harness.probe(port)
            if not state["ok"]:
                raise RuntimeError(f"baml-only failed exact-response preflight: {state}")
            if not probe_result.exists() or probe_result.read_text() != "ready\n":
                raise RuntimeError("malloc pressure-relief probe did not initialize")

            started = time.monotonic()
            load = subprocess.Popen(
                [vegeta, "attack", f"-output={attack}", f"-rate={args.rate}/1s", f"-duration={args.duration}s", "-timeout=5s", f"-targets={target}"],
                stdout=load_log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            processes.append(load)
            next_sample = started
            load_deadline = started + args.duration
            while time.monotonic() < load_deadline:
                now = time.monotonic()
                if now >= next_sample:
                    sample("load")
                    next_sample += args.interval
                else:
                    time.sleep(min(load_deadline - now, next_sample - now, 0.25))
            load.wait(timeout=15)
            if load.returncode != 0:
                raise RuntimeError(f"load process exited {load.returncode}")

            checkpoint["before_gc"] = sample("before_gc")
            vmmap("before-gc")
            heap, gc_seconds, _ = explicit_gc.force_gc(port)
            checkpoint["gc"] = {"elapsed_seconds": gc_seconds, "heap": heap}
            checkpoint["after_gc_immediate"] = sample("after_gc_immediate", include_heap=False)
            time.sleep(1)
            checkpoint["after_gc_1s"] = sample("after_gc_1s", include_heap=False)
            vmmap("after-gc")
            checkpoint["before_relief"] = sample("before_relief", include_heap=False)

            relief_started = time.monotonic()
            checkpoint["pressure_relief"] = invoke_pressure_relief(process.pid, probe_result)
            checkpoint["after_relief_immediate"] = sample("after_relief_immediate", include_heap=False)
            delayed = []
            for delay in (0.1, 0.5, 1.0, 5.0, args.final_delay):
                if delay > args.final_delay or (delayed and delay == delayed[-1]["delay_seconds"]):
                    continue
                time.sleep(max(0, relief_started + delay - time.monotonic()))
                delayed.append({"delay_seconds": delay, "sample": sample(f"after_relief_{delay:g}s", include_heap=False)})
            checkpoint["after_relief_delayed"] = delayed
            vmmap("after-relief")
            checkpoint["exact_response_after_relief"] = harness.probe(port)
            checkpoint["heap_after_measurement"] = sample("heap_after_measurement")
        finally:
            elapsed = time.monotonic() - started
            harness.terminate(processes)
            app_log.close()
            load_log.close()

    report = explicit_gc.report_results(vegeta, [str(attack)]) if attack.exists() and load and load.returncode == 0 else {}
    statuses = report.get("status_codes", {})
    before_relief_rss = checkpoint.get("before_relief", {}).get("process", {}).get("rss_bytes")
    immediate_relief_rss = checkpoint.get("after_relief_immediate", {}).get("process", {}).get("rss_bytes")
    delayed = checkpoint.get("after_relief_delayed", [])
    final_relief_rss = delayed[-1]["sample"]["process"]["rss_bytes"] if delayed else immediate_relief_rss
    summary = {
        "source_revision": manifest["revision"],
        "source_dirty": manifest["source_dirty"],
        "elapsed_seconds": elapsed,
        "requests": report.get("requests"),
        "status_counts": statuses,
        "delivered_rps": sum(statuses.values()) / args.duration if statuses else None,
        "binary": binary,
        "captures": captures,
        "checkpoint": checkpoint,
        "rss_immediate_relief_delta_mib": (immediate_relief_rss - before_relief_rss) / 1048576 if immediate_relief_rss is not None and before_relief_rss is not None else None,
        "rss_final_relief_delta_mib": (final_relief_rss - before_relief_rss) / 1048576 if final_relief_rss is not None and before_relief_rss is not None else None,
        "alive_after_relief": checkpoint.get("after_relief_immediate", {}).get("process", {}).get("alive", False),
        "exact_response_after_relief": checkpoint.get("exact_response_after_relief", {}).get("ok", False),
    }
    (result_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    bad = (
        not summary["alive_after_relief"]
        or not summary["exact_response_after_relief"]
        or any(status != "200" and count for status, count in statuses.items())
        or summary["delivered_rps"] is None
        or summary["delivered_rps"] < args.rate * 0.95
    )
    if bad:
        return 1
    print(f"results: {result_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
