# Threads

Which OS threads a compiled program has, what state belongs to a worker,
what migration stress checks, and an exact list of what assumes one
worker. Several workers are not built. Nothing here is a measurement
except the thread count.

Paths: `rt/` is `crates/baml_native_rt/src/`, `cg/` is
`crates/baml_compiler2_native/src/`. Line numbers are at commit
`39d3ad4c25`.

## OS threads

| thread | created | what it does |
|---|---|---|
| main thread | process start | the one worker: runs every task, the collector, every native that does not use `scope.io` |
| `baml-io` | first `scope.io` job (`rt/native/drive.rs:335-349`) | a Tokio multi-thread runtime with `worker_threads(1)`: runs every I/O job and calls `Inbox::wake` |
| Tokio blocking pool threads | first blocking job (`tokio::fs`, `spawn_blocking`, DNS) | created on demand by Tokio, idle threads exit after its keep-alive |
| `baml-worker-2` | only under `BAML_MIGRATE_STRESS` in a runtime built with checks (`rt/sched.rs:1260-1265`) | takes turns with the main thread as the one worker |

A program that calls no native using `scope.io` has one thread and does
not link Tokio. Measured: the `baml.env.get` probe runs with 2 threads
(`ps -M`), a compute probe with 1.

The worker never blocks in a native on I/O. With nothing runnable it
sleeps in `pthread_cond_wait` on its inbox (`rt/sched.rs:477-499`).

## Per-worker state and how compiled code reaches it

The worker is the `Mutator` (`rt/heap.rs:312-354`). Compiled code holds
its address in `x21` and never caches anything derived from it across a
call that can park: Cranelift's `get_pinned_reg` is read at each use
(`cg/codegen.rs:2924-2929`, with the vendored `fold_pinned_reg_reads`
setting, `cg/codegen.rs:422-427`). The switch frame does not save `x21`;
whoever resumes a task installs its own (`rt/sched/switch.rs:61-77`,
`100-114`). A handler lands with `x21` reloaded from the `Resume` the
unwinder filled (`rt/stubs.rs:309`).

```rust
#[repr(C)]
pub struct Mutator {
    alloc_ptr: usize, alloc_limit: usize,     // inline allocation
    stack_limit: usize,                       // the running task's
    poll: u32,
    scratch: usize,
    hash_seed: u64,
    pub(crate) cur_task: *mut Task,
    tele_node: usize,                         // reserved, unused
    g0_sp: usize,                             // the worker's own stack
    anchor: Anchor,                           // top compiled frame at the runtime boundary
    span_end: usize,
    heap: Heap,                               // the whole heap, owned
    pub(crate) sched: Sched,                  // run queue, timers, task lists, inbox
    roots: Vec<usize>,
    pub(crate) unwind: Unwinder,
    on_worker_stack: bool,
    pub(crate) polling: Option<&'static str>,
}
```

The first ten fields are per worker by nature. `heap` and `sched` are the
process's, and live here only because there is one worker.

## What migration stress simulates

`BAML_MIGRATE_STRESS` (`rt/lib.rs:52-53`, `rt/sched.rs:1194-1276`,
`serve_in_turns`) runs the one worker on two threads that take turns,
never at once. Each time the next task to run last stopped on this
thread, the whole `Mutator` is copied bit for bit to the other of two
mapped places, the old place is made unreadable, and the other thread
goes on. With checks, every stop also walks the stopping task's stack and
ends the program if a return address is not compiled code or the boot
stub (`only_compiled_code_stops`, `rt/sched.rs:650-703`).

It finds: a worker address or thread identity kept across a stop by
compiled code, a stub or a native's scope; a Rust frame left on a stopped
task; thread-bound state held across an I/O wait.

It does not exercise anything that needs two workers running at once:
no data race below, no collection with several mutators, no contention on
the queue, timers or inbox (the scheduler and heap move with the worker),
no wake from another worker's `settle`.

## What assumes one worker

