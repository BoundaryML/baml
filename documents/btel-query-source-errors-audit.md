# Call/spawn source navigation and error origins: implementation audit

Audited 2026-09-25 at `2e8be7ca16760b821836a249a72826520a6156f6`,
the query branch on top of PR #4958. This is a source-code audit and proposed
implementation scope, not an implemented feature or a performance result.
No production code was changed for this audit.

**Status:** implemented the same day. The
[implementation record](btel-query-source-errors.md) has the decisions that
were taken, what differs from this proposal, the limits and the tests.

## Recommendation

Use one focused Opus 5.5 xhigh implementation session for these two features.
They share source coordinates, reader normalization, and playground navigation.
Implement them in reviewable stages within that session: source maps, exception
evidence, reader/query integration, then UI and validation.

This is feasible as a bounded feature, not as a promise of complete old-tracer
parity. Ordinary recorded call/spawn sites are straightforward. Reliable error
origins require new exception-path evidence and identity bookkeeping. Exact
sites for collapsed recursive calls, and completely general rethrow identity,
cannot be obtained merely by exporting the existing source maps and traces.
Where identity cannot be established, report it as unresolved rather than
inventing a causal link.

## What the code already has

| Evidence | Where it exists | What reaches BTEL today |
| --- | --- | --- |
| Caller function, caller PC, callee, call/spawn edge | `bex_vm/src/telemetry.rs`: `CallPathKey`, `resolve_call_path_slow` | Already recorded in `CallPathDefinition` |
| Source ranges and line numbers per PC interval | `bex_vm_types/src/bytecode.rs`: `LineTableEntry`, `CompactCode::line_entry_for_pc` | Not recorded |
| Function name, source path, definition range, argument names | `Function::runtime_metadata`, heap's static metadata snapshot | Recorded once when referenced |
| Original diagnostic stack and cause | `bex_vm/src/vm.rs`: `try_unwind_exception`, `ThrowContext` | Not recorded |
| Failed child task's diagnostic stack | `Future::error_trace`, error branch of `OpCode::Await` | Not recorded |
| Each retained call's failure completion and optional captured error value | `TelemetryState::complete_invocation` | Recorded, without throw identity |
| Distinct error-occurrence identity | No suitable existing durable identity | Not recorded |

The successful instruction/call/return path does not need to resolve source
lines or copy stack traces for either feature.

## Source mapping

The compiler already emits run-length encoded entries containing PC, source
span, one-based line, sequence-point flag and discriminator. Compact-code
construction translates instruction-index PCs to **byte offsets**. The recorder
must export the map corresponding to the code actually executed, with an explicit
coordinate kind and code extent; it must not mix the two tables.

The normal call fast path and spawn opcode record `cur_pc`. The general call
path uses the bytecode caller's `faulting_pc` when dispatching through a hidden
native continuation. This latter location is the BAML call that entered the
native operation; it is not a new user-authored call expression inside Rust.

Proposed implementation:

1. Add optional owned source-map metadata beside the existing cold function
   metadata. Keep the executable `Function`, bytecode/frame layouts, and call
   instrumentation unchanged. Use existing compiler data; no new mapping pass
   is required for the ordinary case.
2. Copy/convert only for recording-enabled engine construction, as the existing
   static metadata bridge does. Publish each referenced function's map once per
   recording. Do not read source files on a call or keep unprotected heap
   pointers in the recorder. This adds startup memory/copying and recording
   bytes; it is not free. Sharing immutable metadata can be a separate measured
   improvement if the initial cost warrants it.
3. Resolve and validate PCs in `btel_reader`. Index resolved locations during
   ingestion/reconciliation, including when a definition arrives later.
   `call_paths` and its aggregate views expose the recorded context site;
   `threads` exposes its spawn site; `calls` exposes a site only with the
   appropriate evidence state.
4. Feed these locations to `playground_btel`. The TypeScript UI already has
   source links and a navigation callback; the current BTEL adapter fills their
   source fields with null.

