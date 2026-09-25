# Recording performance against the unmodified BTEL rewrite

Measured 2026-09-24 on this Linux workstation, an AMD Ryzen 9 5950X.
This compares the **whole query branch's producer changes** against upstream
PR #4958, not just recording completion and not the pre-outcome query branch.

The short check found no consistent large regression. Tiny calls, spawning
and unique captures had first-pass execution medians 1.7–3.7% higher. Repeated
captures initially measured 6.5% higher, but a focused follow-up changed
direction; the pooled execution median was 2.6% higher and pooled CPU time
essentially unchanged. These are descriptive observations on a shared
workstation, not proof of zero overhead or a speedup.

## Revisions and method

- **Before:** `778a5f002e084e4c534af7b4a58ee066b564eed1`, the exact
  `codex/btel-cloud-delivery` / #4958 base. A temporary checkout had no tracked
  changes. The identical existing `btel_record_bench.rs` was copied into it
  as an untracked benchmark example; no producer or Cargo changes were needed.
- **After:** query parent `0735e3909965038bd858c0c727d5422bf47ed89f` plus the
  [recording-completion patch](btel-query-recording-completion.md). This
  includes the earlier cold function/parameter metadata and background
  aggregate outcome counts as well as the new completion work. It cannot
  attribute a difference to one of those changes individually.
- Rust 1.98.0, optimized release builds, incremental compilation disabled,
  separate build directories. The upstream build took 5m44s and the candidate
  build 4m32s; build time is separate from measurement.
- Same fixed work, monotonic clock, four Tokio workers, major GC every eight
  roots, 1 MiB file target and 4 KiB capture payload. No fsync, SQLite query or
  cloud upload in this recording check. Captures use real VM capture with a
  synthetic LLM classification, without network/model work.
- Each workload/version/mode gets a warmup, then three measured repetitions,
  with interleaved randomized order and telemetry-off controls. The focused
  follow-up uses seven repetitions and a different order seed.
- The five-workload run took **79.5s**. The focused repeated-capture follow-up
  took **28.7s**. Total measurement time: **108.2s**. No further repetition was
  used to select a favorable result.

The binary SHA-256 values are:

```text
upstream:  1b8220275fcc0ac6917245309a5ae2a76039a3b53d18266e3623c3c84041d85a
candidate: e5909e6bfe808071fff867d8029b7f85a08942f75f40577146be97798f9a7867
```

Raw samples, the candidate source diff, provenance and both binaries remain
local in `baml_language/target/completion-bench/`. They are excluded from git.

## Execution time

Milliseconds for the fixed workload; median [observed minimum, maximum].
Positive change means more elapsed time. These numbers exclude engine startup
and shutdown; those are reported separately below.

| Workload | Roots | Upstream recording | Query branch recording | Change | Telemetry-off change |
| --- | ---: | ---: | ---: | ---: | ---: |
| Tiny calls | 50,000 | 888.4 [878.1, 894.5] | 920.9 [883.7, 980.0] | +3.7% | +0.8% |
| Dense calls, 1,000 leaf calls/root | 4,096 | 767.8 [601.6, 770.4] | 609.3 [587.2, 740.5] | −20.6% | −0.7% |
| Spawn, 32 children/root | 2,048 | 493.3 [458.4, 520.8] | 501.9 [444.7, 514.3] | +1.7% | −2.0% |
| Repeated capture | 25,000 | 465.6 [457.0, 483.8] | 496.0 [489.0, 557.4] | +6.5% | −2.3% |
| Unique capture | 2,048 | 299.7 [298.3, 309.8] | 309.1 [303.9, 318.8] | +3.1% | −5.2% |

Dense calls varied substantially within both binaries. Do not interpret their
median difference as an established speedup.

Repeated capture had the clearest first-pass slowdown, so it received the
focused follow-up:

| Repeated capture | Upstream | Query branch | Change |
| --- | ---: | ---: | ---: |
| Follow-up, seven samples/binary | 487.9 [441.6, 527.6] ms | 466.1 [456.8, 501.0] ms | −4.5% |
| Both runs pooled, ten samples/binary | 475.2 [441.6, 527.6] ms | 487.3 [456.8, 557.4] ms | +2.6% |
| Pooled process CPU during execution | 790.8 ms | 789.8 ms | −0.1% |
| Pooled per-root p95 latency | 11.45 µs | 11.59 µs | +1.3% |

The follow-up's telemetry-off medians were 403.1 → 406.3 ms (+0.8%). The
first-pass repeated-capture slowdown did not reproduce, but uncertainty about
small overhead remains. There is no evidence here of the large spawn or
unique-capture penalty seen in the earlier direct-SQLite experiment.

## Startup, shutdown, memory and bytes

The hot-loop constraint does **not** mean all work is free. The background
recorder checks existing epoch lifecycle state and retains unsettled epochs;
startup also includes the query branch's earlier function-metadata copy.

Across all ten repeated-capture samples per binary, engine startup medians
were **11.15 → 11.90 ms** (+0.75 ms). The focused run alone was 11.44 →
12.96 ms. This small-program harness is not a measurement of startup scaling
with a large function inventory.

First-pass shutdown/drain medians:

| Workload | Upstream | Query branch |
| --- | ---: | ---: |
| Tiny | 1.24 ms | 0.95 ms |
| Dense | 0.77 ms | 1.09 ms |
| Spawn | 2.94 ms | 2.54 ms |
| Repeated capture | 2.00 ms | 1.37 ms |
| Unique capture | 14.72 ms | 15.17 ms |

These include engine shutdown, final GC and remaining delivery work, not
just writing the end marker. File/window sizes also differ between revisions.
They show no large drain penalty in these workloads.

- Peak process RSS medians changed by −0.1% to −1.5% (rounded); no memory
  increase was apparent in this check. This includes compilation/setup and
  runtime memory, not an isolated recorder allocation measurement.
- BTEL bytes were 0.4% smaller for tiny calls, 4.3% smaller for dense calls,
  8.0% smaller for spawn, 3.2% smaller for repeated captures and 3.3% smaller
  for unique captures. The query branch publishes known function metadata
  once, whereas upstream repeatedly emits unavailable-function observations;
  the size comparison includes that difference as well as new outcomes/finality.
- CAS sizes and object counts were identical in every measured pair.
  Repeated capture stored two objects totaling 4,199 bytes; unique capture
  stored 4,096 objects totaling 8,599,552 bytes. Non-capture workloads stored
  none. Every telemetry-off sample wrote zero recording and CAS bytes.

## Reproduce a quick comparison

From `baml_language`, with the two preserved binaries:

```sh
python3 tools/btel_record_smoke.py \
  --before target/completion-bench/upstream-recorder \
  --after target/completion-bench/completed-recorder \
  --output target/completion-bench/next-paired.json \
  --repeats 3 --budget-seconds 120
```

Use a fresh output filename. To check a suspicious workload, add e.g.
`--workloads capture-repeat --repeats 7 --budget-seconds 45 --seed 1`.
The script records all samples, including warmups, and marks a partial run
failed if it exceeds its budget. Build a new candidate explicitly before
comparing a later change; do not silently reuse the old candidate binary.

This check does not measure cloud bandwidth/delivery, other clock modes,
long-running memory growth, or query latency. The unmodified upstream base
has no new BTEL query reader to compare. Existing reader measurements remain
in [the reader report](btel-query-performance.md) and
[the outcome report](btel-query-outcomes.md).
