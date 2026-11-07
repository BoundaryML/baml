# speedtest

BAML's benchmark suite. Every workload is one program written in BAML, Python
and JavaScript; every implementation must return the same value, and every
trial is checked against it. A run measures each workload as BAML (the VM, at
each telemetry level) and as the reference ports (Node, Bun, Python), and
writes a report: `kpis.json` and a self-contained `kpis.html`.

```
cd baml_language
cargo build --release -p baml_cli -p baml_pack_host

scripts/speedtest run --kpi                    # the KPI suite (kpis.toml), every telemetry level
scripts/speedtest run                          # every workload, VM + references, 5 trials
scripts/speedtest run --filter richards        # workloads whose id or name contains "richards"
scripts/speedtest run --rows vm-off,node       # only these rows
scripts/speedtest run --out bench-report       # also copy the bundle to ./bench-report (e.g. a CI artifact)
scripts/speedtest list                         # every workload
scripts/speedtest compare canary my-branch     # per-workload deltas between two saved baselines
scripts/speedtest report canary/latest         # re-render a saved run's report
```

Every run prints a table and the path of its report. The bundle lives under
`~/.speedtest/runs/<id>/` and is saved as the baseline `<branch>/latest`
(`--tag NAME` saves it under a name too). Needs Python 3.11+ and `uv`; Node
and Bun rows are skipped, and drawn as "not installed", when the tool is
missing.

## Workloads

```
workloads/<category>/<slug>/
  bench.toml   name, description, origin, expect, jobs, warmup
  main.baml    function main() -> T   one job
  main.py      def main()             the same job in Python
  main.js      function main()        the same job in JavaScript
```

`main()` performs one job and returns a deterministic value; `expect` in
`bench.toml` is that value as a string (`string.from` in BAML, `str` in
Python, `String` in JS). A workload that needs state built once and kept, such
as a large live graph, also defines `setup()` and `run(state)`: the harness
then calls `setup()` once, untimed, and times `run(state)`. `main()` stays a
complete job (`run(setup())`), so every `main.baml` is runnable on its own and
compiles into the CodSpeed benches (`crates/baml_tests/build.rs` generates one
`vm_speedtest_*` bench per workload).

`jobs` is how many jobs one trial times, `warmup` how many untimed jobs run
first; size `jobs` so a VM trial times roughly half a second. `category::name`
is the workload's identity: CodSpeed bench names and saved baselines derive
from it.

To add one: create the directory, write the three programs, run
`scripts/speedtest run --filter <slug> --trials 1`; the verify step prints each
implementation's result, which becomes `expect`. Inputs are generated inside
the program (a seeded LCG, identical in all three languages), never pasted in.

## What is measured

Each **trial** is one fresh process. A small driver per language
(`src/speedtest/drivers/`) loads the workload, calls `main()` once to verify,
runs `warmup` jobs, then times `jobs` jobs in-process and prints one JSON line.
The parent records process wall time, CPU time and peak RSS from `wait4`. So
throughput excludes process start and runtime load, and startup is its own KPI
(`process::startup`, a `main` that returns at once).

A run first packs every `main.baml` with the driver (`baml pack`; pack time
and executable size are results), then verifies every (workload, row) once
(a row that fails is reported and not measured), then runs `trials` rounds.
Each round runs every pair once in a seeded shuffled order, so drift spreads
across rows instead of landing on one. Every trial's result is checked again.
The report gives the median, min and max over trials; `trials.csv` has every
trial.

BAML rows set `BAML_TELEMETRY` and record into a fresh `BAML_HOME` per trial;
the recording's size is kept per trial. BAML has no monotonic clock, so its
in-process timer is the wall clock: run on an idle machine, and `--cpu N`
pins every trial to one CPU on Linux.

## KPIs

`kpis.toml` lists the KPIs in page order: a question, a metric
(`throughput`, `process_ms`, `peak_rss_mib`, `binary_kib`, `build_s`) and the
workloads that answer it. `run --kpi` runs exactly those workloads. Each KPI is
one chart with every row as a bar; a row with no value says why instead of
disappearing.

## Rows

`src/speedtest/rows.py` defines the rows: `vm-off`, `vm-low`, `vm-medium`,
`vm-high`, `native-off`, `native-medium`, `rt-off`, `rt-medium`, `node`, `bun`, `python`. A new implementation, such as a natively
compiled BAML backend, is one more `Row` plus how to build and invoke it;
every chart, table and the report pick it up.

### Compiled backends

`native-off` / `native-medium` pack with `--rust` (compiled bodies on the VM).
`rt-off` / `rt-medium` pack with `--rust --rt` (baml_rt, no VM). Each backend
packs once per workload; levels share that executable via `BAML_TELEMETRY`.
Native packs use the release profile and one Cargo build directory per backend
across workloads. Pack time includes compilation with that progressively warm
cache; the first admitted workload pays the cold dependency build cost.
`--rust-build-dir DIR` retains that cache across runs (backend subdirectories;
only one run may use it at a time). Cached dependencies affect pack time, so
record whether the cache was warm when comparing build KPIs.

VM/native pack the BAML speedtest driver. RT packs `main` and uses its native
`--jobs` / `--warmup` JSON driver, retaining its runtime instance across jobs.
Both protocols verify once, warm up, then time complete jobs; RT additionally
checks each timed result and uses a monotonic clock. A future admitted workload
with `setup`/`run` fails explicitly until RT can preserve its setup state.
Only an unsuccessful RT pack with the compiler's `unsupported in baml_rt:`
diagnostic is marked **unsupported by baml_rt**. Other build, verification and
trial errors remain failures. Unsupported cells have no numeric values or ratios.

Explicit `--rows` is respected with `--kpi`; without it, KPI runs include every
BAML row. The terminal and report show **rt speedup vs VM(off)** and **rt speedup
vs Node**: comparison time divided by rt-off time, so above 1x means RT is faster.
The report JSON includes per-backend build records and the sampled one-minute
load range. `--cpu N` pins all verification and timed subprocesses to that CPU;
trial order remains interleaved with a reproducible shuffle.

Tests: `uv run --project tools/speedtest python -m unittest discover -s tools/speedtest/tests`
(from `baml_language`).