Do not infer a call site from the callee's declaration. Validate compiler-local
file IDs against the metadata's file association instead of treating them as
globally meaningful. Missing maps, synthetic functions, unsupported dynamic
functions, out-of-range/sentinel PCs and conflicting definitions remain explicit.

Two important qualifications:

- **Direct recursion collapses call paths.** Both `enter_bytecode` and
  `enter_timing` reuse `saved_call_path` for direct reentry without recording
  that invocation's caller PC. A reentrant call must not display the outer
  caller's location as its own recursive call expression. Expose the aggregate
  context's entry site separately from exact invocation-site evidence; leave
  the latter unresolved for reentry. Fixing this completely changes call-path
  production and is outside this session's frozen normal-path scope.
- **Existing maps need semantic tests.** Direct and indirect call emission
  explicitly restores the enclosing call span after emitting operands. Spawn,
  virtual-call, sysop and await emission need checks with nested operands:
  exporting a map faithfully does not prove it labels the intended expression.
  A demonstrated span bug can be fixed in compiler metadata emission without
  adding runtime instrumentation. Do not claim such a bug is confirmed by
  this static audit alone.

Recorded locations must remain readable without the workspace. Source text
need not be embedded for this milestone. Navigating today's edited file is a
different claim: preserve the recording's source snapshot identity and display
that the target is current/unverified source unless a comparable current
snapshot proves a match. A project hash is not the historical source contents.

## Error evidence: what can be reused, and what cannot

`try_unwind_exception` already runs before frames are removed. It has the thrown
value, fresh/rethrow flags, live throwing PC, function/frame stack, and selected
diagnostic trace. It invokes the existing failed-call completion routine for
each frame actually unwound. A handler in the same frame returns successfully
without any failed-call completion: this is why looking only at `error_calls`
misses caught throws.

`StackFrame` contains a function name, file path, function-definition span and
error line. Its definition span is **not** the throw expression's span. It also
lacks a durable function ID, call ID, error ID and PC. Preserve an exact origin
PC/range at the first observed throw when possible; use an inherited diagnostic
line with an explicit weaker state when that is all the evidence contains.

The existing `ThrowContext` stores an `Arc` to the trace and a cause value.
Contexts are keyed by thrown `Value`, and subsequent fresh throws of that value
replace the context. `UnknownError` conversions can deliberately preserve it.
Future settlement carries the trace to an awaiter, but carries no occurrence ID.
Consequently:

- A CAS ID identifies content, never an error occurrence.
- A value or matching stack trace is not a safe occurrence identifier either.
- Copying a saved diagnostic trace does not prove which retained invocation
  originally threw it.
- `ThrowKind` and `language_is_rethrow` differ: an awaited failure uses
  `VmThrown::rethrow(value, false)`, while language/defer rethrows use `true`.
  Do not use either flag alone as a complete origin-propagation contract.

### Proposed exception-only transport

Use a distinct ID for each observed raise/unwind, plus a separately established
origin ID for propagation. An origin stays unresolved if its link is not proven.
Two independent fresh throws get distinct origins even when values are equal.

A promising way to avoid changing normal completion records is:

1. At the exception funnel, emit an owned, bounded **unwind-begin** record with
   raise/origin evidence, current function/PC and diagnostic information.
2. Let the existing unwinder emit its ordinary failure completions unchanged.
3. Emit **unwind-end** on all exits, including a local catch, unhandled failure
   and exceptional early return. A locally caught throw can therefore exist
   without any failed-call rows.
4. The background recorder tracks the active unwind per telemetry thread and
   attaches its ID to the failure completions in that interval. This also sees
   the final IDs of late-promoted calls, which are allocated inside completion
   and cannot simply be predicted at the throw site.

This is a design candidate, not a proven implementation. Validate ordering
across chunks, worker migration and interleaved threads. Never use one global
"current error". Do not associate cancellations or internal failures outside
the scope with a previous error. End/disabling/failure paths must release owned
payloads and must not leave stale scope state. Missing begin/end evidence must
not silently contaminate later failures.

