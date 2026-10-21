# Native resource workloads

These are the exact BAML snippets from the supplied telemetry resource report,
not approximations. The report itself is not stored here. `burst.baml` is shared
by the saturated-spawn and burst-with-host-idle scenarios.

| Scenario | Source | n | Units/root | Unit | Host idle ms |
| --- | --- | ---: | ---: | --- | ---: |
| Tiny roots | tiny.baml | 1 | 1 | roots | 0 |
| Dense calls | calls.baml | 100000 | 100000 | calls | 0 |
| Spawn throughput | burst.baml | 64 | 2048 | children | 0 |
| Bounded async | dense.baml | 4 | 128 | children | 0 |
| Bursty + idle | burst.baml | 4 | 128 | children | 200 |

Build from `baml_language`:

```sh
cargo build --release -p bex_engine --example resource_benchmark
target/release/examples/resource_benchmark compile \
  --source crates/bex_engine/examples/resource_workloads/tiny.baml \
  --artifact target/tiny.program
BAML_TELEMETRY=off target/release/examples/resource_benchmark run \
  --artifact target/tiny.program --mode off --n 1
```

Artifacts use the existing `Program` Borsh format, as used by the CLI bytecode
cache. They must be compiled by the same build of this binary. Compilation is a
separate process; `run` only reads/deserializes the artifact during load, then
creates one engine and one Tokio runtime during setup. No synthetic LLM metadata
is applied. Ordinary functions retain the runtime's actual auto-capture policy.

Execution defaults to 10000 ms including major GC every eight completed roots
and any host idle. `--duration-ms 0` executes exactly one root without forced GC.
Every successful root must return its argument. Workers default explicitly to
`available_parallelism`, matching the existing runtime benchmark's Tokio
`Runtime::new` convention, and are reported; override with `--workers`.

Modes are `off`, `auto-no-sink`, `local`, `cloud-fast`, and `cloud-slow`.
The parent must set `BAML_TELEMETRY=off` for off and `medium` for all other modes.
The historical `auto-no-sink` scenario name means automatic function policies at
the medium level, with no recording destination.
Local mode requires a fresh `--output-dir` under `target`. Cloud modes require
`--prepare-base-url` pointing at a separate mock-server process; that process
controls delay, not the runner. Cloud delivery otherwise retains default bounds,
timeouts and retry policy. Recording uses explicit 1 MiB / 1 second targets,
bounded queues and the existing no-fsync file writer. CAS counts are separate.

Stdout is flushed JSONL. Phase events have `event: "phase"`, `phase`, and monotonic
`elapsed_ms` relative to runner startup. Boundaries are `load`, `setup`, `execute`,
`drain`, `done`. The final `event: "result"` includes exact phase durations, work,
GC duration, worker count, telemetry result/status, and post-drain file counts.
Scanning files is outside drain. The parent sampler owns OS resource measurement.

## Whole-runtime resource report

On macOS, the standard-library Python orchestrator runs all workloads with
telemetry off, no sink, local files, fast cloud, and 50 ms delayed cloud PUTs:

```sh
cargo build --release -p bex_engine --example resource_benchmark
python3 scripts/telemetry_resources.py \
  --output target/telemetry-resource-report
```

The output directory must be new. `--reference /path/to/report.html` optionally
adds normalized comparisons against an earlier report. Other options control
duration, repeat count, worker count, sampling interval, memory limit, workloads,
and modes; use `--help` for the complete interface.

The default matrix uses two shuffled 10-second runs per sustained workload/mode
and one cold root per mode. The separate mock-server process is excluded from
the measured child's CPU/RSS. CPU counters use the Mach timebase and are checked
against process CPU time before measurement. RSS is sampled, not a true kernel
high-water mark.

Output includes an offline HTML report, SVG overview, summary CSV, metadata,
per-run logs, and raw JSONL samples. Failed or memory-capped runs are excluded
from valid comparisons. Fixed-duration runs complete different amounts of work,
so compare throughput alongside CPU and output size. Two repeats show observed
variability, not statistically significant speedups; hardware, runtime revision,
and worker count matter when comparing reports.

These Auto-policy workloads exercise spans and aggregates, not synthetic
LLM captures. The mock is an HTTP transport control, not real S3, TLS, or durable
storage. Keep generated reports and one-off measurement notes under `target/`.
