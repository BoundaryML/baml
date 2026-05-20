# Lazy biased mutex for container internals

Idea Vaibhav proposed for protecting heap-mutable containers (`Vec<Value>`
backing `Object::Array`, `IndexMap` backing `Object::Map`, instance field
arrays) from concurrent-mutation corruption introduced by BEP-034
`spawn` — without paying any synchronization cost in the (overwhelmingly
common) single-accessor case.

This is a sketch, not a spec. We landed it as backlog item #36 during the
spawn-PR review; capturing it here so the idea survives context resets.

## The problem it solves

Two BAML threads can hold a `HeapPtr` to the same `Object::Array` and
concurrently call `arr.push(...)`. The `Vec::push` underneath does:

```
read len → bounds-check against cap → maybe realloc (free old buffer,
allocate new) → write at buf[len] → store len+1
```

None of those steps are atomic. Two racing pushes can:

- Lose a write (both threads see `len = N`, both store to `buf[N]`, both
  bump to `N+1`; one push silently dropped).
- Use-after-free (thread A reallocs the buffer; thread B was holding a
  pointer to the old buffer mid-write).
- Hit a `Vec` internal `debug_assert!` and SIGTRAP.
- Heap-corrupt in release builds where the assertion is gone.

The BEP-034 reproducer at `crates/baml_tests/tests/spawn_array_race.rs`
demonstrates all three. Currently `#[ignore]`d.

## The mechanism

Per `Object::Array` (and `Object::Map`, and any other mutable container),
add a small synchronization word that biases the fast path toward "no
contention." Two pieces of state:

- `count: AtomicUsize` — incremented on entry, decremented on exit. The
  count snapshot during entry tells you the contention state.
- `mutex: Mutex<()>` — held only when contention is detected. Allocated
  lazily / inline, but never touched on the uncontended path.

Pseudocode:

```rust
fn access(c: &Container, op: impl FnOnce(&mut RawBuffer)) {
    let prev = c.count.fetch_add(1, Acquire);
    if prev == 0 {
        // Fast path: I'm the only accessor. No mutex, no fence beyond
        // the fetch_add. Run the op directly.
        op(unsafe { c.raw_buffer_mut() });
        c.count.fetch_sub(1, Release);
        return;
    }

    // Someone else was in. Spin briefly hoping they leave — they're
    // doing a `Vec::push` so it's usually nanoseconds. SPIN_BUDGET is
    // small (~16-64 cpu_relax cycles).
    for _ in 0..SPIN_BUDGET {
        if c.count.load(Acquire) == 1 {
            // They left; the load saw `prev` (us) as the only count.
            op(unsafe { c.raw_buffer_mut() });
            c.count.fetch_sub(1, Release);
            return;
        }
        std::hint::spin_loop();
    }

    // Sustained contention. Fall back to the OS mutex.
    let _g = c.mutex.lock();
    op(unsafe { c.raw_buffer_mut() });
    c.count.fetch_sub(1, Release);
}
```

Single-threaded BAML pays:

- 1 `fetch_add(_, Acquire)` (zero-cycle on x86 for the atomic-load
  prefix, one µop on ARM with LDXR/STXR; effectively free).
- 1 plain decrement (`fetch_sub` with Release).

No mutex, no syscall, no allocation. Total: ~2 cycles of overhead per
`arr.push(...)`. The vast majority of BAML programs are single-threaded
and pay only this cost.

Two-threaded BAML pays:

- The spin loop (bounded ~50-100 cycles).
- If spin fails: full OS mutex acquire (~50-200 ns on macOS / Linux).
  But only on actual contention.

## Closest published patterns