An owned boxed payload in a new span-record variant should fit inside the
existing 56-byte span slot; verify actual size/alignment and existing variant
layout rather than increasing the budget. Keep 32-byte timing records untouched.
The background dispatch will still change. The wire needs additive optional
error records/links, with a minor-version update and older-file support.

Variable-size stacks/maps must use checked length accounting. The recorder's
existing `MAX_EVENT_BYTES = 128` reservation is for scalar span events: a new
variable-size error payload cannot be written through that assumption unchecked.
Prefer bounded separately accounted error metadata/sections over reserving a
large worst-case stack allowance for every ordinary event. Existing snapshot/CAS
encoding stays unchanged; error value capture remains separately controlled.

### Preserving an origin across boundaries

The implementation must settle this before claiming general propagation:

- **Synchronous unwind:** begin/end scopes give direct causal links to the
  retained frames actually failed. Hidden/timing-only throwers can still have
  a known function/location with no retained `throw_call_id`. Never substitute
  `active_id` when it denotes a retained ancestor rather than the thrower.
- **Language rethrow, defer and normalization:** extend cold exception-context
  bookkeeping, not per-call frames. Existing value-keyed stores are not enough
  to disambiguate nested handlers holding equal values. Use additional proven
  handler/occurrence evidence, or explicitly report ambiguity. Do not silently
  alter language exception semantics to make telemetry easier.
- **Spawn/await:** transfer origin identity with the failed future's evidence.
  A candidate that preserves `Future` layout is a bounded telemetry-side map
  keyed by engine-scoped `FutureId`, populated before error settlement wakes
  awaiters and read only on failed await. It must support multiple awaits;
  removing the entry at the first read is wrong. Define memory bounds, disabled
  recording behavior, settlement races, and lifetime/eviction semantics. Evicted
  links become explicitly unknown. Alternatively, assess sharing an expanded
  cold trace payload, but do not change a heap object's layout or successful
  future path implicitly.
- **Native/sysop/host failures:** record the BAML entry/boundary location when
  available and label it as such. A BAML caller location is not a foreign-language
  throw site. Empty/native-only stacks, cancellation and internal engine faults
  need explicit unavailable/unsupported states.

Exact rethrow ancestry for every legal value/handler case is not established by
this audit. A single session may deliver proven cases plus honest unresolved
states; it must not call that complete old `errors` parity.

## Minimal query and playground contract

Keep `error_calls`: it means retained invocations that failed. Add a separate
error-occurrence surface and a causal call-link surface; do not repurpose failed
calls as distinct errors. Names can be finalized in implementation, but the
three grains must remain separate:

- Origin/raise evidence: occurrence IDs, execution/thread, function and optional
  retained call, recorded source location and its resolution state, provenance
  kind, optional captured value reference and bounded diagnostic trace.
- Propagation evidence: a raise's proven origin, or an explicit unresolved link.
- Failed-call association: occurrence/raise ID linked to an actual retained
  failed call. Population counts remain completion counts, not distinct errors.

The reader validates and reconciles these facts; SQLite stores the derived
rows. Bump index schema/normalization versions as appropriate. Old recordings
still show failed calls and unavailable origin evidence. Source/error queries
must not read captured values unless requested.

In the playground, use occurrence rows when available, retaining the failed-call
fallback for old recordings. Display origin versus propagation/boundary location
accurately, and wire call, spawn and error locations through the existing source
navigation. Do not display a handler, awaiter or callee declaration as an exact
original throw site without evidence.

## One-session handoff and acceptance checks

Give Opus this document plus the existing parity/completion/performance docs.
Have it first record its concrete exception identity and transport decisions,
then implement the stages above in the current worktree. No full-schema redesign,
media/recursive CAS work, cloud query backend, query optimization or unrelated
cleanup belongs in this session.

