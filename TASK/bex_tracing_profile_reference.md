# BEX Tracing and Profiling: Consolidated Reference

**Status:** consolidated reference for the tracing / profiling work landed into `canary` for `github.com/BoundaryML/baml`.

**Audience:** a new engineer, reviewer, or product/infra person who needs to understand tracing/profile without first reading the original ticket, review docs, reconciliation notes, or Antonio's private design pages.

**How to read this document:** start with the simple model, then use the deeper sections as a reference. The document intentionally describes the final system and its final reasons. It does not retell every temporary bug or branch conflict; when a bug led to a better invariant, this document states the invariant directly.

---

## 1. Quick introduction

BEX tracing/profiling gives every BAML runtime function call a precise identity, records call lifecycles efficiently, and writes those records into `.bamlprof` artifacts that can later be reconstructed into call trees, flamegraphs, timelines, profile diffs, and Studio traces.

The final system combines two workstreams:

1. **Runtime identity and `$id`**: every runtime call can be named by a reversible ID, and BAML code can read or override the current call's ID through `$id` / `baml.id.*`.
2. **Profiling ring and `.bamlprof` artifact**: runtime events are pushed through a low-overhead segmented ring and drained by a background consumer into protobuf files.

The central rule is:

```text
Events carry compact IDs.
Headers carry metadata.
Consumers join them later.
```

A runtime event should not carry full source metadata, semantic hashes, rendered trace spans, large payloads, or expensive strings. It should carry small numbers:

```text
thread_id
call_id
parent_call_id
function_id
timestamp_ns
```

The `.bamlprof` header explains those numbers:

```text
process_id
engine_id
program_id
started_at_epoch_ns
function_id -> function metadata
```

A renderer or cloud ingest process joins them later:

```text
CallFunction(function_id=42)
  + header.function_table[42]
  -> user.ExtractResume at resume.baml:...
```

There are three identity layers. Keeping them separate is the main mental model.

```text
Runtime call identity:
  Which exact invocation happened?
  process_euid + engine_id + thread_id + call_id

Function metadata identity:
  Which function definition did that call execute?
  function_id -> function table row

Semantic version identity:
  Which source/version/semantic hash did that definition represent?
  source snapshot + definition key + revision + BEP-053 lanes
```

The VM owns call identity. The VM mints `call_id`s, tracks the current call, implements `$id`, and emits hot-path raw records. The ring moves those records cheaply. The background consumer writes `.bamlprof`. Studio/cloud/tools reconstruct meaning later.

---

## 2. The simple version, explained for a 16-year-old

Imagine BEX is a school.

A **process** is the school building for one day. It gets a unique building ID so records from different days do not mix.

An **engine** is one classroom inside the building. One process can run multiple classrooms.

A **BEX thread** is one group project in a classroom. The main program starts one group project. `spawn` starts another group project.

A **function call** is one student taking a turn in that group project. Every turn gets a local number: `call_id = 1`, `2`, `3`, and so on. Every group project starts its own count from 1.

A **function** is the kind of work the student is doing: `main`, `ExtractResume`, `classifyEmail`, `baml.sys.exit`, a spawn closure, a watch filter, a sysop, etc. Instead of writing the full function name on every note card, BEX writes a small number called `function_id`. The class roster in the header explains that number.

A **CallRef** is the full address of one student turn:

```text
school building + classroom + group project + turn number
process_euid  + engine_id + thread_id     + call_id
```

A plain `call_id = 1` is not enough. Every group project has a first turn. The full `CallRef` is enough to paste into a CLI, log line, or Studio URL and recover the exact runtime call.

A **`.bamlprof` file** is the school's archive notebook. First it writes the class roster, then it writes tiny note cards:

```text
StartThread     "a group project started"
CallFunction    "a student started a turn"
SetFunctionId   "this turn's user-visible $id was overridden"
EndFunction     "the turn ended"
EndThread       "the group project ended"
Heartbeat       "the archive writer is alive"
```

A **ring buffer** is the conveyor belt between the classroom and the archive office. The VM drops small cards onto the conveyor belt and keeps teaching. A background archivist takes cards off the belt and writes the notebook. If the archivist falls behind, the belt links on more belt segments instead of dropping cards or stopping the class.

A **trace UI span** is the cleaned-up presentation for humans. It may hide boring helper calls. But the profiling stream still records real runtime calls because timing, parent edges, `$id`, cancellation, and reconstruction need the full structure.

A **semantic hash** is not a call ID. A call ID says, "Which exact turn was this?" A semantic hash says, "Which version of the assignment instructions was this?" Those are different questions.

The frame to keep in your head:

```text
CallRef        = exact runtime invocation
function_id    = compact key into this artifact's function table
semantic hash  = source/version identity, joined later
```

---

## 3. The system in layers

The system has seven layers. Each layer has one job.

```text
1. Compiler / Program
   Builds the compiled BAML program and function metadata.

2. Engine setup
   Allocates process/engine/program scope and registers function metadata.

3. VM execution
   Runs bytecode/sysops/native calls, mints call IDs, tracks current call,
   implements $id, and emits raw records.

4. Producer hot path
   Writes compact raw records into the current ring.

5. Segmented SPSC ring
   Buffers raw records per (engine, OS thread), growing by linked segments.

6. Background consumer
   Drains rings, transcodes raw records to protobuf DiskEventV1, and writes
   one per-engine .bamlprof file.

7. Cold readers/renderers
   Read .bamlprof, group/sort/reconstruct calls, join to metadata, and render
   flamegraphs/timelines/diffs/Studio views.
```

A shorter version:

```text
Compiler builds the dictionary.
VM writes small numbers.
Ring moves the numbers cheaply.
Consumer writes the file.
Renderer joins numbers to meaning.
```

### 3.1 Hot path versus cold path

