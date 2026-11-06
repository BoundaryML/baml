# Runtime v2: interfaces

Status: draft for review. Decisions and their rationale: `documents/runtime-v2-decisions.md` (numbers below refer to it). This file fixes the contracts that every later job codes against. Anything marked **Open** needs a decision before code depends on it.

Ground rules that every interface assumes:
- One thread owns a heap and runs all BAML code of a program instance (decision 1). Nothing in v2 is `Send` unless stated.
- Objects never move (decision 3). A pointer to a live object is valid for the object's lifetime; no handles, no reloads.
- Every type is concrete at pack time (decisions 2 and 5): generics are monomorphized, so each interface below talks about concrete types only.
- Semantics are the interpreter's. The differential suite is the oracle.

---

## 1. Object layout

### 1.1 Representation of each BAML type

"Slot" is how a value of that type is stored in a local, a field, or a list element.

| BAML type | Slot | Notes |
|---|---|---|
| `int` | `i64`, 8 bytes | Keeps i63 semantics: arithmetic checks the interpreter's `[INT_MIN, INT_MAX]` range and raises the same overflow error. The raw width does not change the observable range. |
| `float` | `f64`, 8 bytes | Raw, never boxed. |
| `bool` | `u8`, padded to 8 in fields | |
| `null` | no storage | Only meaningful inside optionals and unions. |
| enum | `u32` variant index, padded to 8 | BAML enums carry no payload. |
| `string` | pointer to a string object, 8 bytes | String objects are Flat, Slice or Concat (decision 9). Phase 1: Flat only. |
| `bigint`, `Uint8Array`, media, `Resource`, `PromptAst`, `Type`, `$rust_type` | pointer to a heap object of that kind, 8 bytes | |
| class `C<args>` | pointer to a `C<args>` object, 8 bytes | Monomorphized: `Box<int>` and `Box<string>` are different layouts. |
| `T[]` | pointer to a list object, 8 bytes | Element buffer stores `T`'s slot: `int[]` is a buffer of `i64`. |
| `map<K, V>` | pointer to a map object, 8 bytes | Insertion-ordered, as today (`IndexMap`). Entries store `K`'s and `V`'s slots. |
| function value or closure | pointer to a closure object, 8 bytes | Code pointer plus typed captures. |
| `Future<T, E>` | pointer to a future object, 8 bytes | |
| `C?` where `C` is a pointer type | nullable pointer, 8 bytes, null is 0 | Covers the common optional without a tag. |
| any other union, optional scalar, interface existential, `unknown` | tagged slot, 16 bytes: `{ tag: u64, payload: u64 }` | `tag` is a type id from the descriptor table; payload is the member's slot (scalar bits or pointer). |

**Open 1.a.** Whether tagged slots may be narrowed per union (for example `int | null` as 9 bytes). Default: no; fixed 16 bytes keeps codegen and tracing uniform.

### 1.2 Object header

Every heap object starts with one 8-byte word:

```
struct Header { desc: u32, aux: u32 }
```

- `desc` indexes the program's static descriptor table (1.3).
- `aux` is per-kind: string length class, list or map capacity class, or 0. It never holds GC state.
- GC state (mark bits, object-start bits) lives in side bitmaps (2.4), so conservative scanning and sticky marks never write object memory.

Class fields follow the header. Layout rule: pointer-bearing slots first (pointers, then tagged slots), then scalars; each group in declaration order; every slot 8-byte aligned. Declaration order is still what reflection, JSON and printing see, through the descriptor's field table.

### 1.3 Descriptors

One static, immutable descriptor per concrete heap type, emitted at pack time:

| Field | Purpose |
|---|---|
| `kind` | class, list, map, string, closure, future, opaque, ... |
| `size` | fixed size, or element slot size for variable-size kinds |
| `ptr_slots: (offset, count)` | contiguous run of plain pointer slots, traced unconditionally |
| `tagged_slots: (offset, count)` | contiguous run of 16-byte tagged slots, traced when the tag says pointer |
| `elem` | for list and map: element descriptor or scalar kind |
| `type_id` | identity for reflection, `instanceof`, union tags, and dynamic `Value` conversion |
| `fields` | declaration-order table: name, offset, slot kind, field type id (reflection, JSON, printing) |
| `finalizer` | `cleanup` method, if the class has one |

Because of the layout rule in 1.2, a class's pointer map is always two ranges, never a bitmap.

### 1.4 Dynamic boundary

Today's 8-byte tagged `Value` survives only where a value's type is not known statically: host crossings, reflection (`call_any` stubs), the interpreter island, and `unknown`. Conversion is generated per type: `to_value(slot) -> Value` and `from_value(Value) -> Result<slot>`, with the interpreter's coercion and validation rules.

### 1.5 Stack objects

An object proven not to leave its function (decision 10) uses the same layout, header included, placed in the function's frame, so code that reads it cannot tell the difference. The collector needs nothing special: pointers to a stack object fall outside heap blocks and are ignored, and the stack object's own pointer slots are found because the whole stack is scanned conservatively.

---

## 2. Heap and collector

### 2.1 Ownership

```rust
pub struct Heap { /* !Send, !Sync */ }
```

One `Heap` per program instance, owned by the loop thread. Generated code reaches it through a thread-local pointer set when the loop starts, so it is not threaded through every call.

### 2.2 Allocation

```rust
fn alloc(desc: DescId, bytes: usize) -> *mut u8   // returns a zeroed object with header written
```

- Fast path, inlined into generated code: bump a cursor within the current free line run; if it fits, write the header and the object-start bit, return.
- Slow path, out of line: find the next free line run, take a fresh 32 KiB block, or collect. **Any allocation may collect.** Because nothing moves, callers need no spills or reloads.
- Objects larger than 8 KiB go to a large-object space of page-aligned allocations, same marking, swept individually.
- Allocation failure after a full collection raises the interpreter's out-of-memory panic.

### 2.3 Collection

- Trigger: bytes allocated since the last collection exceed a budget derived from the live heap. Reuse today's byte-budget and headroom policy and its tests.
- Collection is stop-the-world on one thread: every other task is already parked at a known point (decision 1), and the running task is inside `alloc`'s slow path.
- Mark, in order:
  1. **Conservative roots:** the running stack from its current stack pointer (registers spilled first) to its base, and every parked fiber's stack from its saved stack pointer to its base.
  2. **Precise roots:** globals, the host handle table, task records, pending I/O completions, telemetry captures, interpreter-island boundary values.
  3. **Heap:** precise, through descriptors (1.3).
- Sweep: lazy. A line with no marked object is free; a block is swept when the allocator next reaches it.
- Finalizers: objects with a `cleanup` found unmarked are queued and their cleanup runs once, as a task on the loop, with the same ordering and once-only guarantees as today. Objects reachable from a pending finalizer stay alive until it runs.
- Weak tables (runtime caches keyed by object) are cleared after marking. Nothing moves, so no re-keying.

### 2.4 Side metadata

Per 32 KiB block:
- Object-start bitmap: 1 bit per 16 bytes, set at allocation (plain store, single thread).
- Mark bitmap: 1 bit per 16 bytes.
- Line marks: 1 byte per 128-byte line.

Conservative candidate validation: is the word inside a heap block? Then search the object-start bitmap backwards to the object's start (interior pointers are accepted, bounded by the object's size) and check it is allocated. If so, mark it.

### 2.5 Write barrier hook

Phase 1 has no generational collection, but generated code and builtins must store pointers through one inline function from day one:

```rust
#[inline(always)] fn write_ptr(obj: *mut u8, slot: *mut usize, value: usize)
```

In phase 1 it is a plain store. Phase 3 adds the sticky-mark-bits check inside it, with no codegen or builtin changes. Scalar stores never go through it.