Each item: where, what it relies on, what it needs.

### Collector

1. `rt/heap.rs:342`, `384-428`. The `Heap` is a field of the `Mutator`;
   `Mutator::new` makes "a heap of its own". Needs a shared heap that
   each worker refers to.
2. `rt/heap.rs:671-733` (`copy_into`). Roots are this worker's anchor
   (684), the tasks of this worker's scheduler (685-696) and this
   worker's root list (697-699). A task `RUNNING` on another worker has
   neither its stack nor its anchor scanned (`task_roots`,
   `rt/sched.rs:539-563`, scans only tasks that are not running). Needs
   every worker stopped at a safepoint with its anchor recorded, and
   every worker's roots walked.
3. `rt/heap.rs:322-324`, `510-513`, `1124-1128`. The poll word is a plain
   `u32`; `Mutator::poll` clears it; nothing sets it. Needs an atomic
   request the collecting worker sets and a `poll_slow` that parks and
   acknowledges.
4. `cg/codegen.rs:1655-1690` (`polled_blocks`) and `2860-2888`
   (`check_stack`). A loop that calls a stack-checking function has no
   poll, and the stack check only throws. Needs the stack-limit trip to
   mean "stop" as well, or polls in those loops.
5. `rt/native/drive.rs:505-516`. `Scope::off_heap` is `work()`. Needs to
   publish "at a safepoint" for the duration and wait out a collection
   before returning. Nothing calls it yet.
6. `rt/native/drive.rs:518-522`, `rt/native/sys.rs:375`. `collect_now`
   calls `collect(0)` directly. Needs the stop request first.
7. `rt/heap.rs:611-624`, `628-634`, `705-719`. The list of owned Rust
   values (`heap.owned`) and its trigger counters are plain. Needs a
   lock or per-worker lists merged at a collection.
8. `rt/unwind.rs:693-698`. An uncaught throw reports, drops owned values
   and exits while other workers would still run. Needs them stopped.

### Allocation

9. `rt/heap.rs:396-407`, `430-436`, `591`, `646`, `655`, `730-732`. One
   span, the whole space. `used` and `live` are `alloc_ptr -
   space.start`. Exhausting the span collects. Needs spans carved from a
   shared frontier, a refill before a collection, and all spans reset
   after one. The inline sequence (`cg/codegen.rs:2988-3049`) does not
   change.
10. `rt/native.rs:1230-1237`. `Scope::reserve` takes one contiguous
    allocation of any size. Needs a path for a request larger than a span.

### Scheduler

11. `rt/sched.rs:207-232`. `Sched` is plain fields with no lock: `queue`
    (a `VecDeque`), `timers`, `all`, `free`, `dead`, `pending`, `next_id`,
    `main_done`, `on_io`. Needs a split: `dead` and `pending` per worker,
    the rest shared and synchronized.
12. `rt/sched.rs:290-347` (`new_task`), `355-378` (`release`),
    `383-391` (`settle_last_switch`). Free list, id counter and the list
    of all tasks. Needs a lock or per-worker free lists and an atomic id.
13. `rt/sched.rs:400-424` (`wake`). The state change is a
    compare-and-swap, the push that follows is onto the plain queue
    (407, 409). Needs a concurrent queue and a signal to a sleeping
    worker.
14. `rt/sched.rs:431-449`, `989-1016`, `285-287`. Timers are this
    worker's heap, in nanoseconds since this `Sched` started. Needs
    shared timers and one clock base.
15. `rt/sched.rs:466`, `1066`. `on_io` is incremented by the parking
    worker and decremented by whoever empties the inbox. Needs one atomic
    count.
16. `rt/sched.rs:477-499` (`sleep`), `1112-1114`. A worker with no
    timers and `on_io == 0` returns, and `run` reports a deadlock. A push
    to the run queue does not signal the condvar it sleeps on. Needs
    global idle accounting.
