# Local Profiling Backend MVP

Status: implementation handoff plan. Product behavior is pinned below; Phase 0
freezes measured limits and durable codecs before the corresponding backend
phases land.

The material under `TASK/reference/` is background only. It is stale in
important places and must not be treated as evidence that a CCT, segmented
evidence store, canonical CAS, query layer, or retention system already
exists.

This plan replaces the invocation-growing profile pipeline with a local
backend built from three linked planes:

1. a calling-context tree (CCT) that aggregates every structural call the
   profiler successfully decodes;
2. exact evidence records only for calls selected by one central capture
   policy; and
3. a project-shared content-addressed store (CAS) for deduplicated captured
   values.

The design is intentionally smaller than the prior draft. It does not add a
global event sequence, a general reorder stage, a full execution timeline,
dynamic error promotion, lifetime span limits, automatic retention, CAS
garbage collection, a hosted backend, or compatibility readers for old
profiling artifacts. It does retain aggregate inclusive, runtime-self, and
await time without sequencing every call record.

## 1. Contract at a glance

This section is a summary. Sections 4–12 are authoritative for interfaces,
schemas, ordering, failure behavior, and resource limits.

### 1.1 Three linked data planes

The backend has three grains:

1. every decoded language-call activation updates one CCT context;
2. root, LLM, and call-site `LocalId` policy may additionally select an exact
   span; and
3. selected value occurrences reference byte-identical bodies in a
   project-shared CAS.

A normal CCT context is keyed by parent context, function, call site, and
call/spawn edge. Repeated invocations update counters rather than adding rows.
Different call sites and spawn edges remain distinct. Suspension and resumption
continue the same invocation.

If pressure prevents normal context creation after drain and rollover, a
preallocated emergency overflow aggregate records incomplete population
without fabricating a parent, function, call site, or depth. Missing or
unjoinable structural facts are explicit population loss, and exact evidence
that cannot obtain a durable CCT join is rejected.

### 1.2 Capture and identity

Root and LLM calls select input, output, and error values. Ordinary calls are
CCT-only unless a `LocalId` is consumed at that call site:

~~~baml
let id = boundary.id()
id.capture(inputs = true)

some_func(value, $id = id)
~~~

`capture(...)` mutates cumulative tri-state role overrides; it does not
select a call or capture anything by itself. `$id = id` is evaluated last,
consumes the handle once, selects exactly `some_func`, and installs the
callee-visible runtime identity. The ordinary-call example records exact
metadata and only the input. A bare `LocalId` on an ordinary call is
metadata-only. Null or omitted arguments leave existing overrides unchanged,
the last non-null value wins, aliases share mutation, and post-consumption
reuse or mutation remains catchable `InvalidArgument`.

`baml.id.set` remains a separate mid-call runtime-identity operation. It
does not select a span or change value roles; an already-selected span retains
the annotation. Runtime IDs never enter CCT identity, and call/runtime-ID
behavior remains available with profiling off.

### 1.3 Exact evidence, errors, and time

An exact span contains start/end metadata and references its CCT context.
Inputs and outputs are occurrence records. Error evidence is unwind-grain:
each applicable VM unwind records one `Fresh` or `Rethrow` capture, one
value attempt, the throwing call/context/site, and links to every selected
error-enabled span terminated by that unwind. Normal CCT parent links
reconstruct the stack; overflow reports it as incomplete. User materialization
of `ErrorContext` is irrelevant to profiler capture.

The CCT aggregates inclusive time, direct synchronous-child inclusive time,
await duration, and await count. Runtime self is derived after merge:

~~~text
self_ns = inclusive_ns - direct_call_child_inclusive_ns - await_ns
~~~

Spawned children are never subtracted. The VM folds suspension into a sparse
per-call accumulator and emits one awaited end variant only for calls that
actually waited. The MVP adds no global/thread event sequence, general reorder
stage, per-suspension record, or per-instance exact-span self/await fields.

### 1.4 Bounded runtime, unbounded admitted lifetime

The process memory governor bounds active epochs, open calls/joins, transports,
writer queues/batches, value work, caches, and open files. These are transient
limits, not per-boundary lifetime caps. Completed CCT and evidence batches
become immutable segments and release memory; later calls and manual captures
retain the same policy.

The evidence/value pipeline reserves before copying or encoding, with a
reusable manual reserve and a single-value guard. Pressure causes explicit
population, evidence, or value loss and never aborts or changes BAML
execution. Section 8 is the complete resource-policy, derived-sizing, and
failure-reason contract; hidden production limits are forbidden.

The project disk byte limit and minimum-free-space guard fail closed for new
profiling data. They never delete committed artifacts. There is no automatic
retention, eviction, retry backlog, or CAS garbage collection. Recovery is an
operator-controlled close/clean-or-free-space/reopen cycle.

### 1.5 Execution lifetime and profiler off

All ordinary and detached spawned descendants remain in their parent's
profiling execution. `detach = true` changes only cancellation-token behavior.
Each child acquires a generation-tagged execution lease before scheduling and
releases it after its final profiler facts. Root return records status and
releases without waiting; the last descendant release submits the terminal
barrier. The profiler does not cancel or join work to finish an artifact.

`BAML_PROFILE=0` creates one shared, immutable off session with no profiler
ring, consumer, CCT/evidence state, value heap, capture hooks, CAS/store handle,
locks, files, directories, timestamps, or lazy manual activation. Internal
roots may also be explicitly suppressed inside an on session. Both modes
preserve language call IDs, runtime-ID stack behavior, `LocalId` mutation and
consumption, and separately requested logging.

### 1.6 Durability and cutover

CAS objects are durable before evidence references them. Context definitions
and selected span starts are durable before dependent evidence. Publication
is batched: a per-process stream writer commits meta and data segments on a
`publish_interval` cadence (default 1 s) rather than per artifact, and the
`RootStarted`/`RootEnded` meta-plane records replace `run.meta` and
`run.end` (superseded by TASK/profiling-backend-streams.md §5.3). Immutable
objects and segments still use rename plus directory fsync; post-rename
ambiguity is retained as one bounded publisher-owned state rather than
misclassified as success or loss.

The new backend has no migration or compatibility reader for `.bamlprof`,
old invocation profiles, stack segments, or old captured-value records. Legacy
output may remain temporarily as a test oracle, then its production
writers/readers are removed. The new clean command only removes the complete
`profiles-v1` root under an exclusive lease; it does not delete mixed legacy
history.

### 1.7 Change discipline

Phase 0 inventories every producer and suppression seam, freezes durable
codecs and golden fixtures, records performance baselines, chooses the three
resource defaults, and freezes the deterministic Section 8 sizing policy
before dependent phases land. An implementation finding that changes capture
policy, record shape, ordering, failure behavior, or a resource guarantee must
amend this document and its acceptance tests before code lands.
## 2. MVP outcome and invariants

The MVP must satisfy these invariants:

1. Every decoded call start contributes to one normal context or one
   emergency overflow aggregate.
2. Repeating an unselected call path changes counters, not CCT cardinality.
3. Every exact span is selected at call entry by root, LLM, or explicit
   `LocalId` policy.
4. Every exact span references the context aggregate for that call, or an
   explicit emergency overflow reference.
5. Every requested value role has an occurrence record or an explicit loss
   state; no selected value silently disappears.
6. Multiple occurrences may reference one `ValueCid`. Occurrence identity and
   content identity are never conflated.
7. Profiler pressure may reduce profile completeness, but never terminates or
   changes application execution.
8. `BAML_PROFILE=0` installs no process profiling storage/value-capture state,
   and an explicitly suppressed internal root installs no per-root profiler
   state even inside an on session.
9. `boundary.id()`, `boundary.id.current()`, `$id`, and call identity continue
   to work with profiling off; `LocalId.capture` preserves mutation/error
   semantics but cannot persist evidence while explicitly off.
10. All immutable artifacts are published atomically and value objects are
    durable before evidence that references them.
11. CCT inclusive/direct-child/await components are additive; derived runtime
    self is exact only when timing health is complete and is never labeled CPU
    time.

The backend answers:

- Which calling contexts ran?
- How many invocations started and how did they end?
- What inclusive, runtime-self, and await time accumulated per context?
- Which contexts came from ordinary call versus spawn?
- Which root, LLM, or manually selected invocations have exact evidence?
- Which captured roles are available, and what CID stores each value?
- What was not captured or persisted, and exactly why?

It does not promise exact rows for ordinary unselected helpers.

## 3. Current canary reality

The useful substrate already present is:

- compact VM call/thread records;
- per-OS-thread producer rings with a native consumer and WASM cooperative
  drain;
- comparable profiling clock ticks across OS threads;
- `ProcessEuid`, `EngineId`, `BexThreadId`, `BexCallId`, `CallRef`,
  `FunctionId`, and `BoundaryId` identity types;
- revision-local function metadata;
- the `boundary.LocalId` language API and dedicated runtime-ID bytecode; and
- native `BAML_PROFILE` default-on gating.

Canary has no `SuspendThread`, `ResumeThread`, `AwaitInterval`, per-call await
accumulator, `self_ns`, or `await_ns` implementation. Reference claims that
these already exist are proposal history, not reusable code.

The invocation-growing paths to replace are:

- `bex_events/src/prof/consumer.rs` protobuf transcode and per-engine
  `.bamlprof` output;
- `run.rs` retained `profile_events`, repeated component reconstruction, and
  invocation-shaped `calls`/`threads`;
- `history/router.rs` retained event routing and repeated graph work;
- `history/boundary_writer.rs` per-thread stack segments;
- `run_wire.rs` invocation-shaped profile delivery; and
- WASM `.bamlprof` chunk delivery.

The present caps do not provide the target guarantees:

- the ring is lossless-by-growth until it aborts the process at its cap;
- `RunStore` and `BoundaryTraceRouter` each retain large event windows;
- repeated reconstruction work grows with retained invocations;
- `TraceHeap` has no byte bound; and
- even `TraceCaptureProducer::disabled()` constructs the heap and associated
  synchronization state.

The current `.bamlvalue` path is mixed-use. It owns captured values, log
events, log loss, `RunStarted`, and `RunCompleted`. It cannot be deleted
wholesale until the non-profile log and lifecycle records are rehomed or
intentionally retained.

## 4. Capture and identity contract

### 4.1 Central resolver

When `RootProfiler::Active`, one module owns capture resolution.
`RootProfiler::Inactive` bypasses this interface entirely after preserving
LocalId consumption/runtime identity:

~~~rust
struct CapturePlan {
    selected: bool,
    roles: RoleMask,
    reasons: SelectionReasons,
}

fn resolve_capture_plan(
    is_boundary_root: bool,
    capture_class: FunctionCaptureClass, // Ordinary | Llm
    local_id: Option<LocalIdOverrides>,
) -> CapturePlan
~~~

Resolution order:

1. start with no exact selection and no value roles;
2. root adds `ROOT` and enables input/output/error;
3. LLM adds `LLM` and enables input/output/error;
4. a passed `LocalId` adds `MANUAL` and selects the call without replacing the
   role mask established by steps 1–3;
5. apply the `LocalId`'s accumulated non-null role overrides.

Reasons are a bitset because a root may also be an LLM or manually identified.
The consumer needs the resolved selection and role bits, not host-specific
`values_enabled` interpretation.

Selection and value roles are independent. Consuming a `LocalId` through
`$id` always selects exact metadata, even when the resolved role mask is
empty. Calling `LocalId.capture(...)` without later passing that handle through
`$id` selects nothing. For an ordinary callee, `capture(inputs = true)` enables
only input. For an LLM callee, omitted roles retain the LLM base policy.

The error bit is an evidence role, not another selector. It enables the
unwind-grain behavior in Section 7.4 only after root, LLM, or explicit
`LocalId` has selected the call. An error in an ordinary CCT-only call never
promotes that call or its ancestors into exact evidence.

`FunctionCaptureClass::Llm` is derived explicitly from canary's
`FunctionMeta::Llm`/compiler capture classification. It is not inferred from
the generic runtime function kind: an LLM function is currently a bytecode
function. The selected row is the outer, user-visible LLM function. Internal
client/sysop helpers remain CCT-only unless they independently satisfy root or
manual policy, and their time remains inside the outer call's inclusive time.

The current reserved `CallFunction.flags: u8` is sufficient for the MVP:

| Bit | Meaning |
|---:|---|
| 0 | input role enabled |
| 1 | output role enabled |
| 2 | error role enabled |
| 3 | selected |
| 4 | selected because root |
| 5 | selected because LLM |
| 6 | selected because explicit LocalId |
| 7 | reserved; must be zero |

This avoids expanding the hot call-start record. The resolved plan must be
available before the structural call-start record is emitted.

### 4.2 LocalId semantics to preserve

`boundary.LocalId` remains:

- a random runtime ID plus tri-state role overrides and a consumed bit;
- mutable through aliases before consumption;
- cumulative across repeated `capture(...)` calls;
- unchanged for a role when that argument is null or omitted;
- last-non-null-wins for each role;
- fluent, returning the same handle;
- single-use when passed through `$id`; and
- scoped to exactly one callee, never inherited by nested calls.

“Later `id.capture`” means later than creation but before `$id` consumes the
handle. Post-hoc capture after the callee has already run is not supported by
this API.

### 4.3 Existing runtime-ID API

`baml.id.set` is public in canary and is not currently marked deprecated.
It is a runtime-identity operation, not the manual-capture API, and this MVP
does not deprecate or remove it.

`baml.id.set` continues to update the current call's runtime identity. It does
not select an exact span, change the call's resolved value roles, or
retroactively capture values. If root, LLM, or a call-site `LocalId` already
selected the call, the profiler retains the resulting runtime-ID annotation
on that span. If the call is CCT-only, no exact span is created solely because
its runtime identity changed.

The implementation may replace the raw `SetFunctionId` record with an
equivalent runtime-ID annotation record only after both the call-site
`LocalId` path and mid-call `baml.id.set` retain their existing language
behavior and selected-span attribution.

No unconditional call-ID or runtime-ID stack behavior is removed.

## 5. Runtime data flow

The hot path is:

~~~text
VM records
  -> bounded per-OS-thread transport
  -> decode/join by IDs
  -> active CCT epoch + selected evidence facts
  -> immutable CCT/evidence segments
  -> local readers
~~~

There is no protobuf transcode, raw-profile file write, general ordering stage,
or repeated full-call reconstruction between the ring and the new consumer.
Live observation must not depend on opening a legacy disk writer.

### 5.1 Execution ownership

The direct consumer must not rediscover execution ownership by retaining a
large graph of unrelated events. There are no admission or terminal control
messages: admission facts (root `ThreadRef`, host runtime token, admission
ticks) ride the reserved registry slot itself and are collected by
`registry.take_admitted()` on the consumer thread, and the terminal
hand-off is the slot's `finish_ready` flag plus a consumer wake
(superseded by TASK/profiling-backend-streams.md §5.5).

Execution lifetime is producer-owned through one bounded registry entry:

~~~rust
struct ExecutionHandle {
    slot: u32,
    generation: u32,
}

