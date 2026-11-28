"""The Rust bench's timed rounds: native bindings against today's SDK.

    bench_run.py <case> <n> [rounds]

Run `scripts/bench.sh` first, and this under `perfq run --bench`. Prints
the median time per call (or per element for the array cases) over the
interleaved rounds, and instructions per call from runs at n and 2n under
/usr/bin/time -l.
"""

import os
import pathlib
import re
import statistics
import subprocess
import sys

OUT = pathlib.Path(__file__).resolve().parents[3] / "target" / "native-rust"
ARRAYS = {"ints_out", "ints_in", "points_out", "points_in"}


def command(side, case, n):
    if side == "native":
        return [str(OUT / "bench"), str(OUT / "libtype_shapes.dylib"), case, str(n)]
    return [str(OUT / "bench_today"), case, str(n)]


ENV = dict(
    os.environ,
    BAML_BRIDGE_PATH=str(OUT / "libbridge_cffi.dylib"),
    BAML_BRIDGE_DISABLE_DOWNLOAD="1",
)


def per_call(side, case, n):
    out = subprocess.run(
        command(side, case, n), check=True, capture_output=True, text=True, env=ENV
    ).stdout
    ns = float(re.search(r"([0-9.]+) ns per call", out).group(1))
    return ns / n if case in ARRAYS else ns


def instructions(side, case, n):
    counts = []
    for size in (n, 2 * n):
        err = subprocess.run(
            ["/usr/bin/time", "-l", *command(side, case, size)],
            check=True, capture_output=True, text=True, env=ENV,
        ).stderr
        counts.append(next(int(l.split()[0]) for l in err.splitlines() if "instructions retired" in l))
    return (counts[1] - counts[0]) // n


def main():
    case, n = sys.argv[1], int(sys.argv[2])
    rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 5
    times = {"native": [], "today": []}
    for _ in range(rounds):
        for side in times:
            times[side].append(per_call(side, case, n))
    print(f"case={case} n={n} rounds={rounds}")
    for side, values in times.items():
        print(f"{side:8} {statistics.median(values):12.1f} ns {instructions(side, case, n):12d} instr")


if __name__ == "__main__":
    main()