17. `rt/sched.rs:226`, `1067-1073`. One `Inbox` per `Sched`; a `Wake`
    holds the parking worker's inbox. Needs a shared inbox or stealing.
18. `rt/sched.rs:891-922` (`exit_pick`), `1129-1165` (`serve`),
    `1095-1119` (`run`). The hand-off pops the plain queue; `run` starts
    one `serve` on the calling thread. Needs N threads and a way to tell
    that all are idle.
19. `rt/sched.rs:147`, written at `267-268`, `327`, `809`, `1102`.
    `Task.flags` is a plain `u32`. Fine while only the running worker
    writes it.
20. `rt/sched.rs:968-984` (`baml_rt_await_error`). The future's flags are
    read, ORed and stored (973-976); `FUTURE_TASK` is loaded, zeroed and
    the task released (977-981). Two workers awaiting one failed future
    release the task twice. Also `baml_rt_task_fail` wakes waiters
    (`784-790`) before the failing task has left its stack, so a waiter
    resumed elsewhere could release a stack still in use. Needs an atomic
    swap of `FUTURE_TASK` and the release done by whichever of exit and
    `await_error` comes second.

### Process-wide singletons

21. `rt/sched.rs:109`, stored at `1100`, `1205`, `1275`. `WORKER` is one
    pointer. Needs a list and a per-thread current worker.
22. `rt/native/drive.rs:217-235`, `284-286`, `rt/heap.rs:353`. The panic
    hook reads `WORKER` and its `polling` name, which `native_poll` sets
    and clears unsynchronized. Needs the name per thread.
23. `rt/native/drive.rs:335-349`. One Tokio runtime with one thread
    serves every worker. Correct, and a bottleneck.
24. `rt/map.rs:49`, `57-63`, `rt/heap.rs:402`. The hash seed is a global
    copied into each mutator; the assembly probes read the copy through
    `x21` (`rt/map.rs:1004`, `1071`), Rust reads the global (140, 153).
    Needs the seed set before the first worker is made. It is.
25. `rt/unwind.rs:159-166`, `190-227`; `rt/heap.rs:295`. The handler
    table (`OnceCell`) and the stack maps are decoded per mutator. Needs
    one shared decode. The trace scratch `Vec` (165) stays per worker.

### Rust slices over heap memory

`natives/README.md:31-34` says `BytesRef::as_slice` and `as_mut_slice` are
the only two. They are not.

26. `rt/native.rs:466-473` (`BytesRef::as_slice`), callers at
    `rt/bytes.rs:115`, `rt/native/structural.rs:184`,
    `rt/native/uint8array.rs:52`, `59`, `rt/native/hash.rs:101`,
    `rt/native/crypto.rs:295`. Compiled code writes bytes inline
    (`cg/codegen.rs:2542`). Needs a copy, a pin or a lock.
27. `rt/native.rs:483-489` (`as_mut_slice`), caller
    `rt/native/uint8array.rs:201` (`sort`). Same, plus exclusion of
    writers.
28. `rt/native.rs:289-297` (`ArrRef::words`), behind `get`, `iter` and
    `as_slice` for `Int` and `Float` (`299-335`). Not documented as
    single-worker. Another worker can push or grow the array
    (`rt/array.rs:101-102`, `cg/codegen.rs:3209-3214`). Callers:
    `rt/native/root.rs:95`, `101`, `110`, `rt/native/uint8array.rs:106`,
    `rt/native/string.rs:667`, `684`. Needs the `BytesRef` treatment.
29. `rt/native/array.rs:57-68` (`elements`, a `&mut [usize]`), used by
    `truncate` (73-80), `remove` (84-93), `_rust_sort`
    (`rt/native/root.rs:65-67`); and `rt/array.rs:117-142`
    (`baml_rt_array_sort`). Need exclusive access to the array.
30. `rt/native/state.rs:152-165`. `OpaqueRef::plain` hands out a
    `&Cell<T>` over heap bytes; `rt/native/hash.rs:97-107` does get then
    set on a hasher through it. Needs a lock or owned state.