struct ExecutionState {
    root_thread_ref: ThreadRef,
    runtime_id: BoundaryId, // host runtime token, opaque to the profiler
    phase: AtomicU8, // ExecutionPhase
    active_threads: AtomicU64,
    root_status: OnceLock<ExecutionEndStatus>,
    finish_control: ReservedControlSlot,
    health: ExecutionHealthBlock,
}

enum ExecutionPhase {
    Open,
    RootReturned,
    Closing,
    Released,
}

struct ExecutionThreadLease {
    execution: ExecutionHandle,
    armed: bool,
}

enum ExecutionEndStatus {
    Succeeded,
    Failed,
    Cancelled,
    Panicked,
    Abandoned,
}
~~~

`ExecutionHandle` is a process-local slab handle, not a durable identifier.
Its generation prevents reuse from targeting a later execution. The registry
slot count is derived from the process memory policy and complete measured
slot/control/health layout; an execution that cannot reserve a stable slot
and its protected control memory starts with profiling off.
`ExecutionThreadLease` is a small field on an outer thread-completion
guard. It does not allocate
another task, map entry, or `Arc` per call, and it is deliberately not owned by
`BexThread`: canary consumes/drops `BexThread` before its outer event-loop
wrapper emits the final `EndThread`.

The registry is a fixed-capacity slot array allocated when the profiler starts.
Execution admission/release may use its bounded free list; the spawn and
completion
paths do not hash, allocate, or take the registry free-list lock. They index
the stable slot directly, validate its generation/phase, and update the live
count atomically. Child acquisition uses a checked acquire/release update.
Each owner publishes its prior profiler-record commits with a release
decrement. The one-to-zero releaser performs the matching acquire fence before
reading `root_status`, changing phase, and making the final control slot ready;
the phase compare-and-swap is acquire/release. This is the standard Arc-style
last-owner synchronization, not a group of unrelated relaxed atomics. Slot
readiness is release-published and acquire-read by the consumer. Slot reuse
increments the generation only after the consumer's terminal hand-off has
entered `Released` and the old state has been cleared; durable terminal
state lives in the stream, not the slot (superseded by
TASK/profiling-backend-streams.md §5.6).

Admission takes no control-lane message and no store I/O (superseded by
TASK/profiling-backend-streams.md §5.5). `register_root` reserves the
registry slot, fixed health block, root lease, and lifetime terminal
control state; if this bounded runtime reservation fails, the root runs
profiler-off and `ProfilerExecutionStateUnavailable` increments. Store
availability is two atomic reads (`is_normal_admission_open`,
`is_indeterminate`); an unavailable or indeterminate store leaves the root
profiler-off with `InactiveReason::StoreUnavailable`. Durable admission is
the `RootStarted` meta record, published in a later batched cycle by the
stream writer. The program is never allowed to emit apparently profiled
calls with no execution owner.

The registration API returns an already-armed
`RootBoundaryCompletionGuard`, not a naked handle that the caller wraps later.
The acknowledgement transfers that guard itself; if the awaiting caller or
ack receiver is dropped, dropping the in-flight guard records the fallback
before releasing the root lease. There is no state in which registration has
succeeded and `active_threads = 1` but no component owns the root lease.

The provisional runtime reservation includes the execution's one terminal
control slot for its whole lifetime. After activation, the last lease release
fills that already-owned slot, marks it ready, and wakes the central consumer;
it does not allocate, block, or compete for general queue capacity from a
destructor. The consumer releases the slot only after the terminal
hand-off. Therefore ordinary runtime pressure can reject the root at
admission, but cannot strand a valid execution at its final release.

This keeps the module interface small: registration returns one fully owned,
already-armed guard when the runtime reservation succeeds, or it releases
the provisional state and the root runs profiler-off; durable admission
metadata follows in the next publication cycle (superseded by
TASK/profiling-backend-streams.md §5.5).

`ThreadRef` and `CallRef` always include `ProcessEuid` and `EngineId` in
addition to logical thread/call IDs. They are safe to persist across processes;
the shorter engine-local pair is never used as a durable key.

Admission attaches the host runtime token and nonoptional program ID to the
root logical thread via the registry slot and the `RootStarted` meta record;
there is no `StartBoundary` fact (superseded by
TASK/profiling-backend-streams.md §5.5). `StartThread` keeps its parent
thread/call reference and gains an optional spawn source span. The consumer
propagates execution ownership and the parent context through that edge. A
child-start that drains before its parent remains in the bounded unresolved
table.

The host runtime token is also the root span's initial runtime ID. Once
this admission fact exists, the profiler does not need a duplicate
host-installed root
`SetFunctionId` record. `SetFunctionId`/its replacement is reserved for actual
language runtime-ID overrides and carries the per-call annotation ordinal.

The first call on a spawned thread uses:

~~~text
parent context = spawning call's ContextKey
call site      = spawn expression's source span
edge kind      = Spawn
~~~

Two spawn expressions in the same parent therefore remain distinct. This
requires plumbing the source span into `StartThread` (or an equivalent
self-contained spawn fact); canary does not carry it today.

The acknowledged root registration creates the first lease and initializes
`active_threads = 1`. Every spawn, including a grandchild spawn, acquires a
child lease by checked atomic increment **before** the child becomes runnable
or its `StartThread` is attempted. The parent owns the new lease until the
spawn task accepts it; a scheduling/setup failure releases it immediately.
Inside the task, canary's existing `SpawnProfCloser` owns the lease while the
child is queued or before its event loop starts. On an abnormal drop it emits
the synthetic `EndFunction`/`EndThread` first and releases the lease last. On
normal loop entry it transfers the lease into a separate outer
`ThreadProfileCompletionGuard`; that guard is held outside the consumed
`BexThread`, records/attempts all final value, `EndFunction`, and `EndThread`
work, completes any post-loop future settlement/unhandled bookkeeping that can
wake another VM, and then releases the lease at the end of the spawned task.
If the task future itself is dropped mid-loop, its execution future is
destroyed before the completion guard releases, so no producer can write after
zero. Field drop order must not be relied on: both guards use an explicit
outer scope/transfer and take the lease only after inner producer futures are
destroyed.

Acquisition is valid only from an armed parent lease while the phase is
`Open` or `RootReturned`; `Closing` and `Released` reject it. Because the parent
still owns a lease until after its spawn operation finishes, the live count
cannot reach zero between a valid child acquire and publication of the child
task.

Child lease acquisition is a producer-local atomic operation. It is not an
acknowledged consumer control message and it does not create another
admission fact, runtime token, stream, or capture-policy root. The
existing `StartThread` parent-thread/parent-call fact carries causal ownership
to the consumer. If that structural record is lost, the lease still prevents
early sealing while population health reports the missing thread attribution.
When the producer itself knows the `StartThread` push was rejected, it stops
further profiler emission for that child subtree but retains the lease until
the child completes. A record accepted and later found corrupt is reconciled
by the consumer's bounded unresolved/loss path.

Immediately after acknowledged root admission, an outer
`RootBoundaryCompletionGuard` owns the root lease. It is outside the fallible
setup and event-loop future and is destroyed only after those inner futures
and their final-record guards have stopped producing. Normal completion maps
the host-visible outcome to `Succeeded`, `Failed`, `Cancelled`, or `Panicked`,
stores that immutable status, changes `Open` to `RootReturned`, and releases
the root lease before returning the BAML result. Exit code zero maps to
`Succeeded`; nonzero exit and ordinary engine failures map to `Failed` while
their detailed error class remains separate evidence/health.

All fallible setup after registration is routed through the guard rather than
using an unclassified `?`: a returned setup/engine error installs `Failed`.
If the host drops/aborts the call future, the still-armed outer guard installs
`Abandoned` and increments fixed `RootAbandoned` health. If Rust is unwinding a
panic, it installs `Panicked` instead. In every fallback it transitions to
`RootReturned` and releases only after the inner execution future can no longer
emit. `Abandoned` is intentionally distinct from `Cancelled`: the profiler
does not pretend that a cancellation token fired. Process abort or crash may
prevent this drop path; reopen then reports the already-defined
unterminated/incomplete artifact. Every guard completion method is idempotent,
and the `OnceLock` forbids a fallback from replacing a classified outcome.

A child that still holds a lease may continue and may spawn grandchildren
after root return. `Closing` is reachable only when `root_status` exists and
the last outer completion guard decrements `active_threads` to zero. At that
point no producer remains that can create another child or profiling record,
so exactly one successful `RootReturned -> Closing` compare-and-swap marks
the reserved slot finish-ready and wakes the consumer. If the root is
itself the last thread, its release takes this same path; there is no
special synchronous-root finalizer.

There is no per-execution Tokio waiter. The final lease release notifies the
central profiler consumer, which already owns draining and publication.
Counter saturation fails the spawn's profiling acquisition before the child
is runnable: the child may still execute, but its profiler mode is `Off`, the
process-global fixed `ProfilerThreadLeaseUnavailable` counter increments, and
that child profiler and its spawned subtree cannot emit structural or evidence
records into the execution. A stale generation is an invariant violation
handled the same way and asserted in tests. Neither case permits an untracked
child to emit profiling records or permits its nonexistent lease to delay
sealing. Language execution, cancellation, runtime IDs, and `$id` are
unaffected by this profiler degradation.

**Decision:** `detach = true` changes cancellation propagation only. It does
not reparent profiling ownership. Detached and ordinary descendants
remain in the original execution and use the same CCT, capture resolver,
evidence writer, and execution health. The detached child keeps its normal
spawned-call runtime identity; profiling never installs a new host runtime
token as its `$id`.

Consequently, root completion and execution finalization are different
events. A descendant error cannot retroactively change the root result
already returned to the host. `RootEnded` records the immutable
`root_status`; descendant
completed/error/cancelled/exit counts remain visible in CCT and evidence
health and may be presented separately as “descendant errors.” A detached task
that runs forever keeps the profile honestly active rather than creating a
false terminal record.

This choice preserves the aggregation law: ten thousand equivalent workers
from one spawn site contribute to one spawned CCT path in one execution.
They do not create ten thousand execution roots, root spans, automatic
root-value captures, cross-execution links, `RootStarted` records, or final
barriers. The
incremental profiler work per spawn is one checked atomic acquire, one small
handle carried by the child, and one atomic release; ordinary call records are
unchanged. The execution-local counter can contend under extreme concurrent
fan-out, but it is touched per spawn/completion rather than per function call
and replaces much heavier task, artifact, and writer creation. Phase 0 measures
this shape before Phase 2; sharded lease counts are not added unless that
benchmark fails and this contract is amended.

Long-lived ownership must not imply long-lived large buffers. CCT and evidence
continue to roll into immutable segments. The process-wide writer may keep
only governor-charged batches and O(1) publication file handles; it must not
reserve a full derived segment target or a permanently open file for every
active execution. Target/maintenance pressure may seal a partial segment
while the execution remains active. Only the fixed execution entry, live
thread/call state, and currently queued bounded work stay resident.

### 5.2 Identity join

The decoder maintains bounded unresolved facts keyed by `CallRef` and thread
identity. It accepts:

- call end before call start;
- runtime-ID annotation before or after call start;
- captured value occurrence before or after span metadata; and
- spawned-thread start before its parent call has been resolved.

A normal call resolves when the consumer has the call-start facts and enough
parent/thread information to derive its context. Call end adds status and
inclusive time whenever it arrives. Parent gaps remaining at final drain are
health loss, not guessed relationships.

The unresolved-join table is a transient resource governed by memory and entry
limits: every parked fact carries an `UnresolvedJoins` reservation, and the
table as a whole (starts, ends, thread starts, thread ends) is capped at a
fixed entry count, beyond which further reorder facts are charged as
`JoinCapacityExceeded` instead of parked. Resolving parked facts is driven by
an explicit worklist, never by recursion on the consumer's stack, so a burst of
parked descendants is bounded by the table, not by stack depth. Expiry during a
live run is based on an explicit age/pressure rule, not arrival order. Final
boundary drain is the authoritative point at which remaining entries become
`UnmatchedCallFact` or `UnmatchedThreadFact`.

### 5.3 Execution final drain

One logical BEX thread may emit records through several OS-thread rings.
Terminal sealing follows this barrier:

1. the root stores immutable `root_status`, transitions to `RootReturned`,
   releases its lease, and returns its result to the host;
2. descendants continue normally; each descendant and grandchild owns one
   lease and releases it through its existing completion guard;
3. the release that changes `active_threads` from one to zero performs the
   sole `RootReturned -> Closing` compare-and-swap—at this point no producer
   remains that could spawn another child;
4. that release sets the reserved slot's `finish_ready` flag and wakes the
   central consumer, without waiting on the profiler;
5. the central consumer snapshots the committed tail of every registered ring
   after execution producers are quiescent;
6. it drains and decodes each ring through that captured tail;
7. it waits for the decoder/evidence writer to acknowledge that barrier token;
8. it converts remaining execution-owned unresolved facts to terminal health;
9. it hands the sealed epoch and remaining evidence to the stream writer as
   one group, enqueues `RootEnded`, and releases the generation-tagged slot
   to `Released` immediately; publication is batched by the writer's cycle
   (superseded by TASK/profiling-backend-streams.md §5.6).

No step creates a per-execution waiter, polls descendants, or blocks delivery
of the root result. Root return is an application event; the final lease
release is the profiler's quiescence signal.

Other executions may continue writing beyond the captured tails. A ring that
registers after the snapshot cannot contain this already-quiescent
execution's records. The finish-ready signal is never a best-effort ring
push: it is state on the execution's reserved slot plus a consumer wake, so
it cannot fail to be delivered. If the process crashes before the batched
`RootEnded` commits, the reader reports the execution as
unterminated/incomplete (superseded by TASK/profiling-backend-streams.md
§5.6–5.7).

Disk degradation is different from barrier uncertainty. The terminal
hand-off still completes and the slot is still released. If
`DiskGuardExceeded` or an actual I/O/device/free-space failure rejects the
terminal publication before rename, the loss is counted (process-global
`root_ended_lost`) and the missing `RootEnded` is the durable incomplete
marker (superseded by TASK/profiling-backend-streams.md §5.7). The
execution's fixed live health is queryable until release, but cannot be
promised durably when the store cannot write. A post-rename terminal failure
remains `RenamedAwaitingDirSync` and blocks further publication until the
single indeterminate path resolves, as Section 7.2 requires.

### 5.4 Inclusive, self, and await time without global ordering

The decoded call clock supports:

~~~text
inclusive_ns = max(0, end_ticks - start_ticks)
~~~

The producer adds one end-record variant:

~~~rust
enum RawCallEnd {
    EndFunction {
        status: FunctionEndStatus,
        thread_id: BexThreadId,
        call_id: BexCallId,
        end_ticks: u64,
    },
    EndFunctionAwaited {
        status: FunctionEndStatus,
        thread_id: BexThreadId,
        call_id: BexCallId,
        end_ticks: u64,
        await_ns: u64,
        await_count: u32,
    },
}
~~~

