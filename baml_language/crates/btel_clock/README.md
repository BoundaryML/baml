# Telemetry clock startup

Every BAML program can begin recording without waiting for counter calibration.
This includes a packed application with an empty entry point:

```baml
function Main() -> void {}
```

Construction selects a raw source and takes bounded reference/UTC samples. It
never sleeps, spins until a deadline, starts a calibration thread, or uses
Quanta's blocking calibration constructor. A timestamp read just reads the
retained counter; it does not consult calibration or conversion state.

| Native source | Scale at startup |
| --- | --- |
| Eligible x86 TSC | CPUID leaf 0x15, when its complete ratio and reference frequency are available |
| Linux/Android/macOS ARM generic timer | CNTFRQ_EL0 for CNTVCT_EL0 |
| Apple OS fallback, including iOS | `mach_timebase_info` for `mach_absolute_time` |
| Windows OS fallback | QPF for QPC; QPF is never used to scale TSC |
| Other OS fallback, including Wasm | The OS monotonic clock's nanosecond scale |

The existing telemetry processor visits pending clocks while the program is
running, even if no producer chunk has been published. It samples a candidate
after at least 20 ms of naturally elapsed execution and checks it against an
independent interval of at least 50 ms. Each visit takes at most three bounded
reference brackets. Once accepted, that source generation can supply the scale
to later runs in the same engine.

The last attached telemetry thread takes a final bounded sample. It does not
wait for the next visit or the calibration deadline. A resolvable short run can
publish an estimated scale with its uncertainty; a run below sampling precision
keeps its raw ticks, invocation counts, and bounded OS elapsed observation.
Short-run estimates are never cached for later runs.

## Recording and queries

Format minor 11 publishes the immutable raw/monotonic/UTC anchor independently
of conversion. Each epoch publishes at most one complete mapping, immediately
when platform-reported or later when measured. Raw records can arrive before
that mapping. Existing complete-mapping recordings remain readable.

```sql
SELECT function_name, duration, timing_status
FROM spans;
```

| `timing_status` | `duration` (nanoseconds) |
| --- | --- |
| `valid` | Converted using a reported or calibrated scale |
| `estimated` | Converted using a short-run estimate |
| `pending` | NULL while a scale is unavailable |
| `below_precision` | NULL when a completed epoch cannot resolve a scale |
| `invalidated_discontinuity` | NULL after an observed counter fault |

Precision and counter validity are separate. A noisy or preempted reference
sample postpones fitting; it does not invalidate the counter. A migration or
restore also does not by itself invalidate a clock. Maintenance checks actual
counter continuity/rate against the OS reference. Observed faults invalidate
active runs conservatively and select a new source generation for future runs.
Completed epochs remain unchanged. Restore discards unfinished fitting; old
results cannot populate a new generation's cache.

Explicit duration-based capture policies retain eligible inputs/outputs from
entry while scale is pending, since VM values cannot be recovered afterwards.
They also retain eligible values conservatively after an observed clock fault;
an invalid conversion cannot reliably decide whether a duration rule was met.
The default policy-zero timestamp path does not inspect the mapping.

## Measurement methodology

The matched benchmark compares baseline `86f95c3e457ed58ac22d0773717eea327ca10a04`
(the packed-wrapper stack) with this change, using Rust 1.98.0 on an Apple M2 Max,
macOS 15.6.1. Both use the unchanged normal release profile: opt-level 3,
fat LTO, one codegen unit, stripped binaries, unwind panics. Baseline and
candidate have separate Cargo target directories. Each application is packed
with its own compiler and host from identical BAML source.

`scripts/btel-startup/benchmark.py` measures 100 fresh processes per variant,
following four warmups. It interleaves baseline/candidate and telemetry off/local
in a seeded random order. It uses an isolated HOME/BAML_HOME and a local fixture
child selected through BAML_TOOLCHAIN. No user credentials or cloud requests are
used. The wrapper case runs `--version`; the empty application has no output.
Local recordings use the same scratch disk and are retained during the run.

The C parent uses `posix_spawn`, a stdout pipe, and `wait4`: wall time includes
spawn through exit; first output is when the parent receives bytes; shutdown
tail is exit minus first output. CPU and peak RSS include reaped child usage.
macOS process accounting also records lifetime footprint, faults, switches,
instructions, cycles, and I/O when available. A separate held-child experiment
samples wrapper/child resident memory and thread counts after 150 ms, four times
per variant. Binary sizes and SHA-256 fingerprints are saved with the raw JSON.

The `btel_clock` startup example measures constructor, first epoch/read readiness,
and final-thread completion using `Instant`. Its 10-million-read loop reports
throughput including the loop, backend branch, and `black_box`; it is not an
isolated counter-instruction latency measurement. Performance results apply to
this native ARM machine. Cross-target compilation does not establish performance
or migration behavior on physical Windows/Linux/iOS/Wasm hardware.

