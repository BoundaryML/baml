#!/usr/bin/env python3
"""Load the packed BAML server, pause, force major GC, and sample heap plus RSS."""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.request

import run as harness


def parse_heap_stats(body):
    values = {}
    for line in body.splitlines():
        key, raw = line.split("=", 1)
        prefix, field = key.split("_", 1)
        values.setdefault(prefix, {})[field] = int(raw)
    required = {"total_objects", "compile_time_objects", "runtime_objects", "active_handles", "tlab_chunks"}
    if set(values) != {"before", "after"} or any(set(stats) != required for stats in values.values()):
        raise RuntimeError(f"invalid /gc response: {body!r}")
    return values


def force_gc(port):
    started = time.monotonic()
    with urllib.request.urlopen(f"http://127.0.0.1:{port}/gc", timeout=10) as response:
        body = response.read().decode()
        python_collected = response.headers.get("X-Python-GC-Collected")
        if response.status != 200 or response.headers.get_content_type() != "text/plain" or response.headers.get("Cache-Control") != "no-store":
            raise RuntimeError(f"invalid /gc response metadata: {response.status} {response.headers}")
    return parse_heap_stats(body), time.monotonic() - started, int(python_collected) if python_collected is not None else None


def report_results(vegeta, files):
    completed = subprocess.run([vegeta, "report", "-type=json", *files], capture_output=True, text=True, check=True)
    return json.loads(completed.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=("baml-only", "python-baml"), default="baml-only", help="BAML host to measure")
    parser.add_argument("--rate", type=int, default=100, help="Requests per second during loaded periods")
    parser.add_argument("--duration", type=float, default=300, help="Total experiment duration before the final GC checkpoint")
    parser.add_argument("--load-seconds", type=float, default=30, help="Loaded seconds between explicit GC checkpoints")
    parser.add_argument("--post-gc-seconds", type=float, default=2, help="Idle observation time after each explicit GC")
    parser.add_argument("--interval", type=float, default=5, help="RSS sample interval during load")
    parser.add_argument("--results-dir", type=Path, help="Result directory; defaults to a timestamped directory under results")
    args = parser.parse_args()
    if args.rate <= 0 or args.duration <= 0 or args.load_seconds <= 0 or args.post_gc_seconds < 0 or args.interval <= 0:
        parser.error("rate, duration, load-seconds, and interval must be positive and post-gc-seconds must be nonnegative")

    manifest_path = harness.BUILD / "manifest.json"
    if not manifest_path.exists():
        parser.error("missing .build/manifest.json; run scripts/build.py first")
    manifest = json.loads(manifest_path.read_text())
    if not manifest.get("explicit_gc_diagnostic"):
        parser.error("the build does not include the explicit-GC diagnostic; rerun scripts/build.py with --explicit-gc-diagnostic")
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_dir = (args.results_dir or harness.ROOT / "results" / f"{timestamp}-{manifest['revision'][:12]}-{args.target}-explicit-gc-{args.rate}rps").resolve()
    result_dir.mkdir(parents=True, exist_ok=False)
    (result_dir / "logs").mkdir()
    (result_dir / "targets").mkdir()
    (result_dir / "attacks").mkdir()
    (result_dir / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    config = {
        "rate": args.rate,
        "duration_seconds": args.duration,
        "load_seconds": args.load_seconds,
        "post_gc_seconds": args.post_gc_seconds,
        "interval_seconds": args.interval,
        "target": args.target,
        "port": harness.PORTS[args.target],
    }
    (result_dir / "config.json").write_text(json.dumps(config, indent=2) + "\n")

    apps_root = harness.BUILD / "apps"
    if args.target == "baml-only":
        app_root = apps_root / "baml-only-explicit-gc"
        command = [app_root / "hello"]
    else:
        app_root = apps_root / "python-baml-explicit-gc"
        command = [harness.BUILD / "pyvenv/bin/python", "-m", "uvicorn", "server:app", "--host", "127.0.0.1", "--port", str(harness.PORTS[args.target]), "--no-access-log"]
    if not Path(command[0]).exists():
        parser.error(f"missing explicit-GC application command: {command[0]}")
    vegeta = harness.BUILD / "tools/vegeta"
    port = harness.PORTS[args.target]
    app_log = (result_dir / f"logs/{args.target}.log").open("wb")
    env = dict(os.environ, PORT=str(port), BAML_PROFILE="0", BAML_TELEMETRY_DISABLED="1", BAML_LOG="off")
    process = subprocess.Popen(list(map(str, command)), cwd=app_root, env=env, stdout=app_log, stderr=subprocess.STDOUT, start_new_session=True)
    processes = [process]
    logs = [app_log]
    attack_files = []
    attack_seconds = 0.0
    samples = []
    checkpoints = []
    started = time.monotonic()

    def rss_sample(phase, cycle):
        stats = harness.process_stats(process.pid)
        value = {
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "elapsed_seconds": time.monotonic() - started,
            "phase": phase,
            "cycle": cycle,
            "process": stats,
        }
        samples.append(value)
        sample_file.write(json.dumps(value) + "\n")
        sample_file.flush()
        print(f"t={value['elapsed_seconds'] / 60:.2f}m cycle={cycle} phase={phase} rss={stats.get('rss_bytes', 0) / 1048576:.2f}MiB", flush=True)
        if not stats.get("alive"):
            raise RuntimeError("packed BAML process exited")
        return value

    try:
        deadline = time.monotonic() + 20
        state = harness.probe(port)
        while not state["ok"] and time.monotonic() < deadline:
            time.sleep(0.25)
            state = harness.probe(port)
        if not state["ok"]:
            raise RuntimeError(f"{args.target} failed exact-response preflight: {state}")
        target = result_dir / f"targets/{args.target}.txt"
        target.write_text(f"GET http://127.0.0.1:{port}/\n")

        started = time.monotonic()
        end = started + args.duration
        cycle = 0
        with (result_dir / "samples.jsonl").open("w") as sample_file:
            while time.monotonic() < end:
                cycle += 1
                load_deadline = min(end, time.monotonic() + args.load_seconds)
                attack = result_dir / "attacks" / f"{args.target}-{cycle:03d}.gob"
                attack_files.append(attack)
                load_log = (result_dir / "logs" / f"load-{args.target}-{cycle:03d}.log").open("wb")
                logs.append(load_log)
                duration = max(0.001, load_deadline - time.monotonic())
                attack_seconds += duration
                load = subprocess.Popen(
                    [vegeta, "attack", f"-output={attack}", f"-rate={args.rate}/1s", f"-duration={duration}s", "-timeout=5s", f"-targets={target}"],
                    stdout=load_log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                )
                processes.append(load)
                next_sample = time.monotonic()
                while time.monotonic() < load_deadline:
                    now = time.monotonic()
                    if now >= next_sample:
                        rss_sample("load", cycle)
                        next_sample += args.interval
                    else:
                        time.sleep(min(load_deadline - now, next_sample - now, 0.25))
                load.wait(timeout=5)
                if load.returncode != 0:
                    raise RuntimeError(f"load process exited {load.returncode}")

                before = rss_sample("before_gc", cycle)
                heap, gc_seconds, python_objects_collected = force_gc(port)
                immediate = rss_sample("after_gc", cycle)
                delayed = []
                delay_started = time.monotonic()
                for delay in (0.1, 0.5, 1.0, args.post_gc_seconds):
                    if delay > args.post_gc_seconds or (delayed and delay == delayed[-1]["delay_seconds"]):
                        continue
                    time.sleep(max(0, delay_started + delay - time.monotonic()))
                    delayed.append({"delay_seconds": delay, "sample": rss_sample(f"after_gc_{delay:g}s", cycle)})
                checkpoints.append(
                    {
                        "cycle": cycle,
                        "elapsed_seconds": immediate["elapsed_seconds"],
                        "gc_elapsed_seconds": gc_seconds,
                        "python_objects_collected": python_objects_collected,
                        "heap": heap,
                        "rss_before_mib": before["process"]["rss_bytes"] / 1048576,
                        "rss_immediate_after_mib": immediate["process"]["rss_bytes"] / 1048576,
                        "rss_delayed": [
                            {"delay_seconds": item["delay_seconds"], "rss_mib": item["sample"]["process"]["rss_bytes"] / 1048576}
                            for item in delayed
                        ],
                    }
                )
    finally:
        elapsed = time.monotonic() - started
        harness.terminate(processes)
        for log in logs:
            log.close()

    report = report_results(vegeta, [str(path) for path in attack_files])
    statuses = report.get("status_codes", {})
    for checkpoint in checkpoints:
        runtime_object_change = checkpoint["heap"]["after"]["runtime_objects"] - checkpoint["heap"]["before"]["runtime_objects"]
        checkpoint["runtime_objects_change"] = runtime_object_change
        checkpoint["runtime_objects_collected"] = max(0, -runtime_object_change)
        checkpoint["rss_immediate_delta_mib"] = checkpoint["rss_immediate_after_mib"] - checkpoint["rss_before_mib"]
        checkpoint["rss_final_delta_mib"] = checkpoint["rss_delayed"][-1]["rss_mib"] - checkpoint["rss_before_mib"] if checkpoint["rss_delayed"] else checkpoint["rss_immediate_delta_mib"]
    summary = {
        "source_revision": manifest["revision"],
        "source_dirty": manifest["source_dirty"],
        "target": args.target,
        "elapsed_seconds": elapsed,
        "cycles": len(checkpoints),
        "scheduled_load_seconds": attack_seconds,
        "status_counts": statuses,
        "requests": report.get("requests"),
        "delivered_rps_during_load": sum(statuses.values()) / attack_seconds if attack_seconds else None,
        "alive_at_last_sample": samples[-1]["process"].get("alive", False) if samples else False,
        "checkpoints": checkpoints,
    }
    (result_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    bad = not summary["alive_at_last_sample"] or any(status != "200" and count for status, count in statuses.items()) or summary["delivered_rps_during_load"] is None or summary["delivered_rps_during_load"] < args.rate * 0.95
    if bad:
        return 1
    print(f"results: {result_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