The **hot path** runs on every call. It must stay small.

Hot path should avoid:

```text
string allocation
semantic hash computation
protobuf encoding
table scans
locks
blocking I/O
engine round-trips per call
global atomics per call when avoidable
```

The **cold path** happens after capture. It can parse protobuf, sort events, build trees, aggregate timing, join semantic metadata, and render UI.

The architecture pushes expensive work to the cold path.

### 3.2 Self-describing does not mean bloated

Antonio's v2 design calls the stream **self-describing**. That means the artifact contains enough information to reconstruct itself without external mutable state.

It does **not** mean every event repeats every identity field.

```text
Header-only:
  process_id
  engine_id
  program_id
  function table

Per event:
  thread_id
  call_id
  parent_call_id
  function_id
  timestamp_ns
```

The file is self-describing because the header and events together are enough.

### 3.3 Profiling stream versus legacy trace stream

The profiling stream records structural runtime calls.

The legacy trace stream is user-visible tracing semantics.

Those are related but not identical. Some calls should be hidden from user-visible `@trace` spans but still recorded structurally. That is why identity and profiling are not implemented as "only visible spans get IDs."

---

## 4. Core vocabulary

**BEX runtime / VM**: the execution layer that runs compiled BAML programs.

**Runtime call**: one invocation of a function at runtime. If `foo` runs 100 times, there are 100 runtime calls.

**Function definition**: the code definition that can be invoked many times. It has metadata such as FQN, source span, kind, and semantic join data.

**BEX thread**: a logical runtime thread. Root execution has one. `spawn` creates more. This is not the same as an OS thread.

**OS thread**: a real operating-system thread. Rings are per `(engine, OS thread)` because producers run on OS threads, but logical BEX threads can migrate across OS threads.

**call_id**: a BEX-thread-local counter for runtime calls. It starts at 1 for each BEX thread.

**function_id**: a compact per-artifact key into the function metadata table. It is not a hash and not stable across recompiles.

**CallRef**: a reversible string encoding of `process_euid + engine_id + thread_id + call_id`. This is the default `$id`.

**RuntimeId**: the value behind `$id`. It is either a default `CallRef` or a UUID override minted by `baml.id.new()`.

**Span**: a user-facing tracing unit. Spans and runtime calls overlap, but a call can exist in the profiling stream without a visible span.

**`.bamlprof`**: the profiling artifact: one length-delimited header followed by length-delimited protobuf events.

**SPSC ring**: single-producer, single-consumer ring. In this system, it is segmented and lossless-by-growth.

**Hot producer**: the VM/runtime path that emits raw records.

**Cold consumer**: the background thread that drains rings and writes `.bamlprof`.

---

## 5. Runtime identity: the quad

Runtime identity is four nested scopes:

```text
process_euid -> engine_id -> thread_id -> call_id
```

### 5.1 `process_euid`

Generated once per process lifetime. It is effectively unique for this process execution.

It exists because OS PIDs are reused and not unique across machines, containers, or time.

### 5.2 `engine_id`

A process-local monotonic engine number.

It exists because one host process can own multiple `BexEngine` instances. Each engine can have `thread_id = 1` and `call_id = 1`, so engine scope is required.

### 5.3 `thread_id`

A logical BEX thread ID inside one engine.

Root invocation gets a thread. Spawned work gets another thread. Thread IDs are scoped by process and engine.

### 5.4 `call_id`

A function invocation ID inside one BEX thread. It is cheap to mint locally.

Each BEX thread starts from `call_id = 1`. Therefore a bare `call_id` is not globally meaningful.

### 5.5 The full identity

A durable runtime call address is:

```text
CallRef = process_euid + engine_id + thread_id + call_id
```

This is the value `$id` returns by default.

### 5.6 Why not a global call counter?

A single global counter would add unnecessary synchronization to every call and would still not tell you which engine/thread structure produced the call. Local counters scoped by header metadata are cheaper and more informative.

---

## 6. Encoded IDs and `$id`

### 6.1 Encoded forms

The final identity types encode to prefixed, versioned, base64url strings:

```text
baml_call_1_<payload>    default call identity
baml_thread_1_<payload>  thread identity
baml_id_1_<payload>      override UUID identity
```

The required property is:

```text
decode(encode(x)) == x
```

Decoders validate prefix, base64, payload length, and version. Typed decoders reject each other's prefixes.

### 6.2 Default `$id`

By default:

```baml
$id
```

returns the current call's `CallRef` encoded as a string:

```text
baml_call_1_<process_euid, engine_id, thread_id, call_id>
```

### 6.3 `baml.id.current()`

Bare `$id` lowers to a runtime read of the current call identity, conceptually:

```baml
baml.id.current()
```

This reads VM state. It should not force a visible trace span.

### 6.4 `baml.id.new()`

Creates a new override UUID and returns:

```text
baml_id_1_<uuid payload>
```

### 6.5 `baml.id.set(id)` and `$id = ...`

A call can install an override:

```baml
function foo() -> string {
  let next = baml.id.new()
  $id = next
  $id
}
```

Direct assignment lowers to `baml.id.set(value)`. This is special lowering; `$id` is not an ordinary mutable local.

The profiling stream emits a `SetFunctionId` record. The name is historical/confusing: it does not change `function_id`. It sets the runtime `$id` override for a specific call. Antonio's earlier docs call this `SetId`; final proto uses `SetFunctionId`.

### 6.6 Override scoping

Overrides are per call frame.

A caller override should survive a nested callee call. A callee override should not overwrite its caller.

Correct mental model:

```text
each call frame has its own optional id override
on call enter: push frame
on $id set: update current frame override
on call exit: pop frame override
```

Example:

```baml
function helper() -> string {
  let h = baml.id.new()
  $id = h
  $id          // helper's override
}

function main() -> string {
  let m = baml.id.new()
  $id = m
  let _ = helper()
  $id          // still main's override
}
```

