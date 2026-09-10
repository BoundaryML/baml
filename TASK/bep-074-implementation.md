# BEP-074 implementation

Design agreed in the implementation thread:

- Context patches belong to `boundary.LocalId`; effective contexts are immutable.
- Reuse the existing per-thread profiler rings for call, context, and log events.
- Resolve execution inheritance in VM frames, independently of profiling.
  Publish effective snapshots through the shared rings for captured calls/logs.
- Persist call/log records in the shared data plane and values in the shared CAS.
- Expose separate `calls` and `logs` SQL relations.
- Log-only collection is memory-only unless the host explicitly enables history.
  Showing logs must not implicitly enable disk profiling.

## Progress

- [x] Add durable `LogEvent` evidence (tag 8), codec, and execution-reader support.
- [x] Test mixed call/log segments, CAS reuse, nullable attribution, absent
      retained call spans, malformed strings, and truncated records.
- [x] Add native profiler ring log events and bounded, generation-checked payload
      slots; release rejected writes immediately and consumed payloads after use.
- [x] Support log-only collection without enabling full profiling. Preserve
      live CLI/LSP/SDK log delivery, including WASM.
- [x] Replace the independent log queue and log-history writer.
- [x] Add copied `LocalId` patches, context events, inheritance, restoration,
      and spawn propagation.
- [x] Add `.context()`, log `event_name`, and query columns/relations.
- [x] Add native BAML behavior tests and transport/query integration tests.

Logs now have one bounded capture/encoding path. Native and WASM collectors
consume the same ring records; `TraceLogger` is only a delivery inbox. Native
history is explicitly enabled and uses the shared stream/CAS owner without
requiring call profiling. Browser history retains the same evidence/value
format in memory. Legacy writers and live fallback serialization are removed;
readers for existing saved artifacts remain.

Lifecycle history contains only run-start/run-completion records. Explicit
native history records its shared store location so replay works across
workspace roots. Lost-value log evidence retains source/preview metadata, and
hosts report subscriber/transport losses.

`LocalId.context()` copies primitive metadata into a patch. Null metadata values
remove keys; omitted/null identity inherits. Ordinary calls share immutable
snapshots, and spawned VMs receive the active snapshot before scheduling.
Return, throw, cancellation, and sysop resume restore the parent scope.

Selected calls retain `CallScope` evidence (tag 9); logs retain their effective
scope directly. Missing capture is unavailable, never replaced with guessed
parent attribution. Rejected scope publication is settled at call end rather
than retaining completed calls until the root finishes.

SQL exposes `logs` and `calls.distinct_id`/`calls.context` through native BAML
value columns. Query executions have independent budgets, hydration state, and
outcomes while retaining snapshot-fixed provider bindings. SQL/tagged-string
execution from BAML itself remains out of scope.

## Integration constraints

- The collector already tracks calls by `CallRef` and defers cross-ring spawn
  dependencies; do not assume one globally ordered stack.
- Rings can reject records. Missing structural/context events must not silently
  substitute an ancestor's context.
- Payload slots and retained value bytes are budgeted. Evidence batching now
  charges variable-sized log/thread metadata in addition to its per-fact minimum.
- Preserve context through unretained calls. Scope metadata is distinct from the
  profiler's existing CCT `ContextKey`/`ContextRef`.
- Evidence tags 0-7 and their golden encodings are unchanged. Readers must
  understand new log/scope tags 8-9.
- Exported builtin constant defaults are carried in compiled interfaces.
  Artifact ABI is now 5, following canary's ABI 4 changes; older compiled
  packages need rebuilding.
- Engine registration retains pending payload ownership through final drain.
  Consumer startup precedes heap acquisition; log publication uses the cached
  per-resume ring and cannot initialize transport under a heap permit.

## Verification

- Native store/events/engine/VM tests and log integration suites.
- Native playground memory-only delivery, explicit history-only replay,
  recorded cross-workspace store resolution, and legacy reader compatibility.
- WASM history and cooperative-ring tests executed with Node 22 and
  wasm-bindgen-test-runner 0.2.122.
- Native and WASM clippy with warnings denied.
- Native BAML scope/default tests, diagnostic/compiler snapshots, and source-less
  package default lowering (including a non-null exported default).
- Native query tests for identity, scope/data access, missing/lost values, and
  per-statement outcome/budget isolation.
