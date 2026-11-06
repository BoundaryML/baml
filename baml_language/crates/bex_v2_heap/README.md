# Runtime v2 heap

A dependency-free, single-owner (`!Send`, `!Sync`), non-moving mark-region heap.
Objects have the specified 8-byte `Header { desc: u32, aux: u32 }`; GC never
writes mark state into object memory. The workspace's existing `crates/*` glob
includes this crate. No existing runtime crate depends on it yet.

Allocation uses 32 KiB aligned blocks, 128-byte lines, 16-byte object alignment,
an inlined bump fast path and a cold slow path. Every allocation zeroes the
entire rounded allocation and publishes its header and start bit. Allocations
strictly above 8 KiB use individually swept large objects, aligned to 64 KiB
(page-aligned on native systems with 4/16/64 KiB pages). Small objects retain
stable addresses; empty lines are reused. Empty blocks remain reserved for
reuse until heap destruction. There is no compaction or RSS limit.

Each block has the specified 2048-bit start bitmap, 2048-bit mark bitmap and
256 line bytes, plus a 2048-entry `u16` requested-size table. This extra side
metadata bounds interior candidates precisely, excluding alignment padding,
without adding an object-header word. Block lookup is hashed by aligned base;
backward bitmap search finds the candidate start. Large-object lookup uses an
ordered address index. Dead starts are removed lazily when allocation reaches
a block; before that, the previous mark bitmap rejects dead candidates. A new
collection finishes that metadata sweep before resetting marks.

The budget is `max(32 MiB, 4 * live_bytes)`, with saturating arithmetic, ported
from `bex_heap/src/gc_policy.rs`. Rounded object bytes, including inline buffers
and large objects, spend the budget. Crossing it enters the slow path on the
next allocation or collects at a safepoint. Marking resets spending; retained
finalizer graphs contribute to live bytes. This crate has no TLABs or external
Rust payload allocator: the old TLAB settlement, shared-payload census and
negative external-release accounting tests are consequently not applicable.
A future opaque builtin with external allocations needs a separate accounting
hook before that storage is covered by the budget.

## Roots, descriptors and unsafe contract

`RootSource` has exactly the spec's two methods. Collection scans conservative
word-aligned `[low, high)` ranges first, then precise roots, then descriptor
edges. All heap tracing is precise. Integers resembling allocated base/interior
addresses intentionally retain objects; addresses outside requested object size,
in alignment padding, or at dead starts do not. Interior precise roots also work.

The owner must spill the running thread's registers, publish its stack range,
keep all fibers parked during scanning, and report every precise owner root.
The library does not discover stacks or install a TLS heap. Allocation,
collection, safepoints, verification and pointer stores are unsafe APIs because
Rust cannot prove the validity/completeness of raw roots. No references may
outlive reclamation; descriptor-covered pointer slots must be null or managed
pointers. Do not mutate the descriptor id. Native conservative scanning remains
compiler/ABI-dependent, not a portable Rust or wasm rooting guarantee.

Descriptors and the boolean pointer-type table are immutable static slices.
`TaggedSlot` is exactly `{ tag: u64, payload: u64 }`: `tag` is a type ID, and
`pointer_types[tag] == true` means trace the payload as a pointer. Scalar tags
never trace their payload, even if the bits resemble an address. Unknown tags
are non-pointer; pack emission must ensure every valid pointer type is present.
Plain pointer and tagged ranges use **byte offsets from the object header** and
slot counts, with widths 8 and 16 respectively. Field tables retain declaration
order independently of physical order. `write_ptr` is the inline plain-store
hook; scalar stores do not use it.

Variable objects carry an `Elements { offset, layout }` in `elem`. `size` is the
element stride, allocation `bytes` includes the header/prefix, and the count is
`(bytes - offset) / size`. Capacity slots must be initialized (zero until used).
Elements may be scalar, pointer, tagged, or an inline fixed descriptor (for map
entries). An inline entry descriptor's offsets are relative to that entry;
it reserves the same first 8 bytes as an ordinary fixed layout, but no header
is consulted there. Fixed prefix ranges are traced in addition to elements.
These conventions support precise list/map buffers without object-specific GC
callbacks. Header `aux` starts at zero and is available to the runtime.

## Finalizers and weak tables

All unreachable, not-yet-cleaned seeds are selected before any seed's graph is
traced, including cycles. Pending cleanup receivers and everything they reach
remain strong roots across subsequent collections. `take_pending_finalizers()`
transfers non-cloneable tokens to the task loop, **not** the keepalive obligation.
The loop calls the indicated static `FinalizerId`, then `complete_finalizer()`
after return (also on thrown cleanup). A dropped token leaves its graph retained;
this favors safety over silently releasing an unfinished cleanup. A resurrected
pointer must be published as a root before completing the token. Completed
objects never enqueue again, even after resurrection. `suppress_finalizer()`
records explicit/defer cleanup before GC queues it.

Ordering follows the existing storage-order test oracle: block creation order,
then slot offset; large objects follow in address order. This is deterministic
within a heap, not a new language-level order between unrelated objects.
Weak object-keyed tables are filtered after finalizer retention. Their values
are opaque non-owning integers, not traced values or ephemerons. Pending-finalizer
keys remain present until the object truly becomes unreachable. Heap destruction
releases storage; shutdown cleanup scheduling belongs to the owner.

## Interface adaptations for review

- `Heap::alloc(desc, bytes, roots)` explicitly accepts the root source. The
  spec's two-argument `alloc(desc, bytes)` entry point needs the task loop's TLS
  wrapper, which is outside this crate. This is the public API deviation; it
  avoids a global root-source lifetime or reentrant mutable-heap borrow here.
