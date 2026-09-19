# /// script
# requires-python = ">=3.11"
# ///
"""Run the standard rate sweep, then use plot.py on each stage directory.

uv run crates/btel_bench/plots/sweep.py /path/to/btel_bench /path/to/new-results

Runs sequentially, timing through the binary's internal measurement boundaries.
One feeder baseline is shared by all stages in each (repetition, threads, rate)
block. Stage order rotates to reduce order bias. The saved identity explicitly
records this correlation; these are matched blocks, not independent pairs.
No outliers are removed. Fixture generation and process startup are not timed.
"""

import argparse
import hashlib
import json
import platform
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--calls", type=int, default=8_000_000)
    parser.add_argument("--threads", type=int, nargs="+", default=[1, 4, 32])
    parser.add_argument(
        "--rates",
        type=int,
        nargs="+",
        default=[
            10_000_000,
            25_000_000,
            50_000_000,
            75_000_000,
            150_000_000,
            250_000_000,
            400_000_000,
        ],
    )
    parser.add_argument(
        "--stages",
        nargs="+",
        choices=["discard", "copy-local", "copy-handoff"],
        default=["discard", "copy-local", "copy-handoff"],
    )
    args = parser.parse_args()
    args.binary = args.binary.resolve()
    if (
        args.repetitions <= 0
        or args.calls <= 0
        or any(n <= 0 or args.calls % n for n in args.threads)
        or any(r <= 0 for r in args.rates)
    ):
        parser.error(
            "positive repetitions/rates required; total calls must divide evenly across threads"
        )
    if any(
        len(values) != len(set(values))
        for values in (args.threads, args.rates, args.stages)
    ):
        parser.error("duplicate scenario settings")
    if args.output.exists() and any(args.output.iterdir()):
        parser.error("output directory must be new or empty; preserve previous results")
    args.output.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    identity = {
        "binary": str(args.binary),
        "sha256": digest,
        "platform": platform.platform(),
        "repetitions": args.repetitions,
        "calls_per_run": args.calls,
        "target_calls_per_second": args.rates,
        "threads": args.threads,
        "stages": args.stages,
        "method": "Sequential matched blocks; one shared feeder baseline per block; rotating stage order. Internal elapsed/CPU boundaries exclude setup. RSS includes preparation. No outlier removal. Batch pacing uses 1024 markers; 80 encoded bytes per function call plus thread boundary markers.",
    }
    saved = {stage: [] for stage in args.stages}
    for stage in args.stages:
        directory = args.output / stage
        directory.mkdir()
        (directory / "identity.json").write_text(
            json.dumps(identity | {"consumer_mode": stage}, indent=2) + "\n"
        )

    def run(threads, rate, mode, stage, rep, warmup=False):
        per_source_rate = rate * 80 // threads
        command = [
            str(args.binary),
            "--consumer-mode",
            stage,
            "--producer-mode",
            mode,
            "--threads",
            str(threads),
            "--calls-per-source",
            str(args.calls // threads),
            "--source-rates",
            ",".join([str(per_source_rate)] * threads),
            "--depth",
            "8",
            "--memory-mib",
            "256",
        ]
        proc = subprocess.run(
            command, capture_output=True, text=True, timeout=120, check=False
        )
        if proc.returncode:
            raise RuntimeError(f"Benchmark failed: {command}\n{proc.stderr}")
        result = json.loads(proc.stdout)
        baseline = mode == "feeder-only"
        expected = {
            "markers": args.calls * 2 + threads * 2,
            "input_bytes": args.calls * 80 + threads * 54,
            "clock_reads": 0 if baseline else args.calls * 2 + threads * 2,
            "ring_bytes": 0 if baseline else args.calls * 80 + threads * 54,
            "consumer_threads": 0 if baseline else 1,
            "downstream_threads": int(not baseline and stage == "copy-handoff"),
        }
        if any(result.get(key) != value for key, value in expected.items()):
            raise ValueError(f"Benchmark workload validation failed: {expected}")
        if sum(source["calls"] for source in result["sources"]) != args.calls:
            raise ValueError("Mismatched source call counts")
        return {
            "threads": threads,
            "calls": args.calls,
            "requested_calls_per_second": rate,
            "mode": mode,
            "rep": rep,
            "warmup": warmup,
            "command": command,
            "exit_code": proc.returncode,
            "result": result,
        }

    def persist(stage):
        directory = args.output / stage
        temporary = directory / "runs.tmp"
        temporary.write_text(json.dumps(saved[stage], indent=2) + "\n")
        temporary.replace(directory / "runs.json")

    for threads in args.threads:
        baseline = run(threads, args.rates[0], "feeder-only", "discard", -1, True)
        for stage in args.stages:
            saved[stage].extend(
                [baseline, run(threads, args.rates[0], "encode-clock", stage, -1, True)]
            )
            persist(stage)
    modes = ["baseline", *args.stages]
    for rep in range(args.repetitions):
        for rate_index, rate in enumerate(args.rates):
            for thread_index, threads in enumerate(args.threads):
                offset = (rep + rate_index + thread_index) % len(modes)
                block = {}
                for stage in modes[offset:] + modes[:offset]:
                    block[stage] = run(
                        threads,
                        rate,
                        "feeder-only" if stage == "baseline" else "encode-clock",
                        "discard" if stage == "baseline" else stage,
                        rep,
                    )
                for stage in args.stages:
                    if (
                        block["baseline"]["result"]["sources"]
                        != block[stage]["result"]["sources"]
                        or block["baseline"]["result"]["loads"]
                        != block[stage]["result"]["loads"]
                    ):
                        raise ValueError("Mismatched workloads in measurement block")
                    saved[stage].extend([block["baseline"], block[stage]])
                    persist(stage)
            print(
                f"Repetition {rep + 1}/{args.repetitions}: {rate / 1e6:g}M calls/sec complete for all threads/stages",
                flush=True,
            )
    if hashlib.sha256(args.binary.read_bytes()).hexdigest() != digest:
        raise RuntimeError("Binary changed during the sweep")
    print(f"Finished. Plot each stage directory under {args.output}", flush=True)


if __name__ == "__main__":
    main()
