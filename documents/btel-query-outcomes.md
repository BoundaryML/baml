# Population outcomes without new VM records

This phase preserves information that completion records already contain but
background aggregation previously discarded. It answers “how many completed
calls succeeded, errored or were cancelled?” for timing-only calls as well as
retained calls. It does not add invocation starts, throw provenance, latency
histograms, or counts for functions excluded by telemetry policy.

The VM emits exactly the same records. The processor increments two counters
per completion (`errored` and `cancelled`); success is the existing total minus
those two. The recorder combines them alongside durations and writes them into
the existing BTEL aggregate batch. Retained spans are already included in those
aggregates and are never counted a second time by the reader. Normal and
recursive reentry nodes both contribute to outcome counts; recursion-aware
timing semantics are unchanged.

## Query it

`call_path_nodes`, `call_path_stats` and `function_stats` each gain `ok_calls`,
`errored_calls`, `cancelled_calls`, and `outcome_state`. Existing columns retain
their meanings. `function_stats` remains one row per execution/function; use
`call_path_stats` for a particular caller/site. For example:

```sh
baml query "SELECT execution_id, fqn, completed_calls,
  ok_calls, errored_calls, cancelled_calls, outcome_state
  FROM function_stats WHERE fqn LIKE 'user.%'
  ORDER BY errored_calls DESC LIMIT 20"
```

To combine executions, guard against unknown populations: SQLite's ordinary
`SUM` skips NULL, which could otherwise turn a partial history into a seemingly
complete error rate. This query returns unknown outcomes if any contributing
population is unknown:

```sh
baml query "SELECT fqn,
  CASE WHEN COUNT(*) = COUNT(completed_calls)
    THEN SUM(completed_calls) END AS completed,
  CASE WHEN COUNT(*) = COUNT(errored_calls)
    THEN SUM(errored_calls) END AS errors,
  CASE WHEN COUNT(*) = COUNT(errored_calls)
         AND COUNT(*) = COUNT(completed_calls)
    THEN 100.0 * SUM(errored_calls) / NULLIF(SUM(completed_calls), 0)
    END AS error_percent
  FROM function_stats GROUP BY fqn ORDER BY errors DESC"
```

An error counts every invocation that completes with an error. One throw
propagating through four invocations can produce four errored calls; these
counts cannot identify four distinct throws. A caught error still counts for
the callee that failed; its caller can complete successfully.


The real CLI run of the test program (`baml run main -- --n 5`) produced:

| Function | Completed | OK | Errored | Cancelled | Evidence |
| --- | ---: | ---: | ---: | ---: | --- |
| user.Recur | 4 | 0 | 4 | 0 | recorded |
| user.Risky | 6 | 2 | 4 | 0 | recorded |
| user.main | 1 | 1 | 0 | 0 | recorded |

`Risky` throws for even arguments; `main` catches them. `Recur` propagates
one error through four recursive invocations. This checks population outcomes
through `baml run` → BTEL → `baml query`, with no CAS reads.

## Evidence and compatibility

| `outcome_state` | Meaning |
| --- | --- |
| `recorded` | Every indexed completion in this population carries outcome evidence. |
| `none_observed` | This defined context has no indexed completions; observed counts are zero. |
| `not_recorded` | The population comes from old deltas without outcomes. Counts are NULL. |
| `partial` | Known and old, unknown populations were combined. Outcome counts are NULL. |
| `invalid` | An aggregate says errors plus cancellations exceed its completion total. Outcome counts are NULL; `issues` explains why. |
| `overflow` | A numeric total exceeds SQLite's signed integer range. That quantity is NULL; fitting counts remain usable. |

These states concern the indexed prefix, not whether the application is still
running or whether recording is lossless. Unfinished invocations are absent.
Clock invalidation does not invalidate counts. Missing context definitions
still leave nodes inspectable in `call_path_nodes` until attribution arrives.
The public `executions` SQL relation keeps its existing retained-outcome
fields. The playground now reads population outcomes from `call_path_stats`
for its calling-context inspector. An opened run's overview sums errored
calls only when known context counts cover the run's completed population;
otherwise it shows the retained-call count and explains that the total is
unavailable. The lightweight execution list still omits the population rollup.
These counts describe failed invocations, not distinct throws.

