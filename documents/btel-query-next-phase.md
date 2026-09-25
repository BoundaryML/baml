# BTEL query expansion: approved implementation scope

> Historical reader-phase brief. The later user-authorized stack integration
> rebases this implementation onto cloud-delivery PR #4958; its base and freeze
> instructions below describe the original phase. Current integration details
> and validation are in `btel-query-prototype.md`. The subsequently authorized
> [population-outcome phase](btel-query-outcomes.md) now implements step 3 for
> outcome counts only, with a narrower explicit VM/event/CAS freeze. The checker
> still uses the historical post-integration producer manifest by default.

Updated 2026-09-24 after Tony's review. This is the current implementation brief and supersedes earlier suggestions to wait for latency targets or optimize the prototype before expanding functionality.

## Priority and ownership

Continue the existing Opus implementation in this worktree, on `antoniosarosi/btel-query-prototype` at base `0c30d21021526ba1c2369e9e021b9d28ac1dc823` plus its current uncommitted changes. Preserve and extend those changes. Do not restart from canary or merge/rebase the producer stack as part of this phase. As checked today, the BTEL PR stack is still open; canary is not a substitute for this baseline.

`baml query` already works end to end: BTEL/CAS reads, incremental SQLite indexing, five initial relations, bracket navigation, CLI output formats, and minimal playground integration. The task is to expand that working implementation to answer as many useful old-tracer questions as the existing recorded evidence supports.

Tony's first priority, especially for today's standup, is functional query coverage demonstrated by real CLI commands. His latest clarification is explicit: **today's goal is that it works; measurement tooling and performance optimization come after the functional milestone.** Do not spend today's implementation effort building a new benchmark workflow or make the functional milestone wait for an optimization campaign or another hour-long recorder benchmark. Produce a runnable demo and a short checkpoint as soon as the first meaningful expansion passes, then continue the rest of the authorized reader work.

The existing Opus session is the implementation owner. Keep editing coordinated; no additional agents are required. If delegating a separable task later, the user's standing preference is Sol agents only apart from this explicitly requested Opus owner. Avoid concurrent writers to the same schema/translator and competing heavyweight builds.

## The agreed sequence

1. **Reader/query functionality now.** Answer everything practicable from existing BTEL + CAS, with honest limitations and the closest useful compatibility with old tracing. Keep the producer exactly at the current prototype baseline, including its existing function/argument metadata patch.
2. **Measure the expanded implementation.** First-index, incremental refresh, warm metadata/statistics queries, captured-value filters, and live-reader interference. Then optimize measured reader/index bottlenecks. Do not confuse view names with intrinsic view overhead; measure the work under the views.
3. **Later: preserve information already present upstream but discarded before BTEL.** Separate, measured processor/publisher work, retaining the VM hot loop/layouts. Do not start this in the current reader phase.
4. **Later: evaluate meaningful VM changes for remaining parity gaps.** Perfect parity is a goal to evaluate against cost, not permission to change the hot loop now.

Do not stop phase 1 asking Tony to choose views versus tables, acceptable ingest MB/s, or speculative scalar-cache policy. Resolve ordinary read-side implementation choices using the existing architecture and evidence. State practical limitations rather than fabricating old information.

## Frozen boundary for phase 1

- No new VM/frame/function hot fields, per-invocation instructions, transport events/layouts, runtime counters, capture policies, metadata production, or writer behavior.
- No additional producer/publisher/processor changes, new protobuf fields or emitted evidence, CAS writer/encoding/hash changes, or new lifecycle marker production in phase 1.
- Preserve the existing prototype's metadata work; do not revert it or count it as a new phase-1 producer change.
- Reader/decoder validation, SQLite ingestion/reconciliation, SQL binding/translation, CLI, playground consumers, and meaningful fixtures/tests are in scope. `btel_file` read-side code and `btel_snapshot` decoding may change without changing writer/hash formats.
- Reader perf improvements needed for a usable implementation are fine, but functional coverage and correctness come first. Do not eagerly decode every capture during indexing merely to implement metadata tables.
- Do not delete old query crates yet; they remain a compatibility reference. No commits, pushes, PRs, or publication unless Tony asks. Preserve other work and do not clean unrelated build directories.

Use `documents/btel-query-phase-a-producer-baseline.json` to audit that frozen producer implementation files remain unchanged from the handoff. It records current bytes, including already accepted uncommitted prototype changes. Tests/examples can be added outside those implementation files. Any necessary exception must be explained before changing production code across this boundary.

## Read these references in order