### 6.7 Why the VM owns call identity

The VM owns call identity because it is the component that knows exactly when calls enter, return, unwind, and read `$id`.

An engine-owned model would require the VM to yield to the engine on every call enter/exit. That is too expensive and conflicts with the ring design.

The final rule:

```text
VM owns per-thread call_id counters.
VM owns current call context.
VM owns per-call $id overrides.
VM encodes CallRef lazily when $id is actually read.
```

This makes `$id` work even when user-visible tracing is off and keeps string allocation off the call hot path.

### 6.8 Current product surface

Supported:

```baml
$id
baml.id.current()
baml.id.new()
baml.id.set(id)
$id = id
```

Not implemented as the current product surface:

```baml
foo($id = baml.id.new())
```

The call-site form is a future product decision. It is additive because the event stream already supports last-wins `SetFunctionId` semantics.

---

## 7. Function metadata and `function_id`

`function_id` answers a different question from `call_id`.

```text
call_id:      which invocation happened?
function_id:  which function definition did it run?
```

A single function can be called many times:

```text
function_id = 42 -> user.ExtractResume
call_id = 1      -> first invocation
call_id = 9      -> later invocation
call_id = 23     -> another invocation
```

### 7.1 What `function_id` is good for

`function_id` is good for compact event records inside one artifact.

```text
CallFunction { function_id = 42 }
header.function_table[42] -> user.ExtractResume
```

### 7.2 What `function_id` is not

`function_id` is not:

```text
a semantic hash
a durable cross-run ID
a Studio URL identity
a source revision identity
stable across recompiles
```

For cross-run profile diffs, FQN is a practical v1 join key. For Studio semantic versioning, use source snapshot / definition key / revision / semantic lanes.

### 7.3 Header function table

The `.bamlprof` header carries function metadata rows. The minimal final proto row mirrors:

```text
function_id
fqn
source_file
span_start
span_end
kind
```

The broader metadata model also includes fields such as display name, owner type, lambda data, definition key, source snapshot, revision ID, and semantic lanes. These are hooks for compiler/cloud enrichment.

### 7.4 Metadata must be coherent with runtime records

The function ID stamped onto function objects and the function table rows must come from the same walk/source of truth.

Do not resolve function IDs by display-name scan on the call hot path. Display names can collide. Per-call resolution should use the actual function object / pointer / stamped ID path.

### 7.5 Unknown and synthetic functions

The final wire contract uses:

```text
function_id = 0
```

for unattributable calls.

Renderers should display those under the reserved `baml.<unknown-function>` bucket. The reserved display row may exist in metadata, but events use `0` as the honest wire sentinel.

Synthetic rows also exist for runtime-created shapes such as spawn closures.

### 7.6 ProgramId caveat

`program_id` exists in the model and header. Its durable source is still not the final semantic source of truth.

In current profiling artifacts, consumers should not treat `program_id` alone as a durable content identity. It is useful for file-local scoping and future joins, but Studio-grade grouping should come from source/package metadata.

---

## 8. Semantic versioning and BEP-053 join

Tracing and function hashing meet through metadata, not hot event emission.

The join path is:

```text
CallFunction.function_id
  -> FunctionMetadata row
  -> FQN / definition_key / source location
  -> source_snapshot_id / revision_id
  -> BEP-053 semantic lanes
```

### 8.1 Why hashes are not on every event

Semantic hashes are source/version identity. Runtime events are invocation identity.

Putting semantic hashes on every event would:

```text
make hot records larger
force source/version work into runtime paths
mix call identity with code identity
make every call pay for metadata it usually does not need
```

The correct layering is:

```text
runtime: tiny IDs
header: metadata table
cloud/studio: semantic enrichment
```

### 8.2 FQN is useful but insufficient

FQN is useful for display and profile diff:

```text
user.ExtractResume
```

FQN is not a stable semantic version. The same FQN can change implementation, dependency-effective hash, source snapshot, or revision history.

### 8.3 Reverts do not erase chronology

Example:

```text
T1: function a() {}
T2: function a() { let x = 0 }
T3: function a() {}
```

T1 and T3 may have identical implementation hashes, but T3 is still a new revision occurrence.

Rule:

```text
semantic hash = content equivalence
revision_id   = chronology/source occurrence
```

---

## 9. `.bamlprof` event and file model

The interim JSONL disk-event path was deleted. The final profiling transport is `.bamlprof`.

A `.bamlprof` file is:

```text
one length-delimited EventFileHeaderV1
then many length-delimited DiskEventV1 messages
```

There is one file per engine per process, under the profiles directory, normally:

```text
.baml/profiles/
```

### 9.1 Header

Conceptual header:

```text
EventFileHeaderV1 {
  process_id
  engine_id
  program_id
  started_at_epoch_ns
  function_table
}
```

`process_id` and `engine_id` scope every local event ID in the file.

`started_at_epoch_ns` is the wall-clock anchor.

`function_table` maps `function_id` to metadata.

### 9.2 Events

MVP event taxonomy:

```text
StartThread
EndThread
CallFunction
SetFunctionId
EndFunction
Heartbeat
```

#### StartThread

A logical BEX thread started.

```text
StartThread {
  thread_id
  parent_thread_id?
  parent_call_id?
  name?
  timestamp_ns
}
```

The first record for a logical thread, after sorting by timestamp within the thread, must be `StartThread`.

#### CallFunction

A runtime function call started.

```text
CallFunction {
  thread_id
  call_id
  parent_call_id?
  function_id
  timestamp_ns
}
```

`parent_call_id` is same-thread only.

#### SetFunctionId

The current call's `$id` override changed.

```text
SetFunctionId {
  thread_id
  call_id
  id
  timestamp_ns
}
```

