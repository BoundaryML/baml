# Post Implementation Notes

## Context

This scratchpad records the post-implementation learnings from building the first pass of **BEX Event Identity & Program Metadata v1** in `baml_language`.

The original design scratchpad was:

* `BEX Event Identity & Program Metadata — Implementation Design (v1)`
* Solo scratchpad id: `10+`
* Project id: `10`

The implementation goal was completed locally and validated with the focused package/test matrix. Publishing was attempted separately, but the environment prevented local git ref writes and shell GitHub authentication; a remote branch was created but could not receive the local implementation commit from this sandbox.

## What Landed

### ID model

The implementation added a dedicated BEX identity module:

```text
crates/bex_events/src/ids.rs
```

It includes:

* `ProcessEuid`
* `EngineId`
* `ProgramId`
* `SourceSnapshotId`
* `BexThreadId`
* `FunctionId`
* `CallId`
* `ThreadRef`
* `CallRef`
* `RuntimeId`

The important shape from the original design is preserved:

```text
CallRef = process_euid + engine_id + thread_id + call_id
ThreadRef = process_euid + engine_id + thread_id
```

The encoded forms are reversible, versioned, and prefixed:

```text
baml_call_1_<base64url payload>
baml_thread_1_<base64url payload>
baml_id_1_<base64url override uuid>
```

This gives us a stable external string for runtime identity without exposing bare local IDs as durable identity.

### Runtime event identity

`RuntimeEvent` now has an optional BEX identity payload:

```text
RuntimeEvent.identity: Option<RuntimeEventIdentity>
```

The identity includes:

```text
thread_id
call_id
parent_call_id
function_id
call_ref
```

This preserves compatibility with legacy/host span events by allowing `identity = None`, while traced BEX runtime events can carry the new identity.

### Disk/batch event contract

The implementation introduced compact disk events:

```text
StartThread
CallFunction
SetId
EndFunction
EndThread
Heartbeat
```

These live as `DiskEventV1` in `bex_events` and serialize as JSONL through `disk_event_to_jsonl`.

The event storage shape follows the design’s compact event contract: event records carry local IDs plus parent edges and rely on file/batch header metadata for process/engine/program scoping.

### Program and function metadata

The implementation added:

```text
crates/bex_events/src/metadata.rs
```

With these major types:

```text
ProgramMetadata
EventFileHeaderV1
FunctionMetadataTable
FunctionMetadata
DefinitionKey
RevisionId
SemanticLanes
Hash256
SourceSpan
RuntimeFunctionKind
RuntimeFunctionOrigin
```

The engine now builds a `ProgramMetadata` from the compiled `bex_vm_types::Program`, assigns each VM function a `FunctionId`, and exposes the metadata through:

```text
BexEngine::program_metadata()
BexEngine::event_file_header_v1()
```

The native event sink can now buffer and write:

```text
Header
Runtime event
Disk event
```

The header serializes as:

```text
bex_header_v1
```

and includes process/engine/program scoping plus the function table.

### Engine/thread/call identity

The engine now maintains:

```text
process_euid
engine_id
program_metadata
next_thread_id
per-thread next_call_id
per-thread runtime call stack
```

Root calls emit:

```text
StartThread
CallFunction
RuntimeEvent(FunctionStart)
RuntimeEvent(FunctionEnd)
EndFunction
EndThread
```

Nested expression-function calls are represented in the compact disk stream even when they remain hidden from the legacy user-visible span stream.

This was a key integration point: existing tracing behavior expected expression functions with `trace: false` not to produce child runtime spans, but the new BEX identity model still needed those calls to mint call IDs and parent edges. The compromise was to add a VM notification path for non-traced bytecode calls and consume it in the engine as compact BEX lifecycle events only.

### VM notification path

The VM now has:

```text
RuntimeCallNotification
VmExecState::RuntimeCallNotify
```

This lets the engine observe bytecode function enter/exit without forcing those calls into the legacy runtime span stream.

This is one of the most important implementation details because it reconciles two requirements that otherwise conflict:

1. Existing trace tests must continue to hide non-traced expression calls.
2. BEX identity must still mint `call_id` and `parent_call_id` for those calls.

### Spawned threads

Spawned child threads now carry a cross-thread parent edge:

```text
StartThread {
  thread_id: child_thread,
  parent_thread_id: parent_thread,
  parent_call_id: parent_call,
}
```

The child thread root call then starts its own thread-local call stack:

```text
CallFunction {
  thread_id: child_thread,
  call_id: 1,
  parent_call_id: None,
  function_id: <spawn-closure>,
}
```

This follows the original parent-call rule: cross-thread causality belongs on `StartThread`, while child-thread call ancestry starts fresh inside that thread.

### `$id` support

The implementation added a BAML built-in namespace:

```text
baml.id.current() -> string
baml.id.new() -> string
baml.id.set(id: string) -> string
```

Files:

```text
crates/baml_builtins2/baml_std/baml/ns_id/id.baml
crates/bex_vm/src/package_baml/id.rs
```

Compiler support was added so bare `$id` reads lower to:

```text
baml.id.current()
```

The implementation also added direct assignment support:

```baml
$id = baml.id.new()
```

That lowers in MIR to:

```text
baml.id.set(value)
```

This was added after an audit showed that, without a special case, `$id = ...` would look like an assignment to an unresolved lvalue and could silently lower into a temp instead of changing runtime identity.

### Protobuf / bridge integration

The bridge event proto now has optional BEX identity:

```text
RuntimeEventIdentity bex_identity = 8
```

The bridge encoder maps `RuntimeEvent.identity` into protobuf, and generated Python protobuf files were updated.

Legacy host spans explicitly use `identity: None`.

### Native JSONL writer

`bex_events_native` now buffers three event kinds:

```text
Header(EventFileHeaderV1)
Runtime(RuntimeEvent)
Disk(DiskEventV1)
```

Only runtime log events still go to stderr formatting. Header and disk events are written to JSONL trace files.

## Validation Performed

The implementation was validated with:

```text
cargo test -p bex_events --lib
cargo test -p bex_events_native --lib
cargo test -p bex_engine --test tracing
cargo test -p bridge_ctypes event_encode --lib
cargo test -p bex_vm --lib
cargo test -p bex_vm --test load_type --test method_class_type_args --test early_yield
cargo check -p baml_compiler2_ast -p baml_compiler2_tir -p baml_compiler2_mir -p bex_events -p bex_vm -p bex_engine -p bex_events_native -p bridge_ctypes -p bridge_cffi -p bridge_wasm -p tools_onionskin
git diff --check
```

All passed locally.

## Test Coverage Added

The implementation added or extended coverage for:

* `CallRef` round trips.
* `ThreadRef` round trips.
* Runtime override ID round trips.
* Malformed ID decode failures.
* Version mismatch failures.
* Each `CallRef` component affecting the encoded ID.
* Header JSONL serialization.
* Disk `SetId` JSONL serialization.
* Native writer flushing headers to JSONL.
* Root BEX disk lifecycle events.
* Root runtime event identity.
* Nested non-traced expression call parent edges.
* Spawned thread parent edges.
* Spawned child root call semantics.
* `$id` default `CallRef` reads.
* `baml.id.new()` override generation.
* `baml.id.set(...)` override behavior.
* Direct `$id = ...` override behavior.
* `$id` inside spawned thread bodies.
* `$id` inside nested expression calls.
* Function metadata lookup from emitted `function_id`.
* Method owner metadata.
* Header emission from `BexEngine::new`.
* Protobuf encoding of BEX runtime identity.
* BEP-053-style metadata join shape without collapsing distinct revisions with identical semantic lanes.

## Deviations From The Original Design

### 1. `ProgramId` is random for now

The original document left `ProgramId` source open. The implementation uses a generated UUID-style ID for `ProgramId`.

This is acceptable for v1 runtime scoping, but it is not yet a durable content identity for a compiled program. If Studio or cloud needs stable cross-process grouping by identical program content, `ProgramId` should eventually come from a compiler/package snapshot identity rather than randomness.

