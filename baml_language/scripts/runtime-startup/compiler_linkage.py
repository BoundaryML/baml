#!/usr/bin/env python3
"""Measure compiler linkage with matched packed-host controls on macOS.

Build the compiler-free hosts with build_compiler_free.py.
This harness compares them with the full hosts from the same source/library
selection, verifies identical embedded program bytes, checks observable behavior,
then interleaves fresh launches with builds idle. Runtime compilation is expected
to fail in the diagnostic hosts; this is not a production feature change.
"""

import argparse
import hashlib
import json
import platform
import random
import statistics
import struct
import subprocess
import sys
from itertools import pairwise
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def embedded_program(path):
    """Read the packed section from a thin little-endian 64-bit Mach-O file."""
    data = path.read_bytes()
    assert struct.unpack_from("<I", data)[0] == 0xFEEDFACF, path
    command_count = struct.unpack_from("<I", data, 16)[0]
    cursor = 32
    found = []
    for _ in range(command_count):
        command, size = struct.unpack_from("<II", data, cursor)
        assert size >= 8 and cursor + size <= len(data), path
        if command == 0x19:
            section_count = struct.unpack_from("<I", data, cursor + 64)[0]
            assert 72 + section_count * 80 <= size, path
            for index in range(section_count):
                section = cursor + 72 + index * 80
                name = data[section : section + 16].rstrip(b"\0")
                if name == b"baaaaaaaaaaaaaml":
                    length, offset = struct.unpack_from("<QI", data, section + 40)
                    assert offset + length <= len(data), path
                    found.append(data[offset : offset + length])
        cursor += size
    assert len(found) == 1, (path, len(found))
    return found[0]


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
    parser.add_argument("--full-hosts", type=Path, required=True)
    parser.add_argument("--without-compiler-hosts", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=100)
    args = parser.parse_args()
    if sys.platform != "darwin" or args.runs < 1:
        parser.error("Requires macOS and a positive --runs value.")
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    hosts = {
        "full": args.full_hosts.resolve(),
        "without_compiler": args.without_compiler_hosts.resolve(),
    }
    compilers = {
        name: getattr(args, name).resolve() / "baml-cli"
        for name in ("baseline", "candidate")
    }
    fixtures = args.fixtures.resolve()
    metadata = {
        name: json.loads((directory / "build-metadata.json").read_text())
        for name, directory in hosts.items()
    }
    assert metadata["without_compiler"]["without_compiler"]
    assert metadata["without_compiler"]["strategy"] == "runtime-only project facade"
    for revision in compilers:
        full = metadata["full"][revision]
        lean = metadata["without_compiler"][revision]
        assert full["host"] == lean["host"]
        assert {
            k: v for k, v in full["libraries"].items() if k != "bex_project"
        } == lean["libraries"]
        assert set(lean["rebuilt"]) == {"bex_project", "bridge_ctypes", "sys_native"}
    for variant in ("control", "profile"):
        full_source = (hosts["full"] / f"{variant}.rs").read_text()
        lean_source = (hosts["without_compiler"] / f"{variant}.rs").read_text()
        assert full_source.count("Some(bex_project::runtime_compiler())") == 2
        assert (
            full_source.replace("Some(bex_project::runtime_compiler())", "None")
            == lean_source
        )

    home = out / "home"
    baml_home = home / ".baml"
    baml_home.mkdir(parents=True, exist_ok=True)
    (baml_home / "config.toml").write_text("[update]\nauto_check = false\n")
    env = {
        "PATH": "/usr/bin:/bin",
        "LANG": "en_US.UTF-8",
        "NO_COLOR": "1",
        "HOME": str(home),
        "BAML_HOME": str(baml_home),
        "BAML_CLI_ALLOW_DIRECT": "1",
        "BAML_TOOLCHAIN": str(fixtures / "child"),
    }
    off = env | {"BAML_TELEMETRY": "off"}
    compile_project = out / "projects/compile"
    compile_project.mkdir(parents=True, exist_ok=True)
    (compile_project / "main.baml").write_text("""function Main() -> int {
  let package = reflect.Package.compile({ "leaf.baml": "function answer() -> int { 42 }" });
  let answer = package.get_function<() -> int>("root.answer") ?? throw "missing answer";
  answer()
}
""")
    projects = {
        name: fixtures / f"projects/{name}"
        for name in ("empty", "hello", "large", "features", "typed", "error")
    }
    projects["wrapper"] = root / "crates/baml/baml_src"
    projects["compile"] = compile_project
    executables = {}
    artifacts = {}
    for revision, compiler in compilers.items():
        for linkage, directory in hosts.items():
            for variant in ("control", "profile"):
                host = directory / f"{revision}-{variant}-host"
                cases = (
                    projects
                    if variant == "control"
                    else {k: projects[k] for k in ("empty", "wrapper")}
                )
                for case, project in cases.items():
                    key = (revision, linkage, variant, case)
                    exe = out / "-".join(key)
                    result = run(
                        [
                            str(compiler),
                            "pack",
                            "Main",
                            "--project",
                            str(project),
                            "--host",
                            str(host),
                            "--output",
                            str(exe),
                        ],
                        off,
                    )
                    (out / f"{exe.name}-pack.log").write_bytes(
                        result.stdout + result.stderr
                    )
                    assert result.returncode == 0, (key, result.stdout, result.stderr)
                    payload = embedded_program(exe)
                    executables[key] = exe
                    artifacts["/".join(key)] = {
                        "bytes": exe.stat().st_size,
                        "sha256": digest(exe),
                        "program_bytes": len(payload),
                        "program_sha256": hashlib.sha256(payload).hexdigest(),
                    }
        for variant in ("control", "profile"):
            cases = projects if variant == "control" else ("empty", "wrapper")
            for case in cases:
                assert embedded_program(
                    executables[revision, "full", variant, case]
                ) == embedded_program(
                    executables[revision, "without_compiler", variant, case]
                ), (revision, variant, case)
        for case in ("empty", "wrapper"):
            assert embedded_program(
                executables[revision, "full", "control", case]
            ) == embedded_program(executables[revision, "full", "profile", case])

    smoke = {}
    for revision in compilers:
        for case in projects:
            child_args = (
                ["--version"]
                if case == "wrapper"
                else ["--name", "Ada"]
                if case == "typed"
                else []
            )
            results = {}
            for linkage in hosts:
                exe = executables[revision, linkage, "control", case]
                result = run([str(exe), *child_args], off)
                results[linkage] = {
                    "exit": result.returncode,
                    "stdout": result.stdout.decode(),
                    "stderr": result.stderr.decode(),
                }
            if case == "compile":
                assert (
                    results["full"]["exit"] == 0
                    and results["full"]["stdout"].strip() == "42"
                ), results
                assert (
                    results["without_compiler"]["exit"] != 0
                    and "runtime compiler was not installed"
                    in results["without_compiler"]["stderr"]
                ), results
            else:
                assert results["full"] == results["without_compiler"], (
                    revision,
                    case,
                    results,
                )
                assert results["full"]["exit"] == (1 if case == "error" else 0), results
            smoke[f"{revision}/{case}"] = results
        for linkage in hosts:
            for case in ("empty", "wrapper"):
                exe = executables[revision, linkage, "profile", case]
                result = run(
                    [str(exe), *([] if case == "empty" else ["--version"])], off
                )
                assert result.returncode == 0 and result.stderr.startswith(
                    b"BAML_STARTUP_PROFILE:"
                ), result.stderr
                assert (
                    result.stdout.decode()
                    == smoke[f"{revision}/{case}"][linkage]["stdout"]
                )

    loader = {}
    for revision in compilers:
        for linkage, directory in hosts.items():
            exe = directory / f"{revision}-control-host"
            info = {"bytes": exe.stat().st_size, "sha256": digest(exe)}
            for option, key in (
                ("-inits", "initializers"),
                ("-fixups", "fixups"),
                ("-imports", "imports"),
            ):
                result = subprocess.run(
                    ["/usr/bin/xcrun", "dyld_info", option, str(exe)],
                    capture_output=True,
                    check=True,
                )
                (out / f"{revision}-{linkage}-{key}.txt").write_bytes(result.stdout)
                lines = result.stdout.decode().splitlines()
                info[key] = (
                    sum(line.strip().startswith("0x") for line in lines)
                    if key == "initializers"
                    else sum(line.lstrip().startswith("__") for line in lines)
                    if key == "fixups"
                    else sum("(from " in line for line in lines)
                )
            loader[f"{revision}/{linkage}"] = info

    variants = [
        (revision, linkage, variant, case, mode)
        for revision in compilers
        for linkage in hosts
        for variant in ("control", "profile")
        for case in (
            ("empty", "hello", "large", "wrapper")
            if variant == "control"
            else ("empty", "wrapper")
        )
        for mode in ("off", "local")
    ]
    data = {"/".join(variant): [] for variant in variants}

    def sample(variant):
        revision, linkage, kind, case, mode = variant
        exe = executables[revision, linkage, kind, case]
        result = run(
            [
                str(hosts["full"] / "measure"),
                str(exe),
                *(["--version"] if case == "wrapper" else []),
            ],
            env | ({"BAML_TELEMETRY": "off"} if mode == "off" else {}),
        )
        assert result.returncode == 0, result.stderr
        value = json.loads(result.stdout)
        assert value["exit"] == 0, (variant, value, result.stderr)
        value["cpu_ms"] = value["user_ms"] + value["system_ms"]
        if kind == "profile":
            prefix = b"BAML_STARTUP_PROFILE:"
            assert (
                result.stderr.startswith(prefix) and result.stderr.count(prefix) == 1
            ), result.stderr
            marks = json.loads(result.stderr[len(prefix) :])
            ordered = [
                ("parent_start", value["start_ns"]),
                *marks.items(),
                ("stdout_eof", value["eof_ns"]),
                ("counters_end", value["counters_end_ns"]),
                ("parent_end", value["end_ns"]),
            ]
            assert all(a[1] <= b[1] for a, b in pairwise(ordered)), ordered
            value["marks"] = marks
            value["phases_ms"] = {
                b[0]: (b[1] - a[1]) / 1e6 for a, b in pairwise(ordered)
            }
            assert abs(sum(value["phases_ms"].values()) - value["wall_ms"]) < 0.001
        else:
            assert not result.stderr, result.stderr
        return value

    for _ in range(3):
        for variant in variants:
            sample(variant)
    seed = 2026100403
    randomizer = random.Random(seed)
    print(
        "Correctness and identical-program checks passed. Measuring with builds idle.",
        flush=True,
    )
    for index in range(args.runs):
        randomizer.shuffle(variants)
        for variant in variants:
            data["/".join(variant)].append(sample(variant))
        if (index + 1) % 20 == 0:
            print(f"{index + 1}/{args.runs} rounds", flush=True)
    summary = {key: summarize(values) for key, values in data.items()}
    report = {
        "metadata": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "seed": seed,
            "runs": args.runs,
            "warmups": 3,
            "source_commit": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "builds": metadata,
            "harness_sha256": digest(Path(__file__)),
            "helper_sha256": digest(hosts["full"] / "measure"),
        },
        "artifacts": artifacts,
        "loader": loader,
        "smoke": smoke,
        "summary": summary,
        "raw": data,
    }
    (out / "results.json").write_text(json.dumps(report, separators=(",", ":")) + "\n")
    for key, value in summary.items():
        print(
            key,
            round(value["wall_ms"]["median"], 3),
            round(value["wall_ms"]["p95"], 3),
            flush=True,
        )
    print(json.dumps(loader, indent=2), flush=True)


if __name__ == "__main__":
    main()