1. This brief: the user's latest priorities and scope.
2. `documents/btel-query-prototype.md`: your implementation record, tests, measurements, and known limits.
3. `documents/btel-logical-schema.md`, especially sections 4–5 and 8–9: evidence map and useful acceptance questions. The 157-column old inventory is reference material, not a requirement to manufacture 157 columns or implement every speculative v2 infrastructure idea before a working demo.
4. Old catalog at `origin/canary:baml_language/crates/baml_query/src/catalog.rs` and current `baml_query_profiles/src/relations.rs`. Canary includes `runtime_id` / `runtime_ids`, which were removed in this stack; do not restore them just for literal compatibility.

The proposed v2 contract is a design draft. Use its semantic distinctions; make focused refinements where actual implementation evidence requires it. Preserve useful prototype queries with aliases where honest. Avoid churn such as redesigning every public ID or adding multi-source federation before completing the useful single-source query surface. Document the implemented contract and any deviations.

## Functional additions

### 1. Threads, spawn structure, and executions

Expose logical threads, recorded parent node, resolved parent thread/call, spawn path, root/execution membership, outcomes, and supported timing. Preserve unresolved identities. Missing resolved parents must not turn into invented roots. A missing completion means incomplete evidence, not proof the application is still running. Recordings with no explicit end remain unsealed, not proven live.

### 2. Calling contexts and timing statistics

Expose call paths and per-path population statistics, including ordinary/reentry counts, completed-call counts, semantic inclusive time, synchronous-child time, await time, and supported self time. Keep raw reentry time available. Distinguish the sum of invocation durations from recursion-aware path-inclusive time. Do not double-count retained completions already included in aggregate deltas. Do not invent started counts, outcome populations, or await counts.

Keep checked arithmetic, valid clock interpretation, late-definition reconciliation, and evidence of missing/conflicting paths. Live-window subtraction can be temporarily unsupported; do not clamp invalid self time into plausible numbers. Preserve aggregate counts when clocks invalidate timing. Expose or keep diagnosable unsigned raw evidence rather than silently losing it to SQLite's signed range.

### 3. Richer retained calls and captured-value queries

Expose useful already-recorded identities/relationships, function metadata, endpoints, durations, outcome, announcement/completion state, capture references, and separate input/output/error states. Add an `error_calls` convenience relation or equivalent documented query, explicitly at retained-call grain. Existing names/arguments/output/error brackets must continue working.

Fix practical value-flow limitations in the SQL frontend (including output handling through supported CTE/subquery/UNION paths) and explicit unsupported comparison behavior as needed. Do not present unavailable CAS data as a legitimate nonmatch without outcome diagnostics. Preserve lazy decoding and per-query memoization. Use source-recorded names, not the current edited program's parameter names.

### 4. Function metadata, clocks, and issues

Expose recorded function definitions/parameter layouts and useful clock/evidence diagnostics. These are reader-side projections of information already present, not new metadata production. Missing dynamic metadata stays missing. Expose enough schema/capability documentation that an agent can discover supported fields and understand unavailable old features.

### 5. Playground consumers

Keep existing runs/values working through the common query path, and fill thread/calling-context views from the newly implemented supported relations. Give CLI coverage priority for the first checkpoint. Do not fake throw stacks or media bytes that were not captured. Separate unsupported capabilities from empty results.

## What must remain explicitly unsupported or limited

- Full-population success/error/cancellation statistics when only aggregates exist.
- Full-population invocation-start/active-call counts.
- Exact latency distributions/extremes for invocations preserved only as aggregate sums/counts.
- Original throw occurrence/site, fresh/rethrow classification, and causal propagation links. Equal error-value CAS IDs do not identify the same throw.
- Durable source call-site lines without a matching immutable PC-to-source artifact.
- Runtime producer-loss counts / absent-capture reasons not recorded in BTEL.
- Proven terminal recording extent or final clock status absent actual final evidence.

Some of these have useful narrower answers: recorded root outcomes, retained call outcomes/latencies, failing-call ancestry, recorded caller PC, and reader-detectable file/CAS problems. Name that narrower scope rather than claiming full parity.

Important later opportunity: `TimingRecord::FunctionTimingCompletion` already carries `outcome`, and all timing completions carry endpoints. The processor currently discards outcome distribution when reducing deltas. Counts or histograms could therefore be added downstream in phase 3 without new VM fields/events, but **not in phase 1**. Additional consumer work can still cause contention/backpressure; benchmark it when that phase begins.

## Acceptance and today's deliverable

Use real programs/recordings and actual `baml query` invocations, not only tests against hand-built SQLite rows. Demonstrate:

