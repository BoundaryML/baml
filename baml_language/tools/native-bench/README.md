# native-bench

The macro benchmark of the native backend. Nine BAML programs run as the
VM at every telemetry level, as native executables, as a frozen native
control, and as hand-written Rust, Go, Node, Bun and Python, on an
ephemeral VM booted from a pinned snapshot: one pinned core, one worker, a
fresh process per trial, every trial checked against an independent
oracle. One command produces one bundle with the numbers (`kpis.json`), the
page (`kpis.html`), the PR summary (`summary.md`) and every trial
(`results/trials.csv`).

`.github/workflows/native-bench.yml` runs it on every pull request that
touches this directory or the workflow, on dispatch against any branch, and
nightly on `canary`. It is advisory: it never gates a merge.

    cd baml_language/tools/native-bench
    cargo build --release -p baml_cli --manifest-path ../../Cargo.toml
    python3 -m bench run ../../.. --cli ../../target/release/baml-cli         # tier A on the Freestyle snapshot
    python3 -m bench run ../../.. --cli ../../target/release/baml-cli \
        --machine machines/local.json --protocol protocols/smoke.json         # plumbing check on this machine
    python3 -m bench kpis <bundle> [--previous <bundle>]                      # re-render kpis.json and kpis.html
    python3 -m bench compare <before> <after>                                 # verdicts between two bundles

The package is Python standard library plus one C file (the observer);
it needs `python3`, `cc`, and for the Freestyle machine `npx` and the key
in `FREESTYLE_API_KEY` or `~/.config/freestyle/api.key`.

## What is measured

Every bar on the page is one **scenario** (a case with fixed inputs) run as
one **row** (an implementation at a telemetry level), `trials` times, in a
fresh process each time. A trial runs `warmup` jobs untimed, then `jobs`
jobs timed inside the process; the timed region is reported by the process
itself over a JSONL protocol (`ready`, `result`, `drained`), and the parent
observer records launch-to-ready, peak RSS from `wait4`, and the pinned CPU.
The throughput charts show jobs per second from the per-job median; the
per-job time, min and max are in each box's table.

### Cases

Each case is a directory under `cases/` with `workload.baml` (the program:
`prepare` builds the input once, `run` is the timed job) and `README.txt`
(the question, the work, what the BAML path exercises, where the timing
boundary is, the evidence the oracle checks, and the claim the number can
support). The same program is hand-ported to Rust, Go, TypeScript and
Python in the snapshot's `ref/` directory with the same boundary.

| case | question |
|---|---|
| `00_startup` | What does one correct invocation cost: launch, load, prepare, first result? |
| `01_array_traversal` | What does traversing, indexing and building an integer array cost? |
| `02_merge_sort` | How well does user-written control flow execute: loops, branches, indexing, mutation? |
| `03_json_aggregate` | What does a useful data-transformation job cost: parse, validate, group, sort, serialize? |
| `04_allocation_retention` | What does allocation cost as the surviving object graph grows (4 or 64 batches kept)? |
| `07_function_calls` | What does a source-level loop of small function calls cost? |
| `09_json_hello` | What does the smallest useful job cost: build a record, serialize it? |
| `10_quick_sort` | Same question as merge sort with a recursive, in-place algorithm. |
| `11_generate_sort` | What does generating 8,192 numbers and sorting them with the library sort cost? |

### Scenarios (protocol `tier-a`)

| scenario | case | inputs | jobs × warmup |
|---|---|---|---|
| `startup` | 00 | none | 1 × 0 |
| `array-empty`, `array-small` | 01 | iterator over 0 and 128 integers | 4096 × 16 |
| `array-iterator`, `array-indexed`, `array-build` | 01 | 8,192 integers, three traversal styles | 128 × 4 |
| `merge-random/sorted/reverse/duplicates` | 02 | 1,024 integers, four orders | 32 × 4 |
| `quick-random/sorted/reverse/duplicates` | 10 | 1,024 integers, four orders | 32 × 4 |
| `json-aggregate` | 03 | 1,024 JSON rows | 32 × 4 |
| `allocation-low-retention`, `allocation-high-retention` | 04 | 128 batches of 2,048 objects, keep 4 or 64 | 8 × 1 |
| `calls-empty`, `calls` | 07 | 0 and 128 calls | 4096 × 16 |
| `json-hello` | 09 | one record | 4096 × 16 |
| `generate-sort` | 11 | 8,192 numbers | 32 × 4 |

Twenty trials per bar, seed 1729, CPU 3, one worker, 600 s timeout per trial.

### Rows

| row | what runs | `BAML_TELEMETRY` | recording |
|---|---|---|---|
| `native-off` | the candidate's emitted executable, built on the VM from a clean target | off | none |
| `native-low`, `native-medium`, `native-high` | not supported yet: drawn as "not yet" on every chart, never run | | |
| `control` | the snapshot's native executable of the same case, frozen when the snapshot was made | off | none |
| `vm-off` | the pinned VM host running the candidate's bytecode | off | none |
| `vm-low`, `vm-medium`, `vm-high` | same, recording to a local sink; the recording must exist and report no delivery loss | low / medium / high | local |
| `rust`, `go` | the reference executables prebuilt into the snapshot | | |
| `node`, `bun`, `python` | the reference sources under the snapshot's pinned runtimes | | |

