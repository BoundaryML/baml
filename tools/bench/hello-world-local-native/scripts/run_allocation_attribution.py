#!/usr/bin/env python3
"""Attribute Python+BAML allocation growth across short load and GC cycles."""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import time
import tracemalloc
import urllib.parse
import urllib.request

import run as harness
import run_explicit_gc as explicit_gc


def request_text(port, path):
    with urllib.request.urlopen(f"http://127.0.0.1:{port}{path}", timeout=30) as response:
        body = response.read().decode()
        if response.status != 200 or response.headers.get_content_type() != "text/plain" or response.headers.get("Cache-Control") != "no-store":
            raise RuntimeError(f"invalid response metadata for {path}: {response.status} {response.headers}")
        return body, response.headers


def force_baml_gc(port):
    started = time.monotonic()
    body, _ = request_text(port, "/gc/baml")
    return explicit_gc.parse_heap_stats(body), time.monotonic() - started


def force_python_gc(port):
    started = time.monotonic()
    body, _ = request_text(port, "/gc/python")
    values = {}
    for line in body.splitlines():
        key, raw = line.split("=", 1)
        values[key] = int(raw)
    required = {"python_collected", "python_allocated_blocks_before", "python_allocated_blocks_after"}
    if not required.issubset(values):
        raise RuntimeError(f"invalid Python GC response: {body!r}")
    return values, time.monotonic() - started


def sample_allocation_stats(port):
    body, _ = request_text(port, "/stats/allocation")
    values = {}
    for line in body.splitlines():
        key, raw = line.split("=", 1)
        prefix, field = key.split("_", 1)
        if prefix != "sample":
            raise RuntimeError(f"invalid allocation stats key: {key!r}")
        values[field] = int(raw)
    required = {"allocator_blocks_in_use", "python_allocated_blocks", "rust_live_blocks", "bex_str_concat_nodes_live"}
    if not required.issubset(values):
        raise RuntimeError(f"invalid allocation stats response: {body!r}")
    return values


def native_thread_count(pid):
    completed = subprocess.run(["ps", "-M", "-p", str(pid)], capture_output=True, text=True)
    if completed.returncode != 0:
        return None
    return max(0, len(completed.stdout.splitlines()) - 1)


def take_snapshot(port, label):
    body, _ = request_text(port, f"/snapshot?{urllib.parse.urlencode({'label': label})}")
    values = {}
    for line in body.splitlines():
        key, raw = line.split("=", 1)
        values[key] = int(raw) if key == "traced_blocks" else raw
    return values


def capture_command(command, output_path, timeout=120):
    started = time.monotonic()
    with output_path.open("w") as output:
        try:
            completed = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, text=True, timeout=timeout)
            return {"command": list(map(str, command)), "output": output_path.name, "returncode": completed.returncode, "elapsed_seconds": time.monotonic() - started}
        except subprocess.TimeoutExpired:
            output.write(f"\ncommand timed out after {timeout} seconds\n")
            return {"command": list(map(str, command)), "output": output_path.name, "timed_out": True, "elapsed_seconds": time.monotonic() - started}


def traceback_record(stat):
    return {
        "count": stat.count,
        "count_diff": stat.count_diff,
        "size_bytes": stat.size,
        "size_diff_bytes": stat.size_diff,
        "traceback": [{"filename": frame.filename, "lineno": frame.lineno} for frame in stat.traceback],
    }


def analyze_tracemalloc(before_path, after_path, output_path):
    before = tracemalloc.Snapshot.load(str(before_path))
    after = tracemalloc.Snapshot.load(str(after_path))
    differences = after.compare_to(before, "traceback")
    positive_counts = sorted((stat for stat in differences if stat.count_diff > 0), key=lambda stat: (stat.count_diff, stat.size_diff), reverse=True)
    positive_sizes = sorted((stat for stat in differences if stat.size_diff > 0), key=lambda stat: (stat.size_diff, stat.count_diff), reverse=True)
    analysis = {
        "before_traced_blocks": len(before.traces),
        "after_traced_blocks": len(after.traces),
        "traced_blocks_change": len(after.traces) - len(before.traces),
        "top_positive_count_differences": [traceback_record(stat) for stat in positive_counts[:100]],
        "top_positive_size_differences": [traceback_record(stat) for stat in positive_sizes[:100]],
    }
    output_path.write_text(json.dumps(analysis, indent=2) + "\n")
    return analysis


