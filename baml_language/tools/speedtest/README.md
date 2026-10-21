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
`vm-high`, `node`, `bun`, `python`. A new implementation, such as a natively
compiled BAML backend, is one more `Row` plus how to build and invoke it;
every chart, table and the report pick it up.
