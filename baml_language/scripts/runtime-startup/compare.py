#!/usr/bin/env python3
"""Compare ordinary packed programs with the startup-experiment prototype.

Build the baseline compiler/host/wrapper normally and the candidate with
--features startup-experiment, using the same release profile and toolchain.
Each directory must contain baml-cli and baml-pack-host. Run only
when builds/tests are idle. No user configuration, keys or cloud endpoints are used.
"""

import argparse
import hashlib
import json
import os
import random
import statistics
import subprocess
from pathlib import Path

SOURCES = {
    "empty": "function Main() -> void {}\n",
    "hello": 'function Main() -> void { baml.io.println("ready"); }\n',
    "features": """
class Result { name: string, count: int }
function leaf(n: int) -> int { n }
function add(a: int, b: int = 10, c: int = 20) -> int { a + b + c }
function fail() -> int { throw "expected" }
function Main() -> Result {
    let top = 40.0;
    let values = [1, 2].map((n: int) -> int { leaf(n) });
    let caught = fail() catch (e) { _ => 0 };
    Result { name: baml.json.to_string(top), count: add(1) + add(1, c = 3, b = 2) + values[0] + values[1] + caught }
}
""",
    "large": "\n".join(
        [f"function F{i}(x: int) -> int {{ x + {i} }}" for i in range(1000)]
        + ["function Main() -> int { F999(1) }"]
    ),
    "typed": "function Main(name: string, n: int = 2) -> string { name + baml.json.to_string(n) }\n",
    "error": "function leaf() -> int { 1 / 0 }\nfunction Main() -> int { leaf() }\n",
}


def run(command, env, **kwargs):
    result = subprocess.run(
        command, env=env, capture_output=True, check=False, **kwargs
    )
    if result.returncode:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {command}\n{result.stdout.decode()}\n{result.stderr.decode()}"
        )
    return result