- The spec leaves variable element count/location and finalizer call ABI
  unspecified. The `Elements` conventions above and a static `FinalizerId`
  plus completion token make those seams concrete. Generated builtin layouts
  and the owner should confirm them before integration.
- The large-object space follows ordinary blocks in finalizer storage order;
  the old heap had no directly equivalent large-object ordering. Review this
  extension if cleanup ordering becomes observable between those spaces.

No header layout, movement, root-trait signature, mark metadata geometry,
collection trigger, or barrier semantics were changed. Allocation retries a
full collection on backing-allocation failure, retries reclaimed line runs, and
panics with `BAML out of memory` if allocation still fails. Connecting that panic
to the interpreter-equivalent language error belongs to the task-loop boundary;
actual OS allocation failure and the future error translation are untested.

## Verification

From this worktree's `baml_language` directory:

```sh
export CARGO_TARGET_DIR="$PWD/crates/bex_v2_heap/target/build"
cargo test -p bex_v2_heap
cargo test -p bex_v2_heap --release
cargo clippy -p bex_v2_heap --all-targets --no-deps -- -D warnings
```

14 integration tests and two compile-fail ownership doctests pass. Tests cover
zeroing/reuse, aliases/cycles, model-checked reachability, stale and interior
candidates, padding rejection, large objects, precise tagged/list/map elements,
finalizer cycles/order/explicit suppression/once-only resurrection, weak clearing,
slow-path/safepoint stress and live-byte budgeting. Debug collections verify every
precise root and traced live field points to an allocated marked object.

The fiber test uses eight corosensei 0.3.4 fibers, each holding a raw object
pointer across ten suspensions. The parent scans only `[saved_sp, stack.base)`;
there is **no object-pointer root registry**. All 80 resumptions check identity,
data and heap recognition; live-byte assertions check each collection happened.
It passes in debug and optimized release builds on Spark/aarch64. Allocation
happens before the parked-collection rounds; this test does not claim to test
running-thread register capture. A stress test separately forces collections
at allocation slow paths and safepoints.

The private development-only corosensei fork adds the prior experiment's
`saved_sp()` accessor. See [vendor/README.md](vendor/README.md) for provenance,
packaging changes and the task-loop integration requirement. `--no-deps` keeps
strict Clippy focused on this crate; upstream vendored corosensei has existing
style/MSRV warnings under this toolchain.

## Benchmarks

Measured on Spark, aarch64 Linux, pinned to CPU 12, with
`rustc 1.96.0-nightly (b90dc1e59 2026-03-04)`. Workspace release settings: opt-level
3, fat LTO, one codegen unit, unwind panics. Run serially:

```sh
export CARGO_TARGET_DIR="$PWD/crates/bex_v2_heap/target/build"
cargo build -p bex_v2_heap --example bench --release
taskset -c 12 "$CARGO_TARGET_DIR/release/examples/bench" \
  | tee crates/bex_v2_heap/bench-results.txt
```

The binary performs five trials per shape, each with a fresh heap, an untimed
verification warmup, then one timed tree job or two timed low-retention jobs.
Heap capacity is retained after warmup; lazy block growth during timed work is
included. Every timed job validates its checksum. `bench-results.txt` retains
all measurements. Table entries are median ms/job; max pause is the largest
single collection across five trials. Lazy allocator sweep time is outside the
reported collection pause. Collection counts are per timed trial.

| Collector / runtime | Trees ms/job | Low-retention ms/job | Trees max pause ms | Low max pause ms |
|---|---:|---:|---:|---:|
| **bex_v2_heap** | **33.618** | **5.893** | **2.259** | **0.347** |
| OwnSemiSpace (reference) | 19.301 | 1.175 | 0.248 | 0.051 |
| MMTk Immix (reference) | 31.672 | 2.326 | 1.886 | 0.772 |
| Node (reference) | 39.067 | 17.223 | — | — |

Current trees range: 33.482–33.822 ms/job, three collections per timed job.
Low-retention range: 5.854–6.037 ms/job, one collection per **two-job** timed
trial (0.5/job). This is the final five-trial run, not a selection of the best
trials from earlier development runs.

References are historical values from `perf/v2-gc/REPORT.md`, not reruns.
This benchmark uses precise roots to isolate heap costs, as do the listed native
references. Binary trees allocates 3,222,190 32-byte nodes (depth 14 plus depth-15
stretch), checksum `3222190`. Low retention allocates 262,144 cells/job with four
inline integers plus a scalar id, retains four batches of 2048, and verifies
`553779200,17813504`. The 8-byte v2 header makes cells 48 bytes rather than the
reference's 56 bytes (64 after this heap's alignment). Trees retain 8 unused bytes
to preserve the reference's 32-byte size.

This is a shape/layout comparison, **not** a controlled collector-only comparison:
the reference used a fixed 16 MiB heap (OwnSemiSpace: two 8 MiB spaces); this heap
uses the required adaptive budget with 32 MiB minimum headroom. Timed reserved
object storage reaches about 33.7 MiB for trees and 32.1 MiB for low retention,
excluding metadata and host allocations. Peak RSS and fragmentation over
long-running agent workloads have not been measured. These are handwritten
kernels, not generated BAML or an end-to-end runtime acceptance test.

Binary trees is below the historical Node time. Low retention misses decision
3's approximately-10%-of-semispace gate substantially. This implementation does
not establish that the full phase-one performance gate passes; further allocator
profiling and memory measurements are needed before making that decision.
