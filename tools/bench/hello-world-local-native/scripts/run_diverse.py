#!/usr/bin/env python3
"""Run fresh-process pure-BAML and Python-BAML workload tests on macOS."""

import argparse
import base64
import ctypes
import datetime
import hashlib
import json
import os
import re
import shutil
import signal
import statistics
import subprocess
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / ".build"
PORTS = {"pure-baml": 8505, "python-baml": 8504}
METRICS_PORT = 8625
WORKLOAD_PATHS = {
    "hello": "/hello",
    "json-64k": "/json-64k",
    "json-1m": "/json-1m",
    "image": "/image",
}


class RUsageInfoV0(ctypes.Structure):
    _fields_ = [
        ("ri_uuid", ctypes.c_uint8 * 16),
        ("ri_user_time", ctypes.c_uint64),
        ("ri_system_time", ctypes.c_uint64),
        ("ri_pkg_idle_wkups", ctypes.c_uint64),
        ("ri_interrupt_wkups", ctypes.c_uint64),
        ("ri_pageins", ctypes.c_uint64),
        ("ri_wired_size", ctypes.c_uint64),
        ("ri_resident_size", ctypes.c_uint64),
        ("ri_phys_footprint", ctypes.c_uint64),
        ("ri_proc_start_abstime", ctypes.c_uint64),
        ("ri_proc_exit_abstime", ctypes.c_uint64),
    ]