def counter_delta(before, after, field):
    first = before["heap"].get(field)
    last = after["heap"].get(field)
    if first is None or last is None:
        return None
    return {"before": first, "after": last, "change": last - first}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rate", type=int, default=1000, help="Requests per second while load is on")
    parser.add_argument("--duration", type=float, default=20, help="Total on/off schedule duration")
    parser.add_argument("--load-seconds", type=float, default=2.5, help="Loaded seconds per cycle")
    parser.add_argument("--idle-seconds", type=float, default=0.5, help="Idle and split-GC seconds per cycle")
    parser.add_argument("--interval", type=float, default=0.5, help="Sampling interval while load is on")
    parser.add_argument("--final-quiescence-seconds", type=float, default=3, help="Additional no-load delay before final counters and captures")
    parser.add_argument("--malloc-stack-logging", action=argparse.BooleanOptionalAction, default=True, help="Enable macOS malloc stack logging")
    parser.add_argument("--results-dir", type=Path, help="Result directory; defaults beneath results")
    args = parser.parse_args()
    if args.rate <= 0 or args.duration <= 0 or args.load_seconds <= 0 or args.idle_seconds < 0 or args.interval <= 0 or args.final_quiescence_seconds < 0:
        parser.error("rate, duration, load-seconds, and interval must be positive and idle-seconds and final-quiescence-seconds must be nonnegative")

    manifest_path = harness.BUILD / "manifest.json"
    if not manifest_path.exists():
        parser.error("missing .build/manifest.json; run scripts/build.py first")
    manifest = json.loads(manifest_path.read_text())
    if not manifest.get("explicit_gc_diagnostic") or not manifest.get("allocation_profiling"):
        parser.error("the build lacks allocation instrumentation; rebuild with --explicit-gc-diagnostic")

    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_dir = (args.results_dir or harness.ROOT / "results" / f"{timestamp}-{manifest['revision'][:12]}-python-baml-allocation-attribution-{args.rate}rps").resolve()
    result_dir.mkdir(parents=True, exist_ok=False)
    for directory in ("attacks", "logs", "python-snapshots", "targets"):
        (result_dir / directory).mkdir()
    (result_dir / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    config = {
        "rate": args.rate,
        "duration_seconds": args.duration,
        "load_seconds": args.load_seconds,
        "idle_seconds": args.idle_seconds,
        "interval_seconds": args.interval,
        "final_quiescence_seconds": args.final_quiescence_seconds,
        "workload": "python-baml-allocation-attribution",
        "tracemalloc_frames": 10,
        "malloc_stack_logging": args.malloc_stack_logging,
        "split_baml_and_python_gc_each_idle_cycle": True,
        "port": harness.PORTS["python-baml"],
    }
    (result_dir / "config.json").write_text(json.dumps(config, indent=2) + "\n")

    app_root = harness.BUILD / "apps/python-baml-allocation-attribution"
    python = harness.BUILD / "pyvenv/bin/python"
    command = [python, "-m", "uvicorn", "server:app", "--host", "127.0.0.1", "--port", str(harness.PORTS["python-baml"]), "--no-access-log"]
    if not python.exists() or not app_root.exists():
        parser.error("missing allocation-attribution app; rebuild with --explicit-gc-diagnostic")

    port = harness.PORTS["python-baml"]
    vegeta = harness.BUILD / "tools/vegeta"
    target = result_dir / "targets/python-baml.txt"
    target.write_text(f"GET http://127.0.0.1:{port}/\n")
    app_log = (result_dir / "logs/python-baml.log").open("wb")
    env = dict(
        os.environ,
        PORT=str(port),
        BAML_PROFILE="0",
        BAML_TELEMETRY_DISABLED="1",
        BAML_LOG="off",
        BAML_ALLOCATION_SNAPSHOT_DIR=str(result_dir / "python-snapshots"),
        PYTHONTRACEMALLOC="10",
    )
    if args.malloc_stack_logging:
        env["MallocStackLogging"] = "1"
    process = subprocess.Popen(list(map(str, command)), cwd=app_root, env=env, stdout=app_log, stderr=subprocess.STDOUT, start_new_session=True)
    processes = [process]
    logs = [app_log]
    attack_files = []
    samples = []
    checkpoints = []
    captures = []
    scheduled_load_seconds = 0.0
    started = time.monotonic()

    try:
        deadline = time.monotonic() + 30
        state = harness.probe(port)
        while not state["ok"] and time.monotonic() < deadline:
            time.sleep(0.25)
            state = harness.probe(port)
        if not state["ok"]:
            raise RuntimeError(f"python-baml failed exact-response preflight: {state}")

        with (result_dir / "samples.jsonl").open("w") as sample_file:
            started = time.monotonic()

            def sample(phase, cycle):
                process_values = harness.process_stats(process.pid)
                process_values["native_threads"] = native_thread_count(process.pid) if process_values.get("alive") else None
                heap = sample_allocation_stats(port) if process_values.get("alive") else None
                value = {
                    "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "elapsed_seconds": time.monotonic() - started,
                    "phase": phase,
                    "cycle": cycle,
                    "process": process_values,
                    "heap": heap,
                }
                samples.append(value)
                sample_file.write(json.dumps(value) + "\n")
                sample_file.flush()
                print(
                    f"t={value['elapsed_seconds']:.3f}s cycle={cycle} phase={phase} "
                    f"rss={process_values.get('rss_bytes', 0) / 1048576:.2f}MiB "
                    f"malloc_blocks={heap.get('allocator_blocks_in_use', 0) if heap else 0} "
                    f"python_blocks={heap.get('python_allocated_blocks', 0) if heap else 0} "
                    f"rust_blocks={heap.get('rust_live_blocks', 0) if heap else 0} "
                    f"bex_str_nodes={(heap.get('bex_str_flat_nodes_live', 0) + heap.get('bex_str_concat_nodes_live', 0)) if heap else 0}",
                    flush=True,
                )
                if not process_values.get("alive"):
                    raise RuntimeError("python-baml process exited")
                return value

            initial_baml, initial_baml_seconds = force_baml_gc(port)
            initial_python, initial_python_seconds = force_python_gc(port)
            time.sleep(args.idle_seconds)
            baseline = sample("baseline_after_split_gc", 0)
            baseline_snapshot = take_snapshot(port, "baseline")
            captures.append(capture_command(["heap", "-zones", "-showSizes", "-sortBySize", str(process.pid)], result_dir / "heap-baseline.txt"))
            captures.append(capture_command(["vmmap", "-summary", str(process.pid)], result_dir / "vmmap-baseline.txt"))

            scheduled_elapsed = 0.0
            cycle = 0
            while scheduled_elapsed < args.duration:
                cycle += 1
                load_started = time.monotonic()
                duration = min(args.load_seconds, args.duration - scheduled_elapsed)
                load_deadline = load_started + duration
                scheduled_load_seconds += duration
                attack = result_dir / "attacks" / f"python-baml-{cycle:03d}.gob"
                attack_files.append(attack)
                load_log = (result_dir / "logs" / f"load-python-baml-{cycle:03d}.log").open("wb")
                logs.append(load_log)
                load = subprocess.Popen(
                    [vegeta, "attack", f"-output={attack}", f"-rate={args.rate}/1s", f"-duration={duration}s", "-timeout=5s", f"-targets={target}"],
                    stdout=load_log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                )
                processes.append(load)
                next_sample = load_started
                while time.monotonic() < load_deadline:
                    now = time.monotonic()
                    if now >= next_sample:
                        sample("load", cycle)
                        next_sample += args.interval
                    else:
                        time.sleep(min(load_deadline - now, next_sample - now, 0.05))
                load.wait(timeout=10)
                if load.returncode != 0:
                    raise RuntimeError(f"load process exited {load.returncode}")

                scheduled_elapsed += duration
                idle_duration = min(args.idle_seconds, args.duration - scheduled_elapsed)
                # Always allow a complete quiescence window for the measured
                # floor, including after a final partial load period.
                idle_deadline = time.monotonic() + args.idle_seconds
                before = sample("before_baml_gc", cycle)
                baml_report, baml_seconds = force_baml_gc(port)
                after_baml = sample("after_baml_gc", cycle)
                python_report, python_seconds = force_python_gc(port)
                after_python = sample("after_python_gc", cycle)
                if time.monotonic() < idle_deadline:
                    time.sleep(idle_deadline - time.monotonic())
                idle_floor = sample("idle_floor", cycle)
                scheduled_elapsed += idle_duration
                checkpoints.append(
                    {
                        "cycle": cycle,
                        "baml_gc_elapsed_seconds": baml_seconds,
                        "python_gc_elapsed_seconds": python_seconds,
                        "python_gc": python_report,
                        "baml_heap": baml_report,
                        "sample_indices": {
                            "before_baml_gc": samples.index(before),
                            "after_baml_gc": samples.index(after_baml),
                            "after_python_gc": samples.index(after_python),
                            "idle_floor": samples.index(idle_floor),
                        },
                    }
                )

            time.sleep(args.final_quiescence_seconds)
            final = sample("final_quiescent", cycle)
            final_snapshot = take_snapshot(port, "final")
            captures.append(capture_command(["heap", "-zones", "-showSizes", "-sortBySize", str(process.pid)], result_dir / "heap-final.txt"))
            captures.append(capture_command(["vmmap", "-summary", str(process.pid)], result_dir / "vmmap-final.txt"))
            captures.append(capture_command(["malloc_history", str(process.pid), "-callTree", "-invert", "-ignoreThreads"], result_dir / "malloc-history-final.txt", timeout=180))
            elapsed = time.monotonic() - started

        report = explicit_gc.report_results(vegeta, [str(path) for path in attack_files])
        statuses = report.get("status_codes", {})
        trace_analysis = analyze_tracemalloc(
            result_dir / "python-snapshots/baseline.tracemalloc",
            result_dir / "python-snapshots/final.tracemalloc",
            result_dir / "tracemalloc-diff.json",
        )
        fields = [
            "allocator_blocks_in_use",
            "allocator_bytes_in_use",
            "allocator_bytes_reserved",
            "python_allocated_blocks",
            "python_tracemalloc_current_bytes",
            "python_tracemalloc_overhead_bytes",
            "rust_alloc_calls_total",
            "rust_realloc_calls_total",
            "rust_dealloc_calls_total",
            "rust_allocated_bytes_total",
            "rust_deallocated_bytes_total",
            "rust_live_blocks",
            "rust_live_bytes",
            "rust_live_blocks_le_16",
            "rust_live_blocks_17_32",
            "rust_live_blocks_33_64",
            "bex_str_flat_nodes_live",
            "bex_str_flat_nodes_created_total",
            "bex_str_flat_nodes_dropped_total",
            "bex_str_flat_payloads_live",
            "bex_str_flat_payload_bytes_live",
            "bex_str_concat_nodes_live",
            "bex_str_concat_nodes_created_total",
            "bex_str_concat_nodes_dropped_total",
            "pyo3_live_handles",
        ]
        deltas = {field: counter_delta(baseline, final, field) for field in fields}
        requests = report.get("requests") or 0
        for value in deltas.values():
            if value is not None:
                value["change_per_request"] = value["change"] / requests if requests else None
        summary = {
            "source_revision": manifest["revision"],
            "source_dirty": manifest["source_dirty"],
            "elapsed_seconds": elapsed,
            "scheduled_duration_seconds": args.duration,
            "scheduled_load_seconds": scheduled_load_seconds,
            "cycles": len(checkpoints),
            "requests": requests,
            "status_counts": statuses,
            "delivered_rps_during_load": sum(statuses.values()) / scheduled_load_seconds if scheduled_load_seconds else None,
            "initial_gc": {
                "baml_elapsed_seconds": initial_baml_seconds,
                "python_elapsed_seconds": initial_python_seconds,
                "python": initial_python,
                "baml_heap": initial_baml,
            },
            "baseline_snapshot": baseline_snapshot,
            "final_snapshot": final_snapshot,
            "counter_deltas": deltas,
            "tracemalloc": {
                "before_traced_blocks": trace_analysis["before_traced_blocks"],
                "after_traced_blocks": trace_analysis["after_traced_blocks"],
                "traced_blocks_change": trace_analysis["traced_blocks_change"],
            },
            "checkpoints": checkpoints,
            "external_captures": captures,
            "alive_at_last_sample": final["process"].get("alive", False),
            "process_deltas": {
                "rss_bytes": final["process"]["rss_bytes"] - baseline["process"]["rss_bytes"],
                "native_threads": final["process"]["native_threads"] - baseline["process"]["native_threads"],
            },
        }
        (result_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps(summary, indent=2))
        bad = not summary["alive_at_last_sample"] or any(status != "200" and count for status, count in statuses.items()) or summary["delivered_rps_during_load"] is None or summary["delivered_rps_during_load"] < args.rate * 0.95
        if bad:
            return 1
        print(f"results: {result_dir}")
        return 0
    finally:
        harness.terminate(processes)
        for log in logs:
            log.close()


if __name__ == "__main__":
    raise SystemExit(main())
