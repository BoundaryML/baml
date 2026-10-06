"""Compare production baml pack vs --no-prune, measuring complete executions.

Example (macOS):
  python3 scripts/runtime-startup/measure-pruning.py --compiler target/release/baml-cli \
    --host target/release/baml-pack-host --child /absolute/path/to/fixed/baml-cli \
    --output target/pruning-production --runs 100
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
CASES = {
    "empty": ("empty", []),
    "features": ("features", []),
    "wrapper_version": ("wrapper", ["--version"]),
    "wrapper_child": ("wrapper", ["run", "--help"]),
}
SOURCES = {
    "empty": "function Main() -> void {}\n",
    "features": '''class Result { name: string, count: int }
function leaf(n: int) -> int { n }
function add(a: int, b: int = 10, c: int = 20) -> int { a + b + c }
function fail() -> int { throw "expected" }
function Main() -> Result {
    let top = 40.0;
    let values = [1, 2].map((n: int) -> int { leaf(n) });
    let caught = fail() catch (e) { _ => 0 };
    Result { name: baml.json.to_string(top), count: add(1) + add(1, c = 3, b = 2) + values[0] + values[1] + caught }
}
''',
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def workers():
    rows = subprocess.check_output(["ps", "-axo", "pid,etime,comm"], text=True).splitlines()
    return [r.strip() for r in rows if r.split()[-1].rsplit("/", 1)[-1] in
            ["cargo", "rustc", "clippy-driver"]]


def idle():
    while active := workers():
        print("Waiting for build workers:", active, flush=True)
        time.sleep(5)


def summary(samples):
    result = {"n": len(samples)}
    for key in samples[0]:
        values = sorted(s[key] for s in samples)
        result[key] = {"median": statistics.median(values),
                       "p95": values[(95 * len(values) + 99) // 100 - 1]}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--child", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=100)
    parser.add_argument("--seed", type=int, default=20261004)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    compiler, host, child, out = (p.resolve() for p in
                                  [args.compiler, args.host, args.child, args.output])
    out.mkdir(parents=True, exist_ok=True)
    helper = out / "measure"
    subprocess.run(["/usr/bin/clang", "-O2", str(ROOT / "scripts/btel-startup/measure.c"),
                    "-o", str(helper)], check=True)
    home = out / "home"
    baml_home = home / ".baml"
    baml_home.mkdir(parents=True, exist_ok=True)
    (baml_home / "config.toml").write_text("[update]\nauto_check = false\n")
    env = {"PATH": "/usr/bin:/bin", "LANG": "en_US.UTF-8", "NO_COLOR": "1",
           "HOME": str(home), "BAML_HOME": str(baml_home),
           "BAML_CLI_ALLOW_DIRECT": "1", "BAML_TOOLCHAIN": str(child)}
    for name, source in SOURCES.items():
        path = out / "sources" / name / "main.baml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source)
    for name in ["empty", "features", "wrapper"]:
        for variant in ["baseline", "pruned"]:
            binary = out / "cases" / name / variant / "app"
            binary.parent.mkdir(parents=True, exist_ok=True)
            cmd = [str(compiler), "pack", "Main", "--host", str(host), "--output", str(binary)]
            cmd += (["--from", str(ROOT / "crates/baml")] if name == "wrapper" else
                    ["--file", str(out / "sources" / name / "main.baml")])
            if variant == "baseline":
                cmd.append("--no-prune")
            # Packing gets a persistent isolated build cache, shared across variants.
            packed = subprocess.run(cmd, env=env | {"BAML_TELEMETRY": "off"},
                                    capture_output=True, text=True)
            if packed.returncode:
                raise RuntimeError(f"packing {name}/{variant} failed:\n{packed.stderr}")

    def binary(case, variant):
        return out / "cases" / CASES[case][0] / variant / "app"

    def run_env(mode):
        return env | ({"BAML_TELEMETRY": "off"} if mode == "off" else {})

    smoke = {}
    for case, (_, params) in CASES.items():
        pair = []
        for variant in ["baseline", "pruned"]:
            r = subprocess.run([str(binary(case, variant)), *params], env=run_env("off"),
                               capture_output=True, timeout=20)
            assert r.returncode == 0 and not r.stderr, (case, variant, r.returncode, r.stderr)
            pair.append(r.stdout)
        assert pair[0] == pair[1], (case, pair)
        smoke[case] = pair[0].decode()

    cells = [(case, variant, mode) for case in CASES for variant in ["baseline", "pruned"]
             for mode in ["off", "local"]]
    raw = {"/".join(cell): [] for cell in cells}

    def measure(case, variant, mode):
        r = subprocess.run([str(helper), str(binary(case, variant)), *CASES[case][1]],
                           env=run_env(mode), capture_output=True, text=True, timeout=20)
        assert r.returncode == 0 and not r.stderr, (case, variant, mode, r.returncode, r.stderr)
        sample = json.loads(r.stdout)
        assert sample["exit"] == 0 and sample["stdout_bytes"] == len(smoke[case].encode()), sample
        sample["cpu_ms"] = sample["user_ms"] + sample["system_ms"]
        return sample

    idle()
    for _ in range(5):
        for cell in cells:
            measure(*cell)
    rng = random.Random(args.seed)
    activity = []
    started = time.time_ns()
    for n in range(args.runs):
        idle()
        activity.append({"round": n, "time_ns": time.time_ns(), "workers": workers()})
        rng.shuffle(cells)
        for cell in cells:
            raw["/".join(cell)].append(measure(*cell))
        if (n + 1) % 20 == 0:
            print(f"{n + 1}/{args.runs} rounds", flush=True)
    summaries = {key: summary(values) for key, values in raw.items()}
    result = {"methodology": {
        "platform": platform.platform(), "samples_per_cell": args.runs,
        "warmups": 5, "seed": args.seed, "order": "shuffled paired fresh processes; warmed file caches",
        "timing": "native clock before posix_spawn until after wait4, including exit/shutdown/destruction",
        "telemetry": "off and default local, isolated HOME, no cloud credentials",
        "compiler_sha256": sha(compiler), "host_sha256": sha(host), "child_sha256": sha(child),
        "harness_sha256": sha(Path(__file__)), "helper_sha256": sha(ROOT / "scripts/btel-startup/measure.c"),
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "started_ns": started, "finished_ns": time.time_ns(),
        "resources": "wait4 root-process rusage; RSS is not aggregate process-tree memory",
        "external_activity": "round-start build-worker snapshots; ordinary desktop activity uncontrolled",
        "command": vars(args) | {k: str(v) for k, v in vars(args).items() if isinstance(v, Path)},
    }, "summary": summaries, "raw": raw, "activity": activity, "smoke": smoke,
        "artifacts": {f"{name}/{variant}": {"bytes": (out / "cases" / name / variant / "app").stat().st_size,
                    "sha256": sha(out / "cases" / name / variant / "app")}
                    for name in ["empty", "features", "wrapper"] for variant in ["baseline", "pruned"]}}
    (out / "results.json").write_text(json.dumps(result, indent=2))
    for mode in ["local", "off"]:
        for case in CASES:
            before = summaries[f"{case}/baseline/{mode}"]["wall_ms"]
            after = summaries[f"{case}/pruned/{mode}"]["wall_ms"]
            print(f'{mode} {case}: {before["median"]:.2f}/{before["p95"]:.2f} -> '
                  f'{after["median"]:.2f}/{after["p95"]:.2f} ms median/p95')


if __name__ == "__main__":
    main()