class MachTimebaseInfo(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


LIBPROC = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
LIBPROC.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
LIBPROC.proc_pid_rusage.restype = ctypes.c_int
LIBSYSTEM = ctypes.CDLL(None)
LIBSYSTEM.mach_timebase_info.argtypes = [ctypes.POINTER(MachTimebaseInfo)]
LIBSYSTEM.mach_timebase_info.restype = ctypes.c_int
MACH_TIMEBASE = MachTimebaseInfo()
if (
    LIBSYSTEM.mach_timebase_info(ctypes.byref(MACH_TIMEBASE)) != 0
    or MACH_TIMEBASE.denom == 0
):
    raise RuntimeError("mach_timebase_info failed")
MACH_TICK_NANOSECONDS = MACH_TIMEBASE.numer / MACH_TIMEBASE.denom


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def process_stats(pid, previous=None, wall_seconds=None):
    info = RUsageInfoV0()
    if LIBPROC.proc_pid_rusage(pid, 0, ctypes.byref(info)) != 0:
        return {"alive": False, "errno": ctypes.get_errno()}
    completed = subprocess.run(
        ["ps", "-o", "pcpu=,etime=", "-p", str(pid)],
        capture_output=True,
        text=True,
        check=False,
    )
    result = {
        "alive": completed.returncode == 0 and bool(completed.stdout.strip()),
        "ri_user_time_mach_ticks": info.ri_user_time,
        "ri_system_time_mach_ticks": info.ri_system_time,
        "ri_resident_size_bytes": info.ri_resident_size,
        "ri_phys_footprint_bytes": info.ri_phys_footprint,
    }
    if completed.stdout.strip():
        pcpu, elapsed = completed.stdout.strip().split(None, 1)
        result["ps_cpu_percent"] = float(pcpu)
        result["process_elapsed"] = elapsed
    if previous is not None and wall_seconds and wall_seconds > 0:
        cpu_delta_ticks = (info.ri_user_time - previous["ri_user_time_mach_ticks"]) + (
            info.ri_system_time - previous["ri_system_time_mach_ticks"]
        )
        result["cpu_percent"] = (
            cpu_delta_ticks
            * MACH_TICK_NANOSECONDS
            / (wall_seconds * 1_000_000_000)
            * 100
        )
    else:
        result["cpu_percent"] = None
    return result


def scrape_metrics():
    try:
        with urllib.request.urlopen(
            f"http://127.0.0.1:{METRICS_PORT}/metrics", timeout=5
        ) as response:
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


def probe(port, path, expected, timeout=15):
    try:
        with urllib.request.urlopen(
            f"http://127.0.0.1:{port}{path}", timeout=timeout
        ) as response:
            body = response.read()
            content_type = response.headers.get("Content-Type", "")
            return {
                "ok": response.status == 200
                and body == expected["body"]
                and content_type.startswith(expected["content_type"])
                and response.headers.get("Cache-Control") == "no-store",
                "status": response.status,
                "body_bytes": len(body),
                "body_sha256": sha256_bytes(body),
                "content_type": content_type,
                "cache_control": response.headers.get("Cache-Control"),
            }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


def expected_responses(image_path):
    image_base64 = base64.b64encode(image_path.read_bytes())
    values = {}
    values["hello"] = {"body": b"Hello World", "content_type": "text/plain"}
    for workload, size, payload_size in (
        ("json-64k", 65536, 65467),
        ("json-1m", 1048576, 1048508),
    ):
        value = {
            "workload": workload,
            "nested": {"level1": {"level2": {"payload": "x" * payload_size}}},
        }
        body = json.dumps(value, separators=(",", ":")).encode()
        if len(body) != size:
            raise RuntimeError(
                f"{workload} fixture is {len(body)} bytes, expected {size}"
            )
        values[workload] = {"body": body, "content_type": "application/json"}
    values["image"] = {"body": image_base64, "content_type": "text/plain"}
    return values


def terminate(process):
    if process is None or process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


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


def percentile(values, fraction):
    ordered = sorted(values)
    if not ordered:
        return None
    return ordered[round((len(ordered) - 1) * fraction)]


def series_summary(samples, field, warmup_seconds):
    points = [
        (sample["elapsed_seconds"], sample["process"][field])
        for sample in samples
        if sample["process"].get(field) is not None
    ]
    warm = [(x, y) for x, y in points if x >= warmup_seconds]
    values = [y for _, y in points]
    return {
        "first_mib": values[0] / 1048576 if values else None,
        "last_mib": values[-1] / 1048576 if values else None,
        "minimum_mib": min(values) / 1048576 if values else None,
        "maximum_mib": max(values) / 1048576 if values else None,
        "slope_mib_per_minute_after_warmup": slope(warm),
    }


def run_one(target, workload, args, expected, result_root):
    run_dir = result_root / "runs" / f"{target}-{workload}"
    run_dir.mkdir(parents=True)
    target_file = run_dir / "target.txt"
    target_file.write_text(
        f"GET http://127.0.0.1:{PORTS[target]}{WORKLOAD_PATHS[workload]}\n"
    )
    app_root = (
        BUILD
        / "apps"
        / ("diverse-baml-only" if target == "pure-baml" else "diverse-python-baml")
    )
    if target == "pure-baml":
        command = [app_root / "server"]
    else:
        command = [
            BUILD / "pyvenv/bin/python",
            "-m",
            "uvicorn",
            "server:app",
            "--host",
            "127.0.0.1",
            "--port",
            str(PORTS[target]),
            "--no-access-log",
        ]
    env = dict(
        os.environ, BAML_PROFILE="0", BAML_TELEMETRY_DISABLED="1", BAML_LOG="off"
    )
    server_log = (run_dir / "server.log").open("wb")
    load_log = (run_dir / "load.log").open("wb")
    server = None
    load = None
    samples = []
    final_metrics = {"status_counts": {}}
    postflight = {"ok": False, "error": "not run"}
    elapsed = 0.0
    stopped_reason = None
    try:
        server = subprocess.Popen(
            list(map(str, command)),
            cwd=app_root,
            env=env,
            stdout=server_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        deadline = time.monotonic() + 30
        preflight = probe(PORTS[target], WORKLOAD_PATHS[workload], expected)
        while not preflight["ok"] and time.monotonic() < deadline:
            time.sleep(0.25)
            preflight = probe(PORTS[target], WORKLOAD_PATHS[workload], expected)
        if not preflight["ok"]:
            raise RuntimeError(f"{target}/{workload} preflight failed: {preflight}")
        load_command = [
            BUILD / "tools/vegeta",
            "attack",
            f"-prometheus-addr=127.0.0.1:{METRICS_PORT}",
            "-output=/dev/null",
            f"-rate={args.rate}/1s",
            "-duration=0s",
            f"-timeout={args.request_timeout}s",
            f"-targets={target_file}",
        ]
        load = subprocess.Popen(
            list(map(str, load_command)),
            stdout=load_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        started = time.monotonic()
        previous = None
        previous_at = None
        with (run_dir / "samples.jsonl").open("w") as sample_file:
            while True:
                sample_at = time.monotonic()
                elapsed = sample_at - started
                if elapsed > args.duration and samples:
                    break
                process = process_stats(
                    server.pid,
                    previous,
                    None if previous_at is None else sample_at - previous_at,
                )
                sample = {
                    "timestamp": datetime.datetime.now(
                        datetime.timezone.utc
                    ).isoformat(),
                    "elapsed_seconds": elapsed,
                    "process": process,
                    "load": scrape_metrics(),
                }
                samples.append(sample)
                sample_file.write(json.dumps(sample) + "\n")
                sample_file.flush()
                print(
                    f"{target}/{workload} t={elapsed:.1f}s cpu={process.get('cpu_percent') or 0:.1f}% "
                    f"resident={process.get('ri_resident_size_bytes', 0) / 1048576:.1f}MiB "
                    f"phys_footprint={process.get('ri_phys_footprint_bytes', 0) / 1048576:.1f}MiB",
                    flush=True,
                )
                if not process.get("alive") or load.poll() is not None:
                    break
                if (
                    process.get("ri_phys_footprint_bytes", 0)
                    >= args.max_phys_footprint_mib * 1048576
                ):
                    stopped_reason = f"ri_phys_footprint reached the {args.max_phys_footprint_mib} MiB safety limit"
                    break
                previous = process
                previous_at = sample_at
                time.sleep(max(0, min(args.interval, args.duration - elapsed)))
        elapsed = time.monotonic() - started
        final_metrics = scrape_metrics()
        terminate(load)
        load = None
        time.sleep(0.25)
        postflight = probe(PORTS[target], WORKLOAD_PATHS[workload], expected)
    finally:
        terminate(load)
        terminate(server)
        server_log.close()
        load_log.close()
    statuses = final_metrics.get("status_counts", {})
    cpu_values = [
        sample["process"]["cpu_percent"]
        for sample in samples
        if sample["elapsed_seconds"] >= args.warmup
        and sample["process"].get("cpu_percent") is not None
    ]
    summary = {
        "target": target,
        "workload": workload,
        "offered_rps": args.rate,
        "elapsed_seconds": elapsed,
        "stopped_reason": stopped_reason,
        "max_phys_footprint_mib": args.max_phys_footprint_mib,
        "response_bytes": len(expected["body"]),
        "response_sha256": sha256_bytes(expected["body"]),
        "status_counts": statuses,
        "completed_requests": sum(statuses.values()),
        "delivered_rps": sum(statuses.values()) / elapsed if elapsed else None,
        "successful_rps": statuses.get("200", 0) / elapsed if elapsed else None,
        "preflight": preflight,
        "postflight": postflight,
        "sample_count": len(samples),
        "warmup_seconds": args.warmup,
        "cpu_percent_after_warmup": {
            "mean": statistics.fmean(cpu_values) if cpu_values else None,
            "median": statistics.median(cpu_values) if cpu_values else None,
            "p95": percentile(cpu_values, 0.95),
            "maximum": max(cpu_values) if cpu_values else None,
        },
        "ri_resident_size_bytes": series_summary(
            samples, "ri_resident_size_bytes", args.warmup
        ),
        "ri_phys_footprint_bytes": series_summary(
            samples, "ri_phys_footprint_bytes", args.warmup
        ),
        "alive_at_last_sample": samples[-1]["process"].get("alive", False)
        if samples
        else False,
    }
    (run_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--rate", type=int, default=1000, help="Offered requests per second"
    )
    parser.add_argument(
        "--duration",
        type=float,
        default=60,
        help="Loaded seconds per fresh-process run",
    )
    parser.add_argument(
        "--interval", type=float, default=1, help="Resource sample interval in seconds"
    )
    parser.add_argument(
        "--warmup",
        type=float,
        default=10,
        help="Seconds excluded from CPU averages and memory slopes",
    )
    parser.add_argument(
        "--request-timeout",
        type=float,
        default=15,
        help="Vegeta request timeout in seconds",
    )
    parser.add_argument(
        "--cooldown", type=float, default=3, help="Idle seconds between runs"
    )
    parser.add_argument(
        "--max-phys-footprint-mib",
        type=float,
        default=4096,
        help="Stop a run when ri_phys_footprint reaches this safety limit",
    )
    parser.add_argument(
        "--targets",
        nargs="+",
        choices=tuple(PORTS),
        default=tuple(PORTS),
        help="Targets to run",
    )
    parser.add_argument(
        "--workloads",
        nargs="+",
        choices=tuple(WORKLOAD_PATHS),
        default=tuple(WORKLOAD_PATHS),
        help="Workloads to run",
    )
    parser.add_argument("--results-dir", type=Path, help="Output directory")
    args = parser.parse_args()
    if (
        args.rate <= 0
        or args.duration <= 0
        or args.interval <= 0
        or args.warmup < 0
        or args.request_timeout <= 0
        or args.cooldown < 0
        or args.max_phys_footprint_mib <= 0
    ):
        parser.error(
            "rate, duration, interval, and request timeout must be positive; warmup and cooldown must be nonnegative"
        )
    manifest_path = BUILD / "manifest.json"
    if not manifest_path.is_file():
        parser.error("missing build manifest; run scripts/build.py first")
    manifest = json.loads(manifest_path.read_text())
    if not manifest.get("diverse_workloads"):
        parser.error(
            "the build does not include diverse workloads; rerun scripts/build.py with --diverse-image"
        )
    image_path = BUILD / "apps/diverse-baml-only/benchmark-image.png"
    expected = expected_responses(image_path)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_root = (
        (
            args.results_dir
            or ROOT
            / "results"
            / f"{timestamp}-{manifest['revision'][:12]}-diverse-{args.rate}rps"
        )
        .expanduser()
        .resolve()
    )
    result_root.mkdir(parents=True, exist_ok=False)
    (result_root / "runs").mkdir()
    shutil.copy2(manifest_path, result_root / "build-manifest.json")
    config = {
        "rate": args.rate,
        "duration_seconds_per_run": args.duration,
        "sample_interval_seconds": args.interval,
        "warmup_seconds": args.warmup,
        "request_timeout_seconds": args.request_timeout,
        "cooldown_seconds": args.cooldown,
        "max_phys_footprint_mib": args.max_phys_footprint_mib,
        "targets": args.targets,
        "workloads": args.workloads,
        "execution": "sequential, fresh server process per target/workload",
        "memory_source": "proc_pid_rusage(RUSAGE_INFO_V0)",
        "cpu_source": "delta(ri_user_time + ri_system_time), converted from Mach absolute-time ticks",
        "mach_timebase": {"numer": MACH_TIMEBASE.numer, "denom": MACH_TIMEBASE.denom},
        "image": manifest["diverse_image"],
    }
    (result_root / "config.json").write_text(json.dumps(config, indent=2) + "\n")
    runs = []
    failed = []
    plan = [
        (target, workload) for workload in args.workloads for target in args.targets
    ]
    for index, (target, workload) in enumerate(plan):
        summary = run_one(target, workload, args, expected[workload], result_root)
        runs.append(summary)
        if (
            not summary["preflight"]["ok"]
            or not summary["postflight"]["ok"]
            or not summary["alive_at_last_sample"]
            or summary["stopped_reason"] is not None
            or any(
                status != "200" and count
                for status, count in summary["status_counts"].items()
            )
        ):
            failed.append(f"{target}/{workload}")
        if index + 1 < len(plan) and args.cooldown:
            time.sleep(args.cooldown)
    overall = {
        "source_revision": manifest["revision"],
        "source_dirty": manifest["source_dirty"],
        "rate": args.rate,
        "duration_seconds_per_run": args.duration,
        "run_count": len(runs),
        "failed_runs": failed,
        "runs": runs,
    }
    (result_root / "summary.json").write_text(json.dumps(overall, indent=2) + "\n")
    print(json.dumps(overall, indent=2), flush=True)
    print(f"results: {result_root}", flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
