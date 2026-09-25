# Remaining tracer parity: questions and implementation cost

Reviewed on 2026-09-24 against query commit `05d32656a` (PR #4986, based on
#4958), plus the playground outcome wiring described below. This review does
not assume that sibling PR #4983 or later upstream changes are included.
The [old schema inventory](btel-logical-schema.md) remains a historical
reference; this document prioritizes useful questions instead of copying its
tables and fields mechanically.

The subsequent [remaining-query audit](btel-query-remaining-questions.md)
covers all nine old relations, including the less visible gaps in liveness,
capture selection, complete function inventory, old identities and internal
CAS inventory. It separates missing evidence from SQL compatibility and
features the old tracer did not establish.

Implementation update: [structured value comparisons](btel-query-value-comparisons.md)
are now implemented for supported, fully captured values.
[Recording completion and final clocks](btel-query-recording-completion.md)
are also implemented on the background/shutdown path. Recorded source sites
are the next producer-metadata candidate. Cyclic/opaque comparisons and
value-based grouping remain unsupported.

Further statistics optimization is deferred. The measured 142→160 ms
function-statistics query is acceptable for this phase. This review and the
playground wiring add no producer changes and need no recording benchmark.

## What works now

The reader answers questions about executions, spawned threads, calling
contexts, completed-call counts, recursion-aware timing, retained arguments,
outputs and error values, function definitions and recording health. New
recordings also have aggregate success/error/cancellation counts, including
calls without retained spans. Missing evidence stays distinguishable from zero.

The small playground follow-up connects those existing SQL outcome columns
to the calling-context inspector. It shows succeeded, errored and cancelled
invocations. An opened execution can show its total errored-call count when
all contexts have known counts and cover the execution's completed population;
otherwise it keeps the retained-call fallback. The lightweight execution list
still omits that population rollup. Neither view calls failed invocations
distinct throws: one error can fail several calls as it propagates.

Old and mixed recordings keep their unknown state. Validation: 243 TypeScript
tests across 29 files, type checking and Biome pass; the real-engine playground
reader test passes for successful and failed calls, including timing-only
completions. Rust formatting and LSP Clippy with warnings denied pass. There
has been no interactive browser/VS Code verification or new performance run.

## Remaining questions

| User question | What is missing today? | Where the work belongs |
| --- | --- | --- |
| “Which calls received this exact list/object? Did two calls return equal structured values?” | Now implemented for supported acyclic values, including JSON comparison literals. Cycles, opaque values, capture truncation and comparison limits stay explicit. | **Reader/SQL only**, completed in the follow-up below. No new recording data. |
| “Which expression called this function or spawned this task?” | BTEL has caller function IDs and PCs, but lacks their PC-to-source mappings. Function-definition spans alone cannot answer this. | **Cold metadata plus reader.** The runtime already has compact-bytecode line tables. Publish mappings once per referenced function and resolve sites during indexing. |
| “Did this recording end? Are its clocks final?” | Now implemented: normal shutdown records the terminal extent and settled clocks. An end marker does not establish lossless capture; an unsealed prefix cannot establish process liveness. | **Recording lifecycle/background work**, completed in the [follow-up](btel-query-recording-completion.md). Older recordings remain unsealed. |
| “What image/audio input did the LLM see?” | The old capture path preserved media content descriptors. Current BTEL capture turns host objects, including media, into opaque placeholders. | **Capture/format work**, followed by reader/UI support. This cannot be repaired by adding a reader endpoint. Coordinate with recursive CAS work. |
| “Why is this capture missing? How much evidence was lost?” | Reader-detectable missing/corrupt blobs and captured truncation are visible. Complete producer loss and per-capture policy explanations are not. | **Mixed.** Some health summaries can use existing background counters; exact absent-capture reasons may need additional capture/producer facts. |
| “How many invocations started, are still active, or suspended on await?” | Timing-only calls supply completions and await duration, not all starts or await counts. Retained starts describe only that subset. | **Producer/VM instrumentation or policy changes.** Completion totals cannot reconstruct these counts. Defer. |
| “Where was this error thrown, caught or rethrown? Which failures share one throw?” | Errored calls and their captured values are known; throw identity, origin and propagation events are not. | **Exception-path instrumentation.** Equal error values or CAS IDs do not establish one shared throw. Defer. |
| “Which native/sysop calls consumed this time?” | Native/sysop functions are hidden by the current telemetry policy. Their separate call population is absent; bytecode-level waits do not reconstruct it. | **Coverage/policy and VM path changes.** Metadata alone cannot create missing invocations. Defer. |
| “Which process/source/function version was this? What are dynamic functions or threads called?” | Recording IDs, some source identity and static function definitions exist; process labels, thread names and dynamic-function definitions have gaps. | **Usually cold metadata or creation-time registration**, with each identity's scope defined explicitly. Lower priority than source sites; do not infer historical metadata from the current workspace. |

“Reader only” means the answer can be obtained from existing BTEL/CAS bytes.
“Cold metadata” still changes the producer, file sizes and startup work; it
does not require more work in each VM dispatch or a new frame layout. These
are distinct costs and should be measured separately when implemented.

## Structured value equality: implemented reader phase

The old [value semantics](../baml_language/crates/baml_query/src/value/semantics.rs)
implement list equality element by element, map equality independent of entry
order, and class equality using its name and field presence as well as values.
The old [SQL lowering](../baml_language/crates/baml_query/src/value/lowering.rs)
also exposes `baml_value_json(...)` as a comparison operand. In contrast, the
new [comparison bridge](../baml_language/crates/baml_query_btel/src/functions.rs)
previously reported unsupported comparisons for two structured operands. It
now delegates to a bounded semantic comparison over decoded captures. The
[hierarchy test](../baml_language/crates/baml_query_btel/tests/hierarchy.rs)
checks class inequality and keeps structured ordering explicitly unsupported.

Start with `=` and `!=` between captured values, plus the JSON-literal helper.
For example, once implemented:

```sql
SELECT call_id, fqn
FROM calls
WHERE args['tags'] = baml_value_json('["urgent", "billing"]');

SELECT a.call_id, b.call_id
FROM calls a JOIN calls b ON a.output = b.output
WHERE a.call_id < b.call_id;
```

These acceptance queries are now supported for the values described in the
[comparison contract](btel-query-value-comparisons.md). A join may compare many
pairs and encounter unsupported values; that contract shows how to restrict
its inputs before comparisons. This phase establishes correct answers before
tuning their execution cost.

Acceptance criteria:

- Equal lists/maps from different captures compare equal; changed elements,
  lengths or keys compare unequal. Map insertion order and storage sharing
  must not masquerade as different logical values.
- Class/enum identity, absent versus null fields, bytes and numeric rules
  match the old supported semantics wherever BTEL retained that evidence.
  Explicitly document any representation that cannot express the old state.
- Missing blobs, relevant truncation, opaque host objects and traversal limits
  cannot silently produce a definite equality answer when evidence is
  insufficient. Report the existing incomplete outcome and diagnostic.
- Shared/cyclic graphs terminate under a bounded traversal. Define the
  supported graph semantics explicitly; unsupported cases remain visible.
- Compare decoded values, not rendered JSON, handle bytes or CAS IDs. Reuse
  the per-query hydration cache; preserve metadata-only queries' zero CAS
  reads and the frozen producer audit.
- Cover SQL lowering, nested paths and a real `baml run` → `baml query` fixture.
  No long benchmark is needed to establish correctness.

This does not automatically fix value-based `DISTINCT`, grouping or `UNION`:
those need a consistent equality/keying contract of their own. Likewise the
old `baml_value_cid('bamlv_1_<hex64>')` constructor names a different identity
format from current CAS IDs; it needs an explicit compatibility decision,
not a reinterpretation of the old bytes.

## Next inexpensive producer candidate: recorded source sites

The existing [compact code line table](../baml_language/crates/bex_vm_types/src/bytecode.rs)
already maps byte-offset PCs to source spans and lines. The VM emits caller
identity/PC when defining a call path. The recording's
[owned metadata table](../baml_language/crates/btel_recorder/src/functions.rs)
already publishes function definitions once per referenced function, without
background heap access. Extending that table is the likely implementation
path; adding source lookups to every call is unnecessary.

Verify PC conventions for ordinary calls, native continuations and spawn
sites, plus recursion-collapsed paths and unavailable dynamic functions.
Record enough file identity to interpret the mapping independently of an
edited workspace. A source-content hash is already passed to the recorder
when available; it is not itself a copy of the source or a line map. Missing
or mismatched source must remain visible in navigation.

This is a separate producer change after the reader task, with focused
startup/recording/file-size measurements and the existing VM boundary audit.

## Boundaries that matter for later work

**Recording completion.** `RecordingBuilder::end_recording` now emits the
terminal marker only after input is exhausted and every observed epoch has
settled. Ordinary `flush_recording`/size/time rotation never ends a recording.
Unsettled epochs keep the remainder unsealed; missing earlier files remain
gaps even if an end file arrives. See the
[lifecycle contract](btel-query-recording-completion.md) for shutdown,
interruption and delivery-failure behavior.

**Media.** The old
[trace heap](../baml_language/crates/bex_engine/src/trace_heap.rs) recognizes
media URL/base64/file descriptors. Actual BTEL
[snapshot capture](../baml_language/crates/bex_vm/src/telemetry/snapshot.rs)
maps `Object::RustData` to `NonSnapshotableValue`; its test explicitly includes
a media value. Ordinary byte arrays are a different, captured representation.
Serving existing byte arrays is reader/UI work, but cannot restore media
descriptors discarded at capture. Supporting media may avoid dispatch changes,
but it still changes capture cost and needs its own measurements.

**Health.** Snapshot limit reasons are already reader-visible. Existing pool
allocation misses are not counts of lost events, and cloud delivery loss
counters count discarded groups/targets, not invocations. A generic `health`
table filled with zeros or relabeled counters would claim more than is known.

**Scope.** Population p95/min/max and histograms are potential new features,
not missing columns from the old `call_path_stats` contract. Do not turn them
into prerequisites for parity. Old store-internal manifests/runtime-option
history also need a present-day use case before porting. Cloud query service
and Boundary Studio remain separate work from this local reader review.