The optional protobuf message `AggregateDelta.outcomes` is field 5 and holds
two unsigned counters. A present empty message means both are zero; an absent
message means unknown. The recorder always writes the message, including
all-success populations (two bytes per encoded aggregate entry before enclosing
length-varint changes). This is an additive field; format major/minor remain
2.1 and readers detect support by presence, not by minor version. Older readers
can ignore it. Existing recording files are never rewritten.

The derived SQLite schema advances from 4 to 5 and rebuilds once from BTEL.
Incremental application retains exact error/cancellation totals and evidence
flags transactionally with the file watermark. Mixing evidence never silently
fills old missing counts with zeros. Checked sums propagate missing values and
overflow through the node, context and function reductions.

## Producer boundary and cost

Only background processor/recorder implementation and their aggregate layout
constants change. Processor deltas grow from 32 to 48 bytes: the fixed 16-slot
combining cache is 768 bytes instead of 512. Recorder totals grow from 24 to
40 bytes per map entry. Map limits, flush policy and queue bounds are unchanged.
Existing exact encoded-length accounting includes the new fields, including
varint growth and overflow spills, for local and cloud delivery.

The engine, VM, frames, timing/span record layouts, ring transport, snapshot
encoding and CAS identity remain unchanged from `5f9e650c8`. Audit the 120
frozen implementation files with:

```sh
python3 documents/check-producer-baseline.py \
  documents/btel-query-outcomes-hot-path-baseline.json
```

The original 153-file and post-cloud 164-file manifests remain as historical
evidence. The latter intentionally no longer passes for the background
processor/recorder files changed in this authorized phase; do not regenerate
it from the new files and claim the whole producer is unchanged.

Cloud golden fixtures add the same optional message inside their existing
recording envelopes. Their byte lengths and hashes change accordingly. Server
plans and CAS bytes/IDs are unchanged; fixture provenance is documented in
`btel_bcs/tests/fixtures/cloud-v1/README.md`.

## Validation and short measurements

Processor/recorder tests cover all 18 span completion forms, all timing
outcome/reentry combinations, cache collisions, flushes, bounded no-sink
combining, counter overflow, wire presence, and exact encoded lengths. Reader
tests cover old/mixed recordings, late definitions, incremental idempotence,
schema rebuild, deletion, malformed counts, clock invalidation and overflow.
A real engine fixture checks caught timing-only errors and recursive error
propagation; CLI tests check timing-only and retained successes together.

Validation on Rust 1.98.0: all 47 query tests, both CLI query tests, the
playground reader integration test, 29 processor tests, 29 recorder tests,
settings/file/cloud suites (including upload goldens), and engine recording
integration tests pass. Clippy with warnings as errors passes on the changed
query/CLI targets and producer/recorder/file/cloud/reader crates; formatting and
the 120-file freeze pass. No full workspace build or interactive UI check was
run for this phase. TypeScript was unchanged.

The later playground wiring passes 243 TypeScript tests, type checking and
Biome, plus the real-engine playground reader test and LSP Clippy with warnings
denied. It changes no producer code and has no new performance measurement.
See the [remaining parity review](btel-query-parity-review.md) for priorities.

Use `tools/btel_record_smoke.py` for an explicitly focused recording comparison.
Before changing recording code, save the already-built baseline example, then
build the new one with the same Rust toolchain/profile/features. From
`baml_language/`:

```sh
python3 tools/btel_record_smoke.py \
  --before target/outcome-bench/before/btel_record_bench \
  --after target/release/examples/btel_record_bench \
  --output target/outcome-bench/comparison.json
```

The five fixed-work cases are tiny roots, 1,000-call loops, 32-child spawns,
repeated captures and unique captures. Each compares both binaries with
recording off and on; a warmup precedes three measured repeats with interleaved
order. The report includes execution/CPU time, p95 call time, drain, peak RSS,
and BTEL/CAS bytes, plus binary hashes. Compilation is separate. A whole-run
120-second budget and 20-second subprocess limit prevent hour-long runs.
`--workloads spawn --repeats 7` narrows a follow-up when a difference needs
investigation. Reports and temporary recordings stay local; failures preserve
a report marked `failed`. These are descriptive shared-machine smoke checks,
not proof of zero recording overhead.