`EndFunctionAwaited` is an alternative encoding of the same one call-end fact,
not a second record. Its payload is the compact end plus twelve timing bytes
before codec framing. Calls whose total and count are both zero use the
existing `EndFunction` byte shape. The codec is versioned and golden-tested;
Phase 0 records the measured encoded sizes rather than copying stale sizes
from the reference proposal.

While profiling is on, each logical VM has a sparse, governor-accounted vector
of `{ call_id, await_ns, await_count }` for open calls that have suspended. It
is call-stack ordered and searched from the end, so the usual current-call
update is one comparison and no hash; capacity is reused after entries pop. It
allocates an entry only when that call first suspends and removes the entry at
call end. Profiling off constructs no vector and performs no timing work.
Repeated suspension of the same call updates its existing entry with
saturating arithmetic. A sysop charge uses `pending_sysop_call_id`; other
charges use the bytecode frame's active call ID. The call ID is captured before
suspension rather than rediscovered after a possible unwind or migration.

One engine helper wraps every **declared VM suspension**:

1. capture the active call ID and start tick immediately before releasing or
   parking the VM;
2. perform the existing wait and permit reacquisition;
3. capture the end tick before the VM executes another instruction or emits a
   later call/end fact; and
4. add the nonnegative elapsed duration and one interval to that call's sparse
   accumulator.

The initial MVP inventory is closed and test-enforced:

- `SysOpResult::Async`, including model/network/host work;
- `VmExecState::Await`;
- `VmExecState::AwaitAny`;
- a child waiting for its task-group slot/entry heap permit after its entry
  call has been opened; and
- `VmExecState::EarlyYield`/GC-permit suspension.

Ready-inline sysops and futures resolved inline in bytecode emit no await
charge. Ordinary internal Rust awaits that are not one of these semantic VM
suspensions remain runtime-self overhead. Any new engine/VM park seam must be
classified into the helper or explicitly documented as runtime-self in the
same change; it may not silently choose its timing meaning.

Await time ends after the logical thread has reacquired the VM permit and is
ready to execute again. It therefore includes executor scheduling delay,
cancellation wake-up, GC work performed while deliberately parked, and permit
reacquisition. This is honest VM-not-executing wall time, not provider-only
latency. Provider attempt timing, if later added, is a separate enrichment.

Cancellation and ordinary error resumption charge the completed interval
before unwind/terminal records. A queued task-group cancellation that
reacquires the child state likewise charges before its synthetic end. An
abnormal task-future drop while parked, or a process crash, may have no honest
resume tick or call end; the existing abandoned/unmatched structural health
then makes timing incomplete. The profiler does not fabricate a terminal
duration merely to close the arithmetic.

The consumer never needs suspension order. The awaited total arrives on the
call's end fact. For each resolved end it:

1. adds inclusive duration and await duration to the call's context;
2. if the call's edge is synchronous `Call`, also adds its inclusive duration
   to its parent **context's** `direct_call_child_inclusive_ns`; and
3. derives the reader-facing aggregate with:

~~~text
self_ns = inclusive_ns - direct_call_child_inclusive_ns - await_ns
~~~

Spawned-child duration is not subtracted. Aggregating the three components
before subtraction makes the formula merge-safe across CCT segments and epoch
rollover. The stored CCT schema retains the components; `self_ns` is a derived
reader column, not a separately accumulated unsigned counter.

For a terminal boundary, timing is `Complete` only when all applicable call
starts/ends and awaited-end variants decoded and joined, the await accumulator
reported no loss/saturation, and subtraction did not underflow. Otherwise the
reader may show observed inclusive/await components but labels self/await
`Incomplete`; it must not present the derived self value as exact. If
`direct_call_child_inclusive_ns + await_ns > inclusive_ns`, self clamps to zero
and health records `SelfTimeUnderflow`. A backwards/non-comparable await clock
adds neither duration nor folded count and records `AwaitClockInvalid` as one
interval loss.

Each logical thread counts declared suspensions locally and flushes
`await_intervals_started` into the boundary's fixed health at thread
completion, avoiding a shared atomic on every wait. Awaited end records carry
the successfully accumulated count. At final drain, any remaining difference
not already assigned to a producer/transport/join reason becomes
`AwaitIntervalUnreconciled`; it is never silently absorbed into self time.

No `event_seq`, per-thread consumer reorder queue, `SuspendThread`,
`ResumeThread`, or persisted per-await row is part of this contract. Live open
spans may show elapsed inclusive time, but durable await/self totals become
authoritative only when their call end is folded.

## 6. CCT model

### 6.1 Stable context identity

A normal context is identified by:

~~~text
ContextTuple {
  program_id,
  parent_context_key,
  function_id,
  call_site,
  edge_kind, // Root | Call | Spawn
}

ContextKey =
  SHA-256("baml-cct-context-v1" || canonical(ContextTuple))
~~~

The MVP uses canary's nonoptional `ProgramId` here. `FunctionId` is
engine-local, and `ProgramId` scopes it so segments from different program
instances cannot collide. The optional source/revision strings remain labels;
they are not allowed to disappear from the hash input because they were never
present.

`ProgramId` is currently random per engine construction. That is sufficient
to merge CCT epochs and segments for one running program instance, which is
the local MVP requirement. It deliberately does not claim that independently
constructed engines running identical source have the same context key. A
future cross-run/cloud merge needs a nonoptional content-derived program
revision ID and a new `ContextKey` version.

The canonical encoding is versioned, fixed-endian, and covered by golden
cross-platform fixtures. A segment stores both the key and tuple. If the same
key is ever observed with a different tuple, the reader reports
`ContextKeyCollision`/corruption and does not merge them.

The random runtime ID and `CallRef` are not in the tuple. They identify exact
invocations and would destroy aggregation if included in CCT identity.

### 6.2 Hot in-memory representation

The active epoch uses dense local node IDs:

~~~rust
struct ActiveContext {
    local_id: u32,
    context_key: ContextKey,
    parent: ParentContextRef,
    function_id: FunctionId,
    call_site: Option<CallSite>,
    edge_kind: EdgeKind,
    delta: CctCounters,
}

enum ParentContextRef {
    Root,
    Local(u32),
    External(ContextKey),
}

struct CctCounters {
    invocations_started: u64,
    spans_selected: u64,
    completed_ok: u64,
    completed_error: u64,
    completed_cancelled: u64,
    completed_exit: u64,
    inclusive_ns: u128,
    direct_call_child_inclusive_ns: u128,
    await_ns: u128,
    await_count: u64,
}
~~~

The persisted counters retain all three additive timing components. Readers
derive `self_ns` after merging segments; writers never encode subtraction as a
negative delta. `direct_call_child_inclusive_ns` includes only children whose
edge kind is `Call`, never `Spawn`.

For every normal context or overflow aggregate:

~~~text
cct_only_invocations = invocations_started - spans_selected

spans_selected
  = span_starts_committed_for_context
  + sum(span_start_loss_for_context_by_reason)
~~~

Only decoded starts enter these per-context equations. Separately, the
producer's fixed boundary health increments `invocations_attempted` before
each structural start push:

~~~text
invocations_attempted
  = sum(normal_context.invocations_started)
  + sum(overflow.invocations_started)
  + structural_starts_lost_before_decode
~~~

This distinguishes “CCT-only by policy” from “selected but not committed” and
from “the structural start never reached a context.”

Repeated hot calls look up a compact tuple containing the parent reference,
function, call site, and edge. Parents created in the current epoch use a dense
`Local` ID. An active parent that began before epoch rollover uses its retained
`External(ContextKey)` until it is lazily interned as a local parent stub.
`ContextKey` hashing happens when a context is first created in an epoch, not
on every invocation.

Counter arithmetic saturates. General saturation records `CounterSaturated`;
await duration/count saturation records `AwaitCounterSaturated`. Neither
wraps.

### 6.3 Epoch rollover

Only the active CCT epoch and currently active/unresolved calls remain in
memory. When its accounted memory reaches the target:

1. request an immediate consumer drain;
2. encode the current additive CCT deltas;
3. atomically publish a CCT segment;
4. release the epoch map; and
5. begin a new epoch.

Readers merge segment deltas by `ContextKey`. Calls that span rollover retain
their `CallRef`, `ContextKey`, and start timestamp in the active-call table.
Their start count may be in one segment and their completion/duration delta in
a later segment. The retained context key also supplies
`ParentContextRef::External` for children entered after rollover, so an active
parent never points at a local node ID from a freed epoch.

There is no lifetime context limit. Active-call and unresolved-join memory is
still transient and bounded because concurrency itself can be unbounded.

The epoch target is a rollover trigger, not an admission ceiling. Crossing it
does not redirect later calls to overflow, change selection, or discard an
older segment. Under sustainable consumer and disk throughput, every decoded
call before and after rollover is folded normally. Overflow or loss is valid
only for the separately named allocation, transport, corruption, or
publication failures in Section 8—not because a boundary has already produced
some number of contexts, invocations, bytes, or segments.

The runtime retains no catalogue that grows once per published epoch. It keeps
only the active epoch, active/unresolved joins, bounded writer batches, and the
next boundary-local segment number. Full readers may allocate in proportion to
the distinct `ContextKey` result they ask to merge, but neither writers nor
readers require memory proportional to invocation count or retain all segment
contents at once.

### 6.4 Emergency overflow

The consumer preallocates a fixed matrix:

~~~text
OverflowAggregate[OverflowReason][EdgeKind]
~~~

MVP reasons are:

- `ContextMemoryUnavailableAfterDrain`;
- `InvalidParentContext`.

These aggregates contain only counts/status and additive
inclusive/direct-child/await timing that can be attributed honestly. Derived
self time is complete only under Section 5.4's timing-health rule. A selected
exact span may reference the matching overflow aggregate and carry its observed
function/call-site labels.

`CctSegmentPublishFailed` and `DiskGuardExceeded` are not overflow reasons.
The same unavailable/full store cannot make a new overflow aggregate durable.
Those failures stop further evidence publication for affected contexts and
remain in preallocated live boundary health until terminal release. The failed
population batch itself is final loss and is never retried or published later.
If terminal store publication is definitely rejected, `run.end` remains absent
and this session never later seals that released run. A reader never interprets
an overflow counter as proof that a failed segment was persisted.

### 6.5 Durable context references

Evidence never stores an epoch-local node ID. Its durable join is the
canonically encoded tagged union:

~~~rust
enum ContextRef {
    Normal(ContextKey),
    Overflow {
        reason: OverflowReason,
        edge_kind: EdgeKind,
    },
}
~~~

There is no `BoundaryRef`: data-plane groups are already keyed by the
execution's root `ThreadRef`, so the overflow variant carries only the
reason and edge kind (superseded by TASK/profiling-backend-streams.md
§4.5). The normal and overflow variants have distinct fixed tags in the
versioned codec. A normal reference resolves to a
`ContextDefinition { key, tuple }` in this group's CCT section or an
earlier committed group of the same execution. An overflow reference
resolves to the one `OverflowDefinition`/aggregate for that execution,
reason, and edge kind.

Definition durability uses no lifetime `ContextKey` set and never scans old
segments. Every context interned into the current epoch carries
`PendingDefinition { compact_tuple }` or `DurableDefinition`. Its first CCT
batch includes the normal/overflow definition; a successful commit changes
the current-epoch state to durable. An epoch is released on the success path
only after those definitions commit. An active call that crosses rollover
retains its own compact tuple and definition state with its external key until
that call closes; this memory is charged to the already-bounded active-call
table, not lifetime history.

A context recreated in a later epoch republishes its small definition even if
an older segment already contains the same tuple. Readers accept identical
definitions and treat a different tuple for the same key as
`ContextKeyCollision`. This deliberate bounded duplication avoids both an
unbounded published-key catalogue and historical scans.

Before an evidence segment is renamed into place, every referenced
`ContextRef` must have a `DurableDefinition` token. Evidence for a pending
definition waits in the already-bounded batch or is co-batched behind the CCT
definition commit. If that CCT batch is terminally lost, the token becomes
`LostDefinition`; dependent evidence becomes `MissingStructuralJoin` or
`StartUncommitted` and cannot publish. After store recovery, a still-active
lost context may republish its retained tuple only when its parent is root or
already `DurableDefinition`; a pending active parent publishes first. If the
parent token/tuple is lost or has already been released, the child cannot
recreate a normal chain: subsequent attribution uses the
`InvalidParentContext` overflow definition, and affected exact evidence is
explicitly incomplete. The implementation never retains ended ancestor tuples
to rescue descendants and never publishes a normal definition whose parent is
missing. No path guesses durability from file existence.

## 7. Exact evidence and CAS

### 7.1 Append-only evidence facts

Selected calls emit facts rather than one mutable span object:

~~~rust
struct SpanStart {
    call_ref: CallRef,
    parent_call_ref: Option<CallRef>,
    thread_ref: ThreadRef,
    context_ref: ContextRef,
    function_id: FunctionId,
    call_site: Option<CallSite>,
    edge_kind: EdgeKind,
    started_ns: u64,
    selection_reasons: SelectionReasons,
    roles: RoleMask,
    runtime_id: Option<RuntimeIdAnnotation>,
}

struct RuntimeIdAnnotation {
    annotation_ordinal: u32,
    runtime_id: BoundaryId,
}

struct SpanEnd {
    call_ref: CallRef,
    ended_ns: u64,
    status: EndStatus,
    inclusive_ns: u64,
}

struct SpanRuntimeId {
    call_ref: CallRef,
    annotation_ordinal: u32,
    runtime_id: BoundaryId,
}
~~~

`SpanRuntimeId` is needed only when the annotation arrives after the start
fact was queued. Readers fold the highest annotation ordinal into the exact
span. The VM mints this small per-call ordinal only for runtime-ID setter
events; it is not a sequence field on ordinary call records. The initial
`LocalId` annotation uses ordinal zero. Repeated `baml.id.set` calls increment
the ordinal, preserving last-wins semantics even when a call resumes on a
different OS-thread ring.

`SpanStart` and `ErrorCapture` carry no `boundary_id`: the data-plane group
is keyed by the execution's root `ThreadRef`, which identifies the
execution (superseded by TASK/profiling-backend-streams.md §4.5). Two new
evidence facts, `ThreadStart` (tag 6) and `ThreadEnd` (tag 7), make thread
lifecycle durable:

~~~rust
struct ThreadStart {
    thread_ref: ThreadRef,
    parent: Option<ThreadRef>,
    spawn_call: Option<CallRef>,
    spawn_site: Option<CallSite>,
    started_ns: u64,
    kind: ThreadKind, // Root | Spawn
    name: String,     // <= 256 bytes
}

struct ThreadEnd {
    thread_ref: ThreadRef,
    ended_ns: u64,
    status: ThreadEndStatus, // Completed | Cancelled | Errored
}
~~~

Both are ordinary evidence facts under the evidence reservation policy; a
`ThreadEnd` is emitted only when its `ThreadStart` was, and reader handling
of duplicates and missing starts is tolerant (streams spec §4.5).

Long-running spans may have their start, related error captures/terminal
links, and end in different evidence segments. `CallRef` and
`ErrorCaptureId` join them. Evidence segment rollover is a file boundary, not
a lifetime retention limit.

