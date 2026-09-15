# Btel producer phase 1

Status: producer implementation completed 2026-09-15. This phase follows removal of the old
tracing pipeline. It defines and validates producer-side invocation and logical-thread telemetry
without building the record buffer, processor, aggregator, publisher, query path, or playground
path.

## Goal

Establish the minimum producer model needed to observe BAML execution while preserving the current
VM call fast path. At the end of this phase:

- every executable invocation has one of three modes: `Hidden`, `Timing`, or `Span`;
- observed invocations complete exactly once with timing, await duration, outcome, and call-path
  information;
- spans and logical threads use one telemetry identity namespace and retain explicit ancestry;
- timing-only BAML invocations can be promoted to spans at completion;
- the state required by return, unwind, cancellation, native continuation, and sys-op suspension is
  present in existing VM/engine ownership structures;
- hidden native calls perform essentially no telemetry work; and
- the incremental cost of the timing path is measured against the optimized VM baseline.

This phase does not send records through a ring buffer. Tests may inspect logical producer events
through a test-only recording seam. The production transport representation is a later phase.

Core transport-independent vocabulary lives in the new `btel_types` crate: typed telemetry and
call-path identities, raw clock instants/durations, await duration, invocation mode/outcome, and
spawn ancestry. VM-specific producer state and events that still contain `HeapPtr` or `Value`
remain in `bex_vm`. Later layers should continue the `btel_*` crate family without moving VM heap
ownership into the transport boundary.

## Executable callable categories

There is no separate "compiler/runtime helper" function kind. The runtime has three executable
`FunctionKind`s:

| Kind | Concrete examples | Default telemetry |
| --- | --- | --- |
| `Bytecode` | a user function such as `HostReadNamed`; a BAML wrapper such as `string.length`; `http.fetch`; an AI function such as `HostExtract` | `Timing`, except AI functions default to `Span` |
| `Native` | `$rust_function` implementations such as `string.char_count`, `baml.sys.argv`, and `baml.json.from_string` | `Hidden` |
| `SysOp` | `$rust_io_function` implementations such as `baml.fs.exists`, `baml.fs.open`, and `baml.http._fetch` | `Hidden` |

`NativeUnresolved` is a serialized/load-time placeholder that becomes `Native`; it is not an
executable telemetry category.

Closures, bound methods, and generic-function values wrap or resolve to an underlying function and
use that function's policy. A host closure is a host boundary routed through a sys-op; it is not a
fourth `FunctionKind`. Any host boundary that should be visible must opt in explicitly.

The consequence of using real runtime kinds is deliberate: a bytecode wrapper is `Timing` even if
it delegates to a hidden native/sys-op. If a standard-library bytecode wrapper should disappear, it
must carry an explicit hidden declaration rather than relying on an invented helper category.

The source language may expose a declaration such as:

```baml
// baml:telemetry=hidden
// baml:telemetry=timing
// baml:telemetry=span
```

The exact syntax is not locked by this document. Phase 1 needs equivalent compiler metadata and a
test override. Native and sys-op policy is immutable. Only BAML/bytecode functions have a runtime-
mutable policy.

## Invocation modes

```text
Hidden
  `-- no event; cannot promote

Timing
  |-- TimingEvent
  `-- LateSpanEvent     (when current completion policy promotes it)

Span
  `-- SpanEvent
