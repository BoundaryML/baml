---
title: "Durable functions: snapshot, migrate, and resume BAML runs"
status: draft
created: 2026-09-19
---

# Durable functions: snapshot, migrate, and resume BAML runs

## Summary

This document proposes a way to make a running BAML function durable. The
runtime serializes the state of a run (call stacks plus the reachable heap)
into a snapshot file. Another process, on the same machine or a different one,
loads the snapshot and continues the run from the point where it stopped. The
same mechanism supports three features:

- **Live migration.** Suspend a run on one machine and resume it on another.
- **Crash recovery.** Resume a run from its most recent snapshot.
- **Asynchronous cloud execution.** Start a function on a remote worker and get
  a handle back. A fresh call is a snapshot with zero executed frames, so
  starting a run remotely and migrating a run mid-flight use one code path.

The design is based on a reading of the `bex_vm`, `bex_heap`, `bex_engine`, and
`sys_ops` crates and on a survey of existing durable execution systems. It does
not build on any earlier durability design document.

The proof of concept uses two naming conventions in place of new syntax. A
function whose name contains `durable` starts a durable run, which turns on
automatic checkpoints and crash recovery. A function whose
name starts with `remote_` executes on a different worker process. No parser or
compiler change is required. The proof of concept is built around a demo with
a React app, two always-on site servers written in BAML, and worker
processes. The section
"Proof-of-concept demo" describes it.

```baml
function durable_research(topic: string) -> Report {
    let outline = DraftOutline(topic);              // LLM call
    let sections: Section[] = [];
    for (let item in outline.items) {
        sections.push(WriteSection(item));          // LLM call
    }
    Report { topic: topic, sections: sections }
}
```

```bash
baml run -f durable_research --snapshot-dir .baml/runs -- --topic "heap snapshots"
```

```bash
baml resume .baml/runs/<run-id>/latest.bamlsnap
```

```bash
baml snapshot inspect .baml/runs/<run-id>/latest.bamlsnap
```

## Background: what the runtime looks like today

The findings in this section come from the source code. File paths are relative
to `baml_language/crates/`.

### The VM is already a resumable state machine

`BexVm::exec()` (`bex_vm/src/vm.rs:7319`) runs bytecode until it needs the host
and then returns a `VmExecState` value (`bex_vm/src/vm.rs:1461`). The variants
are `Await`, `AwaitAny`, `Spawn`, `SysOp`, `Complete`, `Event`, and
`EarlyYield`. The engine loop in `BexEngine::run_thread_event_loop_inner`
(`bex_engine/src/lib.rs:6113`) handles the yield and calls `exec()` again.

The interpreter loop is iterative. BAML frames do not live on the Rust stack.
A native builtin that needs to call back into BAML returns
`NativeCallResult::YieldToCall` with a boxed `Continuation`, and the VM pushes a
`Frame::Native` that holds the continuation (`bex_vm/src/vm.rs:434`). The
`Await` and `AwaitAny` opcodes rewind the program counter before yielding, so
re-executing them after a resume is safe (`bex_vm/src/vm.rs:8871-8896`).

At every yield, the state of a BAML thread is therefore held in data
structures that can be enumerated:

- `BexVm::frames`, a `Vec<Frame>`.
- `BexVm::stack`, a flat `Vec<Value>` that also holds locals.
- Three exception bookkeeping vectors (`thrown_value_causes`,
  `thrown_value_contexts`, `preserved_throw_contexts`).
- The `VmExecState` value that the engine holds while it services the yield.
  For `SysOp`, the arguments are drained from the operand stack into this value
  (`bex_vm/src/vm.rs:6562-6601`), so a snapshot must record it.

The GC already enumerates every heap reference held by a VM through the
`RootHaver` trait (`bex_vm_types/src/roots.rs`, implemented at
`bex_vm/src/vm.rs:10302-10483`). A serializer needs the same enumeration.

### All external effects pass through one dispatch point

Every nondeterministic or external operation is a sys-op. The VM yields
`VmExecState::SysOp { operation, args }`. The engine converts the arguments to
`BexExternalValue`, which contains no heap pointers, and calls
`execute_sys_op` (`bex_engine/src/lib.rs:6999`). The result is a
`Result<BexExternalValue, OpError>` that the engine pushes onto the VM stack.

The `SysOp` enum is generated from stdlib declarations marked
`$rust_io_function`. It has about 125 variants that cover env, stdio, HTTP
client and server, process execution, sleep, file system, glob, TCP, UDP,
WebSocket, time, random, and host callbacks. The dispatch table `SysOps` is a
struct of function pointers with two existing providers (`sys_native` and the
WASM bridge). A journaling or replaying provider can be added as a third
provider without changing the VM.

### LLM calls are BAML code

Provider clients, retry, fallback, round-robin, and stream relay are written in
BAML under `baml_builtins2/baml_std/` and sit on top of the HTTP sys-ops. The
retry counter, the chosen fallback client, and the partially assembled message
history are ordinary heap objects. A heap snapshot captures them without any
provider-specific code.

### Serialization that already exists

- `Program` derives Borsh (`bex_vm_types/src/types.rs:66`). Packed binaries
  already embed it.
- `Object` has a hand-written Borsh implementation through the `ObjectWire`
  proxy (`bex_vm_types/src/types/object.rs:207-320`). It covers classes,
  instances, closures, strings, arrays, maps, cells, bigints, and types.
- The implementation rejects `RustData`, `HostClosure`, `Future`, and
  `UnscheduledFuture`.
- `HeapPtr` rejects serialization in both directions
  (`bex_vm_types/src/heap_ptr.rs:161-177`), so any object that contains a
  pointer fails to serialize today.
- `BexExternalValue` has a protobuf encoding used by the language bridges.
- `TraceHeap` (`bex_engine/src/trace_heap.rs`) copies a value graph out of the
  live heap under a heap permit. It is a working precedent for reading a
  consistent object graph next to a moving collector.

### Obstacles

| Obstacle | Where | Consequence |
|---|---|---|
| `Value` and `HeapPtr` are raw addresses. | `types/value.rs:50`, `heap_ptr.rs:38` | The snapshot needs a pointer-free encoding and a relocation step on restore. |
| Host resources live on the heap as `RustData` handles into a process-global registry (files, sockets, HTTP responses, SSE and WebSocket streams, `StreamAccumulator`). | `sys_native/src/registry.rs`, `bex_resource_types/src/lib.rs:13` | These values cannot move to another process. |
| `HostClosure` refers to an object in the host language process. | `types/object.rs:262` | A run that holds a host callback cannot move. |
| `Frame::Native` holds `Box<dyn Continuation>` (about 23 implementations). | `bex_vm/src/vm.rs:434` | Continuations need a serialization method, or snapshots must wait until no native frame is on a stack. |
| `Future` objects hold Tokio primitives, and each `spawn` creates a separate `BexThread` with its own `BexVm`. | `types/future.rs`, `bex_engine/src/thread.rs:18` | A run is a set of threads plus pending futures, not one VM. The agent runner spawns tool calls (`baml_std/ai/runner.baml:357`). |
| The program counter is a byte offset into `CompactCode`, which is built at load time and is not serialized. | `bytecode.rs:2101` | Resume requires the same program and the same runtime build. |
| The heap is shared by all calls in an engine. | `bex_vm/src/vm.rs:1261` | A snapshot covers the subgraph reachable from one run, not the whole heap. |
| The program image can change at runtime through `reflect` compilation. | `link.rs`, `runtime_compile.rs` | The first milestones refuse to snapshot an engine that has grafted packages. |
| `BexVm` holds process-scoped fields (profiler ring, capture hooks, address-keyed caches, `bex_ref_seed`). | `bex_vm/src/vm.rs:1297-1436` | These fields are dropped at snapshot time and rebuilt on restore. |