def summarize(samples):
    result = {"n": len(samples)}
    for key in samples[0]:
        values = sorted(s[key] for s in samples)
        result[key] = {
            "median": statistics.median(values),
            "p95": values[(len(values) * 95 + 99) // 100 - 1],
            "min": values[0],
            "max": values[-1],
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--runs", type=int, default=100)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=True)
    revisions = {k: getattr(args, k).resolve() for k in ("baseline", "candidate")}
    helper_source = Path(__file__).resolve().parent.parent / "btel-startup"
    for name in ("measure", "child"):
        subprocess.run(
            [
                "/usr/bin/clang",
                "-O2",
                str(helper_source / f"{name}.c"),
                "-o",
                str(root / name),
            ],
            check=True,
        )
    home = root / "home"
    baml_home = home / ".baml"
    baml_home.mkdir(parents=True, exist_ok=True)
    (baml_home / "config.toml").write_text("[update]\nauto_check = false\n")
    base_env = {
        "PATH": "/usr/bin:/bin",
        "LANG": "en_US.UTF-8",
        "NO_COLOR": "1",
        "HOME": str(home),
        "BAML_HOME": str(baml_home),
        "BAML_CLI_ALLOW_DIRECT": "1",
        "BAML_TOOLCHAIN": str(root / "child"),
    }
    pack_env = base_env | {"BAML_TELEMETRY": "off"}
    artifacts = {}
    wrapper_source = Path(__file__).resolve().parents[2] / "crates/baml/baml_src"
    for revision, directory in revisions.items():
        result = run(
            [
                str(directory / "baml-cli"),
                "pack",
                "Main",
                "--project",
                str(wrapper_source),
                "--host",
                str(directory / "baml-pack-host"),
                "--output",
                str(directory / "baml-packed"),
            ],
            pack_env,
        )
        (root / f"{revision}-wrapper-pack.log").write_bytes(
            result.stdout + result.stderr
        )
    for name, source in SOURCES.items():
        project = root / "projects" / name
        project.mkdir(parents=True, exist_ok=True)
        (project / "baml.toml").write_text(f'[package]\nname = "startup_{name}"\n')
        (project / "main.baml").write_text(source)
        for revision, directory in revisions.items():
            output = directory / f"{name}-packed"
            result = run(
                [
                    str(directory / "baml-cli"),
                    "pack",
                    "Main",
                    "--project",
                    str(project),
                    "--host",
                    str(directory / "baml-pack-host"),
                    "--output",
                    str(output),
                ],
                pack_env,
            )
            (root / f"{revision}-{name}-pack.log").write_bytes(
                result.stdout + result.stderr
            )
            artifacts[f"{revision}/{name}"] = {
                "bytes": output.stat().st_size,
                "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
            }
    commands = {
        "empty": [],
        "hello": [],
        "features": [],
        "large": [],
        "typed": ["--name", "test"],
        "typed_help": ["--help"],
        "wrapper_version": ["--version"],
        "wrapper_child": ["noop"],
    }

    def binary(revision, case):
        directory = revisions[revision]
        return directory / (
            "baml-packed"
            if case.startswith("wrapper")
            else f"{'typed' if case == 'typed_help' else case}-packed"
        )

    smoke = {}
    for case, command in commands.items():
        outputs = {}
        for revision in revisions:
            result = run([str(binary(revision, case)), *command], pack_env)
            assert not result.stderr, (case, revision, result.stderr)
            outputs[revision] = result.stdout.decode()
        assert outputs["baseline"] == outputs["candidate"], (case, outputs)
        smoke[case] = outputs["candidate"]
    assert json.loads(smoke["features"]) == {"name": "40.0", "count": 40}
    assert json.loads(smoke["large"]) == 1000
    assert json.loads(smoke["typed"]) == "test2"
    # Both paths must report the same original source file and line at failure.
    error_outputs = []
    for revision, directory in revisions.items():
        result = subprocess.run(
            [str(directory / "error-packed")],
            env=pack_env,
            capture_output=True,
            check=False,
        )
        assert (
            result.returncode != 0
            and b"main.baml" in result.stderr
            and b"leaf" in result.stderr
        ), result
        error_outputs.append(result.stdout + result.stderr)
        (root / f"{revision}-error.txt").write_bytes(result.stdout + result.stderr)
    assert error_outputs[0] == error_outputs[1], error_outputs
    (root / "smoke.json").write_text(json.dumps(smoke, indent=2))
    variants = [
        (case, revision, mode)
        for case in commands
        for revision in revisions
        for mode in ("off", "local")
    ]
    data = {"/".join(v): [] for v in variants}

    def measure(case, revision, mode):
        env = base_env | ({"BAML_TELEMETRY": "off"} if mode == "off" else {})
        result = run(
            [str(root / "measure"), str(binary(revision, case)), *commands[case]], env
        )
        sample = json.loads(result.stdout)
        assert sample["exit"] == 0 and not result.stderr, (
            case,
            revision,
            mode,
            result.stderr,
            sample,
        )
        sample["cpu_ms"] = sample["user_ms"] + sample["system_ms"]
        return sample

    for _ in range(3):
        for variant in variants:
            measure(*variant)
    rng = random.Random(20261004)
    for n in range(args.runs):
        rng.shuffle(variants)
        for variant in variants:
            data["/".join(variant)].append(measure(*variant))
        if (n + 1) % 20 == 0:
            print(f"{n + 1}/{args.runs} rounds", flush=True)
    summaries = {name: summarize(samples) for name, samples in data.items()}
    for directory in revisions.values():
        for name in ("baml-cli", "baml-pack-host", "baml-packed"):
            path = directory / name
            artifacts[str(path)] = {
                "bytes": path.stat().st_size,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
    metadata = {
        "platform": os.uname().sysname,
        "machine": os.uname().machine,
        "runs": args.runs,
        "seed": 20261004,
        "warmups": 3,
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "revisions": {k: str(v) for k, v in revisions.items()},
    }
    (root / "results.json").write_text(
        json.dumps(
            {
                "metadata": metadata,
                "summary": summaries,
                "raw": data,
                "artifacts": artifacts,
            },
            indent=2,
        )
    )
    print(
        json.dumps(
            {
                k: {
                    metric: round(v[metric]["median"], 3)
                    for metric in ("wall_ms", "first_output_ms", "cpu_ms")
                }
                for k, v in summaries.items()
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