The hybrid Vaibhav described stitches together three known ideas
(detailed below as #1, #2, #3). The full design space — including
the alternatives we considered and why we'd pick a hybrid instead of
any single one — is laid out here so future work can revisit the
choice if benchmarks come back differently.

### 1. Seqlock (Linux kernel) — the "even/odd math trick"

A counter starts at 0 (even). Writers increment to 1 (odd, signals
"write in progress"), do the work, increment to 2 (even, signals
"done"). Readers read the counter, do the read, re-read; if it's odd
OR changed, retry the read.

```text
writer:   fetch_add(counter, 1)   // now odd
          ... mutate ...
          fetch_add(counter, 1)   // now even

reader:   loop {
            v1 = load(counter)
            if v1 & 1 != 0 { continue }   // writer in progress
            ... read ...
            v2 = load(counter)
            if v1 != v2 { continue }      // writer ran during read
            break
          }
```

**Used by**: Linux kernel for `jiffies`, `/proc/stat`, the system
clock. Anything where reads vastly dominate writes and the data is
small.

**Tradeoff for BAML**: readers are completely lock-free, but
container internals are write-mostly (`push` / `insert` / `set`), so
the read-side optimization doesn't pay. Also, our "reads" are
container-state operations (length, capacity, buffer pointer) that
the reader is about to use to access the heap; a retry semantic
doesn't compose cleanly with a heap dereference. **Verdict**: not
the right fit alone, but the "in-progress signal via a counter" half
of seqlock is what Vaibhav's pattern borrows.

### 2. Biased locking (HotSpot JVM, pre-JDK 15)

Object header carried three states:

- **Unbiased**: never locked. Acquisition is cheap (no atomic ops).
- **Biased to thread T**: T has locked it before. T can re-acquire
  with a single non-atomic store to the header. Other threads
  trigger revocation (the slow path).
- **Inflated**: a real OS mutex is allocated. Used after revocation
  or sustained contention. Stays inflated forever.

**Why removed in JDK 15**: on modern CPUs (with cheap CAS and big
out-of-order windows), the bookkeeping cost of biased locking
exceeded the savings on `synchronized` blocks. JEP 374 removed it.

**Tradeoff for BAML**: the structural idea (fast path with no mutex,
fall back to a real mutex on contention) is exactly what we want.
The thread-binding part is more than we need — we don't need to
remember *who* the previous accessor was; we only care *if* there is
contention. Vaibhav's pattern is biased-locking without the
thread-bias bookkeeping.

### 3. Adaptive mutex / `parking_lot`

Standard mutex implementations spin briefly before going to sleep, on
the bet that the holder is about to release. Linux
`PTHREAD_MUTEX_ADAPTIVE_NP`, glibc's `__lll_lock_wait`, Rust's
`parking_lot` crate, Java's `synchronized` (after biased was
removed). The spin avoids the syscall cost on short-held locks.

**Tradeoff for BAML**: this gives us the "spin then mutex" half of
Vaibhav's pattern. Used as a building block — we'd literally use
`parking_lot::Mutex` for the fallback path if we wanted, or write
our own with the same shape.

### 4. RCU — Read-Copy-Update (Linux kernel)

Writers create a *new copy* of the data structure with their change
applied, then atomically swap the pointer. Old readers continue on
the old copy; the old copy is freed after a "grace period" — a
quiescent point when all readers have moved on.

**Used by**: Linux kernel for routing tables, file descriptor tables,
SELinux policies. Anything read-mostly with infrequent updates.

**Tradeoff for BAML**: writes are expensive (copy + atomic swap +
deferred free). For container ops that are write-heavy (`push`,
`pop`, `insert`), this is the wrong shape. Also the "grace period"
needs a quiescent-state mechanism we don't have (we'd need an epoch
GC or hazard pointers). **Verdict**: don't do this for containers.

### 5. Plain mutex

A `Mutex<Vec<Value>>` on every container. Every access locks. Every
release unlocks.

**Cost**: 2 atomic CAS per access (lock + unlock). On x86 with
parking_lot's biased futex implementation, ~5-10 cycles uncontended.
**Cumulative** cost across the lifetime of a BAML program is real —
every `arr.push` and `map.insert` and `field.set` pays it.