Last record wins for a single display label. One record is emitted per `baml.id.set()` call. The producer deliberately does not deduplicate.

#### EndFunction

A runtime call frame ended.

```text
EndFunction {
  thread_id
  call_id
  status
  timestamp_ns
}
```

Every `CallFunction` must have exactly one matching `EndFunction`.

#### EndThread

A logical BEX thread ended.

```text
EndThread {
  thread_id
  status
  timestamp_ns
}
```

Every `StartThread` must have exactly one matching `EndThread` unless the process died mid-trace.

#### Heartbeat

A liveness marker stamped by the consumer on a timer.

```text
Heartbeat { timestamp_ns }
```

### 9.3 File order is not event order

File order is ring drain order. It is not guaranteed to be logical event order.

A single logical BEX thread's events can arrive through several rings because tasks can migrate across OS threads.

Reader rule:

```text
Group by thread_id.
Sort each thread by timestamp_ns.
Then reconstruct.
```

### 9.4 Timestamps

`timestamp_ns` is monotonic nanoseconds since the process/profile clock anchor.

It is not wall-clock epoch time.

The header stores the wall anchor:

```text
wall_event_time = started_at_epoch_ns + event.timestamp_ns
```

This keeps ordering/durations monotonic while allowing human wall-clock rendering.

---

## 10. Parent edges and reconstruction basics

### 10.1 Same-thread calls

Nested calls inside one BEX thread use `CallFunction.parent_call_id`:

```text
CallFunction thread=1 call=1 parent=None function=a
CallFunction thread=1 call=2 parent=1    function=b
CallFunction thread=1 call=3 parent=2    function=c
```

This means:

```text
a called b
b called c
```

### 10.2 Spawned threads

Spawned child threads start a fresh same-thread call stack. Cross-thread causality lives on `StartThread`:

```text
StartThread  thread=2 parent_thread=1 parent_call=4 name="child"
CallFunction thread=2 call=1 parent=None function=<spawn-closure>
```

Do not put cross-thread parent data on the child root `CallFunction`.

### 10.3 Reader algorithm

A robust `.bamlprof` reader should:

```text
1. Read EventFileHeaderV1.
2. Read all complete DiskEventV1 messages.
3. Ignore/tolerate a torn tail if the last message is incomplete.
4. Group events by thread_id.
5. Sort each thread by timestamp_ns.
6. Assert the first event for each thread is StartThread.
7. On CallFunction, create a call node keyed by (thread_id, call_id).
8. Link same-thread parent using parent_call_id.
9. Resolve function_id from the header function table.
10. Treat function_id 0 as unknown/unattributable.
11. On SetFunctionId, update that call's effective $id override.
12. On EndFunction, close the call with status.
13. On EndThread, close the thread with status.
14. Link spawned threads using StartThread.parent_thread_id + parent_call_id.
```

---

## 11. Status semantics

### 11.1 FunctionEndStatus

A function status describes how one call frame ended.

```text
OK         normal return
ERRORED    unwound by exception or unrecognized panic class
CANCELLED  closed by cancellation
EXITED     unwound by baml.sys.exit
```

Frame status is about frame fate, not necessarily program outcome.

### 11.2 ThreadEndStatus

A thread status describes how the logical BEX thread ended.

```text
COMPLETED  logical thread completed successfully
CANCELLED  logical thread was cancelled
ERRORED    logical thread ended with an unhandled error/non-success terminal state
```

### 11.3 `baml.sys.exit`

`baml.sys.exit` unwinds frames like a panic but has program-level semantics.

Final rule:

```text
Frame-level:
  frames unwound by exit -> FunctionEndStatus.EXITED

Root thread-level:
  exit(0)    -> ThreadEndStatus.COMPLETED
  exit(n!=0) -> ThreadEndStatus.ERRORED

Spawned child thread-level:
  child terminated by exit -> ThreadEndStatus.ERRORED
  EXITED frames are the reliable signal away from the root
```

### 11.4 Cancellation balance

Cancelled threads must not strand open calls. The engine drains open calls innermost-first with `EndFunction{CANCELLED}`, then emits `EndThread{CANCELLED}`.

---

## 12. Antonio's profiling ring architecture

Antonio's v2 design implements the hot/cold split:

```text
HOT:
  VM producer writes raw records into rings.

COLD:
  background consumer drains rings and writes protobuf .bamlprof files.

RENDER:
  readers reconstruct trees and render derived views.
```

### 12.1 Why a ring exists

Direct disk writes, global mutexes, async channel sends, or per-call engine yields are too expensive on every function call.

The producer hot path should be approximately:

```text
bounds check
memcpy raw record
advance head pointer
Release-store commit_len
continue execution
```

The consumer handles serialization and I/O later.

### 12.2 Per `(engine, OS thread)` rings

There is a segmented SPSC ring per `(engine, OS thread)`.

Why include OS thread?

```text
The producer side is single-producer only if one OS thread owns writes into
that ring. Tokio tasks can migrate across OS threads only at await/resume
boundaries, so the engine refreshes the VM's ring pointer once per exec resume.
```

This means no per-event TLS lookup and no per-event global switch read.

### 12.3 VM-held ring pointer

The VM holds:

```text
prof_ring: current ring handle/pointer
prof_enabled: snapshot of master switch
prof_thread_id: logical BEX thread id
```

The engine refreshes these at the existing resume site before `exec()`. That is enough because `exec()` does not cross `.await`.

If the VM ever starts yielding mid-`exec()` in a way that can migrate OS threads, this model must be revisited.

### 12.4 Raw in memory, protobuf on disk

The ring carries raw fixed-layout records. The consumer transcodes them to protobuf `DiskEventV1`.

This is not enrichment. The content is the same; only the encoding changes.

Why:

```text
raw ring records = cheap memcpy on hot path
protobuf disk records = tool-friendly, evolvable, partial-parseable artifact
```

