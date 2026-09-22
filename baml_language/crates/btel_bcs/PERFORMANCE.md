# Runtime telemetry performance probe

From `baml_language`:

```sh
cargo test --release -p bex_engine --test telemetry_performance \
  -- --ignored --exact telemetry_performance --nocapture
```

This opt-in diagnostic runs each scenario in a fresh process. It is not a
wall-clock assertion in CI. It uses the same mock prepare-plan generator as the
delivery integration tests, with no BCS service or credentials required.

The default matrix has five cyclically balanced trials, 500 measured calls per
trial, and 20 paced warmup calls:

- Telemetry off, local files, cloud, cloud with 50 ms PUT response delays, and
  cloud with PUT responses returning HTTP 503.
- 1,000 trivial BAML leaf calls per invocation; a repeated 64 KiB capture; and
  distinct snapshots containing a shared 64 KiB string plus a changing integer.
- An unpaced sequential caller and a sequential caller nominally paced at 2 ms.

`BTEL_PERF_TRIALS` and `BTEL_PERF_ITERATIONS` override sample counts. Five trials
balance mode positions; fewer trials do not. Compilation, engine construction,
argument/context preparation, and warmup are outside call latency. Shutdown is
measured separately.

## Reading results

Each `TELEMETRY_PERF` line is JSON. It reports per-trial p50/p95/p99/max call
latencies, achieved throughput, the first observed telemetry error, final
delivery status, and request counters before measurement, after measurement,
and after shutdown.

- A low latency with failed delivery is not a successful cloud performance
  result. `pre_error_latency_us` means before an error was observed, not proof
  that those calls were delivered successfully.
- Compare request counters at measurement boundaries. If PUT requests only
  appear after shutdown, the scenario measured enqueue/recording work with
  deferred delivery, not execution under concurrent network load.
- Paced calls are closed-loop service latency, not an independent arrival
  stream or a capacity measurement. Missed deadlines do not generate catch-up
  bursts; timer wakeup delay is outside the call sample.
- Warmup mitigates startup effects but does not prove steady state. Sustained
  delivery claims require multiple flush/upload cycles during measurement.
- Percentiles are calculated within each trial. A median of trial percentiles
  is not the percentile of all pooled samples.

## Scope

The mock HTTP server runs in the same process, so its CPU and allocator work can
compete with execution. There is no TLS or remote network latency in this probe.
Slow/failing cases affect PUTs, not prepare requests. Results describe these
synthetic workloads on the measured machine, not all BAML applications or a
before/after comparison of the publisher-composition refactor.

## Batching probe: 2026-09-21

This earlier probe predates delivery backpressure and owned-record processing.
Its queue-saturation behavior is historical; see the paired comparison below.

Apple M5 Max, 128 GiB RAM, macOS 26.5.2, Rust 1.98.0, workspace release profile.
Five trials per scenario, 500 measured calls plus 20 warmup calls per trial.
No concurrent builds were run during measurement. Values below are medians of
the five trial percentiles, in microseconds.

| Workload / caller | Mode | p50 | p95 | p99 | Delivery |
|---|---|---:|---:|---:|---|
| 1,000 trivial calls / burst | Off | 28.584 | 30.625 | 35.375 | Off |
| 1,000 trivial calls / burst | Local | 67.208 | 76.875 | 80.041 | 5/5 OK |
| 1,000 trivial calls / burst | Cloud | 67.125 | 75.041 | 83.334 | 5/5 OK |
| Repeated 64 KiB capture / burst | Off | 2.208 | 7.958 | 15.750 | Off |
| Repeated 64 KiB capture / burst | Local | 5.875 | 10.625 | 19.709 | 5/5 OK |
| Repeated 64 KiB capture / burst | Cloud | 5.792 | 10.041 | 19.500 | 5/5 OK |
| Distinct 64 KiB captures / 2 ms pacing | Off | 15.666 | 37.125 | 62.291 | Off |
| Distinct 64 KiB captures / 2 ms pacing | Local | 22.416 | 51.542 | 65.417 | 5/5 OK |
| Distinct 64 KiB captures / 2 ms pacing | Cloud | 27.791 | 54.792 | 67.834 | 5/5 OK |

