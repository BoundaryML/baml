#!/usr/bin/env python3
"""Run all five native targets, apply independent load, and sample process RSS."""

import argparse
import datetime
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / ".build"
PORTS = {
    "node-baseline": 8501,
    "node-baml": 8502,
    "python-baseline": 8503,
    "python-baml": 8504,
    "baml-only": 8505,
}
METRICS_PORTS = {name: 8611 + index for index, name in enumerate(PORTS)}


def process_stats(pid):
    completed = subprocess.run(["ps", "-o", "rss=,pcpu=,etime=", "-p", str(pid)], capture_output=True, text=True)
    if completed.returncode != 0 or not completed.stdout.strip():
        return {"alive": False}
    rss, pcpu, elapsed = completed.stdout.strip().split(None, 2)
    return {"alive": True, "rss_bytes": int(rss) * 1024, "cpu_percent": float(pcpu), "elapsed": elapsed}


def scrape(port):
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/metrics", timeout=2) as response:
            body = response.read().decode()
    except Exception as exc:
        return {"available": False, "error": str(exc), "status_counts": {}}
    counts = {}
    for line in body.splitlines():
        if not line.startswith("request_seconds_count{"):
            continue
        status = re.search(r'status="([^"]+)"', line)
        if status:
            counts[status.group(1)] = float(line.rsplit(" ", 1)[1])
    return {"available": True, "status_counts": counts}


def probe(port):
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=2) as response:
            body = response.read()
            return {
                "ok": response.status == 200 and body == b"hello world" and response.headers.get("Cache-Control") == "no-store" and response.headers.get_content_type() == "text/plain",
                "status": response.status,
                "body": body.decode(errors="replace"),
                "content_type": response.headers.get("Content-Type"),
                "cache_control": response.headers.get("Cache-Control"),
            }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


def slope(points):
    if len(points) < 2:
        return None
    x_mean = sum(x for x, _ in points) / len(points)
    y_mean = sum(y for _, y in points) / len(points)
    denominator = sum((x - x_mean) ** 2 for x, _ in points)
    if denominator == 0:
        return None
    bytes_per_second = sum((x - x_mean) * (y - y_mean) for x, y in points) / denominator
    return bytes_per_second * 60 / 1048576