Constraints: retain executable `Function`, `BytecodeFrame`/`Frame`,
`FrameTelemetry`, `Future`, value/GC layouts and existing hot record budgets.
Add no successful call/return/await or per-opcode instrumentation. Exception
branches inside `vm.rs` may change: this does not mean that file stays frozen.
List every cold data-structure change explicitly. A new span enum variant also
requires measured layout/code-generation review; unchanged size alone is not a
performance proof. If these constraints cannot support a case, expose its limit
and document the precise follow-up rather than silently broadening the changes.

Required real-recording tests:

1. Two call sites for the same function; direct, indirect, method and native
   callback calls; nested operands; compact byte-offset mapping.
2. Two spawn expressions; child wrapper versus spawning expression; missing
   metadata and out-of-range PCs; direct recursion reports its site limitation.
3. A fresh throw caught in the same function, and one propagating through
   several functions: distinct error evidence and only the actual failed calls.
4. Equal-valued independent throws; ordinary rethrow; nested equal-valued
   handlers; a different error thrown while handling one; defer and
   `UnknownError` conversion. Verify identities or explicit unresolved states.
5. A child failure awaited by its parent, including repeated awaits, parent
   catch and distinct children throwing equal values. The await line must not
   replace the child's original throw location.
6. A native/sysop/host failure, cancellation and unsupported internal faults;
   interleaved telemetry threads, chunk boundaries, late promotion, GC movement,
   disabled/no-sink telemetry and bounded/truncated payloads.
7. Old files and live prefixes; definitions/links arriving later; missing or
   corrupt error evidence; recorded source still queryable after source edits.
8. CLI queries and playground navigation for call, spawn and origin locations,
   including unavailable and stale/unverified source states.

Run relevant Rust tests/Clippy/formatting and TypeScript tests/typechecking for
the UI changes. First establish correctness. Then run a short paired recording
comparison against `2e8be7ca1` (and the preserved upstream baseline when useful):
ordinary tiny/dense/spawn workloads, startup with a sizable function dictionary,
and a separate throw-heavy workload. Report startup/RSS/BTEL bytes and exception
cost separately from successful execution. Reuse the roughly two-minute harness;
do not launch the old hour-long suite. Builds are separate from measurement time.

## Source anchors checked

- `bex_vm_types/src/bytecode.rs`: `LineTableEntry`, `CompactCode`,
  `Bytecode::lower_to_compact`; `baml_compiler2_emit/src/emit.rs`: line-table emission,
  call/spawn/await terminators.
- `bex_vm_types/src/types/function.rs`: `Function::runtime_metadata`;
  `bex_heap/src/functions.rs`: `static_function_metadata`;
  `bex_engine/src/lib.rs`: recording-enabled construction.
- `bex_vm/src/telemetry.rs`: `enter_bytecode`, `enter_timing`, `spawn_context`,
  `complete_invocation`, `resolve_call_path_slow`.
- `bex_vm/src/vm.rs`: `ThrowContext`, throw-context stores,
  `capture_user_stack_trace`, `try_unwind_exception`, `try_handle_external_thrown`,
  `complete_bytecode_invocation`, error branch of `OpCode::Await`.
- `bex_vm_types/src/errors.rs`: `VmThrown`, `ThrowKind`, `VmError`, `StackFrame`;
  `bex_vm_types/src/types/future.rs`: error trace/settlement;
  `bex_engine/src/future.rs`: identity allocation and `err_future`;
  `bex_engine/src/lib.rs`: `route_unhandled_vm_throw`, `inject_sysop_throw`.
- `btel_records/src/lib.rs`, `btel_processor/src/lib.rs`,
  `btel_recorder/src/recording.rs`, `btel_recorder/proto/recording.proto`,
  `btel_settings/src/layout.rs`, `btel_settings/src/encoding.rs`.
- `baml_lsp_server/src/playground_btel.rs`; playground `telemetry/evidence.ts`,
  `telemetry/TelemetryView.tsx` and `ExecutionPanel.tsx` source callback.

All crate paths above are relative to `baml_language/crates`; playground paths
are relative to `typescript2/pkg-playground/src`.