### 2.6 Modes and checks

- `BAML_GC_STRESS=1`: collect on every allocation slow path and at every safepoint. The differential suite runs under it.
- Debug builds verify after each mark that every precise root and every traced slot points at an allocated object.

**Open 2.a.** Fragmentation fallback (opportunistic evacuation of sparse blocks) is deliberately out of the interface. It would need pinning for conservatively found objects. Revisit only if the long-running live-heap workload shows growth.

---

## 3. Builtin interface

Today every native builtin is `fn(&mut BexVm, &[Value]) -> NativeCallResult` behind generated glue; typed signatures exist only behind it. In v2, generated code calls the typed Rust function directly. The declarations in `baml_std` and their markers stay the source of truth; `baml_builtins2_codegen` gains a v2 output mode (decision 6).

### 3.1 Context

```rust
pub struct Cx { /* heap, task loop, telemetry producer; !Send */ }
```

A builtin that allocates, calls back into BAML, yields or touches runtime state takes `cx: &mut Cx` as its first parameter. Mapping from today's markers:

| Today | v2 |
|---|---|
| `//baml:vm`, `//baml:mut_vm` | `cx: &mut Cx` (no split-borrow cloning: objects are not behind the VM) |
| `//baml:may_yield` | same `cx`; the body may call BAML or yield directly (3.4) |
| `//baml:mut_self` | receiver passed as a typed mutable reference |
| `//baml:fallible`, non-empty `throws` | returns `Result<T, BuiltinError>` |
| `//baml:local_twin` | deleted: there are no locks to skip |
| `//baml:trace_passthrough` | the observation word is Hidden (4.2) |
| `$compiler_intrinsic` | unchanged: lowered inline by the compiler |
| `$rust_io_function` | sysop (3.6) |

### 3.2 Types

| BAML | Parameter | Return |
|---|---|---|
| `int`, `float`, `bool` | `i64`, `f64`, `bool` | same; `int` results go through the i63 range check |
| `string` | `Str` (heap pointer; `.as_str(cx)` flattens lazily and caches) | `Str`, or `&str`/`String` that the glue allocates |
| bytes | `Bytes` (heap pointer, `.as_slice()`) | `Bytes` or `Vec<u8>` |
| `T[]` | `List<T>` (typed: `List<i64>` is a buffer of `i64`) | `List<T>` or `Vec<T>` |
| `map<K, V>` | `Map<K, V>` | `Map<K, V>` or an ordered `Vec<(K, V)>` |
| class `C` | `Ref<C>`, where `C` is a generated `#[repr(C)]` struct matching 1.2 | `Ref<C>` |
| `T?` | `Option<T>` | `Option<T>` |
| union, `unknown`, interface existential | `Dyn` (the 16-byte tagged slot, with typed accessors) | `Dyn` |
| function value | `Closure<fn(A..) -> R>` | `Closure<..>` |
| `$rust_type` | `Opaque<T>`: a heap object owning a Rust `T`, dropped when collected | `Opaque<T>` |

- **Generics:** a builtin over `T[]` is a Rust generic function `fn push<T: Slot>(list: List<T>, item: T)`, instantiated by rustc for each monomorphized call. Type arguments are explicit; `vm.current_call_type_args()` goes away.
- **Pointer stores** into heap objects go through `write_ptr` (2.5). The typed containers do this internally.
- **Allocation** through `cx` may collect at any point; pointers held in Rust locals stay valid (non-moving, conservative stack scan).
- **`$rust_type` state** (regex, crypto, parse caches) moves from `Arc<Mutex<..>>` to `Opaque<T>` owned by the loop's heap.

### 3.3 Errors

`BuiltinError` keeps today's `VmRustFnError` variants and their mapping (`native_error_to_vm_error`): `Panic` and `BamlError` become the same thrown exception values, `Thrown` passes through, `InternalError` is fatal. Messages are produced by the same constructors. A Rust panic in a builtin is an internal error, as today.