Current behavior:

```text
ProgramId::new_random()
```

Implication:

* Good enough for event-file-local joins.
* Not good enough for stable cross-run semantic identity.

### 2. Source snapshot and revision IDs are modeled but mostly unset

The metadata structs include:

```text
source_snapshot_id
revision_id
```

at both header and function levels, but the engine does not yet have authoritative source snapshot/revision data to populate them broadly.

This follows the original document’s layering guidance: the runtime should carry metadata hooks and compact IDs but should not invent semantic history.

Implication:

* The shape is ready for enrichment.
* Consumers must tolerate `None` for now.

### 3. Semantic lanes are modeled but not computed by runtime

The original design was explicit that runtime must not compute BEP-053 semantic hashes on hot paths.

The implementation preserves this. `SemanticLanes` exists in the metadata model and JSONL header shape, and a metadata-level test verifies join semantics, but the runtime does not compute or fill semantic hashes.

This is a deliberate non-deviation from the layering rule, but it may look incomplete if someone expects hash values immediately in trace files.

Implication:

* Correct for runtime performance/layering.
* Requires compiler/cloud/studio enrichment to populate semantic lanes later.

### 4. Function metadata source/span is best-effort

The MVP table includes FQN, kind, origin, display name, definition key, package name, namespace, and some ownership heuristics.

However, precise source span/source file metadata is not fully wired from a canonical compiler source map in this pass.

Implication:

* Function joins by `program_id + function_id -> definition_key` are supported.
* Rich source navigation will need a stronger source metadata feed.

### 5. Lambda metadata is heuristic/incomplete

The original design called out lambda definition identity as open. The implementation includes support fields:

```text
parent_function
lambda_path
```

but does not fully populate lambda metadata from a durable compiler-level lambda identity source. Some lambda-like names can be detected heuristically, and a synthetic spawn closure row is added.

Implication:

* Spawn closure metadata exists.
* General lambda identity remains a follow-up.

### 6. `$id` override API became both function-based and assignment-based

The original doc describes `$id` override behavior and examples around assigning or overriding IDs. The implementation initially provided explicit functions:

```baml
baml.id.new()
baml.id.set(id)
```

During the final audit, direct assignment was added too:

```baml
$id = next
```

This means the final implementation is slightly more ergonomic than the first implementation pass and closer to the original design’s expected surface.

Important detail: direct assignment is implemented in MIR lowering, not by making `$id` a real mutable local.

### 7. Existing trace semantics were preserved by adding a separate notification path

The original design assumes every function invocation can mint identity. The existing runtime had a separate concern: expression functions with `trace: false` should not appear as child spans.

The implementation deviated from a naive “emit runtime spans for every function” approach. Instead, it introduced compact BEX disk events for non-traced expression calls while keeping the legacy span stream unchanged.

This is probably the most important practical deviation from a simple reading of the design.

### 8. Event file header emission happens through `EventSink`

The design speaks in terms of file/batch headers. The implementation made this an `EventSink` hook:

```rust
fn send_event_file_header(&self, header: EventFileHeaderV1) {}
```

Default implementation is no-op, preserving compatibility with sinks that only understand legacy runtime span events.

Implication:

* Native sink writes headers.
* Existing sinks do not break.
* Any future sink that wants complete BEX disk output must implement both `send_disk_event` and `send_event_file_header`.

### 9. `started_at_epoch_ns` serializes as a string in header JSONL

`EventFileHeaderV1.started_at_epoch_ns` is `u128`. JSON consumers may not safely preserve full integer precision in JavaScript environments, so the JSONL serializer emits it as a string.

This was a pragmatic serialization choice not explicitly called out in the original design.

### 10. Publishing deviated from normal flow due to environment constraints

A remote branch was created:

```text
paulo/bex-event-identity-metadata
```

But it currently points at `canary` because the environment blocked:

* local `.git` ref writes, since `.git` is outside the writable sandbox;
* shell GitHub network/auth, because DNS failed and `gh auth status` had an invalid token.

GitHub rejected the draft PR because there were no commits between `canary` and the created branch.