```

### Hidden

Hidden means unobserved. A hidden invocation performs no telemetry clock read, allocates no
`FrameTelemetry`, allocates no identity, performs no promotion-policy check, and has no independent
timing or await accounting. `Hidden -> LateSpanEvent` is unsupported.

Hidden callees leave the logical thread's active telemetry identity and active call path
unchanged. A visible descendant can still execute beneath a hidden callable.

### Timing

A timing invocation is observed from entry but is anonymous. It records duration, self-await
duration, outcome, and call path. It has no `TelemetryId` and does not become the active
telemetry parent.

At completion, a timing-only BAML invocation reads its function's current policy. Policy zero adds
no promotion or capture behavior. When the policy promotes the invocation, the producer allocates
an identity and emits `LateSpanEvent` instead of `TimingEvent`; it never emits both.

### Span

A span is observed and individually identified from entry. It allocates an identity, records its
current active telemetry parent, becomes the thread's active telemetry node, and emits an entry
announcement. Completion emits one `SpanEvent` and restores the previous active telemetry parent.

AI functions are compiler-selected spans with full input, output, and escaping-error capture.
Other span declarations independently select input, output, and error capture.

Input capture is decided and performed at entry. It does not need to survive as a frame flag.
Output and error decisions must survive until completion. By default, effective capture is the OR
of call-site requirements and the applicable function policy. A future policy may add explicit
force-override semantics; phase 1 does not.

Value-copy cost is outside the timing-path performance budget. The clock is read before copying a
completion value, so value serialization time is not charged to the invocation.

### Late span

A late span was fully timing-observed from entry but became individually identifiable only at
completion. `LateSpanEvent` is a separate event variant, so no `LATE_PROMOTED` or
`AWAIT_UNAVAILABLE` flag is needed.

Late promotion:

- allocates a new `TelemetryId` at completion;
- uses the thread's current active telemetry node as its parent;
- does not temporarily become the active parent;
- does not rewrite already-established descendant or spawned-thread relationships; and
- can include entry-time values only when an independent entry-time policy requested their capture.

IDs and timestamps do not imply ordering. A late span can receive a larger identity than a
descendant that began after it.

## Identity and ancestry

Threads and captured function invocations share one runtime-wide `u64` `TelemetryId` namespace.
The event variant identifies the kind of node; no kind bits are reserved in the ID.

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct TelemetryId(NonZeroU64); // 8 bytes

const _: () = assert!(size_of::<TelemetryId>() == 8);
const _: () = assert!(size_of::<Option<TelemetryId>>() == 8);
```

The allocator never returns zero. `Option<TelemetryId>` is also eight bytes and represents a root
with no parent without introducing an in-memory sentinel. A later packed transport may encode
`None` as zero.

The inner field remains private. Production producer code has no constructor from an integer. Only
the allocator can create an ID; other runtime modules can inspect its raw value only when crossing
a wire or diagnostic boundary. A raw checked constructor exists only in unit-test builds:

```rust
impl TelemetryId {
    #[cfg(test)]
    #[inline(always)]
    const fn from_raw_for_test(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    #[inline(always)]
    const fn get(self) -> u64 {
        self.0.get()
    }
}

impl WorkerTelemetryIdAllocator {
    #[inline(always)]
    fn allocate(&mut self) -> TelemetryId {
        // The out-of-line cold target makes this exhaustion edge unlikely.
        if self.next == self.end {
            return self.refill_and_allocate();
        }

        // Range refill guarantees a nonzero, non-wrapping reserved range.
        let raw = self.next;
        self.next += 1;
        debug_assert_ne!(raw, 0);
        TelemetryId(unsafe { NonZeroU64::new_unchecked(raw) })
    }

    #[cold]
    #[inline(never)]
    fn refill_and_allocate(&mut self) -> TelemetryId {
        self.refill_range(); // reserves a nonzero, non-wrapping range
        self.allocate()
    }
}
```

This representation has the same size, alignment, loads, stores, comparisons, and calling
convention as the wrapped integer. The hot allocation path performs no zero check; range refill
owns the overflow and zero checks and is `#[cold]` plus `#[inline(never)]`, making exhaustion the
unlikely edge without relying on unstable branch-hint intrinsics. A future decoder must validate
an incoming integer before constructing its decoder-side typed identity; that does not justify
exposing a constructor to the producer runtime. IDs deliberately implement no arithmetic traits.

```text
Thread #100
  `-- Function #101
        `-- Function #102
              `-- Thread #103
                    `-- Function #104
```

Every identified node has:

```text
id: TelemetryId                // 8 bytes
parent_id: Option<TelemetryId> // 8 bytes
```

Only root threads have no parent. A logical thread always receives an ID. A function receives an
ID only when it is a span from entry or becomes a late span. A timing-only completion has no ID.