1. Recent executions with outcomes and timing.
2. Threads/spawn tree for an execution.
3. Most frequently completed functions and expensive calling contexts.
4. Recursion-aware timing and the difference between retained calls and aggregate population.
5. Parent/thread/context inspection of a retained call.
6. Scalar/nested input filters and captured output/error rendering.
7. Errored execution/retained-call inspection without inventing throw provenance.
8. Function/parameter metadata and schema discovery.
9. Query while recording, then refresh after another immutable segment is published; no reread of unchanged indexed files and a fixed committed query snapshot.
10. Partial/missing/conflicting evidence and unavailable captures are reported honestly. Reuse existing recovery/concurrency tests and extend them only where new semantics require it.

Persist a short `documents/btel-query-demo.md` with reproducible setup/recording commands, actual successful query commands and representative output, the binary/build command, and a compact supported/partial/unsupported question list. Update `documents/btel-query-prototype.md` to make clear what is newly completed and what remains. Preserve existing benchmark results.

Run relevant tests, formatting, and affected-crate Clippy on the pinned toolchain. Keep builds bounded; current filesystem had about 64 GiB free at handoff, and the old temporary build helper was lost on reboot. Do not spend the first checkpoint rebuilding and benchmarking unrelated workspace components. Record any verification failures honestly; a demo is not evidence that all checks passed.

After functionality is demonstrated, collect representative query/perf measurements using the existing harness where applicable and persist results. Reader optimizations can follow those measurements, preserving a functional baseline and a clear before/after comparison. Defer new producer evidence and VM work to their distinct later phases.

## Later phase: short iteration measurements (deferred until functionality works)

Tony explicitly rejected repeating the previous roughly 1.5-hour measurement campaign during development. The requirements below are saved for the later performance phase, not work to start now. Complete functional query coverage and its correctness checks first; do not start benchmark-harness work merely because one early demo query passes.

Provide documented one-command presets (actual option names are an implementation choice):

| Preset | Intended use | Target runtime after explicit build/fixture setup |
|---|---|---|
| Quick, the default | Routine reader/index regression checks | Roughly 30–60 seconds on this machine; bounded by a configurable overall budget, initially 120 seconds |
| Focused | Before/after measurements for a selected ingest, query, value, or incremental-refresh change | Roughly 2–5 minutes, selected cases only |
| Full, explicit opt-in | Broad release/milestone validation and justified producer comparisons | Long runtime allowed; print estimated scope before starting |

Targets are requirements to implement and measure, not claims about a harness already built. Report actual elapsed time and adjust fixture sizes/repetitions until the quick preset is useful on this computer.

Implementation requirements:

- Separate compilation and fixture preparation from timed benchmark runs. Provide explicit setup commands, record their cost separately, and reuse validated release binaries and versioned immutable BTEL/CAS fixtures. Do not silently rerun producer calibration/generation or build the base recorder for every query-only run. Reject stale/incompatible inputs or require explicit setup rather than measuring the wrong binary.
- Keep fixture manifests with relevant format/version, size/count, and workload identities. Preserve input files; create/reset derived indexes only in dedicated benchmark scratch locations. Support repeatable pristine-index and one-fixed-segment incremental cases without accumulating extra data across repeats.
- The quick preset should cover a small representative set: warm count/statistics/tree queries, unchanged-source refresh, a controlled incremental refresh, a captured-value filter/render, and one bounded first-index case. Use enough rows/unique captures to exercise real work; avoid a full Cartesian product of workloads, caches, queries, and rebuilds.
- Build a fresh index once per fixture/repetition only when measuring first-index cost. Do not rebuild it independently before every metadata query. Warm-query cases reuse the derived index; label cache/index conditions precisely. First-index does not mean OS page-cache cold.
- Default to a few repetitions with medians and observed ranges. Allow selecting a single case/workload, repetition count, and baseline result file. Do not advertise tiny differences from a short noisy run as statistically established regressions or speedups. Escalate only an affected case when more confidence is needed.
- Report whole-command wall time as well as available refresh/SQL/CAS timings, file/blob reads, result checks, dataset size, and binary/build identity. Validate answer equivalence where comparable; show changes in semantics/query definitions instead of making invalid before/after comparisons.
- Enforce per-case and overall time budgets, preserve partial raw results, and clearly report timeout/skipped/incomplete status. A truncated run is not a pass. Print concise progress and elapsed time.
- Recorder comparisons, broad growth curves, and OS-cache eviction sweeps belong to explicit focused/full runs. The producer is frozen in phase 1; no need to repeat the full base/off/on recorder matrix after ordinary reader edits. Reader/recorder interference still gets a targeted check when indexing or query resource use materially changes.
- Retain the previous full results and make the expensive suite explicitly opt-in. Document the quick/focused/full commands and observed durations in the demo/benchmark instructions.