### 3.4 Callbacks and yielding: the fiber simplification

Today a builtin that calls BAML (sort with a comparator, `map`/`filter`, map methods that call user `hash`/`==`) must return `YieldToCall` with a boxed continuation and be written as a state machine (`SortByContinuation`, `MapDriver`, about 18 such builtins in `containers.baml` alone).

In v2 every task runs on its own fiber (decision 4) and BAML functions compile to Rust functions, so a builtin calls BAML directly:

```rust
fn sort_by<T: Slot>(cx: &mut Cx, list: List<T>, cmp: Closure<fn(T, T) -> i64>) -> Result<(), BuiltinError> {
    // ordinary merge sort; each comparison is a direct call
    let ord = cmp.call(cx, a, b)?;
}
```

If the callback awaits, the whole fiber parks and resumes later; the builtin's Rust frame simply waits on the stack. Continuation structs, `NativeFrame`, `RootHaver` on continuations and the `may_yield` restrictions on compiled callers all go away. Long builtins call `cx.poll()?` periodically, which may yield to other tasks (5.3) and delivers cancellation.

### 3.5 Shared bodies with the interpreter

- **Heap-neutral bodies** (scalars, `&str`, `&[u8]`, returning owned Rust values) are shared: v2 glue and interpreter glue both call the same function. Example: `split(&str, &str) -> Vec<Range>`.
- **Bodies that walk or build typed objects** are written either against v2 types (and the interpreter keeps its current body), or moved into BAML source where they are compiled by v2 and run as bytecode by the interpreter (decision 6). Per-function triage decides which.
- The interpreter's glue is unchanged.

### 3.6 Sysops

- The typed `RuntimeIo` trait (owned Rust values in, owned values out) is kept as the I/O contract.
- The loop marshals: arguments are copied out of the heap into owned values (`String`, `i64`, `Vec`, `IndexMap`, owned class structs; `function` parameters become rooted handles in the host handle table), the future runs on the I/O runtime, the owned result returns on the completion queue, and is built into heap objects on the loop thread with today's validation (`check_member` against the declared type).
- `SysOpOutput::Ready` results skip the round trip and are built immediately.

**Open 3.a.** Whether moved-to-BAML builtins (decision 6) also replace the interpreter's Rust bodies, making the playground run them as bytecode. Default from decision 6: yes, one copy; measure playground impact on the interpreter benchmarks.

## 4. Telemetry producer

The contract is today's (decision 8): levels, policies, hooks, records and published output stay identical. Only the producer moves, from the VM's call funnel into compiler-emitted code.

### 4.1 What is fixed and what is free

- **Fixed (contract):** every published artifact. Ordinary calls are published as per-call-path aggregates (`count, total_duration, total_self_await, errored, cancelled, panicked`, keyed by path and reentry), built today by the processor from one `FunctionTimingCompletion` per invocation (`btel_processor/src/lib.rs:62`). Spans, call-path definitions, task, network, sysop, error-evidence, manual-span and usage records keep their formats and ordering rules.
- **Free (internal):** how the producer gets data to the processor. A v2 producer may hand the processor pre-combined aggregate deltas for ordinary timing calls instead of one record per invocation, as long as the published aggregates are identical. That is the main cost lever.

### 4.2 Per-function emission