### 12.5 Lossless by growth

The ring is segmented. When the current segment fills, the producer links a fresh or recycled segment and continues.

It does not silently drop records.

It does not block the VM waiting for disk/cloud.

Memory growth is bounded by `BAML_RING_MAX_OVERFLOW_BYTES`. Hitting the cap is a hard process error, not silent trace loss.

### 12.6 D1-D7 locked decisions

Antonio's implementation plan fixed seven concurrency decisions. These are not cosmetic; they are the safety contract of the ring.

**D1 - Drain hand-off.** When the consumer sees `next != null`, it reloads `commit_len` and drains the remainder of the old segment before recycling. This is sound because the producer links `next` only after its last commit to the old segment.

**D2 - Producer owns recycle reset.** After taking a segment from the free list, the producer resets `commit_len = 0` and `next = null`, then publishes the segment through the link store. This gives one initialization path for fresh and recycled segments.

**D3 - Cache layout.** Producer fields, consumer fields, and shared fields are separated onto cache-line-aligned groups. `commit_len` and `next` live away from the buffer pointer. This reduces false sharing on a high-frequency hot path.

**D4 - Wake protocol.** The consumer sets a parked flag, then parks with timeout. The producer checks the flag only on segment fill, not on every event. A possible lost wakeup is benign because the timeout bounds it to one interval of extra ring growth.

**D5a - Ring handle refresh.** The VM holds the ring pointer. The engine refreshes it once per VM resume. There is no TLS lookup per push.

**D5b - Thread death and pooling.** When an OS thread dies, its TLS destructor marks rings orphaned. The consumer drains an orphaned ring to empty, marks it pooled, and future threads can claim pooled rings by CAS. The registry is append-only forever.

**D6 - Capacity framing.** The producer write target is a burst budget, not disk throughput. Sustainable drain rate depends on consumer transcode/write throughput. If production exceeds drain, backlog grows as RAM. The overflow cap is a hard error.

**D7 - Free-list shrink.** The consumer caps each ring's recycled segment free list. Extra retired segments are freed instead of accumulating forever.

### 12.7 Producer protocol in words

On event push:

```text
1. Check whether current segment has space.
2. If not, pop a recycled segment or allocate a new one.
3. Reset the segment if reused.
4. Link it from old head with a Release store.
5. Maybe unpark the consumer if it was parked.
6. Copy record bytes into the segment.
7. Publish the new committed length with a Release store.
```

Steady-state hot path is only the last two steps plus bounds check.

### 12.8 Consumer protocol in words

On drain:

```text
1. Load committed bytes with Acquire.
2. Parse records from tail_pos to committed.
3. If next segment is null, stop: caught up for now.
4. If next exists, reload final committed length for old segment.
5. Drain any remaining bytes.
6. Retire the old segment to the free list or free it.
7. Move to the next segment.
```

### 12.9 Registry and orphan lifecycle

Rings are registered in an append-only global list.

States:

```text
Active   -> current OS thread may produce
Orphaned -> producer thread died; consumer must drain to empty
Pooled   -> drained and reusable by a future OS thread
```

The registry is append-only to avoid lock-free removal complexity and ABA hazards. Memory is bounded by peak concurrent OS threads plus pooled rings.

### 12.10 Why free-list ABA is not a problem

Per ring, the free list has one pusher and one popper:

```text
consumer pushes retired segments
producer pops reusable segments
```

ABA on pop requires another concurrent popper. There is only one. The ring pool claims rings by state CAS on never-freed ring pointers, so it does not need pointer removal either.

### 12.11 Consumer thread constraints

The consumer must never hold a BEX GC heap permit and must never call into VM heap machinery.

It reads rings, transcodes raw records, writes files, emits heartbeats, handles flush/teardown, and exits cleanly.

This is what prevents a deadlock chain where a producer holds a heap permit while waiting for the consumer, while GC waits for all permits.

### 12.12 Performance numbers and expectations

Antonio's implementation notes recorded:

```text
clock read:              about 8.5 ns/read
consumer drain:          about 7.5M events/sec/core
call-pair overhead:      about 63 ns on pure-call microbench
realistic workloads:     about 0% to 4.4% overhead
```

The design target is not "disk writes 100M events/sec." The target is that the VM can write events into rings at burst rates without meaningfully slowing execution. Disk/cloud I/O is explicitly outside the producer budget.

### 12.13 Current rollout/config behavior

The final reconciled canary behavior is:

```text
BAML_PROFILE unset   -> enabled on native targets
BAML_PROFILE=1/true  -> enabled
BAML_PROFILE=0/false -> disabled
wasm32               -> forced off
```

Other knobs:

```text
BAML_PROFILE_DIR              default .baml/profiles
BAML_RING_SEG_BYTES           default segment size
BAML_RING_MAX_OVERFLOW_BYTES  hard live ring memory cap
BAML_RING_FREELIST_CAP        recycled segment cap
BAML_PROF_WAKE_INTERVAL_MS    consumer park timeout
```

### 12.14 Raw record sizes are a design signal, not the API

Antonio's plan listed raw records around tens of bytes, e.g. `CallFunction` and `EndFunction` as fixed records, with `StartThread` variable due to inline thread name. The exact current source of truth is `record.rs`/the proto; the important principle is:

```text
high-volume records are small and fixed-size
StartThread is rare, so it may carry capped inline name bytes
```

---

## 13. Lifecycle examples

### 13.1 Simple nested call

```baml
function a() {
  b()
}

function b() {}
```

Events after sorting within the thread:

```text
StartThread  thread=1
CallFunction thread=1 call=1 parent=None function=a
CallFunction thread=1 call=2 parent=1    function=b
EndFunction  thread=1 call=2 status=OK
EndFunction  thread=1 call=1 status=OK
EndThread    thread=1 status=COMPLETED
```

