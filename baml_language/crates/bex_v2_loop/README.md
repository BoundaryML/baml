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
cargo clippy --manifest-path baml_language/Cargo.toml -p bex_v2_loop --all-targets --all-features -- -D warnings
```

Result: **28 integration tests, 2 unit tests and 2 compile-fail ownership tests
pass**, debug and release; clippy passes with all targets/features. Coverage includes parent/child ordering at each boundary,
non-yielding settled awaits, slice gating, nested spawns/pool reuse, input-ordered
await_any, cancellation/redelivery/shielding/deadlines/inherited sources, gate
admission, Tokio completion and shutdown, cross-thread submissions, CPU/I/O poll
fairness, panics, unhandled errors, forced-unwind drops, foreign-loop rejection,
stack/precise roots, and 10,000 parked tasks. No real heap/compiler differential
suite is claimed. Cargo emits an unrelated existing bridge_rust license metadata
warning; this crate's clippy diagnostics are clean.

## Measurements and spawn/await profile

2026-11-06, Spark aarch64, CPU 12 (Cortex-A725, maximum 2.808 GHz), Linux 4 KiB
pages. Rust `1.96.0-nightly (b90dc1e59 2026-03-04)`, Node `v24.15.0`. Release:
opt-level 3, fat LTO, one codegen unit, unwind panics. Five fresh processes per
mode, one untimed warmup and three timed jobs per process, alternating mode order.
All variants run pinned in a transient systemd user scope with `CPUWeight=10000`
(default nice), to reduce contention from a concurrent external Rust build. The
live cgroup's weight was verified as 10000 and affinity as CPU 12; no system or
other process configuration was changed. This gives scheduling weight, not CPU or
memory-bandwidth isolation. Individual load observations, commands, source hashes
and results are retained in
[profiles/measurements.json](profiles/measurements.json). No timing outliers were
removed (the after run includes a 3.585 ms high observation). Default-priority
runs during saturation are retained separately in [contention.json](profiles/contention.json):
Rust's median was 5.478 ms, Node's 1.567 ms. They are a contention diagnostic, not
the controlled comparison above. Allocation/counter and perf runs are separate
from acceptance timings.

| Work per job | Median | Min–max |
| --- | ---: | ---: |
| Before: spawn/await 10,000, baseline revision `27564fca0e` | 9.137 ms | 9.033–9.288 ms |
| After: spawn/await 10,000, original `measure` harness | **2.520 ms** | 2.494–3.585 ms |
| Node: unchanged repository workload | 1.526 ms | 1.491–1.600 ms |
| Before: 10,000 spawn/return tasks, no awaiting parent | 7.087 ms | 6.988–7.409 ms |
| After: 10,000 spawn/return tasks, no awaiting parent | 1.537 ms | 1.508–1.570 ms |
| Bare coroutine: 40,002 physical switches | 0.090 ms | 0.087–0.104 ms |
| Bare coroutine: 10,000 stack reset/entry/return cycles | 0.151 ms | 0.148–0.154 ms |

**Required ≤2.9 ms target met; ≤1.45 ms stretch target not met.** The primary
result is 252.0 ns per spawn/await pair, 1.65× the measured Node
reference, and 3.63× faster than the remeasured implementation. The
initial implementation's earlier 9.399 ms observation remains in the historical
[measurements.json](measurements.json). The profiling harness independently gives
2.454 ms; acceptance uses the original `measure` harness.

`return` is the child-execution floor: sequentially spawn an immediate-return root
from the driver, run it to completion, read its result, and drop the handle. There
is no awaiting parent, pending-future registration or waiter wakeup. It keeps one
child live at a time, so a fan-out's 10,000 concurrent stack mappings do not muddy
this floor. The child in `await` also returns immediately: its additional cost is
parent parking/resumption and dependency bookkeeping. Rust checks every result;
the unchanged Node driver checks its warmup result. Node awaits `Promise.resolve(1)`;
Rust creates actual child tasks, matching the supplied BAML workload. Runtime/loop
construction is excluded; root creation, children, and result checks are included.
This remains scheduler-only, without compiled BAML, heap allocation or telemetry.

### What perf found

`perf record -e cycles:u -F 999 -g --call-graph dwarf`, pinned to CPU 12, sampled
1,000 baseline jobs and 4,000 optimized jobs (roughly nine/ten seconds each),
using the same weighted scopes as the timing runs. Symbol
reports are [before.txt](profiles/before.txt) and [after.txt](profiles/after.txt).
Use flat self percentages: coroutine stacks make inclusive unwound call trees a
less useful attribution. Both recordings reported zero lost samples.

* Before: the vDSO clock routine alone was **17.65%**; the dominant CAS helper was
  **11.15%**; cancellation-token drop **4.69%**, tree refcount adjustment **4.22%**,
  cancellation tests **3.62%**, plus construction, Arc drops and notification work.
  The empty completion receiver was **4.29%** across start_recv/try_recv. These
  costs came from synchronous token/queue use, not executing tasks on Tokio.
* After: dispatch/settlement in `tick` is **28.89%** of a much shorter run; fiber
  entry **10.71%**, suspension/resume bookkeeping **10.25%**, spawn/reset **10.98%**,
  future recycling/drop **7.01%**, and cancellation checkpoints **6.12%**. These
  symbols include inlined operations; entry/suspend are not pure switch costs.
  The token constructors/locks, empty receive probes, and vDSO clock hotspot are
  gone. An empty linked-token-vector destructor still has a Tokio type name;
  it does not mean tokens or Tokio jobs are being created on the hot path.
* There are **four physical switches per child**, unchanged: parent → scheduler
  → child → scheduler → parent. Per job that is 40,002 switches/20,001 resumes,
  including root entry/exit. The logical minimum parent → child → parent is two;
  direct fiber handoff would need a different driver/root-capture arrangement.
  The standalone switch/reset probes show this is not the large initial cost:
  they exclude task records, cancellation, scheduling and larger Rust frames and
  must not be subtracted as an exact additive decomposition of the full loop.

### Counters and allocations

The `instrument` feature adds owner-thread counters; the profile executable also
counts loop-thread `GlobalAlloc` allocation/reallocation calls. Enable counting
only for separate diagnostic runs, not timings. For one warmed 10,000-child
spawn/await job (plus the root):

| Counter | Before | After |
| --- | ---: | ---: |
| Allocations / bytes requested | 50,003 / 4,640,400 | **0 / 0** |
| Stack-pool hits | 10,001 | 10,001 |
| Future-record pool hits | none | 10,001 |
| New future / cancellation nodes | per spawn | 0 / 0 |
| Completion-queue probes | 20,001 | **0** |
| Timer/cancellation service scans | 20,001 | **0** |
| Cancellation checkpoints | 40,001 | 30,001 |
| Wall-clock reads in scheduler bookkeeping | 40,002 | **0** |
| Architectural slice-counter reads | none | 20,001 |
| Physical fiber switches | 40,002 | 40,002 |

Before, the approximately five allocations per child were its future Rc, own and
I/O cancellation tokens, the waiter's Vec storage, and Tokio's child-registration
Vec (it shrinks when the last child is removed). The no-await baseline measured
30,000 allocations/4,000,000 bytes: future plus two tokens per root, without the
waiter/parent-child vectors. BTree/VecDeque/registry capacities were amortized;
there was no selected gate and no telemetry allocation. The benchmark bodies
are zero-sized Rust closures, so their Box does **not** allocate. A nonzero-sized
capturing closure still allocates its body Box; zero allocations is a claim about
this warmed workload, not arbitrary tasks. Cold records, growing queues, explicit
linked sources, gates and I/O can allocate normally.

### Changes and semantic safeguards

* Tasks use a reusable generational slab instead of BTreeMap entries. Full ids
  are carried by ready queues, waiters and completions. Exhausted generations
  retire a slot rather than wrap; shutdown preserves identities for local reuse.
  Metadata capacity tracks peak concurrency and is released with the loop.
* Future Rc records and waiter capacity return to a bounded free list after the
  last handle drops. Unobserved errors are reported before reset; pointer payloads
  are cleared, so the pool cannot retain managed roots. Still-held futures are
  never recycled. `pool_capacity` bounds both stack and future caches.
* Loop-local cancellation is Rc/Cell state plus inherited parent references.
  A shared cancellation epoch activates scans when cancellation actually changes.
  Reuse requires unique ownership: outstanding cancel handles or descendants keep
  their old authority intact. Ancestor chains are released iteratively, so dropping
  10,000 departed ancestors cannot overflow a small fiber stack. Explicit
  cross-thread linked sources remain Tokio
  tokens and still activate cancellation checks. TaskCancel now follows the
  specified !Send loop ownership; host cancellation uses those linked sources.
* I/O tokens are allocated only when I/O starts. Cancellation discards the old
  token; cleanup's next I/O receives a fresh token. Epoch checks remain. Pure loop
  work never touches Tokio's token tree, notify machinery or completion receiver.
* Queue/timer scans are skipped only when no I/O, host-submission capability,
  slow-path task or changed cancellation epoch can require them. Active I/O,
  deadlines, admission, external cancellation and poll fairness retain their paths.
* AArch64 time slices use the system-wide architectural monotonic counter with
  an instruction/compiler barrier and frequency-based duration conversion; x86-64
  retains Instant. Timers/deadlines still use Instant. Slice reads stay at schedule
  boundaries so a task that delays its first poll still has the correct elapsed
  slice. A single-future await avoids the general input scan and a redundant
  checkpoint; pending/ready/error/cancelled semantics and waiter FIFO are unchanged.
* Stacks were already kept mapped and committed in the pool. The measured reset
  cost did not justify replacing corosensei or adding direct fiber-to-fiber jumps.

No spec change was needed. Additional tests cover retained futures and cancellation
handles across recycling, descendants outliving recycled parents, cancellation
from another thread, stale I/O messages, shutdown reuse, generation exhaustion,
precise-root release, multiple FIFO waiters, deep ancestry destruction, and the
slice clock.

### Parked-task memory after optimization

Three fresh-process trials with 10,000 shallow parked timer tasks: **664.06 MiB**
reserved (64 KiB usable + 4 KiB guard per task), **39.06 MiB** resident fiber pages
(`mincore`, 4 KiB per task), **44.41 MiB** whole-process RSS increase and
671.19 MiB virtual-size increase. Parking took 0.398–0.401 seconds.
This is not a bound for deep compiled frames. Default stack-pool retention remains
128 mappings (8.5 MiB reserved); slab metadata retains peak capacity until loop drop.

### Reproduce

From the worktree root (repeat each timing command in five fresh processes):

```sh
export CARGO_TARGET_DIR="$PWD/target"
# A transient scope changes only the benchmark process scheduling weight.
bench() { systemd-run --user --scope --quiet -p CPUWeight=10000 taskset -c 12 "$@"; }
cargo build --manifest-path baml_language/Cargo.toml -p bex_v2_loop --release --examples
bench target/release/examples/measure
bench target/release/examples/profile return 3
bench target/release/examples/profile switch 3
bench target/release/examples/profile reset 3
NODE_OPTIONS= bench node \
  baml_language/tools/speedtest/src/speedtest/drivers/driver.js \
  baml_language/tools/speedtest/workloads/concurrency/spawn-await-x10k/main.js 3 1
