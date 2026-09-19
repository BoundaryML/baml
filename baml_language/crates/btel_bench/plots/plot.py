# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib==3.10.9"]
# ///
"""Regenerate the standard Btel benchmark plots from saved measurements.

Usage from baml_language:
    uv run crates/btel_bench/plots/plot.py /path/to/results --output /path/to/plots

Input: identity.json and runs.json emitted by the paired replay sweep harness.
Warmups are excluded. Every scenario must have all declared repetitions of
encode-clock and feeder-only, with matching source manifests and load settings.
Rate sweeps have requested_calls_per_second on each run and
target_calls_per_second/calls_per_run in identity.json; volume sweeps vary calls.

Output: summary.json, plus PNG and SVG versions of the established charts.
Without --output, writes to RESULTS/plots. No benchmarks or Rust builds are run.
Medians/min-max and paired CPU differences are preserved; no outliers are removed.
Results describe the synthetic transport benchmark, not isolated VM overhead.
"""

import argparse
import json
import math
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_results(directory):
    identity = json.loads((directory / "identity.json").read_text())
    records = json.loads((directory / "runs.json").read_text())
    repetitions = identity["repetitions"]
    require(
        isinstance(repetitions, int) and repetitions > 0, "Invalid repetition count"
    )
    runs = [r for r in records if not r.get("warmup", False)]
    require(runs, "No measured runs")
    is_rate = "requested_calls_per_second" in runs[0]
    dimension = "requested_calls_per_second" if is_rate else "calls"
    lookup = {}
    transport_settings = set()
    stages = set()
    for run in runs:
        require(("requested_calls_per_second" in run) == is_rate, "Mixed sweep types")
        require(run["exit_code"] == 0, "Dataset contains a failed measured run")
        require(run["mode"] in ("encode-clock", "feeder-only"), "Unsupported mode")
        require(
            run["threads"] > 0 and run["calls"] > 0 and run[dimension] > 0,
            "Threads, calls and requested rates must be positive",
        )
        require(run["rep"] in range(repetitions), "Unexpected repetition index")
        key = (run["threads"], run[dimension], run["rep"], run["mode"])
        require(key not in lookup, f"Duplicate measurement: {key}")
        result = run["result"]
        require(result["producer_mode"] == run["mode"], "Producer mode mismatch")
        require(result["source_threads"] == run["threads"], "Thread count mismatch")
        require(
            sum(s["calls"] for s in result["sources"]) == run["calls"],
            "Source manifests do not match the total call count",
        )
        baseline = run["mode"] == "feeder-only"
        require(
            result["preset"]
            in (
                {"feeder-only"}
                if baseline
                else {"drain-only", "copy-local", "copy-handoff"}
            ),
            "Unsupported consumer stage",
        )
        if not baseline:
            stages.add(result["preset"])
        require(
            result.get("downstream_threads", 0)
            == int(not baseline and result["preset"] == "copy-handoff"),
            "Unexpected downstream worker count",
        )
        require(
            result["consumer_threads"] == (0 if baseline else 1),
            "Unexpected drainer count",
        )
        require(len(result["loads"]) == run["threads"], "Source load count mismatch")
        for metric in (
            "replay_seconds",
            "replay_cpu_seconds",
            "peak_rss_bytes_including_preparation",
        ):
            value = result[metric]
            require(
                isinstance(value, (int, float)) and math.isfinite(value) and value >= 0,
                f"Missing or invalid metric: {metric}",
            )
        require(result["replay_seconds"] > 0, "Elapsed time must be positive")
        for load in result["loads"]:
            require(
                load["start_delay_ms"] == 0 and load["burst_pause_ms"] == 0,
                "Standard plots require no startup delay or burst pause",
            )
            expected_rate = run[dimension] * 80 // run["threads"] if is_rate else 0
            require(
                load["bytes_per_second"] == expected_rate,
                "Source pacing differs from the plotted rate (80 bytes/call)",
            )
            transport_settings.add(
                (
                    result["memory_budget_bytes"],
                    result["segment_bytes"],
                    result["freelist_segments"],
                    result["source_budget"],
                    result["idle_timeout_micros"],
                    result["idle_rings"],
                    load["batch_markers"],
                    result.get("batch_payload_bytes"),
                    result.get("batch_source_ranges"),
                    result.get("batch_queue_capacity"),
                    result.get("batch_retained_capacity"),
                )
            )
        lookup[key] = run
    require(
        len(transport_settings) == 1, "Mixed transport/pacing settings in one sweep"
    )
    require(
        len(stages) == 1,
        "Mixed consumer stages; plot each stage's result directory separately",
    )
    identity["consumer_stage"] = stages.pop()
    identity["stage_description"] = {
        "drain-only": "discard drainer",
        "copy-local": "local copy/reuse",
        "copy-handoff": "copy/handoff + return",
    }[identity["consumer_stage"]]
    threads = sorted({r["threads"] for r in runs})
    values = sorted({r[dimension] for r in runs})
    if is_rate:
        require(
            values == sorted(identity["target_calls_per_second"]),
            "Rate grid does not match identity.json",
        )
        require(
            all(r["calls"] == identity["calls_per_run"] for r in runs),
            "Rate sweep must hold the total call count fixed",
        )
    for count in threads:
        for value in values:
            for rep in range(repetitions):
                keys = [
                    (count, value, rep, mode)
                    for mode in ("encode-clock", "feeder-only")
                ]
                require(
                    all(k in lookup for k in keys),
                    f"Missing pair: {count}, {value}, {rep}",
                )
                full, baseline = (lookup[k]["result"] for k in keys)
                require(
                    full["sources"] == baseline["sources"]
                    and full["loads"] == baseline["loads"],
                    f"Mismatched workload in pair: {count}, {value}, {rep}",
                )
    return identity, runs, is_rate


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "results", type=Path, help="Directory containing identity.json and runs.json"
    )
    parser.add_argument(
        "--output", type=Path, help="Output directory (default: RESULTS/plots)"
    )
    args = parser.parse_args()
    try:
        identity, runs, is_rate = load_results(args.results)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    output = args.output or args.results / "plots"
    output.mkdir(parents=True, exist_ok=True)
    if is_rate:
        from rate import render
    else:
        from volume import render
    render(identity, runs, output)
    print(f"Plots and summary saved to {output.resolve()}")


if __name__ == "__main__":
    main()