## Prior art

Existing systems fall into two groups.

**Journal and replay.** Temporal, Azure Durable Functions, Restate, DBOS,
Inngest, Cloudflare Workflows, Vercel Workflow, and AWS Lambda durable functions
record the result of each step and rebuild state by running the orchestration
function again from the start. This approach forces user code to be
deterministic. It also makes code changes risky for in-flight runs. Temporal
documents patching and worker versioning for this problem, and it caps history
at 51,200 events or 50 MB. Restate recommends keeping handlers short because
long-running handlers pin old deployments.

**Snapshot and continuation.** Unison serializes continuations and identifies
code by content hash, so a continuation never points into changed code. Golem
combines an operation log with periodic memory snapshots. Stackless Python and
Lua Eris pickle coroutines. Eris maps unserializable host values to stable
names through a permanents table and binds them again on load. Stackless refuses
to load a tasklet across bytecode versions. Racket stateless servlets note that
any program change invalidates stored continuations. CRIU, Firecracker, and
Modal memory snapshots work at the process or VM level, where images are
hundreds of megabytes and external connections do not survive a move to another
host.

Three conclusions carry over to this design.

1. A snapshot is tied to an exact code identity. Systems either refuse on a
   mismatch, pin runs to a code hash, or migrate at explicit points.
2. External resources are never captured. They are re-established on resume,
   re-bound by name, or held by the platform on behalf of the run.
3. A snapshot alone does not give exactly-once effects. If the process crashes
   after an effect and before the next snapshot, the effect runs again on
   recovery. Every system solves this with a journaled result, an idempotency
   key, or both.

Modal, Ray, and Dask show the expected shape of an asynchronous remote call:
`spawn` returns a handle with a durable call ID, and the handle supports `get`
with a timeout, `cancel`, and lookup by ID from another process. They ship
closures with cloudpickle, which refers to modules by name and fails at runtime
when the remote environment differs. A BAML snapshot carries its bytecode, so
this class of failure does not apply.

BAML owns the compiler, the VM, the heap, and the sys-op boundary. This
permits a snapshot design that places no determinism requirement on user code,
because resume restores the real state instead of re-running the function.

## Proposed design

### Terms

- A **run** is one root call of a durable function. It has a `RunId`. It owns
  one root `BexThread` and any threads that it spawns.
- A **snapshot** is a self-describing file that contains everything needed to
  continue a run in a new process.
- A **clean point** is a moment when every thread of the run is parked at a
  yield and every value reachable from the run is serializable.

### What a snapshot contains

```
Header
  magic, format version
  runtime build id          (refuse on mismatch)
  program hash              (SHA-256 of the Borsh-encoded Program)
  run id, parent snapshot id, sequence number, wall-clock time
  codec, checksum, optional MAC

Program                     (embedded Borsh bytes, or a hash reference)

Threads[]
  name, parent thread, settles_future
  frames[]
    function ref, function name, pc, locals_offset,
    type_args, call ids, capture mask
  stack[]                   (wire values)
  exception bookkeeping     (three vectors of wire values)
  parked_at
    SysOp { op, args: BexExternalValue[], effect seq }
    Await(future ref) | AwaitAny(future refs)
    Runnable

Globals[]                   (wire values; see open questions)

Objects[]                   (dense array; index is the SnapId)
  ObjectWire bytes, with every pointer written as a SnapRef

Futures[]                   (M2)
  state: Pending | Resolved(value) | Rejected(value) | Cancelled

Effect journal tail         (M2)
```

A wire value is `Null`, `Bool`, `Int`, `OmittedArg`, or `Obj(SnapRef)`. A
`SnapRef` is one of two forms:

- `Runtime(u32)` indexes the `Objects` array.
- `CompileTime(u32)` indexes the object pool of the program. Compile-time
  objects are not copied into the snapshot. `BexHeap` already separates the
  compile-time region (`heap.rs:131`, `is_compile_time_ptr`), and the GC relies
  on the invariant that compile-time objects hold no runtime pointers.

Type references inside `RealizedTy` use the content-addressed `TypeTag` that
`TypeHead` already carries. The serializer writes the tag, and the loader uses
the existing `unresolved(tag)` to `resolve(ptr)` path
(`bex_vm_types/src/type_head.rs:85-130`).

### Writing a snapshot

1. Wait until every thread of the run is parked at a yield. Threads already
   release their heap permit at each async sys-op and each await
   (`bex_engine/src/lib.rs:6409`, `:6817`), so these points exist today.
2. Acquire a heap permit so that the collector cannot move objects during the
   walk.
3. Collect roots from each thread with `RootHaver::collect_roots`, plus the
   values inside each parked `VmExecState`.
4. Walk the object graph breadth-first from the roots. Assign a `SnapId` to
   each runtime object. Stop at compile-time objects.
5. Serialize each object through the existing `ObjectWire` implementation.
6. Serialize frames, stacks, and the parked state.
7. Compress with zstd level 1 and write the file atomically.

If the walk reaches an object that cannot be serialized, the attempt fails and
reports the path from a root to that object, using `Function::debug_locals` to
name the local variable. The run continues and the runtime tries again at the
next yield. This rule is how the proof of concept finds clean points without
any compiler analysis.

Step 5 reuses every existing per-variant Borsh implementation. `HeapPtr`
serialization currently returns an error. The proposal adds a scoped
translation context. While a snapshot writer is active on the current OS
thread, `HeapPtr::serialize` looks the pointer up in the `SnapId` table and
writes a `SnapRef`. Outside a snapshot the behavior is unchanged, so a
malformed program still fails at pack time.

### Loading a snapshot

1. Validate the header. Refuse when the runtime build id or the program hash
   differs.
2. Load the program and build an engine. Skip `$init` and install globals from
   the snapshot.
3. Allocate one placeholder object per snapshot object. `Object` has a fixed
   size of at most 64 bytes and heap chunks never move, so each placeholder has
   a stable address. Record the mapping from `SnapId` to `HeapPtr`.
4. Deserialize each object with the translation context active, so that
   `HeapPtr::deserialize` maps a `SnapRef` to the new address. Overwrite each
   placeholder in place.
5. Rebuild each `BexThread`. Process-scoped fields such as the profiler ring,
   capture hooks, and address-keyed caches start empty.
6. Resume each thread according to `parked_at`:
   - `SysOp`: execute the operation, or take its result from the journal tail,
     push the result, and call `exec()`.
   - `Await` and `AwaitAny`: call `exec()`. The opcode runs again and yields
     again if the future is still pending.
   - `Runnable`: call `exec()`.

An alternative for steps 3 and 4 is to deserialize pointers as placeholder
addresses and then run the GC fix-up pass (`fixup_object_references`,
`bex_heap/src/gc.rs:842`) with a forwarding map from placeholder to real
address. That function only performs map lookups and does not dereference the
old pointer, so it is safe to use this way.

The loader treats a snapshot as untrusted input. It checks that every `SnapRef`
is in bounds, that every program counter lies on an instruction boundary inside
the named function, that stack depth matches the frame layout, and that the
decompressed size is under a limit. Production deployments authenticate
snapshots with a MAC under a per-tenant key.

### Non-serializable values

