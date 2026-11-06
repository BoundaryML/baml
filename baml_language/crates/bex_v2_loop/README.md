# Runtime v2 task loop

A standalone, single-thread-owned (`!Send`, `!Sync`) stackful scheduler for
`FnOnce(&mut TaskCx) -> Result<Value, Err>` bodies. It has no dependency on the
interpreter, VM, or parallel heap job. The workspace's `crates/*` glob includes it.

It provides FIFO Ready/Running/Parked/Done scheduling, pooled lazy stacks,
spawn/await/ordered await_any, owned Tokio I/O, timers, inherited cancellation and
deadlines, shielding, synchronous gate registration, cross-thread root submission,
and shutdown reports for unobserved errors and leaked producers. A completed
fiber's mapping returns to a bounded pool; finished task records are removed.

```rust
use bex_v2_loop::*;
use std::{cell::RefCell, rc::Rc};

let io = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(2).enable_all().build().unwrap();
let mut tasks = TaskLoop::new(
    Config::default(), io.handle().clone(), Rc::new(RefCell::new(())),
);
let result = tasks.spawn(|cx| {
    let child = cx.spawn(|_| Ok(Value::int(42)));
    cx.await_future(&child)
});
assert_eq!(tasks.run_until(&result).unwrap().as_int(), 42);
assert!(tasks.shutdown(std::time::Duration::ZERO).leaked.is_empty());
```

## Scheduling and boundaries

Spawn returns immediately without switching. A child first executes after the
parent's next actual suspension or completion. Awaiting an already settled
future does not switch. A poll checks cancellation every time, drains arrivals
and timers, and switches only with another Ready task and an exhausted slice
(default 1 ms). Compiled loops must eventually call `poll`, conventionally every
16 iterations. There is no preemption of Rust bodies that never poll.

`await_any` scans the input list on every resumption, not completion order.
Settlement is a local field write and FIFO waiter wakeup. Multiple awaits do not
consume a result. Errors preserve the producer's supplied trace. A panic escaping
a Rust task or I/O job becomes `InternalError`.

`request_cancel` fires the producer's own token but leaves its future Pending.
Children inherit the parent's own token, linked sources and deadline; linked
sources survive successful parent completion. Failure/cancellation cascades to
children. Detached and shield-spawned children start with independent tokens.
Catching `Err::Cancelled` does not clear cancellation. An unshielded checkpoint
redelivers it. Shield depth is restored during Rust unwinding. Delivery cancels
and replaces the I/O token; epoch checking discards late completions from the
abandoned operation, allowing shielded cleanup to perform fresh I/O.

`Gate` is a FIFO concurrent-admission limit: registration happens at spawn, before
any child code executes; a ticket holds its permit through task termination.
Cancellation while queued prevents body entry and unregisters the ticket.

`SubmissionHandle` is Send/Sync, accepts owned scalar/string arguments plus a Send
entry closure, and returns an owned result through an `mpsc` receiver. Keep driving
`tick(true)` on the owner thread to serve submissions. `run_until` drives one
chosen future, while `run_until_idle` drains currently runnable work. The Tokio
runtime must be multithreaded. Dropping it returns InternalError to parked I/O
rather than leaving tasks stranded. Shutdown closes host admission and rejects
queued submissions, allows a grace interval, records still-live producers as
leaks, cancels them, allows the same grace for cleanup, then force-unwinds any
remaining fibers. `Drop` uses zero grace; call `shutdown` explicitly for its report.
A running task that never polls can exceed either grace interval.

## Heap seam and stack roots

`LoopHeap::collect(&mut self, &dyn RootSource)` is the independent heap seam;
`()` is the no-op heap for scheduler tests. `TaskCx::collect` calls it synchronously
without yielding. An allocator integration can use this seam on its slow path;
collection is not restricted to scheduler suspension points.

The local `RootSource` has exactly the two section 2.3 method signatures.
`TaskLoop` implements it; the running task supplies the same implementation
through the loop's private Core. Visits must be synchronous; raw addresses must
not escape. The non-moving collector reads roots, rather than relocating them.

* The running stack is `[current_sp, base)`, after a non-inlined register spill.
  AArch64 spills x19–x30 and d8–d15; x86-64 spills rbx/rbp/r12–r15. ABI call rules
  force live caller-saved values to survive the call. The addressable register
  buffer is also visited explicitly. Outside a fiber, pthread stack bounds are
  used for the loop thread.
* Every other live fiber, including Ready fibers, contributes `[saved_sp, base)`.
  **A private corosensei 0.3.4 fork is necessary**: upstream keeps `stack_ptr`
  private. Its saved context already includes preserved registers. The vendored
  accessor exposes that field without guessing layout or reading guard pages.
  See [vendor/README.md](vendor/README.md) for provenance, licenses and changes.
  Existing crates continue using unmodified registry corosensei.
* Precise slots include `SpawnOptions::roots` and every live future's pointer
  payload, including completed futures held only by consumers. `Value::Pointer`
  keeps its word inline in an `UnsafeCell`, so a value on a fiber stack also has
  an addressable pointer word. Pending I/O completions contain only `OwnedValue`
  and therefore have **no heap pointers**; their landing is on the loop thread.
* Boxed Rust closure captures can themselves live off-stack. Register their
  pointer-bearing captures in `SpawnOptions::roots`. Compiler-produced closure
  objects will instead need descriptor tracing. Ordinary pointer locals on a
  fiber do not need explicit registration.

The stack test places a pointer-like word in a live Rust local, collects while
running, parks it, and collects from another fiber; its address and contents are
inside the reported range in both debug and release. A separate test checks a
completed future's precise slot. These tests do not constitute a formal proof of
raw-stack scanning across all compilers/ABIs.