Long-lived execution workers reserve ranges from one runtime-wide atomic allocator and allocate
from those ranges locally. A logical thread does not own an ID range because logical threads may be
short-lived and may migrate. Allocation gaps and numerical order carry no semantics.

The existing host `sys_types::CallId` remains separate. It identifies host requests for
cancellation and duplicate-call detection; it is not a telemetry graph identity.

Cross-thread ancestry always passes through an explicit thread node. A span's parent can therefore
be its owning thread or another span on the same thread. A spawned thread's parent is the spawning
thread's active telemetry node.

## Call paths

`CallPathId` is separate from `TelemetryId`. A telemetry ID identifies one thread or captured
invocation; a call-path ID identifies an aggregation bucket shared by repeated invocations along
the same recorded call path.

```rust
#[repr(transparent)]
struct CallPathId(u32); // 4 bytes; zero is the engine root

const _: () = assert!(size_of::<CallPathId>() == 4);
```

Call path zero is the engine's sentinel root. Each observed invocation resolves an engine-owned
call path from:

```text
parent_call_path
actual_caller_function
caller_pc
callee
edge_kind          // synchronous call or spawn boundary
```

The actual caller and PC remain meaningful through hidden wrappers. For example, a visible callback
invoked by hidden `map` is parented under the nearest visible call path, while its source site still
belongs to `map`.

Call-path lookup returns a `CallPathId` and produces `CallPathDefined` only when it inserts a new
call path. Phase 1 must exercise cache-hit and miss behavior, but the less-than-10-ns timing target
applies to the cache-hit producer path. Call-path misses are measured and reported separately.

The reference design's direct-recursion folding remains in phase 1: only immediate synchronous
recursion with no hidden intermediary may reuse its active call path. Such an invocation sets
`REENTRY`; indirect recursion, recursion through a hidden callable, and spawn boundaries do not.

## Frame and logical-thread state

Telemetry state belongs to the existing invocation frame or continuation ownership. The current
32-byte candidate is:

```rust
#[repr(C)]
struct FrameTelemetry {
    entered_at: ClockInstant,       // 8 bytes
    saved_parent_id: TelemetryId,  // 8 bytes
    await_duration: AwaitDuration, // 8 bytes
    saved_call_path: CallPathId,   // 4 bytes
    flags: u8,                     // 1 byte
    // 3 bytes alignment padding
}

const _: () = assert!(size_of::<FrameTelemetry>() == 32);
const _: () = assert!(size_of::<Option<FrameTelemetry>>() == 32);
```

`Option<FrameTelemetry>` uses the `TelemetryId` zero niche and remains 32 bytes, so the flags do
not need an artificial `PRESENT` bit. A layout assertion must enforce this. The full-width await
duration uses bytes that would otherwise be alignment padding: changing it to four or two bytes
would still leave `FrameTelemetry` at 32 bytes. The current measured enclosing sizes are:

| Layout | `BytecodeFrame` | `NativeFrame` | `Frame` |
| --- | ---: | ---: | ---: |
| Optimized pre-producer baseline | 64 B | 24 B | 64 B |
| Implemented phase 1 producer | 96 B | 24 B | 96 B |
| Rejected 40-byte bytecode payload | 104 B | 24 B | 104 B |

Native invocations are hidden in the implemented phase, so `NativeFrame` does not reserve optional
telemetry storage.

The control byte contains only invocation state that must survive to completion:

```text
SPAN
CAPTURE_OUTPUT
CAPTURE_ERROR
REENTRY
```

`SPAN` distinguishes an entry-selected span from timing. There are no flags for presence, span
kind, input capture, announcement, late promotion, or await availability.

The logical thread owns:

```rust
struct ThreadTelemetry {
    active_id: TelemetryId,       // 8 bytes
    active_call_path: CallPathId, // 4 bytes
}
```

`active_id` begins as the thread's own ID. `active_call_path` begins at the root or structural
spawn call path. Hidden and timing-only invocations do not alter `active_id`. Spans save it in
`saved_parent_id`, replace it with their new ID, and restore it at completion.

The candidate omits an invocation's own ID and call-path ID from the frame. At completion:

- `thread.active_id` is the exiting entry-selected span's own ID; and
- `thread.active_call_path` is the exiting observed invocation's own call-path ID.

An entry-selected span therefore uses the following state transition:

```text
enter:
    frame.saved_parent_id = thread.active_id
    thread.active_id = allocate_telemetry_id()

complete:
    own_id = thread.active_id
    thread.active_id = frame.saved_parent_id
    emit SpanEvent { id: own_id, parent_id: frame.saved_parent_id, ... }
```

`thread.active_id` is ordinary logical-thread-owned memory. Reading or restoring it is a scalar
eight-byte load or store: it is not atomic, thread-local-storage access, reference counting, a
lookup, or an allocation. More importantly, only `Span` performs the install/restore pair:

| Mode | Entry identity work | Completion identity work | `active_id` writes per invocation |
| --- | --- | --- | ---: |
| `Hidden` | None | None | 0 |
| `Timing` | Retain the current parent ID | Emit `TimingEvent`, or allocate an event-only ID for `LateSpanEvent` | 0 |
| `Span` | Allocate an ID, save the parent, install the new ID | Read the installed ID and restore the parent | 2 |

A late-promoted ID is never installed into `thread.active_id`: promotion occurs after all
descendants have completed, so there is no future child that could inherit it. `Timing` still
restores `active_call_path` at completion because timing invocations participate in call-path
aggregation; that is a separate four-byte state transition from telemetry identity. `Hidden`
changes neither active value.

For example, with thread ID `T1`:

```text
physical invocation stack                 thread.active_id

Thread T1                                 T1
  Timing A                                T1   (unchanged)
    Hidden helper                         T1   (unchanged)
      Span B                              B1   (installed by B)
        Timing C                          B1   (unchanged)
```

Completion proceeds from the top:

```text
C completes      active_id stays B1
B completes      emits id=B1, parent=T1; active_id becomes T1
helper completes no telemetry work; active_id stays T1
A completes      active_id stays T1
```

If `A` is late-promoted, it allocates `A1` only while producing
`LateSpanEvent { id: A1, parent_id: T1, ... }`; `active_id` remains `T1`. The already-completed
`B1` remains parented to `T1`, because late promotion does not rewrite descendant ancestry.

Nested entry-selected spans restore in strict LIFO order:

```text
Thread T1             active_id = T1
  enter Span A        active_id = A1, A.saved_parent_id = T1
    enter Span B      active_id = B1, B.saved_parent_id = A1
    complete Span B   emit B1 -> A1; active_id = A1
  complete Span A     emit A1 -> T1; active_id = T1
```

Spawning does not mutate the parent's active ID. A child thread created while `A1` is active gets
its own thread ID `T2` with parent `A1`; its independent logical-thread state starts with
`active_id = T2`.

The frame does not also store `own_id`. Both values are needed at completion, so adding it would be
a real additional eight-byte field: `FrameTelemetry` would cross from 32 to 40 bytes. Overlaying it
with `saved_parent_id` is invalid because the parent is needed to restore the logical thread.

A timing invocation never has an own ID and does not change `thread.active_id`. If it is promoted,
it allocates its ID at completion and uses `saved_parent_id` as the late span's parent.

This depends on a required invariant: before an invocation completes, every synchronous observed
descendant has completed and restored that invocation's active state. Hidden invocations do not
alter the state. Spawned threads own separate state. Return, unwind, native continuation, sys-op,
and cancellation tests must prove this invariant before the 32-byte representation is accepted.
Unwind and cancellation must complete and remove frames strictly from the top down. A visible
native continuation carries its `FrameTelemetry` with the continuation; after its callback exits,
the callback has restored the native invocation's ID before the continuation completes or yields
again. The logical-thread state travels with the logical thread across suspension and worker
migration.

## Clock

The producer distinguishes an instant read from an elapsed amount even though both are represented
as raw `u64` counter values:

```rust
#[repr(transparent)]
struct ClockInstant(u64); // 8 bytes

#[repr(transparent)]
struct ClockDuration(u64); // 8 bytes

const _: () = assert!(size_of::<ClockInstant>() == 8);
const _: () = assert!(size_of::<ClockDuration>() == 8);
```