| Value | Proof of concept | Later |
|---|---|---|
| File, socket, HTTP response, SSE or WebSocket stream, `StreamAccumulator` | Not a clean point. The runtime retries at the next yield. | Re-bindable resources. An SSE stream from a recording proxy becomes a URL key plus a cursor. |
| `HostClosure`, `OpaqueExternalValue` | The run cannot migrate. The error names the variable. | A host-side callback protocol for runs that stay attached to a host process. |
| Media values, `PromptAst`, `LocalIdState` | Add a `SnapshotRustData` trait with a tag registry. These types hold plain data. | |
| `CancellationToken`, `TaskGroupInner` | Not a clean point. | Rebuild the token tree from thread parentage. Serialize the waiter queue. |
| `Frame::Native` continuation | Not a clean point. | Add `fn snapshot(&self) -> (ContinuationTag, Vec<u8>)` to `Continuation`, next to `gc_roots` and `apply_forwarding`. |
| `Future`, `UnscheduledFuture` | Not a clean point when pending. | Serialize the state. Recreate the settlement object and register it with `FutureManager`. |

#### What is live during an LLM call

An audit of the stdlib LLM path produced the following results.

- A plain call `Fn(args)` from BAML code is always non-streaming. It lowers to
  `ai.Agent.new(...).run(Fn@spec(...))`, which performs one `baml.http.send`
  (`baml_compiler2_ast/src/lower_expr_body.rs:1053-1060`). Streaming happens
  only when code names the `Fn@stream` companion. Durable runs use
  non-streaming calls, and no engine change is needed to get that behavior.
- The non-streaming path has no native frames, no spawns, and no futures at
  its HTTP yields. Retry and backoff use `baml.sys.sleep`. Clients, requests,
  the journal, and `reflect.Type` values are plain objects. `ai.Prompt` and
  `ai.OutputFormat` are `RustData`, but they are temporaries that are no longer
  referenced when the request is sent.
- The path has two HTTP yields. The first is `baml.http._send`, and the state
  at that yield is serializable. The second is `Response.text()`
  (`ai/ns_wire/wire.baml:185`). At that yield the `Response` object holds an
  unread `reqwest::Response`, which is a process resource.
- A combined sys-op that returns status, headers, and body text in one
  operation removes the second yield. Every non-streaming client calls `send`
  followed by `text()`, so the change touches about four call sites.
- Three caller choices make a run non-migratable during an LLM call: an
  `on_event` host callback, a tool whose handler is a host function, and
  anything else that places a `HostClosure` in the `Agent` or `Toolbox`. Media
  arguments are `RustData` that holds plain data and need a serializer.
- The streaming path holds an `SseStream` across every `next()` yield. It is
  not a target for snapshots until re-bindable streams exist.
- A callback passed to `.map`, `.filter`, `.find`, or `.reduce` runs under a
  native continuation frame. If the callback yields, the frame is on the stack
  at the yield. A `for` loop over an array uses `baml.iter.ArrayIterator`,
  which is a plain class instance, so `for` loops are serializable today.

One more effect matters for clean points. A local variable slot can still hold
a reference to a consumed `Response` after the last use of the variable. The
serializer sees the slot as a root and the attempt fails until the frame
returns. The retry rule covers this case. Compiler liveness information would
let the serializer treat dead slots as null.

### Snapshot triggers

- **Explicit.** `baml.durable.checkpoint()` is a new sys-op. The engine handles
  it in the `SysOp` arm, in the same way that it special-cases
  `ReflectPackageCompile` (`bex_engine/src/lib.rs:6298`). The thread is parked
  with a known resume action, which is to push `null`. This is the first thing
  to build because tests can trigger it deterministically.
- **Suspend request.** A pause command, SIGTERM, or a cloud control message
  sets a flag. The engine writes a snapshot at the next clean point and ends
  the process with a distinct exit status. This trigger applies to every run
  that a worker hosts, whether or not the run is durable.
- **Policy.** For a durable run only, the engine writes a snapshot at each
  clean yield, subject to a minimum interval and a size threshold. The write
  happens while the run waits on I/O, so it does not add latency to the run.

### Marking a function durable or remote

The proof of concept treats a root call as a durable run when `Function::name`
contains `durable`. The check happens in `BexEngine::call_function`. Only the
root function matters. A durable function that is called from a non-durable
root runs as an ordinary function.

The durable flag does not change how the function body executes, and it places
no restrictions on the code. It is a property of the run. The table shows what
it controls.

| Capability | Any run in a worker | Durable run |
|---|---|---|
| Pause on request, inspect state, resume, migrate, fork | Yes | Yes |
| Automatic snapshot at each clean yield (policy trigger) | No | Yes |
| Recovery after the worker process dies | No. The run is lost. | Yes. The site server resumes it from the latest snapshot. |
| Final snapshot at an uncaught exception | No | Yes |
| Idempotency keys and the effect journal (M2) | No | Yes |
| Durable sleep that releases the worker during a long `sleep` | No | Yes |
| Compiler checks for values held across a yield (M4, `@@durable`) | No | Yes |

An on-request pause works for every run because it costs nothing until it is
used. The automatic behaviors write to storage on every clean yield, so they
are opt-in. The flag does not select where a run executes. The `remote_` prefix
controls that.

A call to a function whose name starts with `remote_` executes on another
worker. All call opcodes converge on `execute_call_from_locals_offset`
(`bex_vm/src/vm.rs:6604`), and the callee name is already in scope at
`vm.rs:6747`. A check before `match callee_kind` (`vm.rs:6769`) drains the
arguments and returns a new `VmExecState::RemoteCall { name, args, return_ty,
throws_ty }`. The check applies only when the callee is not the root frame, so
the worker that receives the call runs the function body normally. This yield
follows the sys-op pattern. It pushes no frame, and the engine later pushes one
result value or injects a throw with `inject_sysop_throw`. Arguments and
results cross the wire as `BexExternalValue`, converted with
`convert_vm_value_to_external_with_type` and
`convert_external_to_vm_value_with_ty`.

The long-term marker is a `@@durable` block attribute. `AttributePosition::Function`
already parses, and adding the attribute requires one entry in
`SCHEMA_ATTRIBUTE_SPECS` (`baml_base/src/language.rs:102`) and a field on
`FunctionMeta`. With the attribute in place, the compiler can run a liveness
analysis at each yield site and report where a durable function holds a
non-serializable value across an LLM call. It can also warn when a durable
function has no clean point between two expensive calls.

### Effects and exactly-once behavior

Snapshots give at-least-once effects. If a process crashes after an HTTP POST
and before the next snapshot, recovery repeats the POST. The design adds two
mechanisms at the sys-op dispatch point.

1. **Idempotency keys.** Each effect gets a key derived from the run id, the
   thread id, and a per-thread effect sequence number. The sequence number is
   part of the thread state in the snapshot, so a repeated effect after
   recovery carries the same key. The HTTP provider sends the key as an
   `Idempotency-Key` header by default.
2. **Effect journal.** A journaling `SysOps` provider appends an intent record
   before an effect and a result record after it. Arguments and results are
   already `BexExternalValue`, which has a wire encoding. After a crash, the
   loader restores the latest snapshot and answers sys-ops from the journal
   tail until the tail is exhausted. The tail only covers the interval since
   the last snapshot, so its length is bounded and no equivalent of Temporal
   Continue-As-New is needed.

Replaying a journal tail requires that execution between the snapshot and the
crash is deterministic given the sys-op results. Time, random, and env are
sys-ops, so single-threaded runs satisfy this. Runs with several threads that
share mutable heap objects can interleave differently. The journal is keyed by
thread and sequence number, and a divergence is detected when the replayed
operation or arguments do not match the record. On divergence the runtime
falls back to at-least-once behavior from the snapshot.