For each compiled function, the compiler emits a prologue and epilogue selected by a per-function **observation word**, resolved once per process from the level, policies and declaration hooks (today's `may_record` and `default_mode` logic, run at startup instead of per call):

| Observation | Prologue | Epilogue |
|---|---|---|
| Hidden (low, off, builtins, passthrough) | nothing beyond one branch on the word | nothing |
| Timing (medium default) | read clock; save caller's active path; set active path | read clock; add to this path's aggregate slot (count, duration, self-await, outcome); restore active path |
| Timing with policy (promotion possible) | as Timing | evaluate `policy::promotes` with the measured duration and outcome; if promoted, emit `LateFunctionSpanCompletion*` exactly as today, else as Timing |
| Span (high, LLM functions, `span_from_entry`, reserved) | allocate span id; emit `FunctionSpanAnnouncement` with captures | emit `SpanUsage` if charged; emit `FunctionSpanCompletion*` with captures |
| Hooked | run the compiled hook prologue (4.6), then one of the rows above | as that row |

The observation word is a global read once per call; when telemetry is off it is the only cost (the spike's off-path baseline already included this branch).

### 4.3 Call paths

- A call-path key stays `(parent path, visible caller, caller site, callee, edge)`. Caller site becomes a static site id (replacing `caller_pc`), mapped to the same source line the interpreter reports.
- For a direct call site the key differs only by `parent`. Each observed call site gets a static one-entry cache `{last_parent, path}`: one compare on the hot path, falling back to the per-loop hash map and, on miss, allocating an id and emitting `CallPathDefined` as today.
- Keys hold function ids, not heap pointers, so nothing is rebuilt after collection.
- Reentry (direct self-call reusing the parent path and setting `REENTRY`) is decided at the call site as today.

### 4.4 Aggregate slots

- One slot per `(path, reentry)` in a per-loop table: `{count, total_duration, total_self_await, errored, cancelled, panicked}`.
- Flushed to the processor as aggregate deltas at every yield point (5.3), at task end, and at shutdown; never held across a suspension, matching today's seal-before-suspend rule.
- Self-await: each observed frame keeps an `await_duration` accumulator, added to by awaits and sysop waits inside it, as today (`FrameTelemetry.await_duration`).

### 4.5 Per-frame state

An observed frame keeps a compact on-stack record (Rust locals in the generated function): `entered_at`, saved active path and active span id, `await_duration`, flags (span, capture output/error, reentry, needs announcement), and the output/error capture requests. This replaces today's 40-byte `FrameTelemetry` inside VM frames and compiled `FrameRecord`s. Errors complete frames from the unwinding path (the `Result` return) using one shared clock read per raise, as today.

### 4.6 Hooks and `$trace`

- A declaration hook compiles to a BAML prologue inside the callee, as in bytecode: it reads the callee's arguments, returns `true`, null or a settings object, and its config and context patch are merged with today's rules (reservation OR-ing, suppression while any hook is evaluating, `EmptySpan`, `Rich`).
- When the resolved level is off, the prologue is skipped by the observation word (today it is patched to a jump).
- `$trace` call-site options, reservations and manual spans keep `package_trace.rs` semantics; they become builtins under section 3.

### 4.7 Tasks, I/O, errors

Emitted by the runtime, not by function prologues, with today's record formats:
- spawn: call path with `Spawn` edge (or the spawner's active path when the target is hidden), `ThreadSpanAnnouncement`, `ThreadSpanRunning` when first scheduled, `ThreadSpanCompletion`;
- sysops: `SysOpTime` when the active path is not root, and await time added to the owning frame;
- network: announcement, events and completion, skipped at low;
- errors: raise origin, stack and frame completions when a recording publisher wants error evidence.

### 4.8 Transport

- One producer per loop thread, bound once when the loop starts (no per-write TLS binding lookup). It keeps the `ThreadSelected` and context selector state per stream so logical task ids still appear as today.
- Same chunked buffer and lanes to the processor thread.
- wasm drops writes, as today.

**Open 4.a.** The processor must accept pre-combined aggregate deltas from the producer alongside per-invocation timing records. This is an internal format change inside the telemetry crates. Default: add it; published output unchanged and checked by the differential harness's telemetry comparison.

## 5. Task loop

### 5.1 Threads

- **Loop thread:** runs all BAML code of one program instance and owns its heap. Only thread that touches BAML objects.
- **I/O runtime:** Tokio, any number of threads. Runs sysop futures, network and host dispatch. Never touches BAML objects: it receives owned Rust values and returns owned Rust values.
- **Telemetry processor:** its own thread, as today. Receives records, never objects.
- Crossing between them is by message: the loop sends an owned request; the completion comes back as an owned result on the loop's completion queue and is turned into BAML objects on the loop thread (hydration).

### 5.2 Tasks

```rust
struct Task {
    fiber: Fiber,              // own stack (decision 4)
    state: Ready | Running | Parked(WaitReason) | Done,
    cancel: TaskCancel,        // today's token: own + inherited sources + deadline
    future: *mut FutureObj,    // the BAML-visible future this task settles
    parent: TaskId,
    gate: Option<GateTicket>,  // today's admission gate
    telemetry: TaskTelemetry,  // task span ids, logical thread id
}
```

- `spawn` allocates the future object, creates the task in `Ready`, registers it with its gate synchronously (as today), and returns the future. **The parent keeps running.**
- **Ordering contract (new, explicit):** a spawned child first runs when its parent next yields (await, sysop, poll yield or completion). Ready tasks run in FIFO order of becoming ready. This fixes the ordering today's code labels a bug (`bex_engine/src/lib.rs:6603`) instead of inheriting Tokio's.
- Fibers come from a pool and are returned when a task finishes, so spawn does not map a new stack each time.

### 5.3 Yield points

A task gives up the thread only at:
1. `await` on a pending future, or `await_any` with none settled;
2. a sysop (I/O) call: it sends the request and parks until its completion arrives;
3. a poll yield: compiled loops poll every 16 iterations (as today); a poll yields if another task is ready and the running task has used its time slice;
4. a builtin's poll hook (decision 6), so long Rust builtins can yield midway;
5. task completion.

Collection can happen at any allocation, not only at yields (2.2).

### 5.4 Loop

```
loop {
    drain completion queue -> mark waiting tasks Ready (hydrate results on this thread)
    run due timers and expired deadlines -> deliver cancellation
    pop next Ready task -> switch to its fiber until it yields
    if nothing is Ready: block on the completion queue (or the next timer)
}
```

### 5.5 Futures

BAML-visible futures stay heap objects with today's states (Pending, Ready, Error, Cancelled, InternalError) and behaviour:
- `await` returns the value, rethrows the producer's error with its saved trace, or raises cancellation.
- `await_any` picks by scanning inputs in order, not by completion order.
- Public `f.cancel()` requests cancellation; the future stays Pending until the producer unwinds.
- `future.all` and friends remain BAML stdlib code built on spawn and await.

Settlement is a plain field write plus moving waiters to `Ready`: no atomics, no `SetOnce`, no `FutureManager` locking.

### 5.6 Cancellation

Unchanged semantics: delivered at yield points and poll checkpoints as `baml.panics.Cancelled`; catching does not clear the token; the next unshielded yield redelivers it; defer and cleanup shielding as today; in-flight sysops get their own token, aborted on delivery and replaced so cleanup can do I/O.

### 5.7 Host entry

- Host calls (SDK, CLI, HTTP server) arrive as messages on the loop's queue: target, owned arguments, context, call id, timeout.
- On the loop thread: validate and hydrate arguments with today's rules (invalid arguments rejected before execution, no side effects from abandoned preparation, depth limit 512), then create a root task.
- Results return as owned values through the existing completion paths (C FFI callbacks, N-API, Python `call_soon_threadsafe`).

**Open 5.a.** `baml serve` and the SDK bridges: one shared program instance (one loop, simplest, one core) or one per worker (scales, no shared state). Default: one shared instance in phase 1; per-worker isolates are phase 4.

**Open 5.b.** Time slice for poll yields. Default: yield only if another task is ready and at least 1 ms has passed since this task was scheduled; measure fairness on a CPU-loop plus I/O workload.