The child environment is scrubbed: only the variables the row needs are
set (`BAML_TELEMETRY`, the worker counts, the bytecode cache switch), so a
developer's shell cannot leak into a measurement.

The control row is the same program at the same level as `native-off`, so
the difference between the two across runs is host drift, not the
candidate. The page reports it; a verdict subtracts it.

## What a run does

1. **Stage** (on the runner). For every case, `baml __emit-rust --report`
   records the admission report, then `--function run --function prepare`
   emits a Cargo project with the `bex_aot` runtime path rewritten to the
   VM's checkout and the toolchain pinned; `baml_language` is archived at
   HEAD; the cases, the protocol, the machine file and this package are
   copied into `stage/`.
2. **Execute** (on the machine, `python3 -m bench execute`). Unpack the
   source. Freeze the snapshot's native executables as the control row.
   Compile every case to bytecode with the pinned host and record the
   seconds. Build every native project from a clean target in release
   (fat LTO, one codegen unit, panic abort; the wall time and stripped size
   are results). Verify every row on every case's edge-case inputs against
   the oracle, one trial each, and stop on any failure. Run the protocol's
   trials in a shuffled order from its seed. Summarize.
3. **Collect** (on the runner). Fetch `results/`, write `manifest.json`,
   render `kpis.json`, `kpis.html` and `summary.md`, delete the
   VM.

The Freestyle provider boots a fresh VM from the snapshot in the machine
file, copies the stage in, starts the execute step detached, polls for its
done file, copies the results back and deletes the VM in a `finally`. A
machine file with `"provider": "ssh"` does the same over SSH to a bare-metal
host; `"provider": "local"` runs here.

## The bundle

    <out>/
      manifest.json      candidate sha and branch, emitter, protocol and machine hashes, toolchains,
                         CPU, kernel, glibc, load before and after, every artifact's bytes and sha256,
                         native build seconds, bytecode compile seconds, verify and trial counts, status
      stage.json         what was emitted and why anything was not
      emitted/<case>/    the admission report, the emit log, the generated project
      results/
        trials.csv       every trial: status, timings, RSS, GC counters, telemetry bytes, hashes
        summary.json     per scenario x row: n, failed, median/min/max per metric, per job
        protocol.json    the protocol as run
        verify.csv       the failed verification trials, if any
        execute.log
      kpis.json          the data model behind the page (schema 2)
      kpis.html          the page: nine KPIs, every implementation on every chart
      summary.md         the table and verdicts for the PR comment

`kpis.html` is self-contained: the template in `bench/templates/` has the
JSON inserted into a `<script type="application/json">` block and draws the
charts client-side. It works from disk, from a workflow artifact and from
the `bench-results` branch. Without JavaScript the JSON is still in the
file and beside it.

## Rules the code enforces

- A native build failure fails the run. The candidate not compiling is the
  first thing the bench exists to notice.
- A row whose artifact is missing is skipped and listed under
  `verify.skipped_rows`; a run with no trials fails.
- Every trial's output is checked against the oracle; a mismatch is a
  failed trial, never dropped, and shows on the page.
- Telemetry rows must produce a recording and report `telemetry_ok` with
  zero delivery loss; off rows must produce none. Recordings are measured in
  bytes and deleted after each trial.
- A regression verdict (`compare`, or `run --previous`) needs the
  candidate's change, net of the control row's change on the same run,
  past `regression.threshold_percent` (5) with disjoint min-max ranges.
  Everything else is `unchanged`.
- Results from different machine files are never compared.

## Files

    bench/                      the package; `bench/README.md` has the module map
    cases/<case>/               workload.baml and README.txt
    protocols/tier-a.json       the matrix above
    protocols/smoke.json        two scenarios, nine rows, three trials, for plumbing
    machines/bench-x86-v1.json  the Freestyle snapshot and its guest layout
    machines/local.json         this machine; native rows only unless a host and references are present
    machines/ssh-example.json   a bare-metal host over SSH

## Changing things

- **A new case**: add `cases/<name>/workload.baml` with `prepare` and `run`,
  write its `README.txt`, add its reference ports to the snapshot's `ref/`,
  add scenarios to the protocol. The oracle in `bench/oracle.py` needs the
  expected output for the new case.
- **A new machine**: a new snapshot is a new file with a new name; the old
  one never changes, so old bundles stay comparable with each other.
- **A new row**: `bench/rows.py` defines what runs and under which
  environment; the page draws every row it finds in the protocol.
- **The page**: edit `bench/templates/kpis.html` and re-render any bundle
  with `python3 -m bench kpis <bundle>`; the data model is `bench/kpis.py`.

## Secrets and cost

The workflow needs the `FREESTYLE_API_KEY` repository secret. A tier-A run
boots one VM for about half an hour and deletes it.
