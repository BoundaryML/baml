# gcbench

gcbench measures what the garbage collector costs BAML programs, in CPU time and
in memory, and how that cost moves when you change a collector setting or the
build.

`tools/speedtest` reports time and peak memory for a fixed set of rows. gcbench
is for collector work:

- it sweeps an environment variable (for example `BAML_GC_GROWTH`) and reports
  one row per value against the defaults;
- it verifies the result of every run;
- it interleaves rounds, so drift on the machine hits every row equally;
- it reports collections and pauses for every row.

It needs Python 3.11 or newer and nothing else. It runs on macOS and Linux.

## A first table

From `baml_language/`:

```sh
cargo build --release -p baml_cli -p baml_pack_host
python3 tools/gcbench/gcbench.py build          # pack, verify, count collections (untimed)
python3 tools/gcbench/gcbench.py run --rounds 5  # measure; prints the summary
```

`run` writes `results.json` (every run) and `summary.md` (the tables) to
`tools/gcbench/results/<timestamp>/`; `gcbench.py summary <results.json>` prints
the tables again.

The default config, `variants.toml`, compares this checkout's release build at
its default settings (the baseline, `vm`) with `BAML_GC_GROWTH` at 1.5, 2, 4 and
8, `BAML_GC_MIN_GROWTH` at 2, 8 and 32 MiB, and one row with both lowered.
A build without the collector settings ignores these variables, and every row
then matches the baseline.

`--variants growth` (labels or groups; the baseline always comes along),
`--values 2,4` and `--workloads live-heap,splay-2k` pick a subset. A full run of the default config takes a few minutes per round.

## Compare two builds

Build `baml-cli` and `baml-pack-host` in both checkouts, then point
`configs/compare-builds.toml` at the other one:

```sh
GCBENCH_BASE=../other-checkout/baml_language \
  python3 tools/gcbench/gcbench.py run --config tools/gcbench/configs/compare-builds.toml
```

A variant names its `baml_cli` (and, optionally, its `pack_host`). Relative paths
are relative to `baml_language/`. `${VAR}` and `~` are expanded. The keys are
described at the top of `variants.toml`. A variant can also bake settings in at
pack time with `pack_toml`, for example `"[pack.gc]\ngrowth = 2"`.

## What a run does

Each workload from `tools/speedtest/workloads/` is packed twice per build: one
program calls it once (cold), the other N times (steady; `iters_override` in the
config). Every result is folded into an accumulator, so no call can be optimised
away, and the program prints the first result and the accumulator.

- **Verification.** Before anything is timed, each program runs once per row.
  The first result must equal the workload's `expect`, and the accumulators of
  all rows must agree. A row that fails is dropped from that workload and
  reported loudly. Every timed run is checked again; if any crashed or gave a
  wrong result, `run` still writes the results and exits with status 4.
- **Rounds.** In each round every workload runs every row, cold then steady.
  The row order rotates from round to round. The tables show the best round
  and, in brackets, the median.
- **Steady** cost per iteration is (cost(N) − cost(1)) / (N − 1). **Cold** is the
  1-iteration program: process start, setup and one call.
- **GC probe.** `build` runs each program once more, untimed, with telemetry on.
  The VM records every collection as a `baml.gc` span, and
  `baml-cli local recordings query` reads the spans back: the number of
  collections and each pause. The collection at shutdown is left out. The timed
  runs have telemetry off unless a variant's `env` sets `BAML_TELEMETRY`.
  `--no-gc-probe` skips the probe.
- **Environment.** Inherited `BAML_GC_*`, `BAML_TELEMETRY`, `MALLOC_*`,
  `GLIBC_TUNABLES` and every swept variable are dropped. Only a variant sets
  them. Each run gets a fresh, empty `BAML_HOME`, whatever the variant says.

## Reading the tables

- **Per workload.** One line per row: steady and cold CPU, steady wall time per
  iteration, peak memory of the N-iteration run, page faults of the cold run,
  collections per iteration, the longest and 99th-percentile pause, the share of
  time spent in pauses, and failed runs.
- **Per swept variable.** Two tables, steady CPU and peak memory. Each has one
  line per value: the geometric mean over the workloads, then each workload.
  Every number is relative to the baseline (or to the variant's `relative_to`),
  so 0.80 means 20% less than the defaults.
- **Geometric mean.** Every row against the baseline: steady CPU, cold CPU and
  peak memory.

The header lists the machine, each binary's path and hash, the commit its
checkout was at during the run (not necessarily the one it was built from), the
time per round and the outside load. It names the rounds that ran while
something else was busy.

## What the numbers mean on each platform

- **macOS.** CPU is cycles and peak is the peak memory footprint, both from
  `/usr/bin/time -l`.
- **Linux.** CPU is user + system CPU time and peak is the peak resident set
  size (`ru_maxrss`), from the program's resource usage. Perf counters are
  often closed to unprivileged users.

Cycles and CPU time are different quantities, and so are footprint and RSS. Do
not compare absolute numbers across platforms. Compare ratios. `results.json`
also keeps the macOS maximum RSS, if you need it next to Linux.

## Keeping noise down

- **Busy machines.** `run` measures how many cores are busy before it starts.
  Above `--max-busy` (1 core), it waits up to `--wait-idle` (120 s), then refuses
  with exit status 3. `--force` measures anyway, with a warning.
- **Job queues.** Set `GCBENCH_QUEUED=1` when a job queue runs gcbench while it
  holds back its other work: gcbench then waits at most 30 s and measures with a
  warning.
- **Other work.** Close browsers and builds. Every round records the outside
  load, and the summary names the noisy rounds.
- **Rounds.** Use at least 5. Peak memory of small heaps can move by a few
  percent from one run to the next.
- **Noise floor.** Keep a repeat row: a row identical to the baseline (growth 4
  in the default config, `base-again` in the compare example). Its distance from
  1.000 is the noise.
- **Iteration counts.** `gcbench.py calibrate --target 1.0` prints per-workload
  counts for about one second per steady run. Keep the counts fixed across the
  runs you compare.

## Memory over time and the baseline

On Linux, `gcbench.py memtrace` runs the baseline's N-iteration programs and
samples their resident memory every 10 ms. It shows whether memory falls after
collections. `--env-sets` runs each program under several environments, for
example glibc's malloc tunables:

```sh
python3 tools/gcbench/gcbench.py memtrace --workloads live-heap,splay-2k \
  --env-sets 'plain=;mmap-threshold=MALLOC_MMAP_THRESHOLD_=131072'
```

`--workloads baseline/empty` runs a program that allocates nothing, from
`workloads/`. Its peak is the runtime's own floor under every workload.

## Self-test

`selftest/vm-sweep.toml` sweeps a variable the VM ignores. Every row should come
out at about 1.000x:

```sh
python3 tools/gcbench/gcbench.py run --config tools/gcbench/selftest/vm-sweep.toml --rounds 2
```