There is likewise no lifetime selected-span or evidence-byte admission
counter. After a segment commits, its batch memory is returned to the process
governor and later root, LLM, and manual selections use the same policy and
transient reservations as earlier selections. A queue rejection affects only
the facts whose concrete enqueue/reservation failed; clearing pressure allows
subsequent facts immediately. The implementation must not latch an execution
into CCT-only mode merely because it is old or has rolled many segments.

### 7.2 Sealed-batch ownership

Each CCT and evidence batch has one owner and this state machine:

~~~text
Open -> Sealed -> Publishing --before rename failure--> Lost(reason)
                           \--rename--> RenamedAwaitingDirSync -> Committed
~~~

The batch owner is the per-session stream writer, which publishes with
per-plane, per-stream sequences (TASK/profiling-backend-streams.md §5.2).
Open/sealed/publishing batches are charged to the process governor and the
bounded writer pipeline; there is no per-execution retry list. Encoding and
rename are attempted once. A failure known to occur before the final rename
may transition to `Lost`: it first folds the exact affected record/count
dimensions into the preallocated live health block, invalidates all dependency
tokens that needed that batch, releases its memory, and can never publish.
`Committed` releases memory only after final-path rename and containing-
directory fsync, then advances the plane high-water. A later health segment
may durably describe a final loss, but the original data batch is never
retried, preventing duplicate deltas.

Failure after rename is not classifiable as loss because the final path may
already be visible or durable. The process-global publisher retains exactly
one `RenamedAwaitingDirSync { path, batch, candidate_sequence }`, keeps
`publish.lock`, and blocks all later publication. A bounded recovery probe may
retry only the containing-directory fsync for that same path. Success commits
that same batch/sequence exactly once; persistent failure remains
indeterminate and bounded rather than reusing the sequence or publishing
around it. The MVP does not automatically unlink an indeterminate final path.
The identical commit point applies to meta segments, data segments, and CAS
objects. A post-rename ambiguity blocks admission of new roots and is owned
by the same single global slot until resolved.

An unambiguous pre-rename I/O or disk-guard publication failure changes the
shared store writer from `Writable` to `Blocked(reason)`. While blocked,
producers/consumer reject new durable population and evidence work under the
corresponding closed health reason before retaining values or sealed batches;
no failed batch backlog accumulates. When `reason = DiskGuardExceeded`, every
bounded ordinary batch
already queued but not renamed transitions once to `Lost(DiskGuardExceeded)`,
releases its reservations/VM roots, and contributes its exact health
dimensions. Producers observe one shared atomic rejection state, so later
manual captures do not copy or hash values that cannot be stored.

`DiskGuardExceeded` stays latched for this store session. There is no periodic
free-space probe on VM calls, automatic deletion, or retry of lost work. The
store may become `Writable` only through an explicit close/reopen after
`baml clean`, a storage-root/budget change, or external space recovery. New
roots run profiler-off while latched. Already-admitted executions finish
their final drain and make one terminal publication attempt; a definite
rejection counts `root_ended_lost` and leaves the execution without a
`RootEnded`; they do not wait for
space. A terminal barrier waits only for bounded batches already in
`Sealed`/`Publishing`/`RenamedAwaitingDirSync`; it never waits on a lifetime
retry collection.

### 7.3 Evidence completeness

Completeness covers every evidence part, not just span admission. Each active
execution owns a fixed-size, preallocated `ExecutionHealthBlock`. Producers and
the consumer update atomic counter arrays directly; reporting loss never
depends on allocating another queue entry in the queue that just failed.

The fixed dimensions are:

- `invocations_attempted` and pre-decode structural loss;
- selection-reason bitset;
- evidence part (`SpanStart`, `SpanEnd`, runtime-ID annotation,
  input/output value attempt or result, error-capture attempt or result, or
  terminal-error link);
- role kind where applicable;
- error unwind kind/source where applicable;
- success/open/loss state; and
- the closed loss-reason enums in Section 8.

The structural push API returns success/failure. Before attempting a selected
`CallFunction` push, the producer increments `spans_selected` in the execution
health block. If the push fails, it increments
`StructuralStartTransportExceeded` and suppresses value copying for that call.
If the start arrives but cannot resolve a context, the consumer records
`MissingStructuralJoin`. The consumer similarly increments preallocated
counters when evidence enqueue or
publication fails; it does not try to describe queue-full by enqueueing into
the same full queue.

The writer does not publish `SpanEnd`, runtime-ID updates, or input/output
values until that call's `SpanStart` is durable. It publishes an
`ErrorCapture` only after its throwing `ContextRef` and at least one selected
span that made the unwind applicable are durable. It publishes a
`TerminalErrorTarget::Capture` only after both that terminal span's
`SpanStart` and the target `ErrorCapture` are durable. If the capture target
is unavailable but the terminal span start is durable, it publishes
`TerminalErrorTarget::Lost(reason)`. If the terminal span's own start is not
durable, it publishes no `TerminalErrorRef` and increments
`terminal_error_link_loss(StartUncommitted)`. No path creates a dangling
reference.

Live checkpoints expose:

- `spans_selected`;
- span starts committed, queued, and lost by reason;
- `open_selected_spans` whose starts exist but whose calls have not ended;
- span ends observed, committed, queued, and lost by reason;
- runtime-ID annotations observed, committed, queued, and lost by reason; and
- value attempts/results queued, available, and lost by reason;
- applicable error unwinds and error captures queued, committed, and lost by
  reason; and
- terminal error links observed, queued, committed, and lost by reason.

After `RootEnded` is committed:

~~~text
spans_selected
  = span_starts_committed
  + sum(span_start_loss_by_reason)

span_starts_committed
  = span_ends_committed
  + sum(span_end_loss_by_reason)

runtime_id_annotations_observed
  = runtime_id_annotations_committed
  + sum(runtime_id_annotation_loss_by_reason)

applicable_value_attempts
  = value_occurrences_available
  + sum(value_loss_by_reason)

applicable_error_unwinds
  = error_captures_committed
  + sum(error_capture_loss_by_reason)

terminal_error_links_observed
  = terminal_error_links_committed
  + sum(terminal_error_link_loss_by_reason)
~~~

`open_selected_spans` is normal at a live checkpoint and must be zero at a
successful terminal seal. A structurally lost call end is one of the
span-end-loss reasons; it never leaves a silently open durable span.

The execution health block is reserved before root execution. If even that
fixed block cannot be allocated, the root runs with profiling disabled
and increments a process-global preallocated
`ProfilerExecutionStateUnavailable` counter. Health snapshots use a reserved
control buffer, independent of the ordinary evidence queue. They are folded
into CCT health and the `RootEnded` record while the store is writable. If
the terminal publication is lost, no `RootEnded` is written and
process-global `root_ended_lost` increments; process-global fixed health
records the terminal failure while the process lives, and this session does
not retry or later seal the released execution. A crashed/full process
cannot promise a durable reason for bytes it never wrote, so readers use the
missing terminal marker.

### 7.4 Value and error occurrences

Inputs and outputs have at most one occurrence for each selected call:

~~~rust
enum ValueRole {
    Input,
    Output,
}

struct ValueOccurrence {
    call_ref: CallRef,
    context_ref: ContextRef,
    role: ValueRole,
    state: ValueState,
}

enum ValueState {
    Available {
        cid: ValueCid,
        codec: CodecVersion,
        encoded_bytes: u64,
    },
    Lost(ValueLossReason),
}
~~~

Errors are not a third `(call, role)` value slot. They are captured once per
qualifying VM unwind and then linked to every selected span that unwind
terminates:

~~~rust
struct ErrorCaptureId {
    thread_ref: ThreadRef,
    unwind_ordinal: u64,
}

enum ErrorUnwindKind {
    Fresh,
    Rethrow,
}

enum ErrorSource {
    Bytecode,
    NativeCall,
    EngineCall,
    FutureResume,
}

struct ThrowSite {
    file_id: u32,
    line: u32,
    start_offset: u32,
    end_offset: u32,
}

struct ErrorCapture {
    id: ErrorCaptureId,
    throw_call_ref: CallRef,
    throw_context_ref: ContextRef,
    throw_function_id: FunctionId,
    throw_site: Option<ThrowSite>,
    kind: ErrorUnwindKind,
    source: ErrorSource,
    value: ValueState,
}

struct TerminalErrorRef {
    call_ref: CallRef,
    target: TerminalErrorTarget,
}

enum TerminalErrorTarget {
    Capture(ErrorCaptureId),
    Lost(ErrorCaptureLossReason),
}

enum ErrorCaptureLossReason {
    ErrorCaptureAttemptTransportExceeded,
    MissingStructuralJoin,
    StartUncommitted,
    EvidenceQueueFull,
    EvidenceSegmentPublishFailed,
    DiskGuardExceeded,
    StoreUnavailable,
}
~~~

`ErrorCaptureId` identifies one profiler-retained execution of the VM unwind
funnel. `unwind_ordinal` increments only on that cold path and is scoped by the
durable logical `ThreadRef`; no counter is added to normal calls or
instructions. The ID is not exposed to BAML and is not stored inside the
thrown value.

For profiler evidence, `Fresh` means this path created the thrown value for a
new fault, while `Rethrow` means an existing thrown value is starting another
unwind. The latter includes an explicit BAML rethrow and propagation from a
settled future. This durable classification is deliberately separate from the
VM's language-semantic rethrow flag below.

`ThrowSite` is the VM's current durable source shape: file ID, line, and exact
byte offsets for the instruction that starts this unwind. It intentionally
does not claim a column; canary's line-table conversion currently hardcodes
column zero. It is required for bytecode-originated unwinds. It is `None` only
when a native or host path genuinely has no BAML source location; readers
display that absence rather than substituting the callee's call site.

An unwind is applicable when either:

- the throw/rethrow occurs in a selected call whose resolved error role is
  enabled; or
- the unwind will terminate at least one selected frame whose resolved error
  role is enabled.

Every path into the shared unwind funnel supplies a transient origin
descriptor:

~~~rust
struct VmThrown {
    value: Value,
    profiler_kind: ErrorUnwindKind,
    language_is_rethrow: bool,
    origin: VmUnwindOrigin,
}

struct VmUnwindOrigin {
    throw_call_ref: CallRef,
    throw_function_id: FunctionId,
    throw_site: Option<ThrowSite>,
    source: ErrorSource,
    selected_error_reasons: Option<SelectionReasons>,
    origin_span_already_terminated: bool,
}
~~~

`VmThrown` is the transient carrier from the point a thrown value is created
or resumed until the shared unwind funnel consumes it. `VmError::Thrown` and
`try_handle_external_exception` are changed to carry `VmThrown`, not a bare
`Value`. Each producer constructs both booleans and the origin before that
information can be lost:

| Producer path | `profiler_kind` | `language_is_rethrow` | `source` | Origin state |
|---|---|---:|---|---|
| bytecode `Throw` or a new VM-generated runtime error | `Fresh` | `false` | `Bytecode` | current bytecode call; still open |
| bytecode `Rethrow`, `ThrowIfPanic`, compiler no-match/defer transparent re-raise | `Rethrow` | `true` | `Bytecode` | current bytecode call; still open |
| direct `NativeCallResult::Error` creating a panic/BAML error | `Fresh` | `false` | `NativeCall` | native call; already terminated when its call pair closed |
| direct `VmRustFnError::Thrown { profiler_kind, .. }` | carried | `false` | `NativeCall` | native call; already terminated when its call pair closed |
| injected host throw or `OpErrorPayload::Vm(Panic/BamlError)` | `Fresh` | `false` | `EngineCall` | engine/sysop call; already terminated |
| injected `OpErrorPayload::Vm(VmRustFnError::Thrown { profiler_kind, .. })` | carried | `false` | `EngineCall` | engine/sysop call; already terminated |
| `FutureRead::Error(existing_value)` resumed into an awaiting VM | `Rethrow` | `false` | `FutureResume` | awaiting bytecode call; still open |
| `FutureRead::Cancelled`, which materializes a new cancellation at the await site | `Fresh` | `false` | `FutureResume` | awaiting bytecode call; still open |

The value-only `VmRustFnError::Thrown(Value)` variant becomes
`VmRustFnError::Thrown { value, profiler_kind }`. Existing native sites that
allocate a new thrown value set `Fresh`; a native that transparently forwards
an existing thrown value sets `Rethrow`. The conversion site assigns
`NativeCall` or `EngineCall` depending on whether the error came directly from
a VM native or through `OpErrorPayload`; the variant itself does not guess its
source.

`language_is_rethrow` preserves canary's current BEP-042 behavior exactly.
Only the bytecode paths that currently call `try_unwind_exception(..., true)`
set it. `try_unwind_exception` uses this flag, never `profiler_kind`, for
`recorded_throw_cause` versus `find_cause_context`. Future/native/engine
propagation may therefore be durable profiler `Rethrow` while remaining
`false` for language cause construction. Changing that language behavior is a
separate, explicitly approved project.

`selected_error_reasons` is present only when the throwing call had an
accepted selected start and its error role is enabled. Bytecode `throw` and
runtime-panic paths set `origin_span_already_terminated = false`: the handler
search decides whether their current frame survives. An inline native,
engine/sysop, or other call already closed as errored before injection sets it
to `true`; if it was selected, that origin call receives a terminal link even
when an outer bytecode frame catches the value.

The handler search already walks the frames the unwind may close. The profiler
reuses that cold walk and keeps a three-state local variable for the current
unwind: `NotApplicable`, `Capturing(ErrorCaptureId)`, or
`Lost(ErrorCaptureLossReason)`. When the throwing call qualifies or immediately
before the first qualifying selected frame is closed, the producer performs
the one admission operation below; it becomes `Capturing` or `Lost` exactly
once. Every later qualifying frame receives a terminal link from that same
state. This includes the remaining entry frame that is closed as unhandled
without being physically popped. A frame in which a handler is found remains
open and therefore receives no terminal link.

The admission operation reserves and submits at most one bounded producer
draft:

~~~rust
struct ErrorCaptureAttempt {
    id: ErrorCaptureId,
    boundary_id: BoundaryId,
    throw_call_ref: CallRef,
    throw_function_id: FunctionId,
    first_selected_call_ref: CallRef,
    throw_site: Option<ThrowSite>,
    kind: ErrorUnwindKind,
    source: ErrorSource,
    value: ErrorAttemptValue,
}

enum ErrorAttemptValue {
    Rooted(RootedVmValue),
    Lost(ValueLossReason),
}
~~~

The producer first reserves the compact attempt metadata from the bounded
evidence lane. If that fails, the unwind state becomes
`Lost(ErrorCaptureAttemptTransportExceeded)` and no capture ID is referenced.
After metadata admission, it mints the ID and reserves the Values owner's
minimum accounted charge, including the small profiler-owned GC-root handle.
Reservation failure still submits the metadata-only attempt with
`ErrorAttemptValue::Lost(ValueMemoryExceeded)` and retains no VM value. Other
failures before a usable root similarly carry their exact `ValueLossReason`.