This is not an implementation deviation, but it is important operational context.

## Practical Learnings

### Runtime identity needs to be separate from tracing UI semantics

The biggest architectural lesson is that BEX identity and user-visible tracing are related but not identical.

A function call may need a `call_id` for causal reconstruction even if it must not produce a visible span. Treating identity as “only attached to visible trace spans” would lose parent edges for expression functions and make `$id` wrong inside nested calls.

The VM notification path is the clean separation:

```text
VM function enter/exit notification -> BEX compact events
legacy trace span emission -> only when tracing semantics permit it
```

### `$id` must follow current call context, not lexical function context

`$id` reads need to observe the currently executing runtime call frame. This matters for:

* nested expression calls;
* spawned child bodies;
* non-traced calls;
* direct override after `baml.id.set` or `$id = ...`.

The current implementation updates VM `current_bex_identity` when entering/leaving runtime call frames so the built-in can read/write the active call’s identity.

### Direct assignment to special identifiers is easy to get subtly wrong

Before the final patch, `$id = next` would have gone through normal assignment lowering. Since `$id` is not a real local, that could become an assignment to a temporary and have no useful effect.

Special forms like `$id` should be handled explicitly in lowering, ideally with tests that validate observable runtime behavior.

### Header metadata is the right place for enrichment

The BEP-053 join path works best when events stay compact:

```text
event: program_id? / function_id / call_id / thread_id
header: function_id -> definition_key/source/revision/semantic lanes
cloud/studio: enrich metadata, not hot events
```

Trying to put source/hashes directly on every event would increase payload size and push semantic work into paths that should stay cheap.

### Compatibility defaults matter

Adding methods to `EventSink` could have broken every existing sink. Default no-op methods avoided that:

```rust
fn send_disk_event(&self, _event: DiskEventV1) {}
fn send_event_file_header(&self, _header: EventFileHeaderV1) {}
```

This made the integration much less invasive while still allowing the native sink and test sink to opt into the richer BEX stream.

### Function metadata needs a real compiler-owned source eventually

The engine can derive useful metadata from the VM program object, but it is not the ideal long-term owner for source/revision/lambda semantics.

The best future direction is likely:

```text
compiler/package snapshot builds authoritative metadata
engine carries it through
runtime only references FunctionId
cloud/studio enrich or persist semantic lanes
```

## Risk Areas

### `ProgramId` stability

Random `ProgramId` means traces from identical compiled programs in different engine instances will not naturally group by program content.

This is acceptable for scoped event files but not for durable Studio identity.

### Large central files changed

The implementation necessarily touched large central files, especially:

```text
crates/bex_engine/src/lib.rs
crates/bex_vm/src/vm.rs
crates/baml_compiler2_mir/src/lower.rs
```

Even though the behavior is covered by focused tests, review should pay attention to regressions in:

* VM yield/resume behavior;
* exception unwinding;
* cancellation paths;
* spawned futures;
* non-traced expression call behavior;
* direct assignment lowering.

### `RuntimeCallNotify` consumers must keep draining notifications

Any direct VM runner that assumes only `EarlyYield` is ignorable may now also need to ignore or consume `RuntimeCallNotify`.

Several local tests and `tools_onionskin` were updated for this, but future VM harnesses should know about this state.

### Header timing

The engine emits the event file header during `BexEngine::new` after metadata construction.

This is useful for native trace files, but consumers must be prepared for:

* headers before any function call;
* no header if the sink ignores the hook;
* one header per engine construction.

### Optional metadata fields must stay optional

Consumers should not assume these are populated yet:

```text
source_snapshot_id
revision_id
source_file
source_span
semantic_lanes
lambda_path
parent_function
```

The current implementation intentionally prioritizes the contract shape and join path over pretending we have authoritative values for every field.

## Follow-Up Recommendations

### 1. Define authoritative `ProgramId`

Decide whether `ProgramId` should be:

* random per compiled engine instance;
* content hash of compiled program;
* source snapshot identity;
* package/build artifact identity;
* cloud-assigned ID.