### 13.2 Spawned child

```baml
function main() {
  spawn "child" {
    child_work()
  }
}

function child_work() {}
```

Events:

```text
StartThread  thread=1 parent_thread=None parent_call=None
CallFunction thread=1 call=1 parent=None function=main

StartThread  thread=2 parent_thread=1 parent_call=1 name="child"
CallFunction thread=2 call=1 parent=None function=<spawn-closure>
CallFunction thread=2 call=2 parent=1    function=child_work
EndFunction  thread=2 call=2 status=OK
EndFunction  thread=2 call=1 status=OK
EndThread    thread=2 status=COMPLETED

EndFunction  thread=1 call=1 status=OK
EndThread    thread=1 status=COMPLETED
```

### 13.3 `$id` override

```baml
function foo() -> string {
  let next = baml.id.new()
  $id = next
  $id
}
```

Events:

```text
CallFunction  thread=1 call=1 function=foo
SetFunctionId thread=1 call=1 id=<override uuid>
EndFunction   thread=1 call=1 status=OK
```

Effective display ID:

```text
if SetFunctionId exists for call:
  last SetFunctionId wins
else:
  default CallRef(process, engine, thread, call)
```

### 13.4 Caught exception

The rule is not "exceptions make traces weird." The rule is precise:

```text
Every entered frame that unwinds gets an EndFunction.
Caught exceptions must not leave stale parents.
After catch, new calls attach to the live caller, not the dead thrower.
```

A caught exception across nested frames should still produce balanced call/end pairs.

### 13.5 Cancellation

Cancellation closes open calls innermost-first as `CANCELLED`, then closes the thread as `CANCELLED`.

```text
CallFunction main
CallFunction child
cancel
EndFunction child status=CANCELLED
EndFunction main  status=CANCELLED
EndThread status=CANCELLED
```

### 13.6 Watch filters

Watch filters are code that runs. Their time is real. They appear in the profiling stream attached under the interrupted call.

Program-only renderers may hide watch-filter subtrees. The producer should still record them.

---

## 14. Reconstruction, timing, and renderers

The `.bamlprof` file stores the raw event log, not aggregates.

Flamegraphs, timelines, and diffs are derived views computed when reading.

### 14.1 Stackless reconstruction

Because each `CallFunction` carries `parent_call_id`, readers do not need to perfectly replay a stack just to link calls.

They still verify balance, but the tree edge is already explicit.

### 14.2 Timing algorithm

A simple inclusive/exclusive timing algorithm:

```text
On CallFunction:
  create frame { enter_ns, function_id, parent_call_id, children_incl_ns = 0 }

On EndFunction:
  incl = end_ns - enter_ns
  excl = incl - children_incl_ns
  add { count += 1, incl += incl, excl += excl } to function aggregate
  add incl to parent.children_incl_ns
```

Use wide accumulators for aggregates.

### 14.3 Speedscope flamegraph

The event log maps naturally to speedscope's evented format:

```text
CallFunction -> open event
EndFunction  -> close event
function_id  -> frame key resolved through header
```

### 14.4 Timeline / waterfall

Because every call has enter/exit timestamps, renderers can show orchestration timing:

```text
call A: 0.0s - 2.0s
call B: 0.5s - 1.8s, child/parallel
call C: 2.1s - 2.4s
```

This cannot be derived from aggregates alone. It requires the raw log.

### 14.5 `baml profile diff`

Profile diff should match functions by FQN for v1 because `function_id` is not stable across recompiles.

```text
join key: FunctionMetadata.fqn
compare: exclusive wall-clock time, count, maybe inclusive time
```

For Studio semantic grouping, use semantic metadata rather than FQN alone.

---

## 15. Legacy tracing interaction

The existing user-visible tracing stream still matters.

The final architecture separates:

```text
profiling stream: structural runtime calls for reconstruction/perf/profile
legacy trace stream: user-visible tracing semantics/spans
```

### 15.1 Hidden calls can still be profiled

Expression functions with `trace: false` should not necessarily create user-visible child spans. But they still need call IDs, parent edges, `$id`, and profiler timing.

Therefore:

```text
not visible in legacy trace != not present in profiling stream
```

### 15.2 `SpanNotification::Unwound`

The ring records profiling frame exits. The legacy trace stream still needs balanced visible spans when exceptions unwind traced frames.

The final implementation reports unwound traced frames so the engine can close visible spans by depth.

### 15.3 RuntimeEvent optional identity

Legacy runtime events can carry optional BEX identity where appropriate. Host spans that do not correspond to a BEX runtime call can have no identity.

This preserves compatibility while allowing BEX-aware traces to join to call identity.

---

## 16. What landed and why

### 16.1 Identity model

Implemented the scoped identity types and reversible encodings:

```text
ProcessEuid
EngineId
ProgramId
SourceSnapshotId
BexThreadId
FunctionId
BexCallId
ThreadRef
CallRef
RuntimeId
```

Why: external call IDs need to be globally meaningful without making every hot event globally heavy.

### 16.2 `$id` language primitive

Implemented:

```text
baml.id.current()
baml.id.new()
baml.id.set(id)
$id read
$id = value
```

Why: users need a stable runtime call identity they can pass through programs, logs, callbacks, and Studio links.

### 16.3 VM-owned identity

The VM now owns call counters/current call/overrides and encodes `CallRef` lazily.

Why: the VM is the only component on the exact call enter/exit/read path, and per-call engine yields are too expensive.

### 16.4 Function metadata table

Events reference `function_id`; the header carries a function table.

Why: hot events stay small while consumers can still render readable function names and source metadata.

### 16.5 `.bamlprof` artifact

Implemented protobuf header + event stream with per-engine output files.

Why: protobuf is partial-parseable, tool-friendly, and evolvable without putting serialization on the hot path.

