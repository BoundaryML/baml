#!/usr/bin/env python3
"""Run fresh-process diverse workloads under a repeating on/off load profile."""

import argparse
import datetime
import json
import os
import shutil
import statistics
import subprocess
import time
import urllib.request
from pathlib import Path

import run_diverse as harness


def report_results(vegeta, files):
    completed = subprocess.run(
        [vegeta, "report", "-type=json", *files],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(completed.stdout)


def request_gc(port, timeout):
    started = time.monotonic()
    try:
        with urllib.request.urlopen(
            f"http://127.0.0.1:{port}/gc", timeout=timeout
        ) as response:
            body = response.read()
            return {
                "ok": response.status == 200 and body == b"ok",
                "status": response.status,
                "body": body.decode(errors="replace"),
                "python_objects_collected": response.headers.get(
                    "X-Python-GC-Collected"
                ),
                "elapsed_seconds": time.monotonic() - started,
            }
    except OSError as exc:
        return {
            "ok": False,
            "error": str(exc),
            "elapsed_seconds": time.monotonic() - started,
        }


def phase_series_summary(samples, field, warmup_seconds, phase=None):
    selected = [
        sample for sample in samples if phase is None or sample["phase"] == phase
    ]
    return harness.series_summary(selected, field, warmup_seconds)


def time_weighted_cpu(samples, warmup_seconds):
    selected = [
        sample for sample in samples if sample["elapsed_seconds"] >= warmup_seconds
    ]
    if len(selected) < 2:
        return None
    first = selected[0]
    last = selected[-1]
    wall_seconds = last["elapsed_seconds"] - first["elapsed_seconds"]
    if wall_seconds <= 0:
        return None
    cpu_ticks = (
        last["process"]["ri_user_time_mach_ticks"]
        + last["process"]["ri_system_time_mach_ticks"]
        - first["process"]["ri_user_time_mach_ticks"]
        - first["process"]["ri_system_time_mach_ticks"]
    )
    return (
        cpu_ticks * harness.MACH_TICK_NANOSECONDS / (wall_seconds * 1_000_000_000) * 100
    )


def run_one(target, workload, args, expected, result_root):
    run_dir = result_root / "runs" / f"{target}-{workload}"
    run_dir.mkdir(parents=True)
    attacks_dir = run_dir / "attacks"
    attacks_dir.mkdir()
    target_file = run_dir / "target.txt"
    target_file.write_text(
        f"GET http://127.0.0.1:{harness.PORTS[target]}{harness.WORKLOAD_PATHS[workload]}\n"
    )
    app_root = (
        harness.BUILD
        / "apps"
        / ("diverse-baml-only" if target == "pure-baml" else "diverse-python-baml")
    )
    if target == "pure-baml":
        command = [app_root / "server"]
    else:
        command = [
            harness.BUILD / "pyvenv/bin/python",
            "-m",
            "uvicorn",
            "server:app",
            "--host",
            "127.0.0.1",
            "--port",
            str(harness.PORTS[target]),
            "--no-access-log",
        ]
    env = dict(
        os.environ, BAML_PROFILE="0", BAML_TELEMETRY_DISABLED="1", BAML_LOG="off"
    )
    server_log = (run_dir / "server.log").open("wb")
    samples = []
    phases = []
    gc_requests = []
    attack_files = []
    load_logs = []
    server = None
    load = None
    elapsed = 0.0
    stopped_reason = None
    previous = None
    previous_at = None
    preflight = {"ok": False, "error": "not run"}
    postflight = {"ok": False, "error": "not run"}
    started = None

    def sample(sample_file, phase, cycle):
        nonlocal previous, previous_at, stopped_reason
        sample_at = time.monotonic()
        process = harness.process_stats(
            server.pid,
            previous,
            None if previous_at is None else sample_at - previous_at,
        )
        value = {
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "elapsed_seconds": sample_at - started,
            "phase": phase,
            "cycle": cycle,
            "process": process,
        }
        samples.append(value)
        sample_file.write(json.dumps(value) + "\n")
        sample_file.flush()
        print(
            f"{target}/{workload} t={value['elapsed_seconds']:.1f}s phase={phase} "
            f"cpu={process.get('cpu_percent') or 0:.1f}% "
            f"resident={process.get('ri_resident_size_bytes', 0) / 1048576:.1f}MiB "
            f"phys_footprint={process.get('ri_phys_footprint_bytes', 0) / 1048576:.1f}MiB",
            flush=True,
        )
        previous = process
        previous_at = sample_at
        if not process.get("alive"):
            stopped_reason = "server process exited"
        elif (
            process.get("ri_phys_footprint_bytes", 0)
            >= args.max_phys_footprint_mib * 1048576
        ):
            stopped_reason = f"ri_phys_footprint reached the {args.max_phys_footprint_mib} MiB safety limit"
        return value

    def wait_with_samples(sample_file, deadline, phase, cycle, next_sample):
        while time.monotonic() < deadline and stopped_reason is None:
            now = time.monotonic()
            if now >= next_sample:
                sample(sample_file, phase, cycle)
                next_sample += args.interval
            else:
                time.sleep(min(deadline - now, next_sample - now, 0.1))
        return next_sample

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
        preflight = harness.probe(
            harness.PORTS[target], harness.WORKLOAD_PATHS[workload], expected
        )
        while not preflight["ok"] and time.monotonic() < deadline:
            time.sleep(0.25)
            preflight = harness.probe(
                harness.PORTS[target], harness.WORKLOAD_PATHS[workload], expected
            )
        if not preflight["ok"]:
            raise RuntimeError(f"{target}/{workload} preflight failed: {preflight}")

        started = time.monotonic()
        end = started + args.duration
        next_sample = started
        cycle = 0
        with (run_dir / "samples.jsonl").open("w") as sample_file:
            while time.monotonic() < end and stopped_reason is None:
                cycle += 1
                on_started = time.monotonic()
                on_deadline = min(end, on_started + args.on_seconds)
                attack = attacks_dir / f"{cycle:03d}.gob"
                attack_files.append(attack)
                load_log = (run_dir / f"load-{cycle:03d}.log").open("wb")
                load_logs.append(load_log)
                attack_duration = max(0.001, on_deadline - on_started)
                load_command = [
                    harness.BUILD / "tools/vegeta",
                    "attack",
                    f"-output={attack}",
                    f"-rate={args.rate}/1s",
                    f"-duration={attack_duration}s",
                    f"-timeout={args.request_timeout}s",
                    f"-targets={target_file}",
                ]
                load = subprocess.Popen(
                    list(map(str, load_command)),
                    stdout=load_log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                )
                phase = {
                    "cycle": cycle,
                    "phase": "on",
                    "started_seconds": on_started - started,
                    "scheduled_end_seconds": on_deadline - started,
                    "attack_duration_seconds": attack_duration,
                }
                phases.append(phase)
                next_sample = wait_with_samples(
                    sample_file, on_deadline, "on", cycle, next_sample
                )
                if stopped_reason is not None:
                    harness.terminate(load)
                else:
                    try:
                        load.wait(timeout=args.request_timeout + 5)
                    except subprocess.TimeoutExpired:
                        stopped_reason = "Vegeta did not drain after the on period"
                        harness.terminate(load)
                phase["ended_seconds"] = time.monotonic() - started
                phase["load_returncode"] = load.returncode
                phase["actual_send_seconds"] = min(
                    attack_duration,
                    phase["ended_seconds"] - phase["started_seconds"],
                )
                load = None

                if args.explicit_gc:
                    gc_result = request_gc(harness.PORTS[target], args.gc_timeout)
                    gc_result.update(
                        {"cycle": cycle, "at_seconds": time.monotonic() - started}
                    )
                    gc_requests.append(gc_result)
                    sample(sample_file, "post-gc", cycle)
                    if not gc_result["ok"] and stopped_reason is None:
                        stopped_reason = "explicit GC request failed"

                if stopped_reason is not None or time.monotonic() >= end:
                    break
                off_started = time.monotonic()
                off_deadline = min(end, off_started + args.off_seconds)
                off_phase = {
                    "cycle": cycle,
                    "phase": "off",
                    "started_seconds": off_started - started,
                    "scheduled_end_seconds": off_deadline - started,
                }
                phases.append(off_phase)
                next_sample = max(next_sample, off_started)
                next_sample = wait_with_samples(
                    sample_file, off_deadline, "off", cycle, next_sample
                )
                off_phase["ended_seconds"] = time.monotonic() - started

            if samples and stopped_reason is None:
                sample(sample_file, "final", cycle)
        elapsed = time.monotonic() - started
        time.sleep(0.25)
        postflight = harness.probe(
            harness.PORTS[target], harness.WORKLOAD_PATHS[workload], expected
        )
    finally:
        harness.terminate(load)
        harness.terminate(server)
        server_log.close()
        for load_log in load_logs:
            load_log.close()

    usable_attacks = [str(path) for path in attack_files if path.exists()]
    report = (
        report_results(harness.BUILD / "tools/vegeta", usable_attacks)
        if usable_attacks
        else {}
    )
    (run_dir / "vegeta-report.json").write_text(json.dumps(report, indent=2) + "\n")
    if not args.keep_attack_results:
        for path in attack_files:
            path.unlink(missing_ok=True)
    statuses = report.get("status_codes", {})
    scheduled_on_seconds = sum(
        phase["attack_duration_seconds"] for phase in phases if phase["phase"] == "on"
    )
    actual_on_seconds = sum(
        phase["actual_send_seconds"] for phase in phases if phase["phase"] == "on"
    )
    cpu_values = [
        sample["process"]["cpu_percent"]
        for sample in samples
        if sample["elapsed_seconds"] >= args.warmup
        and sample["process"].get("cpu_percent") is not None
    ]
    summary = {
        "target": target,
        "workload": workload,
        "explicit_gc": args.explicit_gc,
        "offered_rps_during_on": args.rate,
        "elapsed_seconds": elapsed,
        "scheduled_on_seconds": scheduled_on_seconds,
        "actual_on_seconds": actual_on_seconds,
        "duty_cycle": args.on_seconds / (args.on_seconds + args.off_seconds),
        "cycles_started": len([phase for phase in phases if phase["phase"] == "on"]),
        "stopped_reason": stopped_reason,
        "max_phys_footprint_mib": args.max_phys_footprint_mib,
        "response_bytes": len(expected["body"]),
        "response_sha256": harness.sha256_bytes(expected["body"]),
        "status_counts": statuses,
        "load_errors": report.get("errors", []),
        "vegeta_throughput": report.get("throughput"),
        "latencies_nanoseconds": report.get("latencies"),
        "completed_requests": report.get("requests", sum(statuses.values())),
        "delivered_rps_during_on": sum(statuses.values()) / actual_on_seconds
        if actual_on_seconds
        else None,
        "successful_rps_during_on": statuses.get("200", 0) / actual_on_seconds
        if actual_on_seconds
        else None,
        "preflight": preflight,
        "postflight": postflight,
        "sample_count": len(samples),
        "phases": phases,
        "gc_requests": gc_requests,
        "cpu_percent_after_warmup": {
            "time_weighted_mean": time_weighted_cpu(samples, args.warmup),
            "mean": statistics.fmean(cpu_values) if cpu_values else None,
            "median": statistics.median(cpu_values) if cpu_values else None,
            "p95": harness.percentile(cpu_values, 0.95),
            "maximum": max(cpu_values) if cpu_values else None,
        },
        "ri_resident_size_bytes": phase_series_summary(
            samples, "ri_resident_size_bytes", args.warmup
        ),
        "ri_phys_footprint_bytes": phase_series_summary(
            samples, "ri_phys_footprint_bytes", args.warmup
        ),
        "on_ri_resident_size_bytes": phase_series_summary(
            samples, "ri_resident_size_bytes", args.warmup, "on"
        ),
        "on_ri_phys_footprint_bytes": phase_series_summary(
            samples, "ri_phys_footprint_bytes", args.warmup, "on"
        ),
        "alive_at_last_sample": samples[-1]["process"].get("alive", False)
        if samples
        else False,
    }
    (run_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rate", type=int, default=1000)
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--on-seconds", type=float, default=8)
    parser.add_argument("--off-seconds", type=float, default=2)
    parser.add_argument("--interval", type=float, default=1)
    parser.add_argument("--warmup", type=float, default=10)
    parser.add_argument("--request-timeout", type=float, default=15)
    parser.add_argument("--gc-timeout", type=float, default=30)
    parser.add_argument("--cooldown", type=float, default=3)
    parser.add_argument("--max-phys-footprint-mib", type=float, default=4096)
    parser.add_argument("--explicit-gc", action="store_true")
    parser.add_argument(
        "--keep-attack-results",
        action="store_true",
        help="Retain Vegeta gob files, including every response body",
    )
    parser.add_argument(
        "--targets",
        nargs="+",
        choices=tuple(harness.PORTS),
        default=tuple(harness.PORTS),
    )
    parser.add_argument(
        "--workloads",
        nargs="+",
        choices=tuple(harness.WORKLOAD_PATHS),
        default=tuple(harness.WORKLOAD_PATHS),
    )
    parser.add_argument("--results-dir", type=Path)
    args = parser.parse_args()
    if (
        any(
            value <= 0
            for value in (
                args.rate,
                args.duration,
                args.on_seconds,
                args.off_seconds,
                args.interval,
                args.request_timeout,
                args.gc_timeout,
                args.max_phys_footprint_mib,
            )
        )
        or args.warmup < 0
        or args.cooldown < 0
    ):
        parser.error(
            "positive timing, rate, timeout, and safety-limit values are required"
        )

    manifest_path = harness.BUILD / "manifest.json"
    if not manifest_path.is_file():
        parser.error("missing build manifest; run scripts/build.py first")
    manifest = json.loads(manifest_path.read_text())
    if not manifest.get("diverse_workloads"):
        parser.error("rebuild with scripts/build.py --diverse-image IMAGE")
    image_path = harness.BUILD / "apps/diverse-baml-only/benchmark-image.png"
    expected = harness.expected_responses(image_path)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    suffix = "explicit-gc" if args.explicit_gc else "automatic-gc"
    result_root = (
        (
            args.results_dir
            or harness.ROOT
            / "results"
            / f"{timestamp}-{manifest['revision'][:12]}-diverse-80pct-{suffix}"
        )
        .expanduser()
        .resolve()
    )
    result_root.mkdir(parents=True, exist_ok=False)
    (result_root / "runs").mkdir()
    shutil.copy2(manifest_path, result_root / "build-manifest.json")
    config = {
        "rate_during_on": args.rate,
        "duration_seconds_per_run": args.duration,
        "on_seconds": args.on_seconds,
        "off_seconds": args.off_seconds,
        "duty_cycle": args.on_seconds / (args.on_seconds + args.off_seconds),
        "explicit_gc": args.explicit_gc,
        "python_gc": "BAML full GC followed by CPython generation-2 GC"
        if args.explicit_gc
        else None,
        "sample_interval_seconds": args.interval,
        "warmup_seconds": args.warmup,
        "request_timeout_seconds": args.request_timeout,
        "gc_timeout_seconds": args.gc_timeout,
        "cooldown_seconds": args.cooldown,
        "max_phys_footprint_mib": args.max_phys_footprint_mib,
        "keep_attack_results": args.keep_attack_results,
        "targets": args.targets,
        "workloads": args.workloads,
        "execution": "sequential, fresh server process per target/workload",
        "memory_source": "proc_pid_rusage(RUSAGE_INFO_V0)",
        "cpu_source": "delta(ri_user_time + ri_system_time), converted from Mach absolute-time ticks",
        "mach_timebase": {
            "numer": harness.MACH_TIMEBASE.numer,
            "denom": harness.MACH_TIMEBASE.denom,
        },
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
            or summary["completed_requests"] <= 0
            or summary["stopped_reason"] is not None
            or any(
                status != "200" and count
                for status, count in summary["status_counts"].items()
            )
            or (
                args.explicit_gc
                and not all(item["ok"] for item in summary["gc_requests"])
            )
        ):
            failed.append(f"{target}/{workload}")
        if index + 1 < len(plan) and args.cooldown:
            time.sleep(args.cooldown)

    overall = {
        "source_revision": manifest["revision"],
        "source_dirty": manifest["source_dirty"],
        "rate_during_on": args.rate,
        "duty_cycle": args.on_seconds / (args.on_seconds + args.off_seconds),
        "explicit_gc": args.explicit_gc,
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