`ClockInstant` values may be subtracted only through a saturating operation that returns
`ClockDuration`. Durations may be accumulated with saturating addition. This prevents accidental
mixing of absolute counter reads and elapsed amounts without adding storage or runtime cost.

The producer uses the fastest unfenced constant-rate hardware counter available:

- AArch64: `CNTVCT_EL0`, with `CNTFRQ_EL0` read during initialization;
- x86-64 with invariant TSC: `RDTSC`, with its rate obtained or calibrated outside execution; and
- an OS monotonic fallback where a suitable hardware counter is unavailable.

No fence, per-core correction, synchronization probe, thread pinning, conversion, or lazy
initialization runs on invocation entry or completion. This intentionally prioritizes producer
cost over timestamp accuracy. The measured Apple M2 Max counter frequency is 24 MHz, giving a
41.67 ns tick resolution.

Events retain the raw inner values. They are not nanoseconds: on the measured M2 Max, one tick is
exactly 125/3 ns. Converting on the producer would add work and either require rational arithmetic
or discard precision.

Each run carries clock metadata containing the source and a rational nanoseconds-per-tick rate.
That metadata makes stored tick values interpretable outside the producer. A later publication
format may also carry a counter/wall-clock anchor when absolute wall time is required.

Duration uses saturating subtraction, so a cross-core backward interval becomes zero. A zero
duration retains its real outcome and await measurement. Duration thresholds are pre-resolved to
`ClockDuration` before evaluation.

## Await accounting

Invocation-local await accounting retains the clock's full raw duration:

```rust
#[repr(transparent)]
struct AwaitDuration(ClockDuration); // 8 bytes

const _: () = assert!(size_of::<AwaitDuration>() == 8);

impl AwaitDuration {
    const ZERO: Self = Self(ClockDuration(0));
}
```

Using the raw 64-bit duration avoids conversion and quantization on the wait path. It also avoids
representation choices driven by a future transport: at 24 MHz it spans roughly 24,000 years, and
even at 3 GHz it spans roughly 195 years. Saturating addition handles the theoretical overflow.

Each observed invocation initializes `frame.await_duration` to zero. For each semantic wait owned
by that invocation, the runtime reads the clock before suspension and again before resuming or
terminating, clamps the raw difference with saturating subtraction, and performs one raw
saturating addition:

```text
frame.await_duration =
    frame.await_duration.saturating_add(elapsed_await)
```

Only the suspension-owning observed frame is updated, so the work remains O(1) in call-stack depth.
A child's waits accumulate on the child, not its caller. The producer therefore emits self-await
directly. A later aggregator may derive inclusive await by adding synchronous child contributions.
Spawned-thread waits belong to the spawned thread and are not synchronous child contributions. No
active-stack walk occurs at await time.

No unit conversion runs on ordinary invocation entry, semantic-wait completion, or invocation
completion.

Runtime representation and publication representation are separate decisions. Fixed-width
100 us encodings have the following ranges:

| Published width | Maximum duration | Assessment |
| --- | ---: | --- |
| `u16` | 6.5535 seconds | Too short for routine HTTP and model waits |
| 24 bits | 27 minutes 57.7215 seconds | Compact, but still imposes a relatively short hard limit |
| `u32` | About 4.97 days | Plausible simple fixed-width publication format |

Making a `u16` last longer requires coarser units: 1 ms reaches only 65.535 seconds, 10 ms reaches
about 10.9 minutes, and 100 ms reaches about 1.82 hours while discarding the accepted 100 us
accuracy.

A later publication format can quantize the raw duration and use a `u32`, an unsigned varint, or a
small value plus an escape encoding. For example, an unsigned LEB128 count of 100 us units uses up
to three bytes through about 209.7 seconds and four bytes through about 7.46 hours. That conversion
belongs in the cold processor/publication path. Phase 1 does not force the future producer ring
buffer to use the same representation; its slot width and conversion cost must be benchmarked as
part of the deferred record-buffer design.