`RootedVmValue` is producer-only, not a durable type. Copied bytes are charged
by growing the same Values reservation before allocation. The reservation
covers the root, snapshot, encoder/hash/CAS buffers, and pending evidence for
their complete overlap. The root stays alive only until the engine has copied
the value or recorded loss, and is then released. The attempt tries General
capacity first and may use the reusable Manual reserve when the qualifying
selected frame carries the `MANUAL` reason.

`first_selected_call_ref` is the selected throwing call or the first selected
frame about to be terminated, whichever made the unwind applicable. A frame
whose selected structural start was rejected cannot fill this field and does
not trigger value copying; a later selected frame with an accepted start may
still do so. The structural consumer resolves `throw_call_ref` to the durable
`throw_context_ref`; the VM does not guess a CCT reference.
`throw_function_id` is copied through the error-path draft so an overflow
context still identifies the observed throwing function. For `Rooted`, the
value pipeline deep-copies and encodes the value once and produces
`Available` or a downstream `Lost` reason. For `Lost`, it skips copying and
preserves the producer's reason. Both paths emit one `ErrorCapture`. All
`TerminalErrorTarget::Capture` links for the unwind point to that record and
therefore to the same CID when a CID exists.

The implementation seam is the shared VM unwind funnel, currently
`try_unwind_exception`, plus the native/engine/future paths that feed a thrown
value into it. `try_handle_external_exception`, `VmError::Thrown`, and the
future read/resume bridge are extended to carry or construct `VmThrown`
instead of discarding kind/source/origin information. Those entry paths must
not emit an independent error value first. The old
profiler-specific `maybe_queue_call_error_origin`,
`queue_engine_call_error_origin_capture`, and
`VmCallCaptureKind::Error` paths are replaced. Root error capture also resolves
through this `ErrorCapture`; it must not call the old root value-capture path
for the same thrown value.

The existing `seen_throw_values` suppression must not gate profiling, because
it would erase rethrows. Any equivalent bookkeeping still needed for
language-level cause chains remains separate from profiler capture.
`ErrorCaptureAttempt.kind` is copied from `VmThrown.profiler_kind`; it never
uses `language_is_rethrow` as a proxy.

The compact dispatch loop constructs `ThrowSite` while its live opcode PC is
available and before an unwind mutates frame state. It does not try to recover
the site later from `self.cur_pc`. Legacy dispatch and external injection do
the equivalent at their own entry sites.

This gives the following observable cases:

- A selected throwing call that catches its own error keeps one
  `ErrorCapture`, associated by `throw_call_ref`, and has no terminal link.
- An unselected child whose error terminates selected parent and root spans
  produces one `ErrorCapture` and two terminal links, not two value copies.
- An unselected error caught before any selected frame terminates produces no
  error evidence and no loss.
- A later rethrow enters the funnel again, gets a new `ErrorCaptureId`, and is
  recorded as `Rethrow`. It may deduplicate to the earlier CID, but CID
  equality is content equality, not runtime error identity.

Publication preserves referential integrity. An `ErrorCapture` requires a
durable throwing `ContextRef` and the durable `SpanStart` named by
`first_selected_call_ref`. A capture target in `TerminalErrorRef` requires
both that error capture and the terminal call's `SpanStart` earlier
in the same atomically committed evidence segment or in a prior segment. The
writer's bounded pending joins are keyed by `ErrorCaptureId`; they do not hold
a stack copy or a growing per-span error vector.

For `ContextRef::Normal`, readers reconstruct the structural stack by
following the normal context's parent chain. `ContextRef::Overflow` has no
parent, function, or call-site chain by design; readers report
`StackIncomplete` and show only `throw_function_id`, `throw_site`, and the
overflow reason. They do not fabricate a stack from the selected terminal
spans.

If attempt metadata is admitted but copying, encoding, or CAS publication
fails, the `ErrorCapture` still commits with `value = Lost(reason)` and its
terminal links remain valid. If the capture attempt or the capture's own
structural/start dependency fails while a terminal span start is durable, the
terminal fact uses `TerminalErrorTarget::Lost(reason)`. If the terminal span's
own `SpanStart` is uncommitted, no terminal fact is published and health
increments `terminal_error_link_loss(StartUncommitted)`. If a terminal fact
itself cannot enter or commit, health increments its transport/queue/publish
loss reason. A durable record never references a missing capture, span, or
context.

`NotSelected` is not a loss state. Enabling a role does not itself create an
occurrence:

- input becomes applicable once at selected call entry;
- output becomes applicable only when the call returns a value;
- error becomes applicable only for an unwind under the rules above; and
- cancellation, exit, or an errored call has no output occurrence.

A reader that presents fixed input/output/error slots reports
`NotApplicable` when the corresponding runtime event did not occur. A
successful selected call with no qualifying unwind has no error capture and
no error loss. A call can finish successfully after catching an applicable
error; its captured unwind remains evidence even though it has no terminal
link.

Input/output `ValueCaptureAttempt` and error `ErrorCaptureAttempt` records use
the bounded capture/evidence lane. Error captures and terminal references
append incrementally to evidence segments. No error record selects an
otherwise unselected call.

### 7.5 Whole-value CAS

The MVP stores one encoded value per object:

~~~text
ValueCid =
  SHA-256("baml-value-v1" || codec_version || encoded_body)
~~~

This promises byte-identical deduplication, not semantic equivalence across
different encodings. The codec and hash framing are versioned and pinned by
cross-platform golden fixtures.

Input/output occurrence identity is `(CallRef, ValueRole)`. Error occurrence
identity is `ErrorCaptureId`; a terminal association is identified by its
`CallRef` and target. Content identity is `ValueCid`. The same CID may be
referenced by many roles, error captures, calls, boundaries, and runs.

Publication order is:

1. reserve the value pipeline's minimum charge;
2. copy and encode incrementally, growing the same reservation before every
   allocation and enforcing the derived single-value rule;
3. hash and atomically publish or verify the CAS object;
4. publish any CCT context definition required by the evidence;
5. publish the selected `SpanStart` dependency and the input/output occurrence
   or `ErrorCapture` that references the CID;
6. publish any `TerminalErrorRef` after its capture target; and
7. release temporary buffers and the reservation.

CAS hits rehash the complete existing object and compare the digest, codec
framing, and length with the requested CID before reuse. A conflicting object
at the same path is `CasConflict`, never overwritten.

If evidence publication fails after a new CAS object was written, the object
is an orphan. With no MVP GC it remains until `baml clean`. Disk accounting
includes objects and temporary/orphan bytes.

### 7.6 Value copying

The current deep-copy path is replaced or wrapped by one growable
`Owner::Values` reservation. It covers the entire overlapping lifetime of:

- snapshot/`TraceHeap` bytes;
- encoder output and scratch;
- hashing/publication buffers; and
- evidence metadata waiting for commit.

The copy builder grows the reservation before each allocation and releases all
partial state on failure. It also checks the derived single-value rule while
encoding, before allocating past that limit. A value that crosses the rule
becomes `Lost(ValueTooLarge)`. Until a streaming or disk-backed encoder is
implemented and tested, the design does not claim spill-to-disk support.

## 8. Resource policy and health

### 8.1 Five policy inputs, one memory governor

The configuration interface has five policy inputs:

~~~rust
struct ProfilerConfig {
    enabled: bool,
    store_root: PathBuf,
    process_memory_bytes: u64,
    disk: DiskBudget,
}

struct DiskBudget {
    max_project_bytes: u64,
    minimum_free_bytes: u64,
}
~~~

Only `enabled` and `store_root` are user-facing in the MVP
(`BAML_PROFILE` and `BAML_PROFILE_DIR` on native hosts). The three resource
values use measured production defaults and are injectable through the
configuration interface in tests. No other profiler capacity is an
environment variable or independent policy input.

`ProfilerMemoryGovernor` is process-wide and reserve-before-allocate. Its
interface is:

~~~rust
enum ReservationClass {
    Control,
    Manual,
    General,
}

enum Owner {
    Transport,
    Population,
    ActiveCalls,
    UnresolvedJoins,
    Evidence,
    Values,
    Writer,
}

struct MemoryDenied {
    class: ReservationClass,
    owner: Owner,
    requested_bytes: u64,
    available_bytes: u64,
}

fn try_reserve(
    class: ReservationClass,
    owner: Owner,
    accounted_bytes: u64,
) -> Result<Reservation, MemoryDenied>
~~~

`Reservation` is an RAII proof retained for the complete overlapping lifetime
of the charged capacity. Dynamic structures reserve capacity deltas before
growth. Stable slabs and fixed buffers reserve their complete backing capacity
once at construction and do not double-charge occupied entries.

Accounted bytes are deliberately implementable rather than pretending to know
the allocator's exact internal state:

~~~text
owned allocation capacity
+ fixed per-entry/queue-slot/control overhead
+ encoder/hash/root overlap
+ documented allocator margin
~~~

Each owner has a measured minimum charge for an occupied item, so tiny records
cannot create an unbounded effective count. There are no separate record-count
or in-flight-value-count budgets.

The reservation classes have fixed borrowing rules:

- `Control` protects boundary registration, lifetime terminal control, and
  fixed health reporting. General/manual work cannot borrow it. Boundary
  admission reserves its required control memory for the boundary lifetime.
- `Manual` is a reusable fallback for calls selected by explicit `LocalId`.
  Manual work first tries `General`, then `Manual`. General work cannot borrow
  the manual reserve.
- `General` owns all other profiling work.

`Owner` is diagnostic attribution, not a quota. Except for the two protected
reserves and stable preallocated slabs, unused memory is shared rather than
stranded in per-subsystem partitions.

The governor covers transport segments, decoder batches, stable boundary
state, active CCT epochs, active call/thread/join state, sparse await entries,
evidence batches, value roots/snapshots, encoder/hash/CAS buffers, writer
buffers, and caches. On denial the profiler:

1. signals the consumer and publisher to drain;
2. synchronously releases only owner-local caches, completed buffers, or
   immediately sealable work that require no blocking I/O;
3. retries the same reservation class once without waiting;
4. uses `Manual` fallback only for explicit-`LocalId` work; then
5. records the owner-specific loss or overflow outcome and continues BAML.

This is a nonblocking producer contract: a VM thread never waits for the
consumer, filesystem, or another boundary to return memory. No memory-pressure
path aborts or changes application execution.

Memory-denial health remains fixed-size. It aggregates denials by
`ReservationClass + Owner + failure reason` and may retain total/max requested
and minimum available bytes; it never appends one diagnostic record per denial.

### 8.2 Disk guard

The profiler store is one deep module at the durability seam. Callers do not
manage locks, usage-ledger entries, terminal exceptions, or cleanup policy.
Its interface performs three operations:

~~~text
publish_meta(records, terminal) -> Committed | Lost(reason) | Blocked | Indeterminate
publish_data(groups)            -> Committed | Lost(reason) | Blocked | Indeterminate
publish_cas_object(codec, body) -> (cid, publish result)
~~~

There is no `begin_boundary` or `finish_boundary`: admission takes no store
I/O, and terminal state is a `RootEnded` record inside an ordinary meta
batch (superseded by TASK/profiling-backend-streams.md §5.1).
`publish_meta`/`publish_data` own the store state machine and
CAS/dependency ordering; post-rename ambiguity is never collapsed into
either success or rejection. The terminal meta batch (`terminal = true`) is
one final publication attempt after the final drain but never waits for
space. Tests cross this seam through injected
accounting/filesystem adapters; no caller reimplements disk-full behavior.

Before publishing a segment or unique CAS object, the store checks total bytes
under the profiler root plus reserved temporary bytes and filesystem free bytes
after the proposed write. It rejects publication that would exceed
`disk.max_project_bytes` or `disk.minimum_free_bytes`. Check and reservation
occur under `publish.lock`, so concurrent processes cannot oversubscribe
either guard. A CAS hit consumes no object bytes, but its evidence reference
still needs publication space.

The first `DiskGuardExceeded` rejects that pre-rename batch, atomically latches
normal store admission, and terminally loses other bounded not-yet-renamed
batches under the same reason. One producer atomic read then suppresses value
rooting/copying and ordinary publication work. BAML execution and runtime IDs
continue; committed artifacts remain immutable.

There is no automatic cleanup or in-session polling. Recovery is an explicit
close, clean/change/free-space, and reopen with usage reconciliation. New
boundaries start profiler-off while blocked. Admitted executions still
finish their terminal hand-off, but a definite pre-rename terminal
rejection leaves the execution without a `RootEnded`, as Section 5.3
specifies.

### 8.3 Deterministic derived sizing

One pure, versioned policy converts `process_memory_bytes` and measured layouts
into internal sizing:

~~~rust
fn derive(
    process_memory_bytes: u64,
    measured: MeasuredLayouts,
) -> Result<DerivedSizing, InvalidMemoryBudget>
~~~

`MeasuredLayouts` and representative `DerivedSizing` fixtures are frozen in
Phase 0. Derived values are inspectable in tests and diagnostics but are not
configuration. The algorithm must reject a total too small to construct the
minimum control plane rather than silently weakening guarantees. Session
construction handles `InvalidMemoryBudget` by creating `ProfilerSession::Off`
and returning one host-visible setup diagnostic; it allocates no partial
profiler state and does not affect BAML execution.

| Internal structure | Derived/fixed behavior | Pressure behavior |
|---|---|---|
| Control and manual reserves | Derived with measured absolute minima/maxima from the total budget | Protected borrowing rules from Section 8.1 |
| Stable boundary registry | Slot count derived from complete slot/control/health layout; backing allocation charged once | New root runs profiler-off when no slot/control reservation is available |
| Transport | Ring segment size is one measured batching constant; total live/cached capacity is governor-owned | Wake/drain, release freelist cache, then explicit transport loss |
| Active thread/call/join state | Each entry charges measured capacity plus fixed overhead; stable capacities are derived where required | Owner-specific capacity loss; leases remain authoritative |
| Unresolved joins | Retain until resolved/final drain; under pressure classify and release oldest unresolved facts explicitly | `JoinCapacityExceeded` or unmatched-fact health |
| Active CCT epoch | One derived soft working target inside the shared governor | Drain/seal/rollover; overflow only if a normal context still cannot reserve |
| Await accumulator | Sparse entries use the active-call owner and measured minimum charge | `AwaitAccumulatorMemoryExceeded` |
| Evidence queue | Accounted bytes include record, encoded metadata, and queue-slot overhead | `EvidenceQueueFull`; explicit manual work may try the manual reserve |
| Value work | One reservation covers root, snapshot, encoder, hash/CAS buffers, and evidence metadata | `ValueMemoryExceeded` |
| Single value | `min(fixed absolute ceiling, measured fraction of value working memory)` | `ValueTooLarge` before unbounded allocation |
| CCT/evidence segments | One derived target initially; flush on target, boundary completion, or fixed maintenance tick | Seal and open the next segment; never a lifetime quota |
| Publisher/files | One bounded publisher uses O(1) publication file handles and reopens inputs on demand | Backpressure/loss through governor/store state |
| Consumer wake | Wake-on-write plus one fixed fallback timer | No configurable polling interval |
| Freelist/caches | Opportunistic only and charged at owned capacity | Released before selected/population loss |

