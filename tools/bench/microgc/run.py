#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
import platform
import shutil
import signal
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional

from workloads import WORKLOADS, write_workload


HERE = Path(__file__).resolve().parent
MIB = 1024 * 1024


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run one no-HTTP pure-BAML micro-GC workload")
    parser.add_argument("workload", choices=sorted(WORKLOADS))
    parser.add_argument("--baml-cli", type=Path, help="Diagnostic BAML CLI containing baml.sys.heap_stats()")
    parser.add_argument("--duration-seconds", type=float, default=30)
    parser.add_argument("--rss-limit-mib", type=int, default=2048)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target", help="Optional baml pack target triple")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--print-every", type=int, default=1)
    return parser.parse_args()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def process_stats(pid: int) -> Dict[str, object]:
    result = subprocess.run(
        ["/bin/ps", "-o", "rss=,pcpu=,etime=", "-p", str(pid)],
        check=False,
        capture_output=True,
        text=True,
    )
    fields = result.stdout.split()
    if len(fields) < 3:
        return {"alive": False}
    return {
        "alive": True,
        "rss_bytes": int(fields[0]) * 1024,
        "cpu_percent": float(fields[1]),
        "elapsed": fields[2],
    }


def parse_sample(line: str) -> Dict[str, int]:
    values = {}
    for token in line.split():
        key, separator, value = token.partition("=")
        if not separator:
            raise ValueError(f"invalid sample token: {token!r}")
        values[key] = int(value)
    for required in ("sample", "elapsed_ms", "iterations", "runtime_objects", "allocator_bytes_in_use"):
        if required not in values:
            raise ValueError(f"sample is missing {required}: {line!r}")
    return values


def command_output(command: List[str]) -> str:
    result = subprocess.run(command, check=False, capture_output=True, text=True)
    output = (result.stdout + result.stderr).strip()
    return output


def resolve_cli(argument: Optional[Path]) -> Path:
    if argument is not None:
        cli = argument.expanduser().resolve()
    else:
        discovered = shutil.which("baml")
        if discovered is None:
            raise SystemExit("baml was not found on PATH; pass --baml-cli with a diagnostic CLI")
        cli = Path(discovered).resolve()
    if not cli.is_file():
        raise SystemExit(f"BAML CLI does not exist: {cli}")
    return cli


def build(app: Path, cli: Path, target: Optional[str], skip: bool) -> Path:
    executable = app / "benchmark"
    if skip:
        if not executable.is_file():
            raise SystemExit(f"--skip-build requested but executable does not exist: {executable}")
        return executable
    command = [str(cli), "pack", "main", "--output", str(executable), "--no-progress", "--agent-skill-check", "off"]
    if target:
        command.extend(("--target", target))
    result = subprocess.run(command, cwd=app, check=False, capture_output=True, text=True)
    (app / "build.log").write_text(result.stdout + result.stderr)
    if result.returncode != 0:
        raise SystemExit(f"baml pack failed; see {app / 'build.log'}")
    return executable


def stop_process(process: subprocess.Popen) -> None:
    if process.poll() is not None:
        return
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


def metric(samples: List[Dict[str, object]], field: str, source: str = "heap", scale: int = 1) -> Dict[str, object]:
    values = [sample[source][field] / scale for sample in samples if field in sample[source]]
    return {
        "first": values[0],
        "last": values[-1],
        "min": min(values),
        "max": max(values),
        "change": values[-1] - values[0],
        "downward_resets": sum(1 for before, after in zip(values, values[1:]) if after < before),
    }