### 16.6 Segmented SPSC ring

Implemented lock-free producer push, lossless-by-growth segments, free-list recycling, append-only registry, orphan/pool/claim lifecycle, and background consumer.

Why: the VM must never block on disk/cloud and must never silently drop structural events.

### 16.7 Status and balance hardening

Implemented explicit balance through normal return, exception unwind, cancellation, `baml.sys.exit`, spawned children, watch filters, and sysop pairs.

Why: renderers assume every call has exactly one end. Broken balance corrupts trees, timings, and profile diffs.

### 16.8 Test suite as contract

The tests pin ID round trips, `$id` behavior, spawn edges, cancellation/error/exit statuses, watch filters, unknown sentinel behavior, function metadata coherence, and `.bamlprof` reconstruction.

Why: this system is a contract between VM, engine, artifact writer, renderer, Studio, and future hashing work. Tests are the executable contract.

---

## 17. Operational behavior

### 17.1 Environment variables

```text
BAML_PROFILE
BAML_PROFILE_DIR
BAML_RING_SEG_BYTES
BAML_RING_MAX_OVERFLOW_BYTES
BAML_RING_FREELIST_CAP
BAML_PROF_WAKE_INTERVAL_MS
```

### 17.2 Default output directory

```text
.baml/profiles
```

### 17.3 Native versus wasm

Native targets can run the background consumer.

`wasm32` is forced off in this implementation because there is no consumer thread / native clock path. A cooperative wasm drain is a future design.

### 17.4 Torn-tail tolerance

Readers should keep the complete prefix of a `.bamlprof` file and tolerate an incomplete final message if the process dies mid-write.

### 17.5 Flush and teardown

Engine teardown closes the per-engine file. Flush APIs drain rings and flush writers. Shutdown ordering should stop/join VMs before final drain so the last commits are visible.

---

## 18. Code map

### Identity

```text
crates/bex_events/src/ids.rs
```

Look for `ProcessEuid`, `EngineId`, `BexThreadId`, `BexCallId`, `FunctionId`, `CallRef`, `ThreadRef`, `RuntimeId`, and encode/decode tests.

### Profiling proto

```text
crates/bex_events/src/prof/proto/bamlprof.proto
```

This is the external artifact contract.

### Profiling modules

```text
crates/bex_events/src/prof/mod.rs
crates/bex_events/src/prof/config.rs
crates/bex_events/src/prof/record.rs
crates/bex_events/src/prof/ring.rs
crates/bex_events/src/prof/registry.rs
crates/bex_events/src/prof/consumer.rs
crates/bex_events/src/prof/clock.rs
crates/bex_events/src/prof/file.rs
```

Look here for ring config, raw record encoding, producer/consumer APIs, registry, file writing, and clock behavior.

### VM identity and emission

```text
crates/bex_vm/src/vm.rs
crates/bex_vm/src/package_baml/id.rs
```

Look here for call ID minting, current call context, call enter/exit emission, unwind classification, and `$id` built-ins.

### Engine lifecycle

```text
crates/bex_engine/src/lib.rs
```

Look here for engine IDs, root thread lifecycle, spawn lifecycle, cancellation drains, status mapping, and metadata registration.

### Compiler `$id` support

```text
crates/baml_builtins2/baml_std/baml/ns_id/id.baml
crates/baml_compiler2_ast/...
crates/baml_compiler2_tir/...
crates/baml_compiler2_mir/src/lower.rs
```

Look here for `$id` special syntax, direct assignment lowering, reserved-name diagnostics, and throws facts.

### Tests

```text
crates/bex_events/src/ids.rs tests
crates/bex_events/src/prof/config.rs tests
crates/bex_events/src/prof/concurrency_tests.rs
crates/bex_vm/tests/call_notifications.rs
crates/bex_engine/tests/prof_gate.rs
crates/bex_engine/tests/tracing.rs
compiler snapshot tests around runtime_id misuse
```

---

## 19. Invariants that matter most

These are the rules to preserve in future work.

```text
Every CallFunction has exactly one EndFunction.
Every StartThread has exactly one EndThread.
After sorting by timestamp within a thread, StartThread is first.
CallFunction.parent_call_id is same-thread only.
StartThread carries cross-thread spawn parent edge.
function_id 0 means unattributable/unknown.
Every nonzero function_id resolves in the header function table.
$id reads the current runtime call.
$id overrides are per-call, not global.
function_id is not a semantic hash.
No semantic hashing happens on the call hot path.
Profiling transport never silently drops structural events.
The consumer never touches the GC heap or heap permits.
File order is not event order.
```

---

## 20. Common mistakes to avoid

### Mistake 1: Treating `call_id` as global

`call_id = 1` exists in every thread. Use `CallRef` or full scope.

### Mistake 2: Treating `function_id` as stable

`function_id` is only a key into this artifact's function table.

### Mistake 3: Reconstructing by file order

File order is ring drain order. Sort by timestamp within thread.

### Mistake 4: Skipping hidden calls

A call hidden from legacy trace UI may still need profiler events.

### Mistake 5: Computing hashes in the VM

Runtime events should not compute BEP-053 semantic hashes.

### Mistake 6: Making `$id` a normal local

`$id` needs special compiler/runtime handling.

### Mistake 7: Silent drops

A profiling system that silently drops structural events will create impossible trees. If memory cap is exceeded, fail loudly.

### Mistake 8: Letting a new termination path skip drains

New cancellation, error, drop, sysop, watch, or spawn paths must close open calls.

---

## 21. Reader checklist for `.bamlprof`