## Verification

From the worktree root, keep build artifacts in this worktree:

```sh
export CARGO_TARGET_DIR="$PWD/target"
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_loop
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_loop --release
cargo clippy --manifest-path baml_language/Cargo.toml -p bex_v2_loop --all-targets -- -D warnings
```

Result: **20 integration tests and 2 compile-fail ownership tests pass**, debug and
release; clippy passes. Coverage includes parent/child ordering at each boundary,
non-yielding settled awaits, slice gating, nested spawns/pool reuse, input-ordered
await_any, cancellation/redelivery/shielding/deadlines/inherited sources, gate
admission, Tokio completion and shutdown, cross-thread submissions, CPU/I/O poll
fairness, panics, unhandled errors, forced-unwind drops, foreign-loop rejection,
stack/precise roots, and 10,000 parked tasks. No real heap/compiler differential
suite is claimed. Cargo emits an unrelated existing bridge_rust license metadata
warning; this crate's clippy diagnostics are clean.

## Measurements

2026-11-06, Spark aarch64, CPU 12 (Cortex-A725, maximum 2.808 GHz), Linux 4 KiB
pages. Rust `1.96.0-nightly (b90dc1e59 2026-03-04)`, Node `v24.15.0`. Workspace
release profile: opt-level 3, fat LTO, one codegen unit, unwind panics. All process
threads inherit CPU affinity; other machine activity was not isolated.
[measurements.json](measurements.json) retains individual observations.

| Spawn/await 10,000, time per job | Median | Min–max |
| --- | ---: | ---: |
| This crate, real child fibers | **9.399 ms** | 9.266–9.571 ms |
| Repository Node workload | **1.448 ms** | 1.395–1.475 ms |

Five fresh processes per implementation, alternating run order, one untimed
warmup then three timed jobs per process, matching `bench.toml`. Every Rust job
checks the sum is 10,000; the unchanged Node driver checks the warmup result.
Construction of the runtime/loop is excluded; root creation and all child
spawn/await work are included. Rust reuses the warmed stack pool. Median ratio:
**6.49× Node**. The Node source awaits `Promise.resolve(1)`; Rust creates an actual
child task each iteration, matching the BAML source. This is a scheduler-only
measurement without compiled BAML, heap allocation, or telemetry. The supplied
~124× current-BAML comparison was not remeasured here.

For **10,000 simultaneously parked tasks**, three fresh-process trials agreed:

| Memory | Result |
| --- | ---: |
| Usable reservation per fiber | 64 KiB |
| Guard page per fiber | 4 KiB |
| Total fiber address reservation | **664.06 MiB** (696,320,000 bytes) |
| Resident fiber pages (`mincore`) | **39.06 MiB**, 4 KiB per task |
| Whole-process RSS increase | **48.88 MiB** (50,052 KiB) |
| Whole-process virtual-size increase | 673.89 MiB |

RSS includes task/future/token metadata; the resident stack count measures only
fiber mappings, including guard pages (nonresident). DefaultStack reserves with
mmap/mprotect and commits pages on demand. Parking took 1.021–1.023 seconds.
This is a shallow timer-wait body, not a bound for real BAML frames; deeper calls
commit more pages and can exhaust the fixed reservation. Pool retention is capped
at 128 stacks by default (8.5 MiB reserved including guards), configurable.

Reproduce from the worktree root (repeat timing commands five times, memory three):

```sh
export CARGO_TARGET_DIR="$PWD/target"
cargo build --manifest-path baml_language/Cargo.toml -p bex_v2_loop --example measure --release
taskset -c 12 target/release/examples/measure
env NODE_OPTIONS= taskset -c 12 node \
  baml_language/tools/speedtest/src/speedtest/drivers/driver.js \
  baml_language/tools/speedtest/workloads/concurrency/spawn-await-x10k/main.js 3 1
taskset -c 12 target/release/examples/measure memory
```

## Interface deviations and integration decisions

Scheduling, future-state and cancellation contracts are preserved. The following
are explicit standalone-crate boundaries, not claims of completed integration:

1. **Section 2.3 ownership:** RootSource is locally defined until the heap job's
   trait can be shared. Integrate through an adapter or extract a common traits
   crate; do not make either job depend on the other's implementation. Globals,
   host handles and telemetry roots will need to join the root source then.
2. **Sections 5.2/5.5:** futures are Rc-backed records and values/traces are
   placeholders, as required without a heap/compiler dependency. `Gate` preserves
   synchronous registration, FIFO admission and permit lifetime for a concurrency
   limit. The existing interpreter LimitSet/planner and telemetry identifiers are
   not wired in. The task records retain parent ids for that integration.
3. **Section 5.7:** closure plus owned arguments is the host target for now.
   The real target/context/call-id protocol, type hydration/validation (including
   depth 512), and SDK completion adapters need compiler/host type metadata.
   Pointer values are explicitly rejected at the owned-result boundary. There is
   no pretend validation against unavailable BAML types.
4. **Portability decision:** running-register/root scanning currently compiles
   only on Linux aarch64/x86-64; only aarch64 was executed here. Corosensei supports
   more targets, but their running-stack bound/spill implementation still needs
   a port. Keep this restriction or prioritize macOS before runtime integration.
5. **Scaling decision:** timers/deadlines/admission use a linear scan; external
   cancellation while idle is checked with a maximum 1 ms receive timeout.
   Semantics are intact, but an indexed timer queue and cancellation notifications
   would remove that scan/periodic wakeup at larger scale. The small stack default
   needs validation against compiled workloads before becoming a public runtime
   default; its reservation remains configurable.