def main() -> None:
    args = parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("microgc currently supports the macOS malloc statistics exposed by the diagnostic hook")
    if args.duration_seconds <= 0 or args.rss_limit_mib <= 0 or args.print_every <= 0:
        raise SystemExit("duration, RSS limit, and print interval must be positive")

    workload = WORKLOADS[args.workload]
    app = write_workload(workload, HERE / ".build/apps")
    cli = resolve_cli(args.baml_cli)
    executable = build(app, cli, args.target, args.skip_build)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result_dir = (args.output or HERE / ".build/results" / f"{timestamp}-{workload.name}").expanduser().resolve()
    if result_dir.exists():
        raise SystemExit(f"refusing to overwrite existing result directory: {result_dir}")
    (result_dir / "workload").mkdir(parents=True)
    shutil.copy2(app / "baml_src/main.baml", result_dir / "workload/main.baml")
    shutil.copy2(app / "baml.toml", result_dir / "workload/baml.toml")
    shutil.copy2(app / "build.log", result_dir / "build.log")

    config = {
        "started_at": datetime.now(timezone.utc).isoformat(),
        "workload": workload.name,
        "description": workload.description,
        "batch_size": workload.batch_size,
        "duration_limit_seconds": args.duration_seconds,
        "rss_safety_limit_mib": args.rss_limit_mib,
        "baml_cli": str(cli),
        "baml_cli_version": command_output([str(cli), "--version"]),
        "baml_cli_sha256": sha256(cli),
        "executable": str(executable),
        "executable_sha256": sha256(executable),
        "platform": platform.platform(),
        "machine": platform.machine(),
    }
    (result_dir / "config.json").write_text(json.dumps(config, indent=2) + "\n")

    samples = []
    termination_reason = None
    return_code = None
    process_log_path = result_dir / "process.log"
    samples_path = result_dir / "samples.jsonl"
    with process_log_path.open("w") as process_log, samples_path.open("w") as samples_file:
        process = subprocess.Popen(
            [str(executable)],
            cwd=app,
            stdout=subprocess.PIPE,
            stderr=process_log,
            text=True,
            bufsize=1,
            start_new_session=True,
        )
        assert process.stdout is not None
        try:
            for raw_line in process.stdout:
                line = raw_line.strip()
                if not line:
                    continue
                heap = parse_sample(line)
                external = process_stats(process.pid)
                sample = {
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                    "elapsed_seconds": heap.pop("elapsed_ms") / 1000,
                    "sample": heap.pop("sample"),
                    "iterations": heap.pop("iterations"),
                    "process": external,
                    "heap": heap,
                }
                samples.append(sample)
                samples_file.write(json.dumps(sample) + "\n")
                samples_file.flush()
                if sample["sample"] % args.print_every == 0:
                    print(
                        f"t={sample['elapsed_seconds']:.3f}s iterations={sample['iterations']} "
                        f"rss={external.get('rss_bytes', 0) / MIB:.2f}MiB runtime={heap['runtime_objects']} "
                        f"capacity={heap['tracked_slot_capacity_bytes'] / MIB:.2f}MiB "
                        f"malloc_live={heap['allocator_bytes_in_use'] / MIB:.2f}MiB "
                        f"malloc_reserved={heap['allocator_bytes_reserved'] / MIB:.2f}MiB",
                        flush=True,
                    )
                if external.get("rss_bytes", 0) >= args.rss_limit_mib * MIB:
                    termination_reason = f"RSS safety limit of {args.rss_limit_mib} MiB reached"
                    break
                if sample["elapsed_seconds"] >= args.duration_seconds:
                    termination_reason = f"duration limit of {args.duration_seconds:g} seconds reached"
                    break
            stop_process(process)
            return_code = process.returncode
        finally:
            stop_process(process)

    if len(samples) < 2:
        raise SystemExit(f"expected at least two samples, got {len(samples)}; see {process_log_path}")
    if termination_reason is None:
        raise SystemExit(f"benchmark exited unexpectedly with status {return_code}; see {process_log_path}")

    summary = {
        "started_at": config["started_at"],
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "return_code": return_code,
        "termination": f"terminated by the harness: {termination_reason}",
        "sample_count": len(samples),
        "measured_duration_seconds": samples[-1]["elapsed_seconds"],
        "iterations": samples[-1]["iterations"],
        "metrics": {
            "rss_mib": metric(samples, "rss_bytes", "process", MIB),
            "runtime_objects": metric(samples, "runtime_objects"),
            "reserved_slots": metric(samples, "reserved_slots"),
            "allocations_since_gc": metric(samples, "allocations_since_gc"),
            "tracked_slot_capacity_mib": metric(samples, "tracked_slot_capacity_bytes", scale=MIB),
            "permit_holder_slots": metric(samples, "permit_holder_slots"),
            "malloc_blocks": metric(samples, "allocator_blocks_in_use"),
            "malloc_live_mib": metric(samples, "allocator_bytes_in_use", scale=MIB),
            "malloc_reserved_mib": metric(samples, "allocator_bytes_reserved", scale=MIB),
        },
        "observed_gc_resets": sum(
            1
            for before, after in zip(samples, samples[1:])
            if after["heap"]["allocations_since_gc"] < before["heap"]["allocations_since_gc"]
        ),
    }
    (result_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    print(f"results: {result_dir}")


if __name__ == "__main__":
    main()