31. Safe as long as collection is stop-the-world, because the bytes are
    immutable: `string::text` (`rt/string.rs:22-36`) and what is built on
    it (`rt/native.rs:193-196`, `206-221`, `247-257`, `532-543`,
    `rt/map.rs:443`, `rt/array.rs:136`), `bigint::value`
    (`rt/bigint.rs:43-47`), a trace's `int[]` (`rt/unwind.rs:590-593`).

### Unsynchronized heap objects

32. `rt/map.rs:175-188`, the assembly at `1096-1098`, the inline read at
    `cg/codegen/map.rs:67-87`. A string's hash is cached by loading the
    whole header and storing it back with the hash ORed in. Harmless
    while the other header bits never change; it would lose a concurrent
    update of bits 32-59. Needs a 32-bit store or a compare-and-swap.
33. `rt/map.rs:392-416`, `511-550`, `554-593`, `602-656`, `665-714`,
    `721-731`. Every map mutation is unlocked; `compact` moves records in
    place and `grow` stores entries, index, used and mask separately. The
    probes read them unlocked (`908`, `912`), and compiled code stores
    through the slot address a probe returned
    (`cg/codegen/map.rs:115-125`). The header's high half is reserved for
    a lock word (`rt/layout.rs:259`) that nothing uses. Needs that lock
    or a language rule that a map has one owner.
34. `rt/array.rs:71-104`, `cg/codegen.rs:3168-3220`, `2232-2256`. Push
    reads length, data and capacity and stores element then length;
    readers cache the (length, data) pair per block and drop it only
    after calls that allocate. Concurrent pushes lose elements, and a
    reader can pair a new length with old storage. Needs an owner rule,
    a lock, or the length stored with the storage.
35. `rt/heap.rs:331-332`, `rt/sched.rs:161-162`, `rt/sched/switch.rs:100-114`.
    The telemetry node is reserved in the worker and in the task and
    `switch_in` does not move it. Needs to be swapped on every switch
    like `stack_limit`. `Task.cancel` (`rt/sched.rs:159-160`) is unused
    and would need to be atomic.

### Not a problem

- No write barrier exists and none is needed while collection is fully
  stop-the-world, with any number of workers.
- `on_worker_stack` (`rt/heap.rs:350`) and `Scope.mutator`
  (`rt/native.rs:987-996`) for a native that cannot stop are per worker
  and correct.

## Already written for several workers

- Task states are atomic and follow a protocol that tolerates a waker
  racing a parking worker (`rt/sched.rs:14-21`, `146`, `400-424`,
  `937-956`, `1148-1157`). Future state is an `AtomicUsize`, value
  written before state (`178-182`, `778-784`); compiled `await` loads it
  with acquire ordering (`cg/codegen/spawn.rs:218-227`).
- Spawn runs the child first and the child queues the parent only once
  the parent's stack is free (`rt/sched.rs:752`, `763-766`). An I/O job
  starts only after its task is `PARKING` (`1060-1073`).
- The inbox is thread-safe (`rt/sched.rs:186-204`).
- No Rust frame survives a stop: the driver of an `async` native is
  compiled code, the native's future is `Send`, jobs are
  `'static + Send`, and the scope's worker is refreshed after every stop
  (`rt/native/drive.rs:15-30`, `163-165`, `283`, `411`, `479-483`).
- Task records are 128-byte aligned and never freed, so a stale task
  address is still a task (`rt/sched.rs:133-135`).
- Semaphore state and the HTTP client table are behind mutexes
  (`rt/native/sync.rs:6-8`, `rt/native/http.rs:126`).

## Not built

Everything in "What assumes one worker". In dependency order: a shared
heap with per-worker spans (1, 9), a stop request compiled code can see
at every loop and call (3, 4), root scanning across workers (2), then the
shared scheduler (11 to 18), then the per-object questions (26 to 34),
which are language decisions as much as runtime ones.