The CPU and repeated-capture burst cloud cases issued no PUTs during the
measurement interval: delivery happened during shutdown. They establish
recording/enqueue overhead, not concurrent-upload overhead.

The healthy distinct-capture paced case achieved approximately 501 calls/s.
Per trial it made 62 prepare requests and 248 PUT requests during measurement;
including warmup and final drain, 520 calls produced 66 prepares and 261 PUTs.
All 1,040 distinct snapshot candidates were offered. This scenario exercises
multiple actual delivery cycles, unlike the short burst cases above.

### Overload remains explicit

- Unpaced distinct captures exhausted cloud capacity in all five trials, first
  observed at calls 42-43 including warmup. Their post-disable latency must not
  be reported as cloud performance.
- With 50 ms PUT response delays, distinct captures at nominal 2 ms pacing
  exhausted capacity at call 41 in all five trials. HTTP 503 PUTs exhausted it
  at the same point. The finite queue cannot absorb sustained delivery lag.
- Local distinct captures in the burst case stayed enabled but had median
  trial p50 of 306.625 us and achieved approximately 3,638 calls/s, consistent
  with its blocking disk-delivery policy.
- CPU and repeated-capture failing-PUT scenarios reported failure only during
  shutdown. They do not establish execution latency under an observed failure.

Batching reduces request frequency and preserves explicit memory bounds; it
does not remove the need for an overload policy. These results do not support
a claim of zero telemetry overhead or lossless cloud delivery at arbitrary load.

## Paired owned-buffer comparison: 2026-09-21

Compared preserved release executables with the identical performance harness,
workloads, and delivery settings. The baseline already includes delivery
backpressure and liveness; the change is cloud-only owned-record processing
with cached snapshot accounting. Local and telemetry-off are controls.

Six paired trials per scenario, alternating before/after order and rotating
mode positions. Each ordinary trial has 500 measured calls after 20 warmup
calls. The slow-PUT case has 200 measured calls. All 156 final measurements
completed with telemetry either explicitly off or successfully drained.
No builds or other agent benchmarks ran during measurement.

Numbers below are medians of each version's trial metrics, not pooled
percentiles. The initial prototype run was interrupted near its end and is
not used for this table; the final comparison was uninterrupted.

| Cloud workload | Before p50 (us) | Owned-buffer p50 (us) | Difference (us) | Before p99 (us) | Owned-buffer p99 (us) |
|---|---:|---:|---:|---:|---:|
| 1,000 trivial calls / burst | 67.563 | 67.896 | +0.333 | 83.396 | 85.855 |
| Repeated 64 KiB capture / burst | 5.917 | 5.917 | 0.000 | 18.084 | 21.084 |
| Distinct 64 KiB captures / 2 ms pacing | 22.374 | 23.021 | +0.647 | 63.395 | 64.896 |
| Distinct 64 KiB captures / burst | 5.854 | 6.167 | +0.313 | 3967.042 | 3929.458 |
| Distinct captures / 2 ms pacing, 50 ms PUT delay | 17.438 | 18.125 | +0.688 | 141703.812 | 141622.312 |

Normal-call median differences were below 1 us in these workloads. Trial noise
was material, including in unchanged controls, so this is not evidence of a
universal percentage bound. Repeated-capture p99 rose by 3 us; other tail
differences were mixed.

The millisecond tails in overloaded cases are backpressure, present in both
versions, not the incremental record-movement cost. Burst distinct-capture
throughput was approximately 2,871 versus 2,892 calls/s. With delayed PUTs it
was approximately 108 calls/s in both versions. Both versions drained all
uploads in these runs; unlike the earlier batching-only probe, queue saturation
did not disable telemetry.

Raw paired results, runner, and preserved executables are local build artifacts
under `baml_language/target/telemetry-comparison/`. The final run uses
`final-results.jsonl` and `final-comparison.log`.

Executable SHA-256:

- Before: `bede53607e496affdaaac3f9bc265aa36182343956d991d1fbec4e623a8fd54f`
- Final: `1d58f49fbf1bdfde3009f80d5b3363ce028af23b0a9aeccf722010ada66eb554`