Cooperative migration does not need the journal. The run is suspended at a
clean point where no effect is in flight.

### Cloud execution

A remote start and a migration use the same transfer unit.

```
local process                         cloud
-------------                         -----
build initial snapshot   --POST-->    run store (object storage + index)
  (program, function,                     |
   args, zero frames)                 worker pool: generic `baml-worker`
handle = RunHandle(run_id)                load snapshot -> run -> snapshot ...
handle.result(timeout)   <--poll--    result stored under run_id
```

- The worker is a generic binary that contains the VM and the native sys-ops.
  User code arrives inside the snapshot or is fetched by program hash from a
  content-addressed store (`bex_cache` is already content-addressed). There is
  no per-project image build.
- `RunHandle` follows Modal semantics: `result(timeout)`, `status()`,
  `cancel()`, and `RunHandle.from_id(run_id)`. The `Bex` trait already has
  `start_run` and `cancel_run` (`bex_project/src/bex.rs:47`, `:64`), and
  `bex_events::run` already defines `RunStore` and run identifiers.
- Generated SDK clients gain `b.durable_research.start(args)` next to the
  existing call and stream methods.
- Inside BAML, a remote start returns a serializable handle value, so a durable
  function can hold a handle across its own snapshots. The surface syntax is an
  open question. A library function is enough for the proof of concept.
- `baml.env.get` resolves against secret bindings attached to the run in the
  cloud. Secret strings that reach the heap are written into snapshots, so
  snapshots are encrypted at rest.

Several capabilities follow from owning the runtime.

- **Durable timers.** The section "Durable sleep" describes how a long
  `baml.sys.sleep` releases the worker. In the cloud, a scheduler service takes
  the role of the site server's timer.
- **Signals.** A run can await a named external event, such as a human
  approval. The run is parked in storage, not in memory, until the event
  arrives.
- **Platform-held requests.** The cloud HTTP provider can route LLM requests
  through a proxy that stores the response under the idempotency key. A run
  that crashes during an LLM call resumes and receives the stored response, so
  the call is billed once. If the proxy also records SSE events, an `SseStream`
  becomes re-bindable as a key plus a cursor, which turns the middle of a
  streaming call into a clean point.
- **Fork.** Loading one snapshot twice under two run ids forks the run. This
  supports search over agent trajectories, retrying a step with a different
  prompt, and comparing two code versions from an identical starting state.

### Durable sleep

A long `baml.sys.sleep` in a durable run releases the worker process. The run
exists only as a snapshot file until the deadline, and then it resumes in a
new process. The user writes an ordinary `sleep` call. No new API is needed.

This feature is a pause that the run requests itself, combined with a resume
that the site server triggers with a timer. It reuses the pause mechanism. A
durable thread that waits in `sleep` already parks with kind `sleep` and an
absolute deadline, and a resumed worker already sleeps the remaining time,
pushes `null`, and continues.

**Engine.** When a durable thread reaches `baml.sys.sleep` with a duration at
or above the threshold, it does not start the in-process timer. It registers a
long wait with the pause coordinator, with `wake_at = now + duration`. The
coordinator suspends the run when every live thread of the run is parked in a
long wait. The wake time of the run is the earliest deadline among its
threads. If any thread is still running, the sleeping thread waits in process
as it does today, and the coordinator evaluates the condition again each time
a thread enters or leaves a wait.

**Threshold.** The default threshold is 5 seconds, set by a worker flag. A
suspend costs one snapshot. A resume costs a process start and a program load.
The threshold keeps that overhead small relative to the sleep. A later version
can derive the threshold from the measured resume cost.

**Worker.** The worker writes the snapshot in the same way as for a pause. The
`paused` event carries an added field `wake: { reason: "sleep", at_ts }`. The
process exits with code 75.

**Site server.** The site server sets the run status to `sleeping` and stores
`wake_at` in `meta.json`. A green thread waits until `wake_at` and then runs
the normal resume path. The deadline is an absolute time, so a restarted
server rebuilds its timers from the run store and resumes overdue runs at
once. Absolute deadlines also avoid the clock discontinuity that relative
timers show after a suspend.

**Edge cases.**

- A blocked snapshot makes the thread fall back to an in-process sleep. The
  worker reports the reason once. The result of the program is unchanged.
- A manual resume before the deadline starts a worker that sees the remaining
  time. The run suspends again when the remaining time is at or above the
  threshold.
- A cancel of a sleeping run changes the run record only, because no process
  exists.
- No effect is in flight during a sleep, so a suspend at this point cannot
  repeat an effect.

**Waits on remote results (later phase).** Phase 3 suspends a run only for a
sleep and wakes it only by its timer. The same rule can apply to a thread that waits for
a `remote_` call. The parent suspends, and the arrival of the `remote_result`
wakes it. The site server already stores results for a run that has no process
and passes them with `--remote-result` at resume. With both rules, a run that
waits two hours for a timer or for a remote result holds no process and no
memory.

The wait bookkeeping of the engine already supports this later phase. The
self-suspend rule would drop its "at least one thread sleeps" clause, and the
worker would report a wake reason of `remote_result`. The rule already looks
through a wait that is satisfied: a run whose worker holds a result that no
thread has taken yet is not idle, so an early wake delivers the result before
the run can suspend again.

**A resumed run replays what it missed in order.** A run that had no process
can come back to several waits that are already satisfied: remote results that
arrived and sleeps whose deadline passed. An uninterrupted run saw these one at
a time, and the effects of one landed before the next: a `race` cancelled its
losers, a timeout cancelled its work. If every satisfied thread simply
continued after the restore, the task scheduler would pick the winner of a
race.

- The site server stores the arrival time of every result and passes it to the
  worker. The engine merges the results with the expired sleep deadlines into
  one queue ordered by time.
- A driver task releases one item at a time. It releases the next item when
  the run is quiescent: every live thread is blocked in a wait that can be
  re-issued, and no operation is in flight. A thread whose wait is already
  satisfied does not count as blocked, because it is about to run. The engine
  decides this from the wait itself (the future has settled, the cancel token
  has fired, the task group granted the slot, the host holds the result), not
  from a delay.
- While the queue is not empty, no other sleep or remote wait completes. What
  happens now is later than everything that was recorded.
- The replay keeps a virtual clock, the time of the last released item. A
  sleep that a thread starts during the replay begins at that time. If it ends
  before the last recorded item, it joins the queue and is not slept again, so
  a thread that slept one second between two results still wakes between them.
  After the queue is empty the clock is the real clock. A run that is resumed
  late does not rush through the sleeps it starts afterwards.
- A pause in the middle of a replay needs no extra state. An item that was not
  released is still a sleep with a past deadline or a result that the worker
  holds with its time, and the next restore builds the same queue.

