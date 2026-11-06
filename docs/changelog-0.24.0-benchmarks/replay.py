#!/usr/bin/env python3
"""Validate preserved results or replay the preserved per-process benchmarks."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess


HERE = Path(__file__).resolve().parent


def read(path):
    """Load one preserved JSON evidence file."""
    return json.loads(path.read_text())


def summarize(data, array=False):
    """Calculate medians, ranges, changes, and shared checksums from raw trials."""
    if set(data) != {"023", "024"}:
        raise ValueError("Expected both release labels: 023 and 024")
    if not data["023"] or len(data["023"]) != len(data["024"]):
        raise ValueError("Expected equally many nonempty trials for each release")
    names = ["array_bench"] if array else list(data["023"][0])
    result = {}
    for name in names:
        rows = {
            ver: [row if array else row[name] for row in data[ver]]
            for ver in ("023", "024")
        }
        key = "ms_per_job" if array else ("ms" if "ms" in rows["023"][0] else "us_per_call")
        entry = {"unit": key, "trials_per_release": len(data["023"])}
        for ver in ("023", "024"):
            values = [row[key] for row in rows[ver]]
            if not all(math.isfinite(value) and value > 0 for value in values):
                raise ValueError(f"Invalid timing in {name}, {ver}")
            entry[ver] = {
                "median": statistics.median(values),
                "min": min(values),
                "max": max(values),
            }
        entry["change_percent"] = (entry["024"]["median"] / entry["023"]["median"] - 1) * 100
        if "result" in rows["023"][0]:
            checksums = {row["result"] for release in rows.values() for row in release}
            if len(checksums) != 1:
                raise ValueError(f"Result mismatch for {name}: {checksums}")
            entry["result"] = checksums.pop()
        result[name] = entry
    return result


def stored_summary(directory):
    """Combine the original suite and array summaries from an evidence directory."""
    result = summarize(read(directory / "benchmark-results.json"))
    result.update(summarize(read(directory / "array-results.json"), array=True))
    return result


def validate_preserved():
    """Verify preserved file hashes and reproduce the exact stored summary."""
    for name, expected in read(HERE / "environment.json")["preserved_files_sha256"].items():
        actual = hashlib.sha256((HERE / name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"Preserved evidence changed: {name}")
    actual = stored_summary(HERE)
    if actual != read(HERE / "summary.json"):
        raise ValueError("Stored summary differs from the raw observations")
    return actual


def print_summary(result):
    """Print rounded measurements alongside their full observed ranges."""
    for name, row in result.items():
        lower, upper = row["023"], row["024"]
        print(
            f"{name}: {lower['median']:.3f} -> {upper['median']:.3f} {row['unit']}; "
            f"{row['change_percent']:+.1f}%; "
            f"ranges {lower['min']:.3f}-{lower['max']:.3f} / "
            f"{upper['min']:.3f}-{upper['max']:.3f}"
        )


def replay(args):
    """Run fresh interleaved trials without overwriting historical observations."""
    destination = args.output_dir.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    bindings = {
        "023": (args.lower_python, args.lower_sdk, args.lower_library),
        "024": (args.upper_python, args.upper_sdk, args.upper_library),
    }
    results = {}
    for array in (False, True):
        script = HERE / "original" / ("bench_array.py" if array else "bench.py")
        data = {"023": [], "024": []}
        for trial in range(args.trials):
            order = ("023", "024") if trial % 2 == 0 else ("024", "023")
            for ver in order:
                python, sdk, library = bindings[ver]
                if array:
                    sdk = getattr(args, "lower_array_sdk" if ver == "023" else "upper_array_sdk") or sdk
                env = dict(
                    os.environ,
                    BOUNDARY_API_KEY="local",
                    BAML_HOME=str(destination / f"home-{ver}"),
                    BAML_TELEMETRY="off",
                    BAML_BRIDGE_PATH=str(library.resolve()),
                    PYTHONPATH=str(sdk.resolve()),
                )
                process = subprocess.run(
                    [str(python.absolute()), str(script), ver],
                    env=env,
                    capture_output=True,
                    text=True,
                    check=True,
                )
                if process.stderr:
                    print(process.stderr, end="")
                data[ver].append(json.loads(process.stdout))
                print(f"{'array' if array else 'suite'} pair {trial + 1}: {ver}", flush=True)
        filename = "array-results.json" if array else "benchmark-results.json"
        (destination / filename).write_text(json.dumps(data, indent=2) + "\n")
        results.update(summarize(data, array=array))
    (destination / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    print_summary(results)


def main():
    """Dispatch evidence validation or a caller-configured benchmark replay."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("validate", help="Verify preserved hashes, results, and exact summary; does not benchmark")
    command = commands.add_parser("replay", help="Run fresh measurements with caller-supplied SDK environments")
    for side in ("lower", "upper"):
        command.add_argument(f"--{side}-python", type=Path, required=True)
        command.add_argument(f"--{side}-sdk", type=Path, required=True, help="Import root containing baml_sdk")
        command.add_argument(f"--{side}-array-sdk", type=Path, help="Optional separate SDK import root for the array followup")
        command.add_argument(f"--{side}-library", type=Path, required=True, help="Matching released CFFI shared library")
    command.add_argument("--output-dir", type=Path, required=True, help="New directory for fresh results")
    command.add_argument("--trials", type=int, default=5)
    args = parser.parse_args()
    if args.command == "validate":
        print_summary(validate_preserved())
        print("Preserved hashes, cross-release checksums, and exact summary: PASS")
    else:
        if args.trials < 1:
            parser.error("--trials must be positive")
        replay(args)


if __name__ == "__main__":
    main()