A separate CCT/evidence segment target, extra publisher concurrency, or public
resource control is added only if Phase 0 evidence requires it and this
document is amended. Adaptive behavior must remain deterministic and bounded;
it cannot become an undocumented policy.

Canary's old ring-size/freelist/wake environment variables may exist only as
temporary benchmark/oracle compatibility during cutover. They are not
`ProfilerConfig` inputs and are removed with the legacy profiler.

There is deliberately no lifetime node, span, error-capture, evidence-byte,
boundary, segment-count, retention-age, or GC threshold. Rollover and batching
never become hidden capture quotas.

### 8.4 Failure reason inventory

A failure reason identifies what profiling fact was lost and why. It does not
imply a matching user knob. Memory failures name their reservation owner/class;
disk failures name the disk policy; corruption, clock, I/O, and lifecycle
failures name their actual cause.

Population reasons:

| Reason | Meaning | Admission owner/cause |
|---|---|---|
| `TransportMemoryExceeded` | structural record could not enqueue | `General / Transport` memory denial |
| `ProfilerExecutionStateUnavailable` | root could not reserve fixed execution health/control state | `Control / Population` denial or no derived stable slot |
| `StoreUnavailable` (admission) | store gate was closed or indeterminate at admission; folds the former store-unavailable/store-indeterminate reasons | disk policy/store state; two atomic reads, no I/O |
| `ForkedProcess` | admission attempted in a `fork()` child; root runs profiler-off | fork guard (streams spec §5.8) |
| `ProfilerThreadLeaseUnavailable` | child/subtree could not join profiling before runnable | derived counter saturation or stale generation |
| `RootAbandoned` | host dropped/aborted an acknowledged root before classified result | host lifecycle |
| `BoundaryBarrierControlFailed` | pre-reserved terminal barrier failed to publish/acknowledge | consumer/store shutdown or failure |
| `TerminalStorePublicationFailed` | quiescent terminal `RootEnded` failed; slot already released | disk policy, I/O, device, or free-space change |
| `CorruptRecord` | committed bytes failed decode | corruption |
| `ActiveThreadCapacityExceeded` | decoded child start could not retain bounded thread state; producer lease still owns lifetime | `General / Population` denial |
| `ActiveCallCapacityExceeded` | decoded start could not retain active-call state | `General / ActiveCalls` denial |
| `JoinCapacityExceeded` | unresolved fact was released under pressure | `General / UnresolvedJoins` denial |
| `UnmatchedCallFact` | call start/end remained unresolved at final drain | missing counterpart or prior structural loss |
| `UnmatchedThreadFact` | thread/spawn parent remained unresolved at final drain | missing counterpart or prior structural loss |
| `ContextMemoryUnavailableAfterDrain` | normal context still could not reserve after drain/rollover | `General / Population` denial |
| `CctSegmentPublishFailed` | CCT segment could not commit | writer I/O/permission/device |
| `DiskGuardExceeded` | CCT publication violated disk policy | max-project or minimum-free policy |
| `CounterSaturated` | aggregate exceeded numeric schema | numeric saturation |
| `ContextKeyCollision` | identical key had a different tuple | hash/corruption |
| `PopulationUnpersisted` | terminal store could not persist emergency health | terminal store failure |

Batched publication adds three process-global counters: `meta_batch_lost`
(a meta-pre batch was terminally lost), `root_ended_lost` (a terminal
`RootEnded` record was terminally lost), and
`function_table_publish_failed` (the per-engine `FunctionTableV1` CAS
publication failed); see TASK/profiling-backend-streams.md §5.3/§7.2.

Timing reasons:

| Reason | Meaning | Admission owner/cause |
|---|---|---|
| `AwaitAccumulatorMemoryExceeded` | first suspension could not retain sparse timing state | `General / ActiveCalls` denial |
| `AwaitClockInvalid` | ticks were incomparable or reversed | clock/platform |
| `AwaitCounterSaturated` | call/context await duration or count saturated | numeric schema |
| `AwaitedEndTransportExceeded` | awaited end rejected; full count moves to timing loss | `General / Transport` denial |
| `AwaitedEndUnmatched` | awaited end unresolved at final drain | missing counterpart or prior structural loss |
| `AwaitIntervalUnreconciled` | started waits exceeded folded plus classified loss | corruption/crash-adjacent torn record |
| `SelfTimeUnderflow` | child inclusive plus await exceeded inclusive; self clamps to zero | clock, corrupt/duplicate fact, or prior structural loss |

Any population loss that can remove a call start/end or parent edge also makes
the affected boundary's self/await view incomplete. If producer loss has no
resolvable context, the whole boundary may be marked timing-incomplete rather
than attributing false precision.

Evidence-part reasons:

| Reason | Meaning | Admission owner/cause |
|---|---|---|
| `StructuralStartTransportExceeded` | selected start rejected by structural transport | `General / Transport` denial |
| `StructuralEndTransportExceeded` | selected end rejected by structural transport | `General / Transport` denial |
| `RuntimeIdAnnotationTransportExceeded` | selected runtime-ID annotation rejected | `General / Transport` denial |
| `ErrorCaptureAttemptTransportExceeded` | applicable unwind draft could not enter evidence lane | `General / Evidence`, then `Manual / Evidence` for explicit manual work |
| `TerminalErrorLinkTransportExceeded` | selected terminal link could not enter evidence lane | evidence reservation denial |
| `EvidenceQueueFull` | selected evidence metadata could not enter writer queue | `General / Evidence`, then manual fallback when applicable |
| `MissingStructuralJoin` | required normal/overflow context does not exist | population loss above |
| `StartUncommitted` | dependent fact discarded because selected start never committed | preceding start loss |
| `EvidenceSegmentPublishFailed` | evidence segment could not commit | writer I/O/permission/device |
| `DiskGuardExceeded` | evidence publication violated disk policy | disk policy |
| `StoreUnavailable` | profiler root/lease could not open | path/permission/lock |

Value reasons:

| Reason | Meaning | Admission owner/cause |
|---|---|---|
| `ValueMemoryExceeded` | complete overlapping value reservation failed | `General / Values`, then `Manual / Values` for explicit manual work |
| `ValueAttemptTransportExceeded` | selected attempt/control fact could not enter evidence lane | evidence reservation denial |
| `ErrorCaptureAttemptTransportExceeded` | unwind draft was rejected before error body admission | evidence reservation denial |
| `ValueTooLarge` | encoded value crossed derived single-value rule | sizing policy |
| `CopyFailed` | source value could not be copied | value/runtime |
| `EncodeFailed` | codec rejected or failed | codec |
| `CasWriteFailed` | CAS object could not commit | writer I/O/permission/device |
| `CasConflict` | existing CID path had conflicting bytes/framing | corruption/collision |
| `DiskGuardExceeded` | unique object/reference violated disk policy | disk policy |
| `EvidenceSegmentPublishFailed` | CID existed but occurrence fact did not commit | evidence writer failure |

Process crash loss cannot always be written by the crashed process. Reopen
reports incomplete terminal state and ignores temporary files; this is not
misclassified as memory or disk-policy denial.
## 9. Local artifacts and durability

### 9.1 File layout

~~~text
.baml/
  profiles-v1.lock       # stable lease file; never inside the removed root
  profiles-v1/
    publish.lock
    usage.state
    tmp/
    streams/
      <process-euid hex32>/
        stream.lock
        meta/
          00000000000000000001.bamlmeta
        data/
          00000000000000000001.bamldata
    cas/
      sha256/
        ab/
          <64-hex-digest>.bamlvalue
    runs/                # legacy v1 layout: never read; removed by clean
~~~

There is one stream directory per process, holding one meta plane and one
data plane shared by every execution and engine of that process; there are
no per-execution directories (superseded by
TASK/profiling-backend-streams.md §3).

The stable external lease file prevents a cleaner from deleting the locked
inode and allowing another process to create a different lock while cleanup
is still running. `store.lock` below means this
`profiles-v1.lock`/configured-root sibling.

`usage.state` is a small, atomically replaced byte-accounting ledger protected
by `publish.lock`. It prevents a full directory scan on every publication.
Because this MVP never automatically deletes profile data, committed usage is
monotonic until `baml clean`.

On each process's first store open, it acquires locks in the order
`store.lock(shared) -> publish.lock`, scans committed objects/segments plus
temporary/orphan files once, and atomically reconciles `usage.state` with
physical bytes. This clears a crashed reservation that never wrote bytes while
still charging every temp/orphan byte that exists. Reconciliation does not
delete anything. If the scan or ledger cannot be trusted, store opening fails
closed with `StoreUnavailable`.

The same open scan resolves a publisher that crashed after final rename but
before directory fsync: if the checksummed final path exists, the opener fsyncs
its containing directory and accounts/accepts that exact meta segment, data
segment, or object; if it is absent, the batch remains crash-lost and any
segment sequence is still unused. A conflicting/corrupt final path fails
closed. This is recovery of one indeterminate commit point, not replay of a
lost data batch.

There is no `run.meta` or `run.end`. Immutable execution metadata and the
terminal summary are `RootStarted`/`RootEnded` records in the meta plane,
and program identity plus optional revision/source labels ride
`EngineStarted` (superseded by TASK/profiling-backend-streams.md §4.3).
Instead of a per-run segment fence, `RootEnded` stores the execution's
data-plane extent as O(1) values — `data_first_seq`, `data_last_seq`, and
`data_segment_count` — final at encode time because `RootEnded` publishes
only after every group of that execution is committed or lost.

A crashed or still-active execution has no `RootEnded`; it reads `Running`
while the stream is alive and `Abandoned` afterwards (superseded by
TASK/profiling-backend-streams.md §6.2).

Segment headers are defined by TASK/profiling-backend-streams.md §4.3 (meta
plane, `BAMLMET1`) and §4.4 (data plane, `BAMLDAT1`): magic, schema
version, a checked non-reused per-stream `u64` sequence, `ProcessEuid`,
record/group counts, payload length, and a trailing checksum. Data-plane
groups are keyed by the execution's root `ThreadRef`.

Readers scan immutable segments and ignore temporary files. Missing/corrupt
segments mark the corresponding plane incomplete.

The zero-padded names above are illustrative, not a lifetime
limit. Each plane publishes contiguous sequences starting at one. A candidate
sequence advances only after rename plus directory fsync; a pre-rename lost
batch does not consume a number, while a renamed-indeterminate batch owns its
candidate exclusively until resolved. Per-stream/per-plane publication is
serialized, so concurrent writers cannot allocate the same candidate. Writers retain only
the next sequence/high-water pair and bounded current batch; they do not keep
an in-memory manifest entry per completed segment.

Directory enumeration and segment decoding are streaming. For an ended
execution, the reader compares the observed data segments with
`RootEnded.data_first_seq`/`data_last_seq`/`data_segment_count`, so a
missing interior or final segment is detectable without a manifest. A query
that asks for a fully merged CCT necessarily uses memory proportional to
distinct returned contexts, but not to calls or segment count when the same
contexts repeat. Sequence arithmetic is checked; it never wraps or reuses a
path.

### 9.2 Atomic publication

Writers:

1. acquire and retain `store.lock(shared)` and the stream's
   `stream.lock(exclusive)` for the store session;