Only semantic waits count: unresolved-future waits and asynchronous sys-op waits. Ready fast paths,
cooperative scheduling, GC safepoints, and processor/transport backpressure do not count. An await
ending in cancellation contributes through its cancellation time.

Phase 1 records await duration only. It has no await-count counter, await-count frame state, or
await-count wire field.

## Policies

The policy table contains immutable, pre-resolved policies. Policy zero means no policy-driven
promotion or additional value capture. Duration thresholds are stored as `ClockDuration` values.

BAML/bytecode functions have a runtime-mutable `u16` current policy ID associated directly with
their engine-local function metadata. Policy publication must be synchronized, and completion
reads the current ID. It does not snapshot the policy ID in `FrameTelemetry`. An update may affect
an invocation that began before the update and completes afterward.

Native and sys-op functions use immutable policy. Hidden is their default, so their common path
does not perform a policy lookup. Phase 1 implements that hidden default. The first explicitly
visible native/sys-op will add fixed invocation metadata to its existing continuation state; it
does not require dynamic policy lookup or a pending-sys-op policy owner.

Every completion path for a mutable-policy invocation must be able to recover the underlying
BAML function. The current runtime already preserves that identity:

- bytecode return and unwind have the existing frame's function reference;
- cancellation owns the remaining frames before teardown;
- a native call or continuation leaves its bytecode caller below it;
- a callback into BAML receives its own bytecode frame; and
- sys-op suspension drains arguments but preserves the calling bytecode frame in the VM.

No pending-sys-op policy-owner field is required solely for dynamic policy lookup.

## Logical producer events

Phase 1 defines logical event variants. Exact transport tags and packed byte layouts are deferred
until the record-buffer phase.

```text
ThreadStarted {
    id, parent_id, spawn_call_path, started_at
}

ThreadCompleted {
    id, started_at, completed_at, outcome
}

SpanStarted {
    id, parent_id, call_path, entered_at, captured_inputs
}

TimingEvent {
    call_path, entered_at, exited_at, await_time, outcome, reentry
}

SpanEvent {
    id, parent_id, call_path, entered_at, exited_at,
    await_time, outcome, reentry, captured_values
}

LateSpanEvent {
    id, parent_id, call_path, entered_at, exited_at,
    await_time, outcome, reentry, captured_values
}

CallPathDefined {
    call_path, parent_call_path, actual_caller, caller_pc, callee, edge_kind
}
```

`SpanStarted` is produced only for spans selected at entry. `TimingEvent`, `SpanEvent`, and
`LateSpanEvent` are mutually exclusive completion variants. Await does not produce a separate
event. Their `await_time` field is the invocation's self-await duration.

Outcome is one of `Ok`, `Errored`, `Cancelled`, or `Exited`. `Errored` means an error escaped that
invocation, not merely that an error occurred beneath it. A locally caught exception leaves the
catching frame open; only frames removed by unwinding complete with `Errored`.

Phase 1 does not define a separate event for a throw that is caught within the same invocation.
That can be added later if a concrete product requirement needs it.

## Lifecycle rules

Observed entry occurs only after call validation and argument preparation succeed. Failed call
setup does not create an invocation requiring completion.

Entry performs, in order:

1. Determine `Hidden`, `Timing`, or `Span` from the callable's default, call-site declaration, and
   applicable entry policy.
2. Return immediately for `Hidden`.
3. Resolve the call path from the prior active call path, actual caller function/PC, and callee.
4. Resolve and perform requested input capture while arguments are available.
5. For `Span`, allocate an identity, save the active parent, and install the new active identity.
6. Initialize the frame's await duration to zero, read the entry clock, install `FrameTelemetry`,
   and make the new call path active.
7. Produce `SpanStarted` for `Span`.

Completion performs, in order:

1. Read the exit clock before copying output or error values.
2. Determine the completion outcome.
3. Recover the current function policy for BAML/bytecode.
4. For `Timing`, evaluate late promotion using the current policy and allocate an identity only if
   selected.
5. OR call-site output/error requirements with the current policy and copy the applicable value.
6. Produce exactly one completion variant.
7. Restore the saved call path and, for entry-selected spans, the saved telemetry parent.
8. Remove or finish the invocation frame/continuation.

