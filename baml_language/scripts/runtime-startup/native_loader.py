#!/usr/bin/env python3
"""Isolate native library loading with minimal matched Rust executables on macOS."""

import argparse
import hashlib
import json
import random
import statistics
import subprocess
import sys
from itertools import pairwise
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, env):
    return subprocess.run(command, env=env, capture_output=True, check=False)


def summarize(samples):
    result = {"n": len(samples)}
    for key in samples[0]:
        if key in ("marks", "phases_ms") or key.endswith("_ns"):
            continue
        values = sorted(sample[key] for sample in samples)
        result[key] = {
            "median": statistics.median(values),
            "p95": values[(len(values) * 95 + 99) // 100 - 1],
            "mean": statistics.mean(values),
            "min": values[0],
            "max": values[-1],
        }
    if "phases_ms" in samples[0]:
        result["phases_ms"] = {
            key: {
                "mean": statistics.mean(sample["phases_ms"][key] for sample in samples),
                "median": statistics.median(
                    sample["phases_ms"][key] for sample in samples
                ),
            }
            for key in samples[0]["phases_ms"]
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--profile-hosts", required=True, type=Path)
    parser.add_argument("--runs", type=int, default=200)
    args = parser.parse_args()
    if sys.platform != "darwin" or args.runs < 1:
        parser.error("Requires macOS and a positive --runs value.")
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    previous = args.profile_hosts.resolve()
    profile = (previous / "profile.rs").read_text().split("mod host {", 1)[0]
    (out / "control.rs").write_text("fn main() {}\n")
    (out / "profile.rs").write_text(
        profile
        + "fn main() { profile::mark(1); profile::mark(21); profile::emit(); }\n"
    )
    variants = [
        (libraries, kind)
        for libraries in ("system", "host_libraries")
        for kind in ("control", "profile")
    ]
    commands = {}
    libraries = {}
    for variant in variants:
        selection, kind = variant
        exe = out / "-".join(variant)
        command = [
            "rustc",
            "--edition=2024",
            "-C",
            "opt-level=3",
            "-C",
            "lto=fat",
            "-C",
            "codegen-units=1",
            "-C",
            "strip=symbols",
            "-C",
            "panic=unwind",
            str(out / f"{kind}.rs"),
            "-o",
            str(exe),
        ]
        if selection == "host_libraries":
            command += [
                "-C",
                "link-args=-framework Security -framework CoreFoundation -liconv",
            ]
        subprocess.run(command, check=True, capture_output=True)
        commands[exe.name] = command
        linked = subprocess.check_output(["/usr/bin/otool", "-L", str(exe)], text=True)
        libraries[exe.name] = linked
        if selection == "host_libraries":
            assert all(
                name in linked
                for name in (
                    "Security.framework",
                    "CoreFoundation.framework",
                    "libiconv.2",
                    "libSystem.B",
                )
            ), linked
    env = {"PATH": "/usr/bin:/bin", "HOME": str(out), "LANG": "en_US.UTF-8"}

    def sample(variant):
        exe = out / "-".join(variant)
        result = run([str(previous / "measure"), str(exe)], env)
        assert result.returncode == 0, result.stderr
        value = json.loads(result.stdout)
        assert value["exit"] == 0
        value["cpu_ms"] = value["user_ms"] + value["system_ms"]
        if variant[1] == "profile":
            prefix = b"BAML_STARTUP_PROFILE:"
            assert result.stderr.startswith(prefix), result.stderr
            marks = json.loads(result.stderr[len(prefix) :])
            value["marks"] = marks
            ordered = [
                ("parent_start", value["start_ns"]),
                *marks.items(),
                ("stdout_eof", value["eof_ns"]),
                ("counters_end", value["counters_end_ns"]),
                ("parent_end", value["end_ns"]),
            ]
            assert all(a[1] <= b[1] for a, b in pairwise(ordered)), ordered
            value["phases_ms"] = {
                b[0]: (b[1] - a[1]) / 1e6 for a, b in pairwise(ordered)
            }
            value["before_main_ms"] = (marks["main_enter"] - value["start_ns"]) / 1e6
            assert abs(sum(value["phases_ms"].values()) - value["wall_ms"]) < 0.001
        else:
            assert not result.stderr, result.stderr
        return value

    for _ in range(3):
        for variant in variants:
            sample(variant)
    seed = 2026100404
    randomizer = random.Random(seed)
    data = {"/".join(variant): [] for variant in variants}
    for _ in range(args.runs):
        randomizer.shuffle(variants)
        for variant in variants:
            data["/".join(variant)].append(sample(variant))
    report = {
        "runs": args.runs,
        "warmups": 3,
        "seed": seed,
        "commands": commands,
        "libraries": libraries,
        "artifacts": {
            name: {"bytes": (out / name).stat().st_size, "sha256": digest(out / name)}
            for name in commands
        },
        "harness_sha256": digest(Path(__file__)),
        "helper_sha256": digest(previous / "measure"),
        "summary": {key: summarize(values) for key, values in data.items()},
        "raw": data,
    }
    (out / "results.json").write_text(json.dumps(report, separators=(",", ":")) + "\n")
    for name, value in report["summary"].items():
        print(
            name,
            "median/p95",
            value["wall_ms"]["median"],
            value["wall_ms"]["p95"],
            "before_main",
            value.get("before_main_ms"),
            flush=True,
        )


if __name__ == "__main__":
    main()
