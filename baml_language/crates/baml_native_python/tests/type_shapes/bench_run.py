"""One bench job: a case, every configuration, interleaved rounds.

    bench_run.py <case> <n> [rounds] [configs...]

Prints, per configuration, the time per call (the median of the rounds,
each a median of 3 runs in one process) and the instructions per call (the
difference between runs at n and 2n under /usr/bin/time -l, over n). A
configuration is `<sdk> <mode> <model>[ gil]`: today's SDK (`vm`) or the
native bindings, async, sync, or pure Python (`py`), and how models are
made; `held` holds the GIL while BAML runs (released by default). Run setup.sh first, and this
under `perfq run --bench`.
"""

import os
import pathlib
import statistics
import subprocess
import sys

WORK = pathlib.Path(__file__).resolve().parents[1]
PY = WORK / "venv" / "bin" / "python"
BENCH = WORK / "tests" / "bench.py"

DEFAULT = [
    "vm async validate",
    "vm sync validate",
    "native async validate",
    "native async construct",
    "native async native",
    "native sync validate",
    "native sync construct",
    "native sync native",
    "py py validate",
    "py py construct",
    "py py native",
]


def env(config):
    sdk, _, model, *rest = config.split()
    path = WORK / ("vm" if sdk == "vm" else "native")
    return dict(
        os.environ,
        PYTHONPATH=f"{path}:{WORK / 'tests'}",
        BENCH_MODEL=model,
        BENCH_RELEASE_GIL="0" if "held" in rest else "1",
    )


def run(config, case, n, inner):
    mode = config.split()[1]
    out = subprocess.run(
        [str(PY), str(BENCH), mode, case, str(n), str(inner)],
        env=env(config), check=True, capture_output=True, text=True,
    ).stdout
    return float(out.split()[3])


def instructions(config, case, n):
    mode = config.split()[1]
    counts = []
    for size in (n, 2 * n):
        err = subprocess.run(
            ["/usr/bin/time", "-l", str(PY), str(BENCH), mode, case, str(size), "1"],
            env=env(config), check=True, capture_output=True, text=True,
        ).stderr
        counts.append(next(int(line.split()[0]) for line in err.splitlines() if "instructions retired" in line))
    return (counts[1] - counts[0]) // n


def main():
    case, n = sys.argv[1], int(sys.argv[2])
    rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    configs = sys.argv[4:] or DEFAULT
    for config in configs:
        run(config, case, n, 1)  # warm caches and first-run scans
    times = {config: [] for config in configs}
    for _ in range(rounds):
        for config in configs:
            times[config].append(run(config, case, n, 3))
    print(f"case={case} n={n} rounds={rounds}")
    for config in configs:
        ns = statistics.median(times[config]) * 1e9
        print(f"{config:28} {ns:14.1f} ns {instructions(config, case, n):12d} instr", flush=True)


if __name__ == "__main__":
    main()