2. build/encode the bounded batch under a memory reservation;
3. acquire `publish.lock`;
4. reread current `usage.state`, reserve bytes, and recheck free space;
5. write a uniquely named file in `tmp/`;
6. flush and fsync the file (every file fsync routes through the
   platform's `sync_file`);
7. rename to its final content/sequence path and enter
   `RenamedAwaitingDirSync`;
8. fsync the containing directory, resolving that state to `Committed`; and
9. advance the live plane high-water value, commit the usage reservation, and
   release `publish.lock`/memory.

If step 8 fails, the publisher retains the single indeterminate state and
`publish.lock` as specified in Section 7.2; it does not execute step 9, reuse
the candidate, or allow another publication around the visible path. The same
protocol governs the terminal meta batch carrying `RootEnded`; the registry
slot is already released at hand-off, so durable terminal state lives in
the stream alone (superseded by TASK/profiling-backend-streams.md §5.6).

CAS objects required by evidence publish first. Any `ContextKey` whose bounded
dependency token is not yet `DurableDefinition` publishes its definition in a
CCT segment before the evidence segment rename, following Section 6.5; no
lifetime lookup is performed. Within evidence, a dependency may appear earlier
in the same atomically published group: `SpanStart` precedes its dependent
facts, and `ErrorCapture` precedes every `TerminalErrorRef` that targets it.
Otherwise the dependency must already exist in an earlier committed group of
the same execution.

Full CAS-hit verification does not hold the project publication lock:

1. while holding the shared store lease, rehash the immutable existing object
   and capture its file identity/size;
2. acquire `publish.lock` and re-stat the path;
3. accept the hit only if the same immutable file identity/size is still
   present;
4. if it changed or disappeared, release the lock and restart verification;
5. if a previously absent path appeared during a put race, release the lock,
   verify the winner outside the lock, and retry; and
6. if it remains absent, perform the guarded no-overwrite rename/publication.

A memory-governed cache may remember verified CID plus immutable file identity
to avoid repeat reads. External mutation that changes the identity invalidates
the cache.

Stream publishers may build batches concurrently. The project
`publish.lock` serializes only final existence/identity checks, disk
accounting, no-overwrite CAS publication, and segment publication, so two
processes cannot oversubscribe the disk budget or race a CID path. A large
dedupe hit never serializes unrelated CCT/evidence writers for the duration of
a full read/hash. Publications are batched to keep this lock off the VM path.
Readers and writers hold a shared `store.lock` lease. `baml clean` requires
the exclusive lease and only then acquires `publish.lock`. The global lock
order is always `store.lock -> stream.lock -> publish.lock`; no path may
acquire them in another order.

Temporary and orphan files count toward disk usage. The MVP does not silently
delete them during admission or capture. `baml clean` removes them.

## 10. Live and local reader model

The MVP does not retain or broadcast a per-fact live update log. The central
consumer folds decoded facts directly into the bounded CCT/evidence writer
batches described above. CCT deltas remain additive by `ContextKey`; exact
facts remain the types in Section 7.

The stream writer exposes one O(1), atomically read stream checkpoint, and
each already-bounded active execution exposes one execution checkpoint
(superseded by TASK/profiling-backend-streams.md §5.6):

~~~rust
struct StreamCheckpoint {
    high_water: StreamHighWater, // last committed sequence per plane
    pending_groups: u32,
    pending_meta: u32,
    oldest_pending_age: Option<Duration>,
    publication_inflight: bool,
}

struct ExecutionCheckpoint {
    root: ThreadRef,
    health: ExecutionHealthSnapshot,
    queued: QueueHealthSnapshot,
    data_first_seq: u64,
    data_last_seq: u64,
}
~~~

The high-water fields advance only after the corresponding directory fsync.
A live reader polls the stream checkpoint, cursors on `StreamHighWater`,
streams newly committed segment files, and filters data-plane groups by
root `ThreadRef`. Current partial batches are reported only as bounded
queued/in-flight health and become queryable after normal size/age/terminal
sealing. A slow or disconnected reader retains no profiler-owned backlog; it
catches up from immutable files. There is no subscriber queue, retained
`ProfilerUpdate` history, or invocation-shaped in-memory RunStore in the MVP.
Push notification streams and uncommitted per-fact live views are deferred.

Local readers:

- enumerate and decode segment files incrementally rather than loading every
  segment body or a lifetime-sized segment manifest;
- merge CCT deltas by `ContextKey`;
- fold span facts by `CallRef`;
- join exact spans to context definitions by `ContextRef`;
- index error captures by `ErrorCaptureId` and join terminal spans through
  `TerminalErrorRef`;
- reconstruct an error's call stack from a normal throwing `ContextRef`'s CCT
  parent chain rather than a repeated stack blob, or report
  `StackIncomplete` with observed throw labels for an overflow context;
- derive runtime `self_ns` from merged inclusive/direct-child/await components
  and surface timing completeness separately;
- resolve value bodies lazily by CID; and
- expose population, exact evidence, error evidence, and value health
  separately.

The reader does not reconstruct all invocations from a raw event log. For an
ended execution the reader validates the data plane against `RootEnded`'s
extent fields; an active reader uses the stream high-water as a consistent
committed prefix.

## 11. Profiler off and benchmarking

`BAML_PROFILE=0` must result in:

- no profiler ring registration or refresh;
- no profiler consumer task/thread, wake handle, decoder, or health block;
- no CCT maps or segment writers;
- no evidence queues;
- no CAS/store handles, lock-file opens, usage scan, or profiler filesystem
  reads/writes;
- no sparse VM await accumulator or await-clock work;
- no profiler `TraceHeap`, value cache, capture hook, policy resolution, value
  root/copy/encode/hash, or loss accounting; and
- no profiler files or directories created.

Runtime call IDs and `$id` semantics remain. `LocalId.capture` still mutates
and validates the language handle, but neither it nor a root/LLM call can
enable profiling lazily. Explicit logging requested by the host remains an
independent module; it must not reach the profiler `TraceHeap` or store as a
side effect.

`ProfilerSession::Off` is selected once at process/store-session construction
and injected into engines. VM call, return, unwind, suspension, and spawn hooks
receive `RootProfiler::Inactive`; its adapter is a predictable no-op after
language call-ID/runtime-ID work. It never reads the environment or discovers
whether a manual capture occurred. Enabling profiling requires a new shared
profiler/store session, normally a new process; an active root never changes
handles.

An `On` session may also return `Inactive(Suppressed)` for internal roots. The
MVP migrates every current `.with_profile_enabled(false)` caller—test
collection, GC finalizers, and LSP test-registry collect/expand/serialize—to
the explicit `SuppressInternal` intent, and adds the exec JSON argument/output
helpers (`baml.json.deserialize` / `baml.json.serialize` roots created by
`baml run` and packed executables around the user's function; they previously
shared the target's capture stream and never had their own profile). An
invocation therefore publishes exactly one run, the user boundary; a
user-written `from_json` override that runs during argument decoding is inside
the suppressed helper root, while the same override called from the user's
function remains profiled. Suppression covers that root and its
descendants: no boundary registration, capture resolver, timestamps, await
accumulator, structural/evidence hook, or artifact, while IDs and independently
requested logs remain functional.

Master session/root inactivity dominates all host capture requests. The MVP
removes profiling `FunctionCallContext.value_capture`,
`CaptureDefaults.values_enabled`, and `CallContextCapture` propagation.
Selected-evidence ownership is created only by `RootProfiler::Active`.
CLI/exec/LSP/WASM hosts receive a separate optional logger interface instead
of `TraceCaptureProducer::logs_only`/`logs_enabled`; no enabled legacy producer
can bypass `ProfilerSession::Off`.

No compiler flag is added. Use the existing benchmark split:

- regular runtime/compiler/package benchmarks pin `BAML_PROFILE=0`; and
- `baml_tests/benches/profiling_overhead.rs` pins `BAML_PROFILE=1`.

Before cutover, measure:

- pure-call throughput and bytes per call pair;
- ready-inline sysop and future throughput, proving they retain the compact end
  and perform no await allocation;
- one-wait and many-waits-per-call throughput/bytes, including the one awaited
  end variant and sparse accumulator;
- profiler-off allocation count and bytes;
- profiler-on unselected-call overhead;
- selected root/LLM/manual value overhead;
- CCT/evidence rollover throughput and resident memory across many segments;
- evidence/CAS throughput for misses and dedupe hits; and
- behavior at each memory and disk guard.

The off-path allocation test must prove that constructing and running an
engine with profiling disabled creates no profiler `TraceHeap`, task, file
descriptor, lock, directory, or store state. A fixture containing root, LLM,
manual `LocalId.capture`, error unwind, await, and spawn paths must leave a
pre-existing profiler root byte-for-byte/metadata unchanged while preserving
all language identity results.

## 12. Cutover and removal plan

No old artifact migration or reader is required. Deletion must still preserve
unrelated language identity, logging, and run-lifecycle behavior.

### 12.1 Keep

- compact producer record encoding and clock;
- ring registry/wake/drain substrate after making it nonfatal and bounded;
- runtime identity types and unconditional call-ID behavior;
- `boundary.LocalId` and `boundary.id.current()`;
- the existing `baml.id.current()`, `baml.id.new()`, and `baml.id.set()`
  runtime-identity behavior;
- revision/function metadata needed for CCT labels;
- host boundary lifecycle;
- host runtime token minting; and
- non-profile structured logging.

### 12.2 Replace

- raw/protobuf transcode with direct decode/join;
- invocation-shaped RunStore profile state with CCT/evidence updates;
- history event routing with boundary-aware segmented writers;
- sequential value IDs and boundary-local large-only blobs with
  occurrence records plus project CAS;
- engine-local/host-injected profile capture with the shared
  `ProfilerSession -> RootProfiler` admission interface;
- shared logging/profile capture with a separate optional logger interface;
- disabled value-producer construction with a state-free inactive root
  adapter; and
- fatal ring overflow with bounded loss/health.

### 12.3 Remove after cutover

The removal inventory must be refreshed with `rg` immediately before deletion
because canary is moving. It includes at least:

- per-engine and per-thread `.bamlprof` writers/readers;
- profiling protobuf schema, transcode, build-script generation, Cargo cfg and
  now-unused dependencies;
- `RunStore.profile_events`, connected-component scans, call/thread
  reconstruction, and invocation-shaped wire fields;
- `BoundaryTraceRouter` and stack segment paths;
- WASM raw-profile chunks and artifact notifications;
- bridge/LSP/history tests that assert old artifacts;
- pack-host flush logic named around old profiles;
- profiling `FunctionCallContext.value_capture`,
  `CaptureDefaults.values_enabled`, `CallContextCapture` producer propagation,
  and host injection of enabled `TraceCaptureProducer` values;
- `TraceCaptureProducer::logs_only`, `logs_enabled`, and other profile-owned
  logging plumbing after CLI/exec/LSP/WASM use the logger interface;
- generic `.with_profile_enabled(false)` call-site gating after every internal
  caller uses `RootProfileIntent::SuppressInternal`;
- old profiling docs and gitignore entries; and
- legacy value-reader code that is profile-only.

Do not delete `history/boundary_writer.rs` or `.bamlvalue` ownership wholesale
until `LogEvent`, log `CaptureLoss`, `RunStarted`, and `RunCompleted` have a
clear non-profile home. Captured values move to evidence/CAS; logging remains
functional with profiling off. The post-cutover history writer and reader use
a fresh tree, `.baml/history-v1/`, so the reader can never open a mixed
pre-cutover file; `.baml/history/` is legacy, never discovered or read, and is
operator-cleaned. Plain `baml clean` never deletes this mixed legacy history.
We accept leaving unreachable legacy captured-value bytes rather than
parsing/rewriting mixed files as a migration project.

Removing legacy writer/reader code does not authorize deleting legacy files
from disk. Plain `baml clean` is scoped to `profiles-v1` only; all legacy
artifacts require an offline/operator cleanup outside this MVP.

### 12.4 Runtime-identity compatibility

This profiler cutover does not deprecate the public `baml.id` API.
`baml.id.set` remains a mid-call runtime-identity mutation and is not a capture
selector. For a call already selected by root, LLM, or call-site `LocalId`, its
latest annotation remains visible on the exact span. For an unselected call,
it changes language-visible identity without creating exact evidence.

Any future deprecation of `baml.id.current/new/set` is a separate language
proposal with its own compatibility window. It is not authorized by this
document and is not required to remove `.bamlprof`.

## 13. Implementation phases

Each phase implements the authoritative contracts referenced below; it does
not restate or weaken them.

### Phase 0: freeze contracts and measurements

- Inventory all canary call/thread producers, suspension and unwind entries,
  `EndFunction` emitters, internal-root suppressions, host value/logging
  injection, and `$id` call-kind support.
- Freeze durable `CallRef`/`ThreadRef` scope, nonoptional `ProgramId`,
  `ContextKey`, `ValueCid`, awaited-end, and error-record codecs with
  cross-platform golden fixtures.
- Pin existing `LocalId`/runtime-ID behavior with tests.
- Record structure sizes, event rates, on/off baselines, and timing/spawn
  costs. Choose production memory/disk defaults and freeze
  `ProfilerSizingPolicy::derive(...)`, minimum charges, protected-reserve
  rules, and representative derived-sizing fixtures before their phase lands.

### Phase 1: central policy and true off

- Implement the Section 11 process/store `ProfilerSession::Off | On` and sole
  per-root `begin_root(UserBoundary|SuppressInternal)` interface.
- Put the Section 4 resolver inside active roots and encode its result in the
  reserved `CallFunction.flags` byte.
- Remove the independent `FunctionCallContext` value-capture path and move
  CLI/exec/LSP/WASM logging to its own optional logger interface.
- Preserve all language IDs, `LocalId` mutation/consumption, and logging in
  inactive modes; extend off, suppressed, active, and logging-only benchmarks.

### Phase 2: bounded decode, ownership, and timing

- Decode before legacy writers and join bounded structural state by
  `CallRef`/`ThreadRef`.
- Implement Section 5.1 acknowledged registration, generation-tagged leases,
  outer root/child completion guards, stable slots, Arc-style last-owner
  ordering, and the cross-ring terminal barrier. Preserve detach as
  cancellation-only and carry spawn source spans.
- Add governor-charged active-thread/call/unresolved state and the O(1)
  publisher handles; replace fatal ring overflow with explicit nonfatal loss.
- Implement Section 5.4's sparse await accumulator and single
  `EndFunctionAwaited` variant across every declared suspension seam.
- Keep old output only as an oracle; transcode the awaited variant as an
  ordinary legacy end until Phase 7.

### Phase 3: store and segmented CCT

- Implement the Section 8.2 store interface and two-phase root admission:
  reserve runtime/control ownership before `run.meta`, then handle
  `Admitted | Rejected | Indeterminate` with no later fallible capacity step.
- Implement Section 6 context identity, dense active epochs, additive timing,
  rollover, emergency aggregates, population health, and atomic CCT segments.
- Keep definition durability bounded to current epochs and active calls;
  republish parent-first or degrade explicitly, never scan/retain lifetime
  ancestry.
- Implement Section 7.2's one-owner batch state machine, final pre-rename loss,
  and single global post-rename indeterminate state.

### Phase 4: selected evidence and errors

- Emit/fold `SpanStart`, `SpanEnd`, and runtime-ID annotations under the
  Section 4 root/LLM/call-site-`LocalId` resolver.
- Replace legacy error-origin/root duplication with the Section 7.4 unwind
  observer and explicit `VmThrown` carrier; preserve profiler kind, language
  rethrow semantics, source, live-PC throw site, and all producer mappings.
- Emit one error attempt per applicable unwind and fan out terminal links
  without duplicate value/stack copies.
- Add the governor-charged evidence queue, protected manual fallback, segment
  rollover, and `baml.id.set` annotation without promotion.

### Phase 5: bounded values and CAS

Implement Section 7.5–7.6 reservations, the single-value guard, codec/CID
fixtures, project-shared atomic CAS, dependency-ordered evidence publication,
and complete value/error reconciliation.

### Phase 6: readers and cutover

- Use shared domain types for live and durable readers; replace
  invocation-shaped RunStore/wire/history reconstruction.
- Use O(1) committed checkpoints and reader-owned cursors, with `run.end`
  per-plane fences for interior/tail-loss detection.
- Rehome non-profile log/lifecycle artifacts and add exclusively leased
  `baml clean` scoped to `profiles-v1`.

### Phase 7: remove legacy systems

After the oracle window, remove production dual-write, `.bamlprof`,
invocation reconstruction, old stack/value ownership, protobuf/build/bridge
surfaces, raw WASM chunks, and stale tests/docs. Refresh the inventory with
`rg` against then-current canary and verify logging still works and no old
artifact is emitted.

### Phase 8: failure and performance gate

Run Section 14's failure, crash, corruption, concurrency, resource, and
performance suite. Verify no profiler path aborts BAML, off mode owns zero
profiler resources, and measured regressions match the Phase 0 thresholds.
Record every accepted deviation in this document before merge.
## 14. Acceptance gates

Every named case below is required. Grouping related cases does not permit one
fixture to stand in for another when their failure seams differ.

Note: gates below that mention `run.meta`, `run.end`, per-boundary
directories, `Sealed`/`ReleasedIncomplete`, or `begin_boundary` are
replaced by the gates in TASK/profiling-backend-streams.md §9; all other
gates remain in force verbatim.

### Population and boundary lifetime

- **Context identity:** one versus one million calls on one path changes only
  counters; two source call sites, call versus spawn, and two spawn expressions
  create distinct contexts. Await/resume remains one invocation.
  `ProcessEuid` prevents otherwise-identical engine/thread/call IDs from
  colliding, and end-before-start across OS-thread rings joins by `CallRef`.
- **Rollover:** hot paths and calls spanning epochs merge by `ContextKey`;
  active parents accept children through an external parent key. Repeated
  sustainable rollover causes neither overflow nor loss. Recreated contexts
  republish definitions without old-segment scans or a lifetime published-key
  set.
- **Definition dependencies:** evidence waits for definition durability.
  Injected definition-batch loss loses dependent evidence. A live context may
  republish its retained tuple parent-first; an unavailable ended parent
  degrades to `InvalidParentContext` rather than a dangling definition or
  retained ancestry.
- **Terminal ordering:** `run.end` follows descendant quiescence and the
  cross-ring barrier. Root return does not wait for ordinary/detached children;
  the last lease closes the boundary. A child may spawn a grandchild after root
  return. Simultaneous root/child completion and a root with no descendants
  each submit exactly one barrier through the same last-owner path.
- **Child completion:** queue rejection, cancellation before first poll, task
  drop, engine error, and scheduling failure release one acquired lease.
  Barriers placed (a) after inner-loop return but before final `EndThread` and
  (b) during post-loop future settlement prove that neither consumed
  `BexThread` nor early `EndThread` can release ownership before all final
  producer work stops.
- **Root completion:** host drop/abort after acknowledged start yields
  `Abandoned` plus `RootAbandoned`; panic yields `Panicked`; setup/engine error
  yields `Failed`. Each releases once. Dropping the start-ack receiver leaves
  no naked handle: the transferred armed guard closes the boundary as
  abandoned.
- **Detach and fan-out:** ordinary and detached descendants keep the original
  boundary, runtime identity, resolver, and spawn edge; detach creates no ID or
  selection. Ten thousand equivalent detached workers create one boundary/run/
  spawned context with ten thousand invocations, not extra roots, root
  captures, directories, or barriers. Descendant errors do not rewrite the
  immutable root result. A forever-running child honestly keeps the run open;
  profiling never cancels or joins it. Orderly engine shutdown preserves
  canary's existing wait-only behavior; this MVP adds no cancellation sweep.
- **Loss and races:** losing child `StartThread` reports attribution loss while
  its lease still prevents early seal. Counter saturation or stale generation
  makes that subtree profiler-off with
  `ProfilerThreadLeaseUnavailable` and preserves execution/identity.
  Deterministic concurrency modeling covers root/last-child, post-root
  grandchild, abnormal drop, and slot reuse without early close, double
  release, or stale attachment. Start/final-control failure never creates a
  falsely owned or terminal run. Ring pressure never aborts, and emergency
  overflow appears only after drain/rollover cannot create a normal context.

### Timing

- **Encoding:** a never-suspended call uses compact `EndFunction` with zero
  await; one or ten waits use one `EndFunctionAwaited` carrying the summed
  duration and count. Ready-inline sysops/futures allocate no sparse entry.
- **Attribution:** async sysop, Await, AwaitAny, task-group/entry permit,
  EarlyYield, normal resume, cancellation wake-up, and OS-thread migration
  charge the call captured before suspension.
- **Arithmetic:** synchronous-child inclusive time is subtracted once; spawned
  time is never subtracted. Merge-before-subtract across calls, epochs, and
  segments matches a single fold, including calls crossing rollover. Runtime
  self is wall-clock, not CPU; await includes declared scheduling, cancellation,
  GC-while-parked, and permit delay.
- **Loss:** injected accumulator reservation, saturation, invalid clock,
  awaited-end transport loss, unmatched/corrupt end, and self underflow each
  report their exact reason, preserve execution, and mark timing incomplete.
  Host/task abandonment invents no resume or duration.
- **Reconciliation:** terminal `await_intervals_started` equals folded count
  plus classified loss; live checkpoints show open-call counts separately.
  No timing fixture observes an event sequence, per-wait durable record, or
  reorder queue.

### Policy and identity

- Root and outer `FunctionMeta::Llm` calls select input/output/error on every
  host; generic bytecode functions and internal LLM sysop helpers do not.
  Ordinary helpers without `LocalId` remain CCT-only.
- The canonical ordinary-call flow
  `let id = boundary.id(); id.capture(inputs = true);
  ordinary(value, $id = id)` captures exact metadata plus input only. A bare
  ID is metadata-only; all-false is metadata-only; `capture(...)` without
  later `$id` captures nothing. On LLM calls, omitted roles inherit LLM policy.
  In particular, `capture(output = false)` keeps LLM input/error and disables
  output.
- Delayed/fluent/aliased mutation, null/omitted accumulation, and last-non-null
  wins are preserved. Reuse or mutation after consumption is catchable
  `InvalidArgument`.
- Preserve the current call-kind matrix: bytecode/sysop calls accept explicit
  `LocalId`; unsupported native builtin/host-callable paths retain their
  rejection behavior. Manual IDs never affect CCT cardinality.
- Profiling off preserves `$id`, call identity, all `LocalId` mutation/
  consumption/errors, and install/restore behavior while producing no
  profiler state or evidence. `baml.id.set` does not select an ordinary call.

### Evidence, errors, and CAS

- **Reconciliation:** every selected start commits or has one start-loss
  reason; every requested value is `Available` or `Lost(reason)`; every
  applicable attempt resolves or contributes to its exact transport/admission
  loss. Start, end, runtime-ID, error-capture, terminal-link, and value
  equations reconcile independently.
- **Applicability:** success without qualifying unwind creates no error fact;
  errored/cancelled calls create no output fact. Catching `(error)` versus
  `(error, context)` yields identical profiler evidence. A selected
  throw-and-catch retains a capture but no terminal link; an unselected child
  caught before a selected frame terminates creates neither capture nor loss.
- **Fan-out:** one unwind terminating selected parent and root creates one
  capture/value attempt and two links. A selected engine/sysop origin already
  closed as errored still receives its terminal link. An unhandled root reuses
  the unwind capture/CID and creates no duplicate root-error value.
- **Stack and identity:** normal `ContextRef` plus `ThrowSite` reconstructs the
  stack without a frame array; overflow reports `StackIncomplete` with observed
  throw labels/reason. Separate fresh unwinds get monotonic IDs. Rethrow gets a
  new ID and `Rethrow` kind even when its CID matches; CID never implies error
  identity.
- **Producer mapping:** bytecode, native, engine, and future paths preserve
  closed `ErrorSource` and `Fresh/Rethrow`. Bytecode uses live-PC file/line/
  byte range without claiming a column. Existing future error is rethrow;
  newly materialized cancellation is fresh. Native thrown carriers always
  carry kind; direct native uses `NativeCall`, the same carrier inside
  `OpErrorPayload::Vm` uses `EngineCall`. BEP-042 cause fixtures remain
  identical with profiling on/off.
- **Value/admission failure:** denying the Values owner's minimum byte charge
  commits metadata with `Lost(ValueMemoryExceeded)` and valid links. Failure
  while growing the same reservation, copying, encoding, or publishing CAS
  commits `Lost(reason)`. Attempt/dependency failure creates no dangling
  target: a durable terminal span may reference `Lost(reason)`, while an
  uncommitted start emits no link and records `StartUncommitted`. Link failure
  has its own counter and never duplicates the value.
- **CAS:** identical encoded bytes across roles, calls, boundaries, and runs
  create one object. Object durability precedes references; conflicts are
  reported and never overwritten. Large hits rehash outside `publish.lock`,
  and injected put races retry safely without holding the lock for the read.
- **Segmentation and pressure:** sustainable load rolls evidence without loss;
  injected pressure reports the documented reason. Long spans may cross
  segments. Manual capture behavior is unchanged after many rollovers, and
  clearing transient pressure admits later evidence without a lifetime quota
  or latched CCT-only mode.
- **Batch failure:** injected pre-rename CCT/evidence publication failure makes
  the batch final `Lost`, releases reservations/roots, invalidates dependencies,
  and can never commit or double-count later. A blocked store retains only
  fixed health, not a failed-batch queue.

### Resource safety and durability

- All five policy inputs, three reservation classes, every owner denial, and
  each derived-capacity pressure seam have deterministic tests. Total
  accounted memory stays inside the governor plus its allocator margin; tiny
  items pay minimum charges, oversized values become `ValueTooLarge`, Control
  remains protected, and manual work tries General then Manual capacity.
- Active-thread exhaustion reports loss without defeating producer ownership.
  Many quiet boundaries use O(1) publisher file handles and do not reserve a
  full segment each. A long boundary publishes thousands of segments without
  retaining a manifest/object per segment. Streaming query memory scales with
  returned contexts, not invocation/segment count.
- Registration reserves registry/health/control capacity before disk.
  Runtime-capacity failure writes no `run.meta`; store rejection releases
  provisional state and starts profiler-off. Durable metadata has no later
  fallible capacity step. Post-rename metadata fsync failure returns
  `Indeterminate`, keeps profiling off, and retains only the one store-owned
  path.
- At the exact disk limit, minimum-free-space floor, or injected external
  ENOSPC, the failing and queued pre-rename batches become
  `Lost(DiskGuardExceeded)` once, release memory/roots, and latch the shared
  atomic gate before later value work. Permission and other definite I/O
  failures use their closed reasons. Committed artifacts and BAML identity/
  execution are unchanged; no blocked-work list grows.
- An admitted boundary still drains terminal control. Definite pre-rename
  `run.end` failure records `TerminalStorePublicationFailed` and releases
  through `ReleasedIncomplete`; post-rename ambiguity retains the one
  indeterminate slot. New boundaries remain profiler-off while latched.
- Two-process publication cannot oversubscribe byte/free-space guards.
  Checkpoints distinguish queued/in-flight from loss, and all terminal
  equations hold without dangling references or silently open spans. Injected
  span-end and runtime-ID failures reconcile independently from start loss;
  error-capture and terminal-link failures reconcile independently from each
  other.
- Crash tests leave committed segments readable and temporary files ignored.
  Post-rename fsync failure retains exactly one
  `RenamedAwaitingDirSync` candidate under the global lock; no publication
  passes/reuses it, retry commits once, and `run.end` cannot seal early. Reopen
  validates/fsyncs the visible path or reports incomplete.
- Deleting an interior or tail segment is detected using sequence plus
  `run.end` fences. A stalled live reader creates no backend backlog and
  resumes from its own cursor/checkpoint.

### Off, cleanup, and removal

- `BAML_PROFILE=0` constructs no profiler ring, task, consumer, CCT/evidence/
  CAS state, value heap, allocation owner, descriptor, lock, directory, clock
  sample, resolver, copy, or artifact—even across root, LLM, manual-role,
  all-false, error, await, and ordinary/detached-spawn fixtures.
- One injected off session shared by multiple engines creates no process
  profiler state. A fresh injected on session is separate; environment
  mutation or another engine does not reconfigure an existing session.
- Test collection, GC finalizers, and LSP test-registry collect/expand/
  serialize are suppressed inside an on session while a user root remains
  active. Manual capture never lazily activates inactive mode or emits
  profiler-dependent warning/loss.
- Structured and logging-only fixtures work while profiler-off and allocate
  only the independent logger. No legacy host value-capture/default/logs-only
  path remains; CLI/exec/LSP/WASM cross only the logger interface.
- `baml clean` refuses with a shared lease, removes only the exclusive new
  `profiles-v1` root, and never touches legacy history. Disk-blocked sessions
  perform no automatic deletion, retention, sweep, retry, or call-path polling;
  reopen reconciles usage.
- After Phase 7 no production `.bamlprof`, stack segment, raw WASM chunk,
  invocation-shaped profile, or old profile-only plumbing remains. Old
  readability is neither tested nor promised.

### Performance

- Pure call pairs retain their structural size except for the existing flags
  byte; there is no sequence increment. Only awaited calls add the twelve-byte
  timing payload. Repeated waits update one sparse entry; off and ready-inline
  paths allocate none. Phase 0 pins pure/inline/one-wait/many-wait thresholds.
- Spawn adds one checked atomic acquire, one compact generation-tagged handle,
  and one release—no child ID, acknowledgement, writer/file, root capture, or
  waiter. On/off ordinary/detached fan-out benchmarks include slot lookup,
  phase/generation checks, and contention before Phase 2.
- Existing contexts avoid rehashing. Rollover adds no lifetime check and does
  not slow later manual capture as segment count grows; benchmarks cover many
  forced segments, peak memory, throughput, and publication-lock time.
- Definition dependency checks remain O(1) current-epoch/token work; late
  evidence never scans segments or a lifetime key set. Disk-blocked admission
  is one atomic read and performs no value root/copy/encode/hash/enqueue.
- Successful calls do not increment unwind ordinals, walk stacks for error
  selection, or allocate profiler stack traces.
- Off/manual/await/unwind/spawn benchmarks include unavoidable language-ID cost
  but zero profiler-state cost. Suppressed-root benchmarks include only
  `begin_root(SuppressInternal)`, not registry/store/ring/capture work.
- All on-mode, timing, spawn, rollover, blocked-store, off, and suppression
  thresholds are measured in Phase 0 and approved before their dependent phase
  merges.
## 15. Explicitly deferred

- automatic retention;
- CAS garbage collection or refcounts;
- background maintenance scheduling;
- cloud upload and acknowledgement;
- semantic value dedupe or Merkle/chunk DAGs;
- query language and hosted query service;
- exact rows for every invocation;
- post-hoc capture after a call has completed;
- dynamic error-based span selection;
- persistent runtime error identity or cross-unwind identity correlation;
- user-defined general function policies;
- full execution timelines;
- per-instance exact-span self/await timing and durable in-progress await
  accrual before call end;
- per-record global or thread sequence numbers;
- separate profiling boundaries/runs for detached descendants;
- session-scoped or dynamically reparented background-task lanes;
- large-value streaming/spill until proven;
- source/local-variable snapshots;
- per-execution synchronous durability (durability stronger than the
  batched `publish_interval` window);
- profiling in `fork()` children; and
- old artifact migration/readers.

## 16. Decision traceability and handoff gates

The product decisions are closed. This table identifies their authoritative
contracts; it does not restate them.

| Decision | Pinned result | Authoritative section |
|---|---|---|
| Manual capture | `$id = id` selects one callee; accumulated `LocalId.capture` overrides apply after the callee's base role policy | Section 4 |
| Runtime identity | Call IDs and `baml.id.current/new/set` remain language behavior; `baml.id.set` is not a capture selector | Sections 4 and 11 |
| Error evidence | One capture per applicable unwind, classified `Fresh` or `Rethrow`, with one value attempt and fan-out terminal links | Section 7.4 |
| Spawn/detach | Every descendant stays in the parent's profiling boundary; detach changes cancellation only | Section 5.1 |
| Timing | CCT stores additive inclusive, direct-child, and await components and derives runtime self without event sequencing | Section 5.4 |
| Runtime limits | Memory limits transient work, not boundary lifetime; completed facts roll into immutable segments | Sections 6–8 |
| Disk exhaustion | New profiling data fails closed without deleting committed data or affecting BAML execution | Sections 7.2 and 8.2 |
| Profiler off | One shared off session owns no profiler resources; identity and logging still work | Section 11 |
| Old artifacts | No migration or compatibility reader; production legacy paths are removed after the oracle window | Section 12 |
| Execution identity | An execution is the thread tree under a parentless root thread; `ExecutionId` is the root `ThreadRef`; no `BoundaryId` in durable formats | TASK/profiling-backend-streams.md §2 |
| Layout | One stream of meta/data segments per process; batched publication through a per-session stream writer | TASK/profiling-backend-streams.md §3–5 |

Phase 0 must produce these engineering artifacts before dependent phases land:

1. complete canary producer, suspension, unwind, internal-root, value-capture,
   logging, and `$id` call-kind inventories;
2. measured production defaults for process memory and both disk policies,
   plus the versioned sizing algorithm, minimum charges, reserve borrowing
   rules, and representative derived outputs;
3. canonical encodings and cross-platform golden fixtures for durable IDs,
   records, `ContextKey`, and `ValueCid`; and
4. recorded on/off baselines plus approved performance-regression thresholds.

These gates do not authorize policy reinterpretation. Any implementation
finding that changes capture policy, record shape, ordering, failure behavior,
or a resource guarantee must amend this document and its acceptance tests
before code lands.