```text
[ ] Read header before events.
[ ] Treat process_id + engine_id as artifact scope.
[ ] Do not rely on program_id as semantic source identity yet.
[ ] Build function_id -> metadata map.
[ ] Read only complete length-delimited messages.
[ ] Tolerate torn tail.
[ ] Group by thread_id.
[ ] Sort each thread by timestamp_ns.
[ ] Expect StartThread first after sort.
[ ] Link same-thread parent calls by parent_call_id.
[ ] Link spawned threads by StartThread parent fields.
[ ] Treat function_id 0 as unknown/unattributable.
[ ] Apply SetFunctionId as last-wins $id override.
[ ] Verify call/end balance.
[ ] Verify thread start/end balance.
[ ] Use FunctionEndStatus for frame fate.
[ ] Use ThreadEndStatus for thread outcome.
[ ] Use FQN for simple cross-run profile diff, not function_id.
[ ] Use semantic metadata for Studio-grade versioning.
```

---

## 22. Maintainer checklist for future changes

```text
[ ] VM still mints call_id for every runtime call.
[ ] $id works when user-visible tracing is disabled.
[ ] CallRef string encoding remains lazy.
[ ] Overrides remain per-call and survive nested callees.
[ ] Every call enter path has exactly one exit/unwind/cancel close path.
[ ] Cancellation drains open calls.
[ ] sys.exit frame and thread statuses remain distinct.
[ ] Watch filters are recorded or intentionally accounted for.
[ ] function_id stamping and function table construction share one source.
[ ] function_id 0 remains the unattributable sentinel.
[ ] Readers are not asked to rely on raw file order.
[ ] No semantic hashing is added to hot event emission.
[ ] Native profiling can be disabled with BAML_PROFILE=0/false.
[ ] wasm profiling stays off unless a wasm-specific drain design lands.
[ ] Ring producer never blocks or locks.
[ ] Consumer never touches GC heap/permits.
[ ] New proto fields are additive/evolvable.
```

---

## 23. Deferred and future work

These are intentionally outside the landed MVP or still need stronger metadata sources.

### 23.1 Renderer product surfaces

The `.bamlprof` format enables flamegraphs, timeline/waterfall, and `baml profile diff`. Renderer/tooling can evolve without changing the producer.

### 23.2 Payload capture

Args/results/errors/LLM payloads should attach to `call_id` / `CallRef` in future events. They need redaction, caps, and product policy.

### 23.3 Markers

GC pauses, LLM/HTTP waits, FFI calls, scheduling events, and other markers should carry the enclosing `call_id` explicitly.

### 23.4 Cloud wire

A cloud consumer can stream the same `DiskEventV1` records. The producer and ring do not need to change.

### 23.5 Live metrics

A live in-memory consumer can run the same reconstruction algorithm continuously, with quiescent-drain APIs for synchronous reads.

### 23.6 Compiler-owned metadata

Engine-derived metadata is useful, but authoritative source/revision/lambda metadata should eventually come from compiler/package/source snapshot infrastructure.

### 23.7 ProgramId and source snapshot source of truth

`ProgramId` should eventually be clarified as random runtime ID, compiled artifact ID, source snapshot ID, package/build identity, or cloud-assigned ID.

---

## 24. Glossary

**BEX thread**: logical runtime thread inside BEX. Not an OS thread.

**CallRef**: reversible external string for one runtime call: process + engine + BEX thread + call.

**call_id**: per-BEX-thread local ID for one runtime call.

**engine_id**: process-local ID for one `BexEngine`.

**function_id**: per-artifact function table key. Not a semantic hash.

**FunctionEndStatus**: how one call frame ended.

**ThreadEndStatus**: how one logical BEX thread ended.

**Heartbeat**: consumer/liveness event.

**Hot path**: code that runs per call/event and must stay cheap.

**Cold path**: consumer/rendering/enrichment path after event capture.

**ProcessEuid**: effectively unique process lifetime ID.

**RuntimeId**: `$id` value, either default `CallRef` or override UUID.

**SetFunctionId**: final proto event that sets a call's `$id` override. It does not change `function_id`.

**SPSC ring**: single-producer, single-consumer ring buffer.

**Semantic lanes**: BEP-053 hash lanes for semantic version identity.

**Span**: user-visible tracing unit; related to but not identical with profiling calls.

**Torn tail**: incomplete final length-delimited message caused by process death mid-write.

---

## Appendix A. Source note

This document consolidates the uploaded implementation design, post-implementation notes, review, review response, reconciliation handoff, and Antonio's two pasted notes:

```text
bex-event-stream-design-v2.md
bex-event-stream-impl-plan.md
```

The final document also uses the landed proto/module/config/code-level contract for PR #3740 and reconciles terminology differences:

```text
Antonio design name: SetId
Final proto name:    SetFunctionId
Meaning:             set this call's $id override, not function_id metadata
```

It intentionally presents final rules directly instead of narrating each branch's intermediate state.

---

## Appendix B. One-page summary

The final tracing/profile system is:

```text
VM-owned call identity
+ reversible CallRef / ThreadRef / RuntimeId encodings
+ $id current/new/set language surface
+ per-call override stack
+ compact CallFunction / EndFunction lifecycle records
+ StartThread / EndThread thread lifecycle records
+ SetFunctionId last-wins $id override records
+ function table in the artifact header
+ segmented SPSC profiling rings
+ background .bamlprof consumer
+ semantic metadata joined later, not computed hot
```

Core identities:

```text
Runtime identity:    process_euid + engine_id + thread_id + call_id
Function metadata:   function_id -> fqn/source/kind table row
Semantic versioning: source_snapshot + definition_key + revision + BEP-053 lanes
```

Core rule:

```text
Events carry compact IDs.
Headers carry metadata.
Consumers join them later.
```

Core invariants:

```text
Every CallFunction has one EndFunction.
Every StartThread has one EndThread.
Parent edges are explicit.
$id reflects the current call.
function_id is not a semantic hash.
No semantic hashing happens on the event hot path.
The ring never silently drops structural events.
```