**Tradeoff for BAML**: pessimistic for the dominant case
(single-threaded). It's the safest and simplest, but the cost is
non-trivial for code that the user expects to be free. **Verdict**:
the lazy biased mutex (Vaibhav's pattern) strictly dominates.

### 6. Flat combining (PPoPP'10, Hendler/Incze/Shavit/Tzafrir)

Threads that want to mutate a shared structure publish their request
to a per-thread slot, then try to become the "combiner" — the single
thread that actually does all queued ops in batch. Others wait for
their results to be published.

**Used by**: research / specialty concurrent data structure libraries.
JDK's `LongAdder` is conceptually related.

**Tradeoff for BAML**: very high throughput under heavy contention,
but high latency per individual op (you wait for the combiner to
get to your request). For BAML where most ops are single-threaded
and contention is rare, the overhead of publishing the request
beats the benefit. **Verdict**: not the right tool.

### 7. Intel TSX / hardware lock elision

Speculative execution of critical sections. The CPU optimistically
runs the locked region without taking the lock; if no conflict is
detected, the transaction commits. On conflict, roll back and take
the lock for real.

**Used by**: Intel processors with TSX, Java's `synchronized` blocks
under some configurations.

**Tradeoff for BAML**: requires TSX (only Intel, not ARM/Apple
Silicon — which is most of our user base). The fallback path is
still a real mutex. Doesn't help vs Vaibhav's pattern, which has
zero mutex on the uncontended path. **Verdict**: not portable enough.

### 8. Hazard pointers / epoch-based reclamation

Lock-free data structure technique. Each thread publishes pointers
it's currently using; freers check before reclaiming.

**Used by**: lock-free queues, hash tables in concurrent runtimes
(Folly, Crossbeam).

**Tradeoff for BAML**: would let us avoid serializing reads entirely,
but we already get that from STW GC (which handles the
reclamation-safety problem differently — by stopping every reader
before freeing). **Verdict**: redundant with our existing GC model.

### Summary table

| Pattern | Single-thread cost | Multi-thread cost | Fits BAML? |
|---|---|---|---|
| **Lazy biased mutex** (Vaibhav's) | 2 cycles (atomic incr+decr) | Spin then mutex | **Yes — picks the best of #1+#2+#3** |
| Seqlock | 0 (reads completely free) | Writer is fast; reader retries on race | Wrong for write-mostly containers |
| Biased locking | ~0 (non-atomic store) | One-way revocation, then mutex | Over-engineered (thread-bias bookkeeping not needed) |
| Adaptive mutex (parking_lot) | 2 CAS | Spin then mutex | Pessimistic on single-thread (always atomic) |
| Plain mutex | 2 CAS | Mutex serializes | Pessimistic on single-thread |
| RCU | 0 (reads free) | Writer copies + swaps; reclaim after grace | Wrong for write-heavy containers, no grace mechanism |
| Flat combining | High (publish request) | Excellent throughput | Wrong for low-contention workloads |
| Intel TSX | 0 (speculative) | Fallback to mutex | Not on ARM/Apple Silicon |
| Hazard pointers | 0 (publish in TLS) | Lock-free | Redundant with STW GC |

**Why Vaibhav's pattern wins for BAML**: BAML's workload is dominated
by single-threaded container ops, with rare (but real, per BEP-034)
multi-threaded races. We want the single-threaded cost to be
essentially zero. Of the patterns above, only the lazy biased mutex
and biased locking achieve that, and the lazy biased mutex is
simpler (no per-thread bookkeeping). The combination of seqlock-style
counter + JVM-style fast-path + parking_lot-style fallback hits the
sweet spot.

I don't know a single canonical name for this exact combination, but
it's a common DIY shape when you want JVM-biased-locking semantics
without the JVM bookkeeping. Closest published analog: **flat
combining** (PPoPP'10) or Intel TSX-style speculative lock elision,
both of which try to avoid the mutex on the uncontended path but pay
much higher fixed costs than what we'd implement.

## Why this beats the alternatives

| Approach | Single-thread cost | Multi-thread cost | Notes |
|---|---|---|---|
| **This pattern (lazy biased mutex)** | ~2 cycles (atomic incr+decr) | Spin then mutex | What we'd do |
| Plain mutex | ~2 CAS (lock + unlock) | Mutex serializes | Pessimistic for the common case |
| Seqlock (readers always free) | 0 on reads, contended writes serialize | Readers retry on race | Better for read-mostly. Container internals are write-mostly (push/pop/set). |
| RCU (copy-on-write) | 0 on reads | Writer allocs new copy, old freed after grace period | Heavy for in-place container ops |
| No protection (status quo) | 0 | Heap corruption (the reproducer) | Status quo on canary |

For BAML's spawn workload, **single-threaded is the dominant case**
(most BAML programs don't use `spawn`; among those that do, most
container ops happen on per-thread data). The lazy biased mutex
matches the cost profile to the workload.

## What it does and does not protect against

**Protects** against:

- Lost pushes on `Vec::push` (the 1002-vs-1003 case in the
  `spawn_array_race` reproducer).
- Use-after-free on capacity reallocation (the SIGTRAP case).
- Map hash-chain cycles forming under racing insert.
- Any container-level invariant that requires the container's internal
  state to be consistent during an op.

**Does NOT protect** against:

- The `Value`-tearing race described in `spawn_value_tearing.md`. That's
  a `Value`-shape problem solved by the tagged-pointer refactor
  (perf/tagged-ptr-value branch, in progress). Orthogonal.
- User-visible logic races (lost-write semantics from the user's
  perspective). If two `arr.push(x)` calls race, exactly one survives.
  Under this pattern they both survive but the user can't predict the
  order. That's still "user's problem."
- GC traversal vs racing mutator. We already handle this via stop-the-
  world GC; this pattern doesn't change that.

## What it would take to ship

1. Define a `LazyBiasedMutex` struct with the `count`/`mutex` fields and
   the `access` method. Live in `bex_heap` or `bex_vm_types`.
2. Embed one instance in each mutable `Object` variant that can be
   spawn-shared: `Array`, `Map`, `Instance` (for the field array),
   `Cell`. Bump `Object` size by 16 bytes per variant (one
   `AtomicUsize` + one `Mutex<()>` header).
3. Route all mutating native methods through `access()`. Probably ~30
   call sites in `bex_vm/src/package_baml/{array,map,instance,cell}.rs`.
4. Benchmark vs spawn-baseline (single-threaded should be ~zero
   regression; concurrent should fix the reproducer with sub-100ns
   tail).

## Open questions

- **Where does the `Mutex<()>` actually live?** Inline in every container
  costs 16-32 bytes per object. Or: lazy — store a `Box<Mutex<()>>`
  that's only allocated on first contention. The biased-locking
  pattern goes the lazy route; we'd save 16 bytes per uncontended
  container but pay one heap alloc on first contention.
- **Counter wraparound** — `AtomicUsize` doesn't wrap practically, but
  if we shrink it to a u8 (which is enough — we'd never have >255
  concurrent accessors on one container) we save bytes.
- **Should the spin budget adapt?** A counter of how often spin-then-
  fall-back-to-mutex happens for this specific container could let us
  skip the spin entirely on heavily-contended containers. Probably
  not worth it for v1.
- **Interaction with the tagged-pointer Value refactor.** Orthogonal —
  the Value refactor makes individual `Value` slot reads atomic; the
  lazy biased mutex protects the surrounding container structure
  (length, capacity, buffer pointer). Both are needed for full
  spawn-safe containers.
- **Map cycle detection** — even with the mutex, racing writes that
  somehow bypass it (defensive code) should not infinite-loop. We'd
  still want the CLR-style collision-counter backstop on hash chain
  walks (see `vm_safety_under_races.md` item 3). The mutex is the
  optimistic path; the collision counter is the safety net.

## Related docs

- `spawn_value_tearing.md` — the orthogonal `Value`-shape race that
  tagged-pointer Value fixes.
- `vm_safety_under_races.md` — synthesis of JVM/CLR/Go patterns;
  Vaibhav's pattern lives in section "(5) Lazy biased mutex" of the
  applicable adoptions list.
- BEP-034 PR `feature/bep-034-spawn-await` — what introduced the
  ability to race on shared containers in the first place.
- `crates/baml_tests/tests/spawn_array_race.rs` — the reproducer for
  the failure mode this would fix. Currently `#[ignore]`d pending
  either this pattern or a different approach landing.
