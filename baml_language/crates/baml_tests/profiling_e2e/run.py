#!/usr/bin/env python3
"""Release-packed whole-program profiling benchmark.

Hyperfine is preferred by the benchmark protocol. When it is unavailable this
script uses a direct repeated-process runner: no CLI, interpreter, test harness,
or shell is present in the timed interval.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import threading
import time
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
WORKSPACE = SCRIPT_DIR.parents[2]
TARGET = WORKSPACE / "target" / "profiling-e2e"
PACKED = TARGET / "profiling-e2e-packed"
VERIFIER = WORKSPACE / "target" / "release" / "examples" / "profiling_e2e_verify"
FIXTURE = SCRIPT_DIR / "workload.baml"
WORK_DIR = TARGET / "work"
PROFILE_ROOT = WORK_DIR / ".baml" / "profiles-v1"
PROFILE_LOCK = WORK_DIR / ".baml" / "profiles-v1.lock"


def run_checked(command: list[str], *, cwd: Path, env: dict[str, str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def build_and_pack(base_env: dict[str, str]) -> None:
    build_env = dict(base_env)
    build_env["BAML_PROFILE"] = "0"
    run_checked(
        [
            "cargo",
            "build",
            "--release",
            "-p",
            "baml_cli",
            "--bin",
            "baml-cli",
            "-p",
            "baml_pack_host",
            "--bin",
            "baml-pack-host",
            "-p",
            "baml_tests",
            "--example",
            "profiling_e2e_verify",
        ],
        cwd=WORKSPACE,
        env=build_env,
    )
    TARGET.mkdir(parents=True, exist_ok=True)
    pack_env = dict(build_env)
    pack_env["BAML_CLI_ALLOW_DIRECT"] = "1"
    run_checked(
        [
            str(WORKSPACE / "target" / "release" / "baml-cli"),
            "pack",
            "--file",
            str(FIXTURE),
            "main",
            "--output",
            str(PACKED),
        ],
        cwd=WORKSPACE,
        env=pack_env,
    )


def clean_profile_store() -> None:
    if PROFILE_ROOT.exists():
        shutil.rmtree(PROFILE_ROOT)
    if PROFILE_LOCK.exists():
        PROFILE_LOCK.unlink()


def read_pipe(pipe: Any, chunks: list[bytes], first_byte: list[int | None]) -> None:
    while True:
        data = os.read(pipe.fileno(), 4096)
        if not data:
            return
        if first_byte[0] is None:
            first_byte[0] = time.perf_counter_ns()
        chunks.append(data)


def packed_arguments(scenario: str, tasks: int, iterations: int, inner_rounds: int) -> list[str]:
    return [
        str(PACKED),
        "--scenario",
        scenario,
        "--tasks",
        str(tasks),
        "--iterations",
        str(iterations),
        "--inner_rounds",
        str(inner_rounds),
    ]


def run_packed(
    base_env: dict[str, str],
    profile: bool,
    scenario: str,
    tasks: int,
    iterations: int,
    inner_rounds: int,
    timeout_seconds: float,
) -> dict[str, Any]:
    clean_profile_store()
    environment = dict(base_env)
    environment["BAML_PROFILE"] = "1" if profile else "0"
    command = packed_arguments(scenario, tasks, iterations, inner_rounds)
    stdout_chunks: list[bytes] = []
    stderr_chunks: list[bytes] = []
    first_stdout: list[int | None] = [None]
    ignored_first_stderr: list[int | None] = [None]
    started = time.perf_counter_ns()
    process = subprocess.Popen(
        command,
        cwd=WORK_DIR,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    assert process.stderr is not None
    stdout_thread = threading.Thread(
        target=read_pipe,
        args=(process.stdout, stdout_chunks, first_stdout),
        daemon=True,
    )
    stderr_thread = threading.Thread(
        target=read_pipe,
        args=(process.stderr, stderr_chunks, ignored_first_stderr),
        daemon=True,
    )
    stdout_thread.start()
    stderr_thread.start()
    try:
        return_code = process.wait(timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        stdout_thread.join()
        stderr_thread.join()
        elapsed = (time.perf_counter_ns() - started) / 1_000_000_000
        raise RuntimeError(
            f"profiling-{'on' if profile else 'off'} packed {scenario} run exceeded the "
            f"{timeout_seconds:g} second stall timeout after {elapsed:.3f}s; "
            f"stdout_bytes={sum(map(len, stdout_chunks))}, "
            f"stderr_bytes={sum(map(len, stderr_chunks))}"
        )
    finished = time.perf_counter_ns()
    stdout_thread.join()
    stderr_thread.join()
    stdout = b"".join(stdout_chunks)
    stderr = b"".join(stderr_chunks)
    if return_code != 0:
        raise RuntimeError(
            f"packed {scenario} run exited {return_code}:\n{stderr.decode(errors='replace')}"
        )
    if stderr:
        raise RuntimeError(f"packed {scenario} run wrote stderr:\n{stderr.decode(errors='replace')}")
    if first_stdout[0] is None:
        raise RuntimeError(f"packed {scenario} run produced no output")
    output_seconds = (first_stdout[0] - started) / 1_000_000_000
    total_seconds = (finished - started) / 1_000_000_000
    return {
        "stdout": stdout,
        "total_s": total_seconds,
        "output_s": output_seconds,
        "flush_tail_s": total_seconds - output_seconds,
    }


def directory_bytes(root: Path) -> int:
    return sum(path.stat().st_size for path in root.rglob("*") if path.is_file())


def verify_run(scenario: str, tasks: int, iterations: int) -> dict[str, Any]:
    if not PROFILE_ROOT.is_dir():
        raise RuntimeError("profiling-on run produced no profiles-v1 store")
    verifier_env = dict(os.environ)
    verifier_env["BAML_PROFILE"] = "0"
    verification = subprocess.run(
        [str(VERIFIER), str(PROFILE_ROOT), scenario, str(tasks), str(iterations)],
        cwd=WORK_DIR,
        env=verifier_env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if verification.returncode != 0:
        raise RuntimeError(
            "durable profiling verification failed:\n"
            + verification.stdout
            + verification.stderr
        )
    summary = json.loads(verification.stdout)
    summary["store_bytes"] = directory_bytes(PROFILE_ROOT)
    return summary


def assert_off_is_state_free() -> None:
    if PROFILE_ROOT.exists() or PROFILE_LOCK.exists():
        raise RuntimeError("profiling-off run created profiler store state")


def summarize(samples: list[dict[str, Any]], tasks: int, iterations: int, inner_rounds: int) -> dict[str, Any]:
    total = [sample["total_s"] for sample in samples]
    output = [sample["output_s"] for sample in samples]
    tail = [sample["flush_tail_s"] for sample in samples]
    median_total = statistics.median(total)
    kernel_calls = tasks * iterations
    inner_updates = kernel_calls * inner_rounds
    return {
        "runs": len(samples),
        "median_s": median_total,
        "mean_s": statistics.mean(total),
        "stdev_s": statistics.stdev(total) if len(total) > 1 else 0.0,
        "min_s": min(total),
        "max_s": max(total),
        "median_output_s": statistics.median(output),
        "median_flush_tail_s": statistics.median(tail),
        "kernel_calls_per_s": kernel_calls / median_total,
        "inner_updates_per_s": inner_updates / median_total,
        "raw_total_s": total,
        "raw_output_s": output,
        "raw_flush_tail_s": tail,
    }


def benchmark_scenario(
    base_env: dict[str, str],
    scenario: str,
    tasks: int,
    iterations: int,
    inner_rounds: int,
    warmups: int,
    runs: int,
    profile_modes: list[bool],
    timeout_seconds: float,
) -> dict[str, Any]:
    expected_output: bytes | None = None
    durable_summary: dict[str, Any] | None = None
    for warmup in range(warmups):
        order = [False, True] if warmup % 2 == 0 else [True, False]
        for profile in order:
            if profile not in profile_modes:
                continue
            sample = run_packed(
                base_env,
                profile,
                scenario,
                tasks,
                iterations,
                inner_rounds,
                timeout_seconds,
            )
            if expected_output is None:
                expected_output = sample["stdout"]
            elif sample["stdout"] != expected_output:
                raise RuntimeError(f"{scenario} output mismatch during warmup")
            if profile:
                durable_summary = verify_run(scenario, tasks, iterations)
            else:
                assert_off_is_state_free()

    measured: dict[str, list[dict[str, Any]]] = {"off": [], "on": []}
    for run in range(runs):
        order = [False, True] if run % 2 == 0 else [True, False]
        for profile in order:
            if profile not in profile_modes:
                continue
            sample = run_packed(
                base_env,
                profile,
                scenario,
                tasks,
                iterations,
                inner_rounds,
                timeout_seconds,
            )
            if expected_output is None:
                expected_output = sample["stdout"]
            elif sample["stdout"] != expected_output:
                raise RuntimeError(f"{scenario} output mismatch in measured run {run + 1}")
            if profile:
                durable_summary = verify_run(scenario, tasks, iterations)
                measured["on"].append(sample)
            else:
                assert_off_is_state_free()
                measured["off"].append(sample)

    assert expected_output is not None
    result = {
        "scenario": scenario,
        "status": "passed",
        "tasks": tasks,
        "iterations": iterations,
        "inner_rounds": inner_rounds,
        "warmups": warmups,
        "measurement_engine": (
            "direct repeated-process invariant runner (Hyperfine measured separately)"
            if shutil.which("hyperfine") is not None
            else "direct repeated-process fallback (Hyperfine unavailable)"
        ),
        "command_off": ["BAML_PROFILE=0", *packed_arguments(scenario, tasks, iterations, inner_rounds)],
        "command_on": ["BAML_PROFILE=1", *packed_arguments(scenario, tasks, iterations, inner_rounds)],
        "output_utf8": expected_output.decode("utf-8"),
        "output_sha256": hashlib.sha256(expected_output).hexdigest(),
    }
    if False in profile_modes:
        result["off"] = summarize(measured["off"], tasks, iterations, inner_rounds)
    if True in profile_modes:
        assert durable_summary is not None
        result["durable"] = durable_summary
        result["on"] = summarize(measured["on"], tasks, iterations, inner_rounds)
    if set(profile_modes) == {False, True}:
        absolute = result["on"]["median_s"] - result["off"]["median_s"]
        result["absolute_overhead_s"] = absolute
        result["relative_slowdown"] = result["on"]["median_s"] / result["off"]["median_s"]
    return result


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario", choices=["baseline", "stress", "all"], default="all")
    parser.add_argument("--iterations", type=int, default=50_000)
    parser.add_argument("--inner-rounds", type=int, default=640)
    parser.add_argument("--stress-tasks", type=int, default=(os.cpu_count() or 1) * 2)
    parser.add_argument("--warmups", type=int, default=None)
    parser.add_argument("--runs", type=int, default=None)
    parser.add_argument("--profile-mode", choices=["both", "off", "on"], default="both")
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=TARGET / "results.json")
    parser.add_argument("--no-build", action="store_true")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    if shutil.which("hyperfine") is None:
        print(
            "Hyperfine is unavailable; using the direct repeated-process fallback.",
            file=sys.stderr,
            flush=True,
        )
    else:
        print(
            "Hyperfine is installed, but this runner currently records output/flush split and "
            "validates every durable run with its direct fallback engine.",
            file=sys.stderr,
            flush=True,
        )
    base_env = dict(os.environ)
    WORK_DIR.mkdir(parents=True, exist_ok=True)
    if not arguments.no_build:
        build_and_pack(base_env)
    if not PACKED.is_file() or not VERIFIER.is_file():
        raise RuntimeError("release packed binary or verifier is missing; rerun without --no-build")

    scenarios: list[tuple[str, int, int, int]] = []
    if arguments.scenario in ("baseline", "all"):
        warmups = arguments.warmups if arguments.warmups is not None else 3
        runs = arguments.runs if arguments.runs is not None else 15
        scenarios.append(("baseline", 1, warmups, runs))
    if arguments.scenario in ("stress", "all"):
        warmups = arguments.warmups if arguments.warmups is not None else 2
        runs = arguments.runs if arguments.runs is not None else 10
        scenarios.append(("stress", arguments.stress_tasks, warmups, runs))

    results = {
        "schema": 1,
        "hyperfine_available": shutil.which("hyperfine") is not None,
        "logical_cpus": os.cpu_count(),
        "configured_profiler_budget_bytes": 256 * 1024 * 1024,
        "packed_binary": str(PACKED),
        "packed_sha256": hashlib.sha256(PACKED.read_bytes()).hexdigest(),
        "fixture": str(FIXTURE),
        "release_build_command": [
            "cargo",
            "build",
            "--release",
            "-p",
            "baml_cli",
            "--bin",
            "baml-cli",
            "-p",
            "baml_pack_host",
            "--bin",
            "baml-pack-host",
            "-p",
            "baml_tests",
            "--example",
            "profiling_e2e_verify",
        ],
        "pack_command": [
            str(WORKSPACE / "target" / "release" / "baml-cli"),
            "pack",
            "--file",
            str(FIXTURE),
            "main",
            "--output",
            str(PACKED),
        ],
        "scenarios": [],
    }
    profile_modes = {
        "both": [False, True],
        "off": [False],
        "on": [True],
    }[arguments.profile_mode]
    failed = False
    for scenario, tasks, warmups, runs in scenarios:
        print(
            f"benchmarking {scenario}: tasks={tasks} iterations={arguments.iterations} "
            f"inner_rounds={arguments.inner_rounds} warmups={warmups} runs={runs}",
            flush=True,
        )
        try:
            result = benchmark_scenario(
                base_env,
                scenario,
                tasks,
                arguments.iterations,
                arguments.inner_rounds,
                warmups,
                runs,
                profile_modes,
                arguments.timeout_seconds,
            )
        except Exception as error:
            failed = True
            result = {
                "scenario": scenario,
                "status": "failed",
                "tasks": tasks,
                "iterations": arguments.iterations,
                "inner_rounds": arguments.inner_rounds,
                "warmups": warmups,
                "runs": runs,
                "profile_mode": arguments.profile_mode,
                "timeout_seconds": arguments.timeout_seconds,
                "command_off": [
                    "BAML_PROFILE=0",
                    *packed_arguments(scenario, tasks, arguments.iterations, arguments.inner_rounds),
                ],
                "command_on": [
                    "BAML_PROFILE=1",
                    *packed_arguments(scenario, tasks, arguments.iterations, arguments.inner_rounds),
                ],
                "error": str(error),
            }
        results["scenarios"].append(result)

    output = arguments.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    print(json.dumps(results, indent=2, sort_keys=True))
    print(f"wrote {output}")
    if not failed:
        clean_profile_store()
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