Return, unwind, cancellation, and `baml.sys.exit` all use this completion operation. Completion is
explicit; it does not rely on `Drop`, which lacks outcome and value context.

Native continuations move their fixed telemetry ownership when the current implementation pops and
rebuilds the continuation frame. A callback does not complete and restart the native invocation.
Sys-op state survives the VM-to-engine handoff and asynchronous wait. Hidden native/sys-op calls
skip this lifecycle entirely.

A logical thread starts when its creation is committed, before scheduling, and completes after all
of its observed invocations have been finalized. Scheduling delay is included in thread duration.
Spawn records the current active telemetry ID as the child's parent. Detachment changes
cancellation behavior, not ancestry.

## Phase 1 implementation order

1. Add producer types, compile-time layout assertions, clock initialization/read helpers, and
   logical event definitions.
2. Add `ThreadTelemetry`, thread identity allocation, and thread start/completion lifecycle.
3. Add `Timing` to direct bytecode entry/return, with a test-only event recorder.
4. Cover bytecode unwind, cancellation, explicit exit, and root completion.
5. Add entry-selected spans, active-parent restoration, and mutually exclusive span completion.
6. Add dynamic BAML policy lookup and late promotion.
7. Add O(1) invocation-local self-await accounting.
8. Cover native continuations, callbacks, sys-ops, and worker migration.
9. Run lifecycle, identity, error/stack, cancellation, spawn, GC, logging, and call-overhead tests;
   inspect optimized assembly for hidden and timing paths.

Each step must preserve the optimized exact-call/direct-return dispatch. The phase does not require
a ring buffer to validate the state machine.

## Implemented performance result

The preserved one-million-call workload was run with `DIVAN_MAX_TIME=1` before and after the
producer implementation:

| Build | Fastest | Median |
| --- | ---: | ---: |
| Optimized pre-producer baseline | 36.45 ms | 37.00 ms |
| Phase 1 producer | 41.31 ms | 41.80 ms |
| Increment per complete call | 4.86 ns | 4.80 ns |

The workload, benchmark filter, and time setting were unchanged. The measured producer path is
below the 10 ns incremental target. Value copying, call-path misses, event transport, and
backpressure remain outside this benchmark and this phase.

## Acceptance criteria

- Hidden native/sys-op invocations perform no telemetry clock read, allocation, policy lookup, or
  state mutation.
- Every successful observed entry produces exactly one mutually exclusive completion, including
  return, escaping error, cancellation, and explicit exit.
- A caught error does not complete the catching invocation.
- Timing invocations allocate no telemetry identity unless promoted.
- Late promotion emits `LateSpanEvent`, retains complete timing/await/outcome data, and does not
  rewrite descendant ancestry.
- Span and thread ancestry is constant-time and correct across hidden wrappers, callbacks, spawn,
  suspension, migration, unwind, and cancellation.
- Await accounting performs constant work per suspension/resumption and no active-stack walk.
- `Option<FrameTelemetry>` is 32 bytes and the enclosing frame sizes match layout assertions on
  supported 64-bit native targets.
- The hidden path has no material regression against the current optimized baseline.
- The timing path targets less than 10 ns incremental cost per complete enter/exit pair on the
  primary benchmark machine, excluding value copying, call-path-table misses, test recording, and
  later transport backpressure. Report median and minimum from interleaved runs with identical
  validated workloads.

## Deferred

- record-buffer format, capacity, rollover, wakeup, and backpressure;
- processor, aggregation, self-time/inclusive-await derivation, snapshots, and publication;
- production value serialization and storage;
- exact source syntax for telemetry annotations;
- production policy-table publication and its control-plane update API;
- fixed-policy opt-in for the first semantically visible native/sys-op;
- compiler and call-site metadata for explicit hidden/span/capture declarations;
- cross-VM call-path interning (phase 1 emits globally unique definitions and caches within a VM;
  the processor may initially deduplicate equivalent definitions);
- policy force-override behavior;
- caught-throw events;
- await count;
- query, CLI, LSP/playground, and cloud integration; and
- anonymous/user-created spans beyond captured function invocations.
