#!/usr/bin/env python3
"""Run native targets under a repeating on/off load profile."""

import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import time

import run as harness


def report_results(vegeta, files):
    completed = subprocess.run([vegeta, "report", "-type=json", *files], capture_output=True, text=True, check=True)
    return json.loads(completed.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rate", type=int, default=100, help="Requests per second sent independently to each target during on periods")
    parser.add_argument("--duration", type=float, default=300, help="Total wall-clock duration in seconds")
    parser.add_argument("--on-seconds", type=float, default=30, help="Length of each load-on period")
    parser.add_argument("--off-seconds", type=float, default=1, help="Length of each load-off period")
    parser.add_argument("--interval", type=float, default=5, help="RSS sample interval in seconds")
    parser.add_argument("--results-dir", type=Path, help="Result directory; defaults to a timestamped directory under results")
    args = parser.parse_args()
    if args.rate <= 0 or args.duration <= 0 or args.on_seconds <= 0 or args.off_seconds <= 0 or args.interval <= 0:
        parser.error("rate, duration, on-seconds, off-seconds, and interval must be positive")

    manifest_path = harness.BUILD / "manifest.json"
    if not manifest_path.exists():
        parser.error("missing .build/manifest.json; run scripts/build.py first")
    manifest = json.loads(manifest_path.read_text())
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_dir = (args.results_dir or harness.ROOT / "results" / f"{timestamp}-{manifest['revision'][:12]}-{args.rate}rps-{args.on_seconds:g}s-on-{args.off_seconds:g}s-off").resolve()
    result_dir.mkdir(parents=True, exist_ok=False)
    (result_dir / "logs").mkdir()
    (result_dir / "targets").mkdir()
    (result_dir / "attacks").mkdir()
    (result_dir / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    config = {
        "rate_per_target_during_on": args.rate,
        "aggregate_rate_during_on": args.rate * len(harness.PORTS),
        "duration_seconds": args.duration,
        "on_seconds": args.on_seconds,
        "off_seconds": args.off_seconds,
        "interval_seconds": args.interval,
        "ports": harness.PORTS,
    }
    (result_dir / "config.json").write_text(json.dumps(config, indent=2) + "\n")

    apps_root = harness.BUILD / "apps"
    py = harness.BUILD / "pyvenv/bin/python"
    node = Path(manifest["node_binary"])
    app_commands = {
        "node-baseline": [node, apps_root / "node-baseline/server.cjs"],
        "node-baml": [node, apps_root / "node-baml/server.js"],
        "python-baseline": [py, "-m", "uvicorn", "server:app", "--host", "127.0.0.1", "--port", "8503", "--no-access-log"],
        "python-baml": [py, "-m", "uvicorn", "server:app", "--host", "127.0.0.1", "--port", "8504", "--no-access-log"],
        "baml-only": [apps_root / "baml-only/hello"],
    }
    vegeta = harness.BUILD / "tools/vegeta"
    processes = []
    apps = {}
    logs = []
    attack_files = {name: [] for name in harness.PORTS}
    attack_seconds = {name: 0.0 for name in harness.PORTS}
    samples = []
    phases = []
    started = time.monotonic()
    next_sample = started

    def sample(phase):
        elapsed = time.monotonic() - started
        value = {
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "elapsed_seconds": elapsed,
            "phase": phase,
            "apps": {name: harness.process_stats(process.pid) for name, process in apps.items()},
            "probes": {name: harness.probe(port) for name, port in harness.PORTS.items()},
        }
        samples.append(value)
        sample_file.write(json.dumps(value) + "\n")
        sample_file.flush()
        rss = " ".join(f"{name}={stats.get('rss_bytes', 0) / 1048576:.1f}MiB" for name, stats in value["apps"].items())
        print(f"t={elapsed / 60:.1f}m phase={phase} {rss}", flush=True)
        if not all(stats["alive"] for stats in value["apps"].values()) or not all(state["ok"] for state in value["probes"].values()):
            raise RuntimeError("an app exited or failed an exact-response probe")

    def wait_with_samples(deadline, phase):
        nonlocal next_sample
        while True:
            now = time.monotonic()
            if now >= deadline:
                return
            if now >= next_sample:
                sample(phase)
                next_sample += args.interval
                continue
            time.sleep(min(deadline - now, next_sample - now, 0.25))

    try:
        for name, command in app_commands.items():
            log = (result_dir / "logs" / f"{name}.log").open("wb")
            logs.append(log)
            env = dict(os.environ, PORT=str(harness.PORTS[name]), BAML_PROFILE="0", BAML_TELEMETRY_DISABLED="1", BAML_LOG="off")
            process = subprocess.Popen(list(map(str, command)), cwd=apps_root / name, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            apps[name] = process
            processes.append(process)
        for name, port in harness.PORTS.items():
            deadline = time.monotonic() + 20
            state = harness.probe(port)
            while not state["ok"] and time.monotonic() < deadline:
                time.sleep(0.25)
                state = harness.probe(port)
            if not state["ok"]:
                raise RuntimeError(f"{name} failed exact-response preflight: {state}")
            target = result_dir / "targets" / f"{name}.txt"
            target.write_text(f"GET http://127.0.0.1:{port}/\n")

        started = time.monotonic()
        next_sample = started
        end = started + args.duration
        cycle = 0
        with (result_dir / "samples.jsonl").open("w") as sample_file:
            while time.monotonic() < end:
                cycle += 1
                on_started = time.monotonic()
                on_deadline = min(end, on_started + args.on_seconds)
                loads = []
                for name in harness.PORTS:
                    attack = result_dir / "attacks" / f"{name}-{cycle:03d}.gob"
                    attack_files[name].append(attack)
                    log = (result_dir / "logs" / f"load-{name}-{cycle:03d}.log").open("wb")
                    logs.append(log)
                    duration = max(0.001, on_deadline - time.monotonic())
                    attack_seconds[name] += duration
                    command = [vegeta, "attack", f"-output={attack}", f"-rate={args.rate}/1s", f"-duration={duration}s", "-timeout=5s", f"-targets={result_dir / 'targets' / f'{name}.txt'}"]
                    process = subprocess.Popen(list(map(str, command)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                    loads.append(process)
                    processes.append(process)
                phases.append({"cycle": cycle, "phase": "on", "started_seconds": on_started - started, "scheduled_end_seconds": on_deadline - started})
                wait_with_samples(on_deadline, "on")
                for process in loads:
                    process.wait(timeout=5)
                    if process.returncode != 0:
                        raise RuntimeError(f"load process exited {process.returncode}")
                on_ended = time.monotonic()
                phases[-1]["ended_seconds"] = on_ended - started
                if on_ended >= end:
                    break
                off_deadline = min(end, on_ended + args.off_seconds)
                phases.append({"cycle": cycle, "phase": "off", "started_seconds": on_ended - started, "scheduled_end_seconds": off_deadline - started})
                wait_with_samples(off_deadline, "off")
                phases[-1]["ended_seconds"] = time.monotonic() - started
    finally:
        elapsed = time.monotonic() - started
        harness.terminate(processes)
        for log in logs:
            log.close()

    reports = {name: report_results(vegeta, [str(path) for path in files]) for name, files in attack_files.items()}
    scheduled_on_seconds = sum(phase["scheduled_end_seconds"] - phase["started_seconds"] for phase in phases if phase["phase"] == "on")
    summary = {
        "source_revision": manifest["revision"],
        "source_dirty": manifest["source_dirty"],
        "rate_per_target_during_on": args.rate,
        "elapsed_seconds": elapsed,
        "scheduled_on_seconds": scheduled_on_seconds,
        "on_seconds": args.on_seconds,
        "off_seconds": args.off_seconds,
        "cycles_started": cycle,
        "phases": phases,
        "variants": {},
    }
    for name in harness.PORTS:
        points = [(value["elapsed_seconds"], value["apps"][name]["rss_bytes"]) for value in samples if value["elapsed_seconds"] >= 60 and value["apps"][name].get("alive")]
        statuses = reports[name].get("status_codes", {})
        summary["variants"][name] = {
            "rss_first_mib": samples[0]["apps"][name].get("rss_bytes", 0) / 1048576 if samples else None,
            "rss_last_mib": samples[-1]["apps"][name].get("rss_bytes", 0) / 1048576 if samples else None,
            "rss_slope_mib_per_minute_after_60s": harness.slope(points),
            "status_counts": statuses,
            "requests": reports[name].get("requests"),
            "attack_seconds": attack_seconds[name],
            "delivered_rps_during_on": sum(statuses.values()) / attack_seconds[name] if attack_seconds[name] else None,
            "alive_at_last_sample": samples[-1]["apps"][name].get("alive", False) if samples else False,
            "exact_probe_at_last_sample": samples[-1]["probes"][name].get("ok", False) if samples else False,
        }
    (result_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    bad = [name for name, state in summary["variants"].items() if not state["alive_at_last_sample"] or not state["exact_probe_at_last_sample"] or any(status != "200" and count for status, count in state["status_counts"].items()) or state["delivered_rps_during_on"] is None or state["delivered_rps_during_on"] < args.rate * 0.95]
    if bad:
        print(f"failed variants: {', '.join(bad)}", file=sys.stderr)
        return 1
    print(f"results: {result_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