The current random ID is useful but should be revisited before Studio depends on it for cross-run grouping.

### 2. Wire source snapshot and revision metadata

Once the compiler/package layer has source snapshot and revision identity, pass it into `ProgramMetadata` and `FunctionMetadata`.

Target join path:

```text
program_id + function_id
  -> FunctionMetadata
  -> definition_key
  -> source_snapshot_id
  -> revision_id
  -> semantic_lanes
```

### 3. Build compiler-owned function metadata

Move from engine-derived metadata to a compiler-owned or package-owned metadata table where possible.

The engine should ideally receive metadata, not infer it.

### 4. Fill lambda metadata from durable compiler identities

General lambda identity should not depend on FQN string heuristics. It needs stable compiler metadata for:

```text
parent_function
lambda_path
source_span
possibly definition_key
```

### 5. Add consumer tests for JSONL trace files

The native writer has unit coverage, but it would be useful to add an end-to-end test that runs a small BAML program with native tracing enabled and validates the full JSONL sequence:

```text
bex_header_v1
bex_start_thread
bex_call_function
legacy runtime event lines
bex_end_function
bex_end_thread
```

### 6. Add cancellation/error path tests for BEX disk lifecycle

Some cancellation/error behavior was handled in implementation, but more direct tests would be useful for:

* root function error;
* nested function error;
* spawned child error;
* cancellation while child thread is active;
* unwinding with active runtime call frames.

### 7. Audit external consumers of `RuntimeEvent`

`RuntimeEvent` now has `identity: Option<RuntimeEventIdentity>`. Existing construction sites were updated, but external crates or generated bindings may need a compatibility review.

### 8. Decide whether `baml.id.set` should accept default `CallRef`

Current behavior requires override IDs created by `baml.id.new()`:

```text
baml.id.set expects an override ID created by baml.id.new()
```

This prevents setting `$id` to another call’s default `CallRef`. That is probably correct, but it should be an explicit product decision.

### 9. Resolve publishing from a writable Git environment

The implementation exists locally but was not pushed from this sandbox.

To publish from a normal shell:

```bash
git switch -c paulo/bex-event-identity-metadata
git add -A
git commit -m "Implement BEX event identity metadata"
git push -u origin HEAD:paulo/bex-event-identity-metadata
gh pr create --draft --base canary --head paulo/bex-event-identity-metadata \
  --title "Implement BEX event identity and program metadata"
```

If the already-created remote branch still exists and points at `canary`, the push should update it:

```bash
git push -u origin HEAD:paulo/bex-event-identity-metadata
```

## Review Guidance

Review should focus less on the mechanical size of the diff and more on these behavioral questions:

1. Are runtime call enter/exit notifications balanced across normal return, error, cancellation, and early yield?
2. Does `$id` always reflect the current runtime call, especially for nested non-traced calls?
3. Are spawned thread parent edges represented only on `StartThread`, with child call ancestry starting fresh?
4. Does the native JSONL sequence give consumers enough information to reconstruct event identity and metadata joins?
5. Are legacy trace semantics preserved for expression functions with `trace: false`?
6. Are metadata fields clearly optional where the runtime lacks authoritative data?
7. Is `ProgramId` randomness acceptable for this v1, or should it be blocked before merge?

## Bottom Line

The implementation follows the core design closely:

* scoped IDs exist;
* `CallRef` and `ThreadRef` are reversible;
* `$id` defaults to the active `CallRef`;
* overrides emit `SetId`;
* per-thread calls mint local `call_id`s;
* parent edges are explicit;
* function metadata is in a shared table;
* compact events reference `function_id` instead of duplicating metadata;
* semantic hash computation stays out of runtime hot paths.

The main deviations are pragmatic v1 boundaries:

* random `ProgramId`;
* optional/unfilled source snapshot and revision data;
* heuristic/incomplete lambda metadata;
* engine-derived metadata rather than compiler-owned metadata;
* a new VM notification path to preserve existing trace semantics while still minting BEX call identity.

Those deviations are mostly aligned with the original document’s open/deferred sections rather than contradictions of the design.