def terminate(processes):
    for process in reversed(processes):
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
    deadline = time.monotonic() + 5
    for process in processes:
        if process.poll() is None:
            try:
                process.wait(timeout=max(0.1, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rate", type=int, default=300, help="Requests per second sent independently to each target")
    parser.add_argument("--duration", type=float, default=300, help="Run duration in seconds; zero runs until interrupted")
    parser.add_argument("--interval", type=float, default=15, help="RSS sample interval in seconds")
    parser.add_argument("--results-dir", type=Path, help="Result directory; defaults to a timestamped directory under results")
    args = parser.parse_args()
    if args.rate <= 0 or args.duration < 0 or args.interval <= 0:
        parser.error("rate and interval must be positive and duration must be nonnegative")
    manifest_path = BUILD / "manifest.json"
    if not manifest_path.exists():
        parser.error("missing .build/manifest.json; run scripts/build.py first")
    manifest = json.loads(manifest_path.read_text())
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_dir = (args.results_dir or ROOT / "results" / f"{timestamp}-{manifest['revision'][:12]}-{args.rate}rps").resolve()
    result_dir.mkdir(parents=True, exist_ok=False)
    (result_dir / "logs").mkdir()
    (result_dir / "targets").mkdir()
    (result_dir / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    config = {"rate_per_target": args.rate, "aggregate_rate": args.rate * len(PORTS), "duration_seconds": args.duration, "interval_seconds": args.interval, "ports": PORTS, "metrics_ports": METRICS_PORTS}
    (result_dir / "config.json").write_text(json.dumps(config, indent=2) + "\n")

    apps_root = BUILD / "apps"
    py = BUILD / "pyvenv/bin/python"
    node = Path(manifest["node_binary"])
    app_commands = {
        "node-baseline": [node, apps_root / "node-baseline/server.cjs"],
        "node-baml": [node, apps_root / "node-baml/server.js"],
        "python-baseline": [py, "-m", "uvicorn", "server:app", "--host", "127.0.0.1", "--port", "8503", "--no-access-log"],
        "python-baml": [py, "-m", "uvicorn", "server:app", "--host", "127.0.0.1", "--port", "8504", "--no-access-log"],
        "baml-only": [apps_root / "baml-only/hello"],
    }
    processes = []
    apps = {}
    loads = {}
    logs = []
    interrupted = False
    started = time.monotonic()
    samples = []
    try:
        for name, command in app_commands.items():
            log = (result_dir / "logs" / f"{name}.log").open("wb")
            logs.append(log)
            env = dict(os.environ, PORT=str(PORTS[name]), BAML_PROFILE="0", BAML_TELEMETRY_DISABLED="1", BAML_LOG="off")
            process = subprocess.Popen(list(map(str, command)), cwd=apps_root / name, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            apps[name] = process
            processes.append(process)
        for name, port in PORTS.items():
            deadline = time.monotonic() + 20
            state = probe(port)
            while not state["ok"] and time.monotonic() < deadline:
                time.sleep(0.25)
                state = probe(port)
            if not state["ok"]:
                raise RuntimeError(f"{name} failed exact-response preflight: {state}")

        for name, port in PORTS.items():
            target = result_dir / "targets" / f"{name}.txt"
            target.write_text(f"GET http://127.0.0.1:{port}/\n")
            log = (result_dir / "logs" / f"load-{name}.log").open("wb")
            logs.append(log)
            command = [BUILD / "tools/vegeta", "attack", f"-prometheus-addr=127.0.0.1:{METRICS_PORTS[name]}", "-output=/dev/null", f"-rate={args.rate}/1s", "-duration=0s", "-timeout=5s", f"-targets={target}"]
            process = subprocess.Popen(list(map(str, command)), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            loads[name] = process
            processes.append(process)

        started = time.monotonic()
        with (result_dir / "samples.jsonl").open("w") as sample_file:
            while args.duration == 0 or time.monotonic() - started < args.duration:
                elapsed = time.monotonic() - started
                sample = {
                    "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "elapsed_seconds": elapsed,
                    "apps": {name: process_stats(process.pid) for name, process in apps.items()},
                    "loads": {name: scrape(METRICS_PORTS[name]) for name in loads},
                    "probes": {name: probe(PORTS[name]) for name in apps},
                }
                samples.append(sample)
                sample_file.write(json.dumps(sample) + "\n")
                sample_file.flush()
                rss = " ".join(f"{name}={stats.get('rss_bytes', 0) / 1048576:.1f}MiB" for name, stats in sample["apps"].items())
                print(f"t={elapsed / 60:.1f}m {rss}", flush=True)
                if not all(stats["alive"] for stats in sample["apps"].values()) or not all(state["ok"] for state in sample["probes"].values()):
                    break
                time.sleep(min(args.interval, max(0, args.duration - elapsed)) if args.duration else args.interval)
    except KeyboardInterrupt:
        interrupted = True
    finally:
        elapsed = time.monotonic() - started
        final_load = {name: scrape(METRICS_PORTS[name]) for name in loads}
        terminate(processes)
        for log in logs:
            log.close()

    summary = {
        "source_revision": manifest["revision"],
        "source_dirty": manifest["source_dirty"],
        "rate_per_target": args.rate,
        "elapsed_seconds": elapsed,
        "interrupted": interrupted,
        "variants": {},
    }
    for name in PORTS:
        points = [(sample["elapsed_seconds"], sample["apps"][name]["rss_bytes"]) for sample in samples if sample["elapsed_seconds"] >= 60 and sample["apps"][name].get("alive")]
        statuses = final_load.get(name, {}).get("status_counts", {})
        summary["variants"][name] = {
            "rss_first_mib": samples[0]["apps"][name].get("rss_bytes", 0) / 1048576 if samples else None,
            "rss_last_mib": samples[-1]["apps"][name].get("rss_bytes", 0) / 1048576 if samples else None,
            "rss_slope_mib_per_minute_after_60s": slope(points),
            "status_counts": statuses,
            "delivered_rps": sum(statuses.values()) / elapsed if elapsed else None,
            "alive_at_last_sample": samples[-1]["apps"][name].get("alive", False) if samples else False,
            "exact_probe_at_last_sample": samples[-1]["probes"][name].get("ok", False) if samples else False,
        }
    (result_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    bad = [name for name, state in summary["variants"].items() if not state["alive_at_last_sample"] or not state["exact_probe_at_last_sample"] or any(status != "200" and count for status, count in state["status_counts"].items()) or state["delivered_rps"] is None or state["delivered_rps"] < args.rate * 0.95]
    if bad:
        print(f"failed variants: {', '.join(bad)}", file=sys.stderr)
        return 1
    print(f"results: {result_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