bench target/release/examples/measure memory

# Separate counted build; do not use its elapsed_ns for acceptance.
CARGO_TARGET_DIR="$PWD/target/count-after" cargo build \
  --manifest-path baml_language/Cargo.toml -p bex_v2_loop \
  --release --example profile --features instrument
taskset -c 12 target/count-after/release/examples/profile await 1 count

# Keep symbols for profiling while retaining release optimizations.
CARGO_TARGET_DIR="$PWD/target/profile-after" \
  CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none \
  cargo build --manifest-path baml_language/Cargo.toml -p bex_v2_loop \
  --release --example profile
# perf was obtained with: nix build nixpkgs#linuxPackages.perf --no-link --print-out-paths
# Use that result's bin/perf as PERF.
bench "$PERF" record -e cycles:u -F 999 -g --call-graph dwarf \
  -o target/profile-after/perf.data -- \
  target/profile-after/release/examples/profile await 4000
"$PERF" report -i target/profile-after/perf.data --stdio --no-children \
  --call-graph none --sort symbol
```

The before binary was built before edits, from `27564fca0e` plus the diagnostic
harness/counters, with counters disabled for timings and perf. Raw perf recordings
remain under this worktree's target directories; portable symbol summaries and
raw measurement observations are committed under `profiles/`.

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
5. **Scaling decision:** active timers/deadlines/admission scan slab capacity
   (peak concurrency); external
   cancellation while idle is checked with a maximum 1 ms receive timeout.
   Semantics are intact, but an indexed timer queue and cancellation notifications
   would remove that scan/periodic wakeup at larger scale. The small stack default
   needs validation against compiled workloads before becoming a public runtime
   default; its reservation remains configurable.

### Runtime-owned values outside fiber stacks

The integration API can register a precise pointer using
`IntegrationRoots::root`. Its `IntegrationRoot` lease retains a stable slot;
the loop enumerates that slot while the lease lives. Dropping a lease requires
no loop borrow, including during fiber destruction. Expired weak registrations
are pruned on registration. The caller must keep the owning heap alive and may
register only null or live pointers from that heap. This does not expose root
access through `TaskCx` or make task contexts implement `RootSource`.

### Quiet poll fast path

The loop maintains bit 0 of an owner-thread checkpoint word exposed only through
`IntegrationRoots::poll_word` for runtime integration. It is clear when no ready
peer, cancellation/service source, I/O or host submission can require work.
Local cancellation and spawning request work immediately. External tokens and
deadlines keep polling active; an epoch/maintenance refresh cannot suppress them.
Bit 1 belongs to the integrated heap. A quiet poll skips the previous cancellation
ancestry walk and maintenance; active polls retain the original yield policy.