To reproduce after preparing `OUTPUT/{baseline,candidate}` artifacts:

```sh
cargo build --release -p baml_cli -p baml_pack_host -p btel_clock \
  --example startup --bins --locked
BAML_TELEMETRY=off baml-cli pack Main --project crates/baml/baml_src \
  --host baml-pack-host -o baml-packed
# Pack an identical project containing the empty Main above as tiny-packed.
python3 scripts/btel-startup/benchmark.py OUTPUT --samples 100
```

The harness saves `timings[-raw].json`, `clocks[-raw].json`, `resident.json`, and
`artifacts.json` under OUTPUT. Rebuild the baseline in an isolated checkout with
the same startup example before measuring. Run after build/test processes have
finished; the host is a developer machine, not a controlled lab.

## Matched results (2026-10-04)

Clock measurements use 100 fresh processes; elapsed values are µs. The raw read row is loop throughput in ns/read.

| Clock metric | Baseline p50 / p95 / p99 | Candidate p50 / p95 / p99 |
| --- | --- | --- |
| Construction (µs) | 75010.229 / 75023.291 / 75028.834 | 2.062 / 3.167 / 3.625 |
| First epoch/read ready (µs) | 75014.292 / 75046.209 / 75065.375 | 2.896 / 5.250 / 8.375 |
| Final-thread completion (µs) | 0.042 / 0.125 / 0.292 | 3.688 / 7.458 / 8.208 |
| Raw read loop (ns/read) | 0.701 / 0.860 / 0.883 | 0.677 / 0.804 / 0.819 |

| Packed application / telemetry | Baseline wall p50 / p95 / p99 (ms) | Candidate wall p50 / p95 / p99 (ms) | Baseline → candidate CPU p50 (ms) | Baseline → candidate peak RSS p50 (MiB) |
| --- | --- | --- | --- | --- |
| Wrapper --version, off | 24.61 / 28.17 / 31.39 | 24.98 / 29.06 / 32.80 | 22.66 → 23.09 | 31.88 → 31.93 |
| Wrapper --version, local | 102.44 / 105.79 / 110.90 | 27.03 / 29.44 / 34.40 | 46.27 → 25.43 | 33.74 → 33.91 |
| Empty Main, off | 22.08 / 25.69 / 26.61 | 22.06 / 24.84 / 48.43 | 20.24 → 20.24 | 29.07 → 29.05 |
| Empty Main, local | 99.18 / 101.77 / 104.86 | 24.07 / 27.43 / 32.48 | 42.95 → 22.48 | 31.09 → 31.21 |

| Wrapper output metric, local telemetry | Baseline p50 / p95 / p99 (ms) | Candidate p50 / p95 / p99 (ms) |
| --- | --- | --- |
| First output | 95.36 / 97.57 / 102.49 | 20.62 / 22.43 / 25.55 |
| Exit after first output | 7.12 / 8.21 / 11.65 | 6.26 / 7.06 / 9.32 |

Held wrapper + child, four runs per variant:

| Mode | Baseline RSS p50 [min, max] (MiB) | Candidate RSS p50 [min, max] (MiB) | Total threads, baseline → candidate |
| --- | --- | --- | --- |
| off | 33.23 [31.41, 33.55] | 32.82 [31.08, 35.22] | 15 → 15 |
| local | 34.32 [32.52, 35.72] | 35.95 [33.98, 37.00] | 17 → 17 |

| Artifact | Baseline bytes | Candidate bytes | Change |
| --- | --- | --- | --- |
| baml-cli | 50,367,408 | 50,417,184 | +49,776 (0.099%) |
| baml-pack-host | 30,053,488 | 30,070,256 | +16,768 (0.056%) |
| baml-packed | 33,223,762 | 33,240,530 | +16,768 (0.050%) |
| tiny-packed | 33,025,618 | 33,042,386 | +16,768 (0.051%) |
| clock-startup | 335,952 | 336,080 | +128 (0.038%) |

The telemetry-off p50 controls are approximately unchanged. Tail outliers remain visible in the tables; this is a developer machine, not an isolated latency lab. Held RSS uses only four runs and overlapping ranges, so it does not establish a memory regression or equivalence. The bounded final reference sample adds about 3.65 µs median to clock completion; packed process exit still improves substantially.

All summary counters, resident samples, artifact sizes and SHA-256 hashes are retained in [`results-20261004.json`](../../scripts/btel-startup/results-20261004.json). These include footprint, instructions/cycles, faults, switches, CPU and I/O counters. Raw samples remain in `target/btel-clock-metrics-20261004/` in this checkout.