**Identifiers that survive a recovery.** The site server answers a remote call
that it has seen before from the stored result, by call id. A recovery starts
again from an older snapshot and makes some calls a second time, so a call
must get the same id in every execution. A counter that all threads share
does not give that: concurrent threads take numbers in scheduling order. The
engine therefore gives every thread a path in the spawn tree (`0` for the
root, the parent's path plus the spawn index for a child) and counts the
remote calls per thread. Both are part of the thread's snapshot state, and the
call id is derived from them. A thread spawns its children and makes its calls
one after the other, so the path and the count do not depend on the scheduler.

**A child run has one owner.** A fork waits on the child runs of its source.
The child belongs to the run that dispatched it, and the other run holds an
inherited wait. A run does not cancel a child through an inherited wait, and
it does not cancel a child that another run of the site still waits on. A
resumed worker reports the remote waits of its restored threads, and the site
server settles each one from a stored result, from the child that still runs,
or with a new dispatch from the recorded arguments. A production system would
keep the waiters of a child on the child's own site, so that the rule also
covers a fork that moved to another site and is cancelled there.

**Resume must be idempotent.** A timer can fire late or more than once, for
example after a server restart, and a user can press Resume while a timer is
about to fire. The resume path changes the run status from `sleeping` to
`starting` with a compare-and-set and ignores a request that finds any other
status. Two workers must never start from one snapshot under one run id.

**The site server owns the deadline.** The worker reports the remaining
duration, and the site server computes `wake_at` on its own clock. Clock skew
between a worker machine and the server then has no effect.

**Waits of unknown length.** A length threshold only works when the duration
is known in advance, as it is for `sleep`. For a wait on a remote result or on
an external signal, the run suspends after every thread has been idle for an
idle timeout. Restate uses this rule for its handlers.

**How other products handle a long wait.** The facts below were checked
against product documentation on 2026-09-20.

| Product | What suspends | Trigger | State capture | Restore |
|---|---|---|---|---|
| Trigger.dev Cloud | the task process | `wait.for`, `wait.until`, or `triggerAndWait` longer than 60 seconds. Waits over 5 seconds are not billed. | CRIU checkpoint of the process | timer or subtask completion restores into a new environment. Not available when self-hosted. |
| Temporal, Azure Durable Functions, Inngest, Cloudflare Workflows, Vercel Workflow | the workflow | every `sleep`, with no threshold | a timer record in the service | the function runs again from the start and replays its journal |
| Restate | the handler invocation | an inactivity timeout, about one minute by default | journal | journal replay |
| DBOS | nothing. The process keeps sleeping. | none | the wake time in Postgres | after a crash, the sleep step waits for the remainder |
| Fly.io Machines | the whole VM | explicit suspend, or no traffic | Firecracker memory snapshot, 2 GB RAM limit, discarded on deploy or host migration | a few hundred milliseconds, on a request |
| E2B | the whole sandbox | explicit pause or sandbox timeout | memory and filesystem, about 4 seconds per GiB to pause | about 1 second |
| CodeSandbox | the whole VM | inactivity timeout | Firecracker memory snapshot | 0.5 to 2 seconds |
| Modal | nothing. A sleeping function keeps its container. | none | memory snapshots apply to cold starts only | not applicable |
| Cloudflare Durable Objects | the object | 10 seconds without events | none. In-memory state is discarded. | an alarm, a request, or a WebSocket message |
| LangGraph | the graph thread | `interrupt()` | application checkpoint | the interrupted node runs again from its start. There is no timed resume. |

Trigger.dev is the closest match. It snapshots the running process instead of
replaying a journal, it applies a threshold so that short waits stay in
process, and it uses one mechanism for timers and for waits on child tasks.
Its snapshot is a process image, so remote connections are stale after a
restore and the checkpoint size grows with process memory. The VM products
suspend on idleness because they cannot see what the program is waiting for.

A BEX snapshot of the demo run is about 500 bytes and is written in under one
millisecond. The restore cost is dominated by loading the program, about 1.26
seconds in a debug build, and that cost sets the lower bound for a useful
threshold. A bytecode cache or an embedded program removes most of it.

**Timeline.** A sleeping interval is drawn as a gap with its own style and the
label `sleeping until <time>`, so it reads differently from a pause that the
user requested.

### Code versions

The proof of concept refuses to load a snapshot unless the program hash and the runtime
build id match exactly. Embedding the program in the snapshot makes this
restriction easy to live with, because a snapshot always carries the code it
needs. Old versions cost storage only.

The later design pins a run to its program hash by default, as Unison and
Restate do. An opt-in upgrade path follows:

1. The compiler emits a content hash per function and a frame-shape descriptor
   per yield site, which lists live slots and their types.
2. A snapshot identifies each frame by function hash and yield-site id, not by
   a raw byte offset.
3. A run can move to a new program when every function on its stacks is
   unchanged, or when the frame shapes at the parked sites are compatible.
4. Otherwise the run stays pinned. Tooling lists in-flight runs per program
   hash and supports forced cancel.

The snapshot format version and the runtime build id are separate from user
code hashes. CI keeps golden snapshot fixtures for each format version.

## Debugging and tooling

### Inspecting a snapshot

A snapshot embeds the program, which includes line tables
(`LineTableEntry`, `bytecode.rs:1856`) and local variable names
(`DebugLocalScope`, `bytecode.rs:1870`). The inspector can therefore show
source-level state without running any code and without the original project
directory.

`baml snapshot inspect <file>` prints:

- The header: run id, sequence number, program hash, runtime build, sizes.
- One section per thread with a stack trace. Each frame shows the function,
  the source location, and the named locals with their values.
- The parked state of each thread, including sys-op arguments.
- Heap statistics: object count and bytes by type and by class, and the
  largest retained subgraphs.
- For a failed snapshot attempt, the path from a root to the value that
  blocked it.

Two export flags cover visualization.

- `--dot` writes a Graphviz graph. Agent heaps are usually small enough that
  the whole graph is readable.
- `--heapsnapshot` writes the V8 `.heapsnapshot` JSON format. Chrome DevTools
  loads it in the Memory panel and provides a summary by class, a containment
  tree, retainer paths, dominator sizes, and a comparison view between two
  files. The comparison view serves as a diff between two checkpoints. Hermes
  and Bun already emit this format from engines other than V8. Stack frames
  map to synthetic nodes that point to their locals. The exporter is a small
  amount of code because the format is two flat integer arrays and a string
  table.

The profiling store is already queryable with SQL through `baml query`. The
same layer can expose `snapshot_frames` and `snapshot_objects` relations, so
that questions such as "which runs are parked inside `WriteSection`" become
queries.

### Timeline and time travel

A durable run that keeps its snapshots forms a timeline. The playground
already has a run store and a WebSocket session
(`baml_lsp_server/src/playground_runs.rs`). The proposed view shows:

- One row per checkpoint with its duration, the effects since the previous
  checkpoint, and their payloads.
- The stack and locals of a selected checkpoint.
- A diff of two adjacent checkpoints.
- Two actions: fork from this checkpoint, and resume from this checkpoint with
  the current code. The second action is subject to the version rules above.

Replay-based systems rebuild state by running code, so they cannot show the
state of a checkpoint without execution. A snapshot design shows it directly.

### Debugger attachment

The repository has no Debug Adapter Protocol implementation today. A
post-mortem adapter over a snapshot is the smallest useful one. It follows the
model of `dlv core` and the lldb `coreFile` option:

- `launch` takes a snapshot path and reports a stopped state.
- `threads`, `stackTrace`, `scopes`, and `variables` read from the snapshot.
  Heap objects expand lazily.
- `continue` resumes the snapshot in a local engine. Sys-ops run live or come
  from a recorded journal.
- `stepBack` loads the previous checkpoint, which gives coarse time travel in
  the standard VS Code interface.

When a durable run ends with an uncaught exception, the runtime writes a final
snapshot at the throw site. That file serves as a core dump. A developer can
open it in the debugger, fix the code, and resume the run from the previous
checkpoint.

A snapshot can also serve as a test fixture. A test loads a captured state,
supplies mock sys-op results, and asserts on the outcome.

### Snapshot cost

The cost is proportional to the subgraph reachable from the run, not to the
size of the shared heap. The estimates below use published throughput numbers
for pointer-rich graphs (0.5 to 2 GB/s for Borsh-class encoders, about 500 MB/s
per core for zstd level 1) and have not been measured on BEX.

| Reachable heap | Walk and encode | zstd -1 | Store | Total |
|---|---|---|---|---|
| 1 MB | about 1 ms | about 2 ms | 2 to 10 ms to a local disk or a database row, 20 to 200 ms to S3 Standard | under 15 ms locally |
| 100 MB | about 100 ms | about 200 ms | about 0.4 s for 33 MB | about 0.7 s |
| 1 GB | about 1 s | 0.3 to 2 s | 0.5 to 4 s | 2 to 7 s |

An agent heap is mostly message history and usually falls between 50 KB and
2 MB. An LLM call lasts 1 to 60 seconds. A snapshot per clean yield therefore
costs well under one percent of wall time. The threads of the run are already
parked when the snapshot is taken, so the write overlaps the wait. A storage
PUT costs a small fraction of the price of the LLM call that it protects.

Large heaps need incremental snapshots. The old generation already has a card
table for the write barrier (`heap.rs:166`). A delta snapshot can write the
dirty cards and the new allocations against a base snapshot. This work is not
planned until measurements show a need.

The runtime records walk time, encode time, compressed bytes, and object count
for each snapshot as `bex_events` markers, so that the cost is visible in the
existing profiling tools.

## Proof-of-concept demo

The goal of the proof of concept is to learn which parts of the design work
and which do not, with a demo that makes the behavior visible. A React app
starts a BAML function on a server, pauses it, shows the paused source line and
the full program state, resumes it in a new process, and reports how long each
step took. A `remote_` function runs on a second site, and the app shows the
logs of both sites next to each other.

### Architecture

```
  React app (Vite)
    │  HTTP commands + one SSE stream per site
    ├──────────────────────────────┐
    ▼                              ▼
┌──────────────────────────┐    ┌──────────────────────────┐
│ Site server "local"      │    │ Site server "cloud"      │
│ BAML program, always on  │◀──▶│ BAML program, always on  │
│ :8787                    │HTTP│ :8788                    │
│ run store .baml/runs/    │    │ run store .baml/runs/    │
└───────┬──────────────────┘    └───────┬──────────────────┘
        │ stdio, JSON lines             │ stdio, JSON lines
        ▼                               ▼
  worker process (one run)        worker process (one run)
  BexEngine + snapshot code       BexEngine + snapshot code
```

There are three components.

**Site server.** A site server is an always-on BAML program. The demo runs two
instances of the same program with different `SITE` and `PORT` values, and
each instance knows the URL of its peer. A site server does four things:

- It serves an HTTP API for run control and an SSE stream of events.
- It starts worker processes with `baml.sys.start_process`, reads JSON event
  lines from each worker's stdout on a green thread, and writes JSON command
  lines to the worker's stdin.
- It keeps the run store: `meta.json`, `events.jsonl`, the binary snapshots,
  and the JSON state dumps for each run.
- It forwards `remote_` calls and migrating snapshots to its peer over HTTP.

A site server never executes user code and never parses a binary snapshot.

The transport follows the `BoundaryML/codemode` project, which is a working
BAML server with a React front end. That project uses HTTP routes for commands
and `baml.http.Response.new_streaming` for a Server-Sent Events stream. Its
README records that the BAML WebSocket accept path returns HTTP 500 on current
toolchains, with a repro in `repro/ws_accept.baml`. The demo therefore uses no
WebSockets. Commands such as pause and resume are plain requests. The event
stream is the only part that needs a push channel, and SSE covers it.

**Worker.** A worker is one OS process that hosts one run segment. It is a
hidden CLI subcommand that embeds `BexEngine`:

```bash
baml worker --project ./demo --run <run-id> --start durable_plan_trip --json-args '{"city":"Lisbon"}'
```

```bash
baml worker --project ./demo --run <run-id> --resume .baml/runs/<run-id>/snap-0001.bamlsnap
```

The worker writes one JSON event per line to stdout and reads one JSON command
per line from stdin. It opens no sockets. This also makes the worker usable
from a terminal without any server, which helps during development of the
snapshot code. A pause writes a snapshot and ends the process. A resume starts
a new process, so the app shows a different PID for the same run. One run per
process has three benefits for the proof of concept:

- The existing engine-wide park mechanism is the pause mechanism. No per-call
  thread registry is needed.
- The question of globals shared between runs does not arise.
- Ending the process is a visible proof that the state lives in the file.

Process start and program load are part of resume latency in this model. The
worker reports them separately so that the cost is known. A later version keeps
a pool of idle workers that accept many runs.

**React app.** The app is a Vite project. The Vite dev server proxies
`/local/*` and `/cloud/*` to the two site servers. The app has five panels:

- A source view that highlights the line of the most recent yield and the
  paused line.
- Controls: start, pause, resume, resume on the other site, end the worker
  process, and fork.
- A state tree for a paused run: threads, frames, named locals, expandable
  objects, and heap totals by object kind.
- A timeline with one lane per site. The section "Timeline view" describes it.
  Pause, snapshot, and resume timings appear on the timeline markers.
- One log lane per site. Each line shows the worker PID, the run id, and the
  text.

### Protocol

**App to site server.** Commands are HTTP requests. Every command returns
immediately with the current run record. Results that take time arrive on the
event stream.

| Request | Purpose |
|---|---|
| `POST /api/runs { function, args }` | Create a run and start a worker. |
| `GET /api/runs`, `GET /api/runs/:id` | Run records: status, PID, last position, snapshots, timings. |
| `POST /api/runs/:id/pause` | Request a pause. The response has status `pausing`. |
| `POST /api/runs/:id/resume { site? }` | Resume from the latest snapshot on this site, or send the snapshot to the peer and resume there. |
| `POST /api/runs/:id/kill` | End the worker process without a snapshot. |
| `POST /api/runs/:id/fork { snapshot }` | Create a new run from a snapshot of this run. |
| `GET /api/runs/:id/snapshots/:n/state` | The JSON state dump of a snapshot. |
| `GET /api/source?file=` | Source text for the source view. |
| `GET /api/events` | SSE stream of all events on this site. The first event is a full list of runs. |

**Site server to site server.**

| Request | Purpose |
|---|---|
| `POST /api/remote/runs { function, args, parent_run, callback }` | Start a child run for a `remote_` call. |
| `POST /api/runs/:id/remote_result { child_run, value or error }` | The callback that delivers a child result to the parent's site. |
| `POST /api/runs/import { meta, snapshot_base64 }` | Receive a migrating run. Agent snapshots are small, so a base64 body is sufficient. |

**Worker events on stdout.** The site server tags each event with the site,
the run id, and the PID, appends it to `events.jsonl`, and sends it to every
SSE subscriber.

| Event | Purpose |
|---|---|
| `hello { pid }` | The worker started. |
| `log { stream, text }` | Output of `baml.io.print*`. |
| `position { thread, file, line, function, reason }` | Emitted at each yield. |
| `remote_call { child_run, function, args }` | A `remote_` call was reached. |
| `pausing { waiting_on }` | The pause is delayed by an operation in flight. |
| `blocked { path_to_value }` | A snapshot attempt failed. The worker retries at the next yield. |
| `paused { snapshot_path, state_path, stats }` | The snapshot is written. The process ends next. |
| `resumed { stats }`, `completed { value }`, `failed { error, stack }` | Lifecycle. |

**Worker commands on stdin.** `pause`, `cancel`, and
`remote_result { child_run, value or error }`.

### Pause mechanics

The worker pauses a run in four steps.

1. It sets a pause flag and the engine-wide `park_requested` flag. A VM in a
   compute loop observes `park_requested` at control-flow instructions and
   returns `VmExecState::EarlyYield` within 4096 checks
   (`bex_vm_types/src/lib.rs:267`, `bex_heap/src/gc_policy.rs:18`). At that
   yield the program counter is written back to the frame and the operand
   stack is consistent (`bex_vm/src/vm.rs:7769-7777`).
2. The engine loop checks the pause flag before each call to `exec()` and
   holds the thread there. This gate stops a thread that returns from a sys-op
   before it runs further.
3. A thread that is waiting inside an async sys-op records its parked state
   (`op`, externalized arguments, and a deadline for `sleep`) on its
   `BexThread` before it releases the heap permit. A `sleep` and a `remote_`
   wait can be re-issued after a resume, so they count as parked. Any other
   operation in flight delays the pause until it completes, and the worker
   reports `pausing { waiting_on: "baml.http.send" }`.
4. When every thread is parked, the worker calls
   `HeapPermitManager::request_park()` to hold all threads, writes the
   snapshot and the JSON state dump, reports `paused`, and ends the process.

The JSON state dump uses existing pieces. `capture_stack_trace`
(`vm.rs:5102`) gives the frames and lines. `Function::local_names` and
`Function::debug_locals` map stack slots to variable names. No runtime reader
of that debug information exists today, so the worker adds one.
`TraceHeap::copy_named_values_bounded` (`bex_engine/src/trace_heap.rs:183`)
renders values and already handles cycles and `RustData`.

### Remote calls in the demo

1. The VM yields `RemoteCall`. The worker emits `remote_call` and logs
   `waiting for remote run <child> on site cloud`.
2. The local site server sends `POST /api/remote/runs` to its peer with the
   function name, the arguments, and a callback URL.
3. The cloud site server starts a worker with `--start remote_fetch_weather`.
   The child run produces its own events on the cloud event stream.
4. When the child completes, the cloud site server calls the callback. The
   local site server stores the result in the parent's run record and writes a
   `remote_result` command to the parent's worker if one is running.
5. The parent thread is parked at `RemoteCall { child_run }` while it waits.
   This state is serializable, so the parent can be paused during the wait.
   When the parent resumes, the site server passes the stored result to the
   new worker at startup.

Step 5 is the central scene of the demo. The parent starts on the local site
and calls the cloud site. The user pauses the parent and its process ends. The
cloud lane continues to log. The user resumes the parent in a new process, and
it continues with the result.

A `remote_` call inside `spawn { }` needs no extra mechanism for execution.
The spawned BAML thread yields `RemoteCall` and waits, and the parent thread
continues. The events `thread_started { thread, parent_thread }` and
`thread_ended { thread }` let the app draw the branch. Pausing such a run
requires snapshot support for several threads and for pending futures, which
is phase P3.

Both sites load the same project directory in the proof of concept, so a
remote start sends a function name and arguments. The general design sends an
initial snapshot that carries the program.

### Timeline view

The app draws a timeline from the event streams of both sites. Every event
carries a timestamp, the site, the run id, the parent run id, the segment
number, and the thread id. A segment is one worker process in the life of a
run.

- Each site is a lane. Time runs left to right.
- Each run segment is a bar in the lane of the site that hosts it. The bar
  shows the PID. A run that has been paused and resumed appears as several
  bars with a gap between them. The gap is drawn as a dashed line and labeled
  with the snapshot size, because no process exists during that interval.
- The interval in which a thread waits for a remote result is drawn in a
  lighter shade.
- A `remote_call` event draws a connector from the parent's bar to the start
  of the child's bar in the other lane. The child's completion draws a
  connector back to the point where the parent received the result.
- A `spawn` draws a branch inside the run's bar, so a `spawn { remote_f() }`
  appears as a branch that leaves the parent and crosses to the other lane
  while the parent's main thread continues.
- A migration draws a connector from the end of the last segment on one site
  to the start of the next segment on the other site.
- Markers on a bar show snapshots, blocked snapshot attempts, and pause
  requests. Selecting a marker shows its timing breakdown and opens the state
  tree for that snapshot.

The two site servers run on one machine in the demo and share a clock. A
deployment on separate machines needs a clock offset estimate per site, which
the app can take from the round-trip time of a request.

The timeline is drawn with SVG in React and needs no charting library.

### Demo program

This program compiles and runs with the current CLI.

```baml
class TripPlan {
    city: string
    ideas: string[]
    weather: string
}

function remote_fetch_weather(city: string) -> string {
    baml.io.println("[cloud] looking up weather for " + city);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(3000n));
    "sunny in " + city
}

function durable_plan_trip(city: string) -> TripPlan {
    let ideas: string[] = [];
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}
```

The demo project also contains `plan_trip`, which has the same body as
`durable_plan_trip` under a name without the marker. The pair supports a
comparison. When the user ends the worker process of each run with `kill -9`,
the site server reports the plain run as lost and resumes the durable run from
its latest automatic snapshot.

A program that uses `while` and `for` loops, class instances, strings, arrays,
maps, `println`, and `sleep` parks with no native frames and no `RustData`, so
it needs no runtime changes beyond the snapshot code. Phase P4 adds an LLM
call.

### Measurements

The worker reports these numbers for each pause and resume. The app shows them
and the server stores them in `events.jsonl`.

| Phase | Measurement |
|---|---|
| Pause | Time from the request until all threads are parked, and the operation that delayed it. |
| Snapshot | Graph walk time, encode time, compress time, write time, object count, raw bytes, compressed bytes, program bytes. |
| Blocked attempts | Count, and the root-to-value path of each failure. |
| Resume | Process start time, program load time, decode and allocate time, time until the first `exec()` call. |
| Remote call | Argument bytes, result bytes, dispatch latency, child run time. |

### Runtime changes required

| Change | Location | Size |
|---|---|---|
| `bex_snapshot` crate: format, writer, loader, validator, JSON state dump | new crate | large |
| Scoped pointer translation for Borsh | `bex_vm_types/src/heap_ptr.rs` | small |
| In-place object overwrite for the loader | `bex_heap` | small |
| `pause_all()` and a registry of live `BexThread` cells | `bex_engine/src/lib.rs:3414`, `:4040`, `:5877` | small |
| Pause gate and recorded parked state in the engine loop | `bex_engine/src/lib.rs:6113`, `:6409` | medium |
| `VmExecState::RemoteCall` and its engine arm | `bex_vm/src/vm.rs:6769`, `:1523`, `bex_engine/src/lib.rs:6258` | small |
| Public access to `capture_stack_trace`, `locals_offset`, `Frame::function`, and the typed value converters | `bex_vm/src/vm.rs:5102`, `:315`, `:464`, `bex_engine/src/conversion.rs` | trivial |
| Log forwarding through `SysOpsBuilder::from_ops(SysOps::native()).with_io_instance(...)`, modeled on `PlaygroundIo` | worker only | none in the engine |
| JSON arguments and results through `baml_exec::dispatch::build_args_from_signature` and `baml.json.serialize` | worker only | none in the engine |
| `baml worker` hidden subcommand with stdio JSON lines | `baml_cli/src/commands.rs:242` | medium |
| Thread start and end events for the timeline | `bex_engine/src/lib.rs:4040`, `:5877` | small |

## Milestones

Phases P0 and P1 have no dependency on each other and can proceed in parallel.

**P0: plumbing without snapshots.**

- The site server in BAML: the HTTP API, the SSE stream, worker start through
  `baml.sys.start_process`, the run store, and peer forwarding.
- The `baml worker` subcommand with `--start`, JSON events on stdout, and JSON
  commands on stdin.
- Log forwarding, `position` events at each yield, and thread events.
- `RemoteCall` routing from one site server to the other, both for a direct
  call and for `spawn { remote_f() }`.
- The React app with the source view, the log lanes, and the timeline.
- Result: the demo program runs from the app. The local lane shows
  `waiting for remote run`, the cloud lane shows the remote function, and the
  timeline shows the connector between the lanes.

**P1: snapshot core.**

- The `bex_snapshot` crate, pointer translation, and the in-place loader.
- Export and import of frames, stack, and exception vectors on `BexVm`. The
  writer refuses when a `Frame::Native` is present.
- A `baml.durable.checkpoint()` sys-op that gives tests a deterministic
  snapshot point.
- `baml snapshot inspect` with text and JSON output.
- Tests: a round trip inside one process, then a snapshot in one process and a
  resume in another. Each test compares the final result with an uninterrupted
  run. The test programs cover nested calls, `while` and `for` loops, class
  instances, arrays, maps, closures, and a `catch` block.

**P2: pause and resume in the demo, single thread.**

- `pause_all`, the pause gate, recorded parked state, and re-issue of `sleep`
  with the remaining time.
- `worker --resume`, the state tree, the paused line, and timing data on the
  timeline markers.
- Pause during a blocking remote wait, and resume on the other site.
- Result: the central scene of the demo works, and the first cost numbers
  exist.

**P3: threads and futures.**

- Snapshot support for several `BexThread`s in one run, for pending and
  settled `Future` objects, and for the cancel token tree.
- Registration of restored futures with `FutureManager`.
- Result: a run that has a pending `spawn { remote_f() }` can pause and
  resume, and the timeline shows the branch across the pause.

**P4: findings and LLM calls.**

- The combined send-and-read-text HTTP sys-op, and its use in the non-streaming
  clients.
- An LLM call in the demo program, with no `on_event` callback and no host
  tools.
- `blocked` diagnostics in the app, which show the reason when a run cannot
  pause at a given point.
- A written list of constructs that block snapshots in practice. This list
  sets the priority of the next work items, such as serializable array
  continuations and media values.

**P5: demo extensions.**

- Fork from a snapshot.
- The name-based durable check and policy snapshots at each clean yield.
- Stepping back to an earlier checkpoint from the timeline.
- Recovery after `kill -9`. The site server detects the worker exit through
  `Process.wait`, resumes a durable run from its latest policy snapshot, and
  marks a plain run as lost. The demo runs `durable_plan_trip` and `plan_trip`
  side by side to show the difference.
- `--heapsnapshot` export for Chrome DevTools.

**After the proof of concept.**

- M2, real agents: native continuation serialization, idempotency keys, and
  the effect journal provider.
- M3, cloud: a worker pool, program transfer by hash, `RunHandle`, SDK `start`
  methods, durable timers, signals, and the recording HTTP proxy.
- M4, versions and debugger: per-function hashes, frame-shape descriptors, the
  `@@durable` attribute with compiler checks, the DAP adapter, and playground
  integration.

## Design tradeoffs

**Snapshot instead of journal and replay.** Replay places a determinism
requirement on all orchestration code and makes every refactor a risk for
in-flight runs. Golem shows that replay is fragile even inside a sandbox. A
snapshot restores real state, so user code has no such constraint, resume time
depends on heap size and not on history length, and state is inspectable
without execution. The cost is a serializer for VM state and a strict tie
between a snapshot and its program. The journal remains in the design as a
bounded tail for exactly-once effects.

**Language-level snapshot instead of process-level snapshot.** CRIU or
Firecracker would capture a BEX process without any VM work. Those images are
hundreds of megabytes, do not preserve connections across hosts, cannot be
inspected at source level, and cannot be forked cheaply. A language-level
snapshot of an agent is kilobytes to megabytes.

**Finding clean points at runtime instead of at compile time.** The proof of
concept attempts a snapshot and retries on failure. This needs no compiler
work and produces a precise diagnostic. It does not guarantee that a run ever
reaches a clean point. The `@@durable` analysis in M4 provides that guarantee.

**Embedding the program in each snapshot.** This makes snapshots
self-contained and makes version pinning automatic. It increases snapshot size
by the size of the program. The cloud path stores programs by hash and embeds
only the hash.

**Name-based marking.** A substring check is imprecise and is not a stable
interface. It is acceptable for a proof of concept because it requires no
change to the parser, the HIR, or the SDK generators. Until phase P5 the
marker has no observable effect, because pause and resume work for every run.

**One worker process per run in the demo.** This choice reuses the engine-wide
park mechanism, avoids shared globals, and makes a pause visible as a process
exit. It adds process start and program load to every resume, and it is not the
production model. The worker reports those costs separately so that the
snapshot cost can be read on its own.

**Site servers outside the runtime internals.** A site server handles JSON
messages, files, and child processes only. The worker owns snapshot encoding
and the state dump. This keeps the server small enough to write in BAML and
keeps the binary format private to one crate while it changes.

**HTTP and SSE instead of WebSockets.** Commands are request and response
pairs, and only the event stream needs a push channel. SSE is one-directional,
reconnects automatically in the browser, and works on the current BAML
toolchain. The BAML WebSocket accept path does not work on current toolchains.

**Stdio between the site server and its workers.** A worker needs no port, no
registration step, and no HTTP client. The site server owns the process handle,
so ending a worker is a call to `Process.kill`. The limitation is that a site
server can only host workers on its own machine, which is the intended model:
each machine runs one site server.

## Open questions

1. **Globals.** Globals are frozen after `$init`, but the objects that they
   reference can be mutable. The proposal snapshots globals as roots and skips
   `$init` on restore, which assumes one engine per resumed run. An engine that
   hosts several runs needs a rule for shared global objects.
2. **Program counter encoding.** Decided: a snapshot stores an instruction
   index into `Bytecode::instructions`, not a byte offset into `CompactCode`.
   This removes the dependency on the compact lowering. The work item is a
   mapping between the two forms. Until that mapping exists, the proof of
   concept stores the byte offset and relies on the runtime build id check.
3. **Dead stack slots.** A slot that holds a consumed resource blocks a
   snapshot until its frame returns. Phase P4 measures how often this happens.
   The fix is liveness information from the compiler.
4. **Site server in BAML.** The design depends on `baml.sys.start_process`
   with long-lived pipes and on several concurrent SSE subscribers. The
   codemode project exercises the SSE part. Phase P0 shows whether the process
   pipes hold up under a worker that runs for minutes.
5. **Runtime-compiled code.** Functions created through `reflect` are runtime
   heap objects and serialize through `ObjectWire`, but `PackageIndex` and
   `DynDispatchTables` are keyed by pointer and need a rebuild step.
6. **Finalizers.** The `CleanupLatch` bit and the pending finalizer queue must
   keep run-once semantics across a migration.
7. **Trace continuity.** Call ids and `bex_ref_seed` are scoped to a process.
   The resumed process must emit events that join the original run in the
   profiling store.
8. **Secrets.** Encryption at rest protects snapshots in storage. A later
   option is to treat values returned by `baml.env.get` as re-bindable handles
   so that secret bytes are never written.
9. **Surface syntax for remote start.** The proof of concept uses the
   `remote_` name prefix. Candidates for the final syntax are a library
   function, a method on function references, and a placement argument on
   `spawn`.
10. **In-flight operations at pause.** The proof of concept waits for an HTTP
    operation to complete before it pauses. The alternative is to cancel the
    operation and issue it again after the resume, which is only safe for
    idempotent requests.
