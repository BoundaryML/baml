#!/usr/bin/env python3
"""Matched macOS startup/clock measurements; isolated HOME, no cloud credentials.

Each of BASELINE and CANDIDATE under OUTPUT must contain baml-packed,
tiny-packed and clock-startup. Build/pack both revisions with the same release
profile and toolchain before running; never share Cargo target dirs across them.
"""
import argparse
import ctypes
import hashlib
import json
import pathlib
import random
import statistics
import subprocess
import time


def summarize(rows):
    result = {"n": len(rows)}
    for key in rows[0]:
        if not isinstance(rows[0][key], (int, float)):
            continue
        values = sorted(row[key] for row in rows)
        result[key] = {
            "p50": statistics.median(values),
            "p95": values[max(0, (len(values) * 95 + 99) // 100 - 1)],
            "p99": values[max(0, (len(values) * 99 + 99) // 100 - 1)],
            "min": values[0], "max": values[-1],
        }
    return result


class TaskInfo(ctypes.Structure):
    _fields_ = [(n, ctypes.c_uint64) for n in (
        "virtual", "resident", "user", "system", "threads_user", "threads_system"
    )] + [(n, ctypes.c_int32) for n in (
        "policy", "faults", "pageins", "cow", "sent", "received", "mach_syscalls",
        "unix_syscalls", "switches", "threads", "running", "priority"
    )]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--samples", type=int, default=100)
    args = parser.parse_args()
    root = args.output.resolve()
    source = pathlib.Path(__file__).resolve().parent
    project = root / "fixture" / "project"
    home = root / "fixture" / "home"
    project.mkdir(parents=True, exist_ok=True)
    home.mkdir(parents=True, exist_ok=True)
    (home / ".baml").mkdir(exist_ok=True)
    (home / ".baml" / "config.toml").write_text('[update]\nauto_check = false\n')
    (project / "baml.toml").write_text('[package]\nname = "startup_metrics"\n')
    for name in ("measure", "child"):
        subprocess.run(["/usr/bin/clang", "-O2", str(source / f"{name}.c"),
                        "-o", str(root / name)], check=True)
    base_env = {
        "PATH": "/usr/bin:/bin", "LANG": "en_US.UTF-8", "HOME": str(home),
        "BAML_HOME": str(home / ".baml"), "NO_COLOR": "1",
        "BAML_TOOLCHAIN": str(root / "child"),
    }

    def environment(mode):
        return base_env | ({"BAML_TELEMETRY": "off"} if mode == "off" else {})

    def measure(binary, command, mode):
        result = subprocess.run([str(root / "measure"), str(binary), *command],
                                cwd=project, env=environment(mode),
                                capture_output=True, text=True, check=True, timeout=15)
        row = json.loads(result.stdout)
        assert row["exit"] == 0 and not result.stderr, (row, result.stderr)
        row["cpu_ms"] = row["user_ms"] + row["system_ms"]
        row["exit_after_first_output_ms"] = (
            row["wall_ms"] - row["first_output_ms"]
            if row["first_output_ms"] >= 0 else -1
        )
        return row

    rng = random.Random(20261004)
    raw = {}
    for case, filename, command in (
        ("wrapper_version", "baml-packed", ["--version"]),
        ("empty_program", "tiny-packed", []),
    ):
        variants = {f"{revision}_{mode}": (root / revision / filename, mode)
                    for revision in ("baseline", "candidate") for mode in ("off", "local")}
        raw[case] = {name: [] for name in variants}
        for _ in range(4):
            for binary, mode in variants.values():
                measure(binary, command, mode)
        for _ in range(args.samples):
            order = list(variants)
            rng.shuffle(order)
            for name in order:
                binary, mode = variants[name]
                raw[case][name].append(measure(binary, command, mode))
        print(case, {name: summarize(rows)["wall_ms"] for name, rows in raw[case].items()}, flush=True)
        (root / "timings-raw.json").write_text(json.dumps(raw, indent=2))
    (root / "timings.json").write_text(json.dumps(
        {case: {name: summarize(rows) for name, rows in variants.items()}
         for case, variants in raw.items()}, indent=2))

    clocks = {revision: [] for revision in ("baseline", "candidate")}
    for _ in range(args.samples):
        order = list(clocks)
        rng.shuffle(order)
        for revision in order:
            result = subprocess.run([str(root / revision / "clock-startup")],
                                    env=environment("off"), cwd=project,
                                    check=True, capture_output=True, text=True, timeout=15)
            assert not result.stderr, result.stderr
            clocks[revision].append(json.loads(result.stdout))
    (root / "clocks-raw.json").write_text(json.dumps(clocks, indent=2))
    (root / "clocks.json").write_text(json.dumps(
        {name: summarize(rows) for name, rows in clocks.items()}, indent=2))

    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                ctypes.c_void_p, ctypes.c_int]
    residents = {}
    for revision in ("baseline", "candidate"):
        for mode in ("off", "local"):
            rows = []
            for _ in range(4):
                p = subprocess.Popen([str(root / revision / "baml-packed"), "hold", "0"],
                                     cwd=project, env=environment(mode), stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True)
                line = p.stdout.readline().strip()
                assert line.startswith("ready "), line
                child = int(line.split()[1])
                time.sleep(.15)
                processes = []
                for pid in sorted({p.pid, child}):
                    info = TaskInfo()
                    length = lib.proc_pidinfo(pid, 4, 0, ctypes.byref(info), ctypes.sizeof(info))
                    assert length == ctypes.sizeof(info), (pid, length)
                    processes.append({"role": "child" if pid == child else "wrapper",
                                      "rss_bytes": info.resident, "threads": info.threads})
                _, error = p.communicate(timeout=10)
                assert p.returncode == 0 and not error, (p.returncode, error)
                rows.append({"processes": processes,
                             "rss_bytes": sum(row["rss_bytes"] for row in processes),
                             "threads": sum(row["threads"] for row in processes)})
            residents[f"{revision}_{mode}"] = rows
    (root / "resident.json").write_text(json.dumps(residents, indent=2))

    artifacts = {}
    for revision in ("baseline", "candidate"):
        artifacts[revision] = {}
        for filename in ("baml-cli", "baml-pack-host", "baml-packed", "tiny-packed", "clock-startup"):
            path = root / revision / filename
            artifacts[revision][filename] = {
                "bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()
            }
    (root / "artifacts.json").write_text(json.dumps(artifacts, indent=2))
    print("complete", root, flush=True)


if __name__ == "__main__":
    main()
