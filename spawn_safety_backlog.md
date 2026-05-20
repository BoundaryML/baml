# BAML spawn-safety backlog

Complete backlog of every applicable idea from the JVM / .NET / Go
research (see `vm_safety_under_races.md` for the source material) plus
the BAML-specific ideas, ordered by priority. Captured before context
reset; will outlive any single session.

Goal: BAML's runtime tolerates user-code data races introduced by
BEP-034 `spawn` without crashing, segfaulting, leaking pointers, or
violating memory safety. User-visible logic races (lost writes, wrong
values) are accepted — same contract as JVM and .NET.

## Status of related work

| Item | Status |
|---|---|
| BEP-034 spawn/await/cancel | Merged-pending on PR #3520; spawn-baseline benchmarks captured in `~/Desktop/github/baml-perf/spawn-baseline-c4ed637cf/` |
| Canary baseline benchmarks | Captured in `~/Desktop/github/baml-perf/canary-baseline-338e5f2a1/` |
| `Value` tearing reproducer doc | `spawn_value_tearing.md` at repo root |
| `Vec::push` race reproducer | `crates/baml_tests/tests/spawn_array_race.rs` (`#[ignore]`d) |
| GC park / new_permit deadlock | Fixed and merged via #3535 |
| JVM/.NET/Go research synthesis | `vm_safety_under_races.md` at repo root |
| Tagged-pointer `Value` refactor | In progress on `perf/tagged-ptr-value`; stalled mid-types.rs |

---

## Must-do (correctness or BAML-defining)

### 1. Tagged-pointer `Value` (one-word `Value`)

**Doc**: `spawn_value_tearing.md`.

**Problem**: `Value` is currently 16 bytes (1-byte enum discriminant + 8-byte payload + padding). A write is two `mov` instructions. Two threads writing different-variant `Value`s to the same heap slot can produce a torn `(tag, payload)` pair, and the next reader matching on the tag and accessing the payload as that variant gets a fake `HeapPtr` to follow → SIGSEGV or worse.

**Fix**: shrink `Value` to a `#[repr(transparent)] struct Value(u64)` with low-3-bit tagging. Aligned 8-byte loads/stores are hardware-atomic on every supported target → torn reads become impossible.

**Bit layout** (low 3 bits = tag):
- `0x0` → `Null` (the only zero pointer)
- `0x2`, `0x4` → `Bool(false)`, `Bool(true)` sentinels
- `0x6` → `OmittedArg` sentinel
- low bit set → `Int(i63)`, shift-right with sign extension
- low 3 bits zero, non-zero → `Object(HeapPtr)` (aligned heap pointer)

**Float handling**: heap-box as `Object::Float(f64)`. Trades float-arithmetic cost (one heap alloc per intermediate result) for the uniform 8-byte representation. BAML programs are integer-and-object-heavy; the cost is bounded.

**Range loss**: integers shrink from i64 to i63 (max ~4.6 quintillion). Nanosecond timestamps fit until ~2200.

**Status**: branch `perf/tagged-ptr-value` started. `types.rs` `Value` definition + `Object::Float` variant landed. The match-arm migration across the workspace is in progress. Mechanical perl rename pass (with `\b` word boundaries) handles ~80% of sites; the remaining ~20% need manual fix-up for match patterns + Float-boxing TLAB threading.

**Expected perf delta**: aim for neutral-to-better vs `spawn-baseline-c4ed637cf`. Halved cache footprint per `Value` slot should help, especially on `vm_field_access_50k` and `vm_array_iter_10k` benchmarks.

---

### 2. Iteration backstop on map hash chain walks

**Source**: .NET `Dictionary<K,V>` pattern. Every walk of a `next`-chain in `Dictionary` carries a collision counter; if it exceeds the bucket array length (which means the chain has formed a cycle), throws `ConcurrentOperationsNotSupported`. CLR's only defense against the Java-7 `HashMap` CPU-spin bug.

**Why**: prevents the single worst failure mode under racing map writes — a BAML thread spinning forever in a method that should return in microseconds. The Java-8 `HashMap` redesign avoids the *deterministic* cycle but not all racing scenarios.

**Implementation**: in `Object::Map` operations (`insert`, `remove`, `get`, etc.), every `next`-pointer walk gets a counter. If `count > bucket_array.length`, throw a new BAML panic class `baml.panics.ConcurrentMutation` (user-catchable; not a runtime `fatal`).

**Cost**: one increment + branch per probe step. Negligible vs hash computation.

**Where**: probably 3-5 sites in `Object::Map` methods. Search for `.next` chain walks.

**Order**: ship before tagged-pointer `Value` because it's a standalone correctness improvement with no dependency.

---

### 3. Lazy biased mutex per container

**Doc**: `lazy_biased_mutex_for_containers.md`.

**Problem**: even with tagged-pointer `Value` fixing individual slot reads, the container's internal state (`Vec` length / capacity / backing pointer; `IndexMap` hash table) can still get corrupted under racing `push` / `insert`. The `spawn_array_race.rs` reproducer shows this: lost pushes and SIGTRAP from `Vec` internal `debug_assert!`.

**Fix**: per `Object::Array` / `Object::Map` / `Object::Instance` / `Object::Cell`, embed an `AtomicUsize count` + (optional) `Mutex<()>`. Fast path: `fetch_add(1)`, if was 0 do the op directly. Contention: spin briefly, then fall back to the mutex.

**Cost**: ~2 cycles per access in the dominant (uncontended) case. The OS mutex only kicks in on actual contention.

**Closest published patterns**: seqlock (Linux), biased locking (HotSpot pre-JDK15), adaptive mutex (parking_lot).

**Order**: after tagged-pointer `Value`, before relaxed atomics. The full backlog item is in `lazy_biased_mutex_for_containers.md`.

---

### 4. `Relaxed` atomics on cross-thread-shareable `Value` slots

**Source**: Rust language memory model. Without explicit atomic ordering, plain field stores on shared memory are formally UB in Rust — the optimizer is allowed to assume races never happen and miscompile accordingly. Hardware delivers atomicity for free on aligned 8-byte stores; we need to communicate that to the language model.

**Implementation**: mark `Value` slots in `Object::Array`, `Object::Map`, `Object::Instance`, `Object::Cell` as `AtomicU64`. Read/write via `AtomicU64::store(value.bits(), Relaxed)` / `AtomicU64::load(Relaxed).into()`. Stack / frame / TLAB-private slots stay plain `Value`.

**Cost**: zero in steady state. `Relaxed` lowers to the same `mov` as plain stores on x86-64 / ARM64 / RISC-V — it's purely a language-level annotation.

**Why now matters**: the `bits()` / `from_bits()` accessors are baked into the `Value` API for exactly this purpose (see `types.rs`).

**Order**: stack on top of tagged-pointer Value (depends on the `Value(u64)` repr).

---

### 5. STW GC as load-bearing invariant

**Doc**: `vm_safety_under_races.md` item 6.

**Action**: add a comment to `bex_heap/src/heap_guard.rs` and to the spawn-runtime docs stating that the entire mutator-vs-mutator safety story assumes stop-the-world GC. Concurrent / incremental GC would re-introduce mutator-vs-GC races that JVM/CLR/Go all engineer around but BAML currently doesn't have to.

**Why**: keeps future GC work honest. Anyone proposing concurrent GC must revisit this whole backlog.

**Cost**: zero — pure documentation.

---

## Should-do (correctness gaps, lower priority)

### 6. Bounds-check on live `len` (document the existing invariant)

**Source**: JVM / .NET pattern. `ArrayList.get(i)` and `List<T>::get(i)` both re-read `size` / `_size` at access time, never cache it.

**Status**: already true in BAML's `Vec<Value>` access via `as_array_mut`. We need to document the invariant and add a clippy lint or test to prevent regressions.

**Action**: add a doc comment to `bex_vm::as_array` / `as_array_mut` enforcing "always re-read length, never cache across a potential-grow point." Add a regression test that exercises racing grow.

**Cost**: doc-only.

---

### 7. `baml.panics.ConcurrentMutation` panic class

**New stdlib class** for the iteration-backstop fault (item 2) plus any future "this would have raced; bailing" detections. User-catchable from BAML (`catch (e) { ConcurrentMutation => ... }`).

**Definition**: in `ns_panics/panics.baml`, alongside existing `Cancelled`, `DivisionByZero`, etc.

**Fields**: `message: string`, optionally `container_type: string` for debuggability.

**Order**: ships with item 2 (iteration backstop is its first user).

---

### 8. Atomic backing-buffer-pointer swap on grow

**Source**: JVM / .NET pattern. When `ArrayList.grow()` reallocates, the swap of `elementData = newArray` is an aligned reference write — atomic on the hardware. Readers holding a pointer to the old backing array complete their op on the old array (which the GC keeps alive), then re-resolve on next access.

**Problem in BAML**: Rust's `Vec` does its grow internally and the buffer swap is not exposed as an atomic-by-design operation. Racing grow is UB in Rust's `Vec`.

**Fix** (heavyweight): replace `Object::Array`'s `Vec<Value>` with a custom container whose backing pointer is `AtomicPtr<[AtomicU64]>` and whose grow atomically swaps it.

**Cost**: significant engineering, only justified if the lazy-biased-mutex (item 3) doesn't fully cover the case. The mutex serializes mutators so two grows can't race — at which point the underlying `Vec::push` is single-threaded and safe.

**Defer** unless we observe a real production use-after-free that the mutex misses (e.g. via a corner case in the spin budget).

---

### 9. Custom container with atomic-pointer-swap grow

Same as item 8 but framed as a discrete artifact: a `SharedVec<T>` type that lives in `bex_heap` or `bex_vm_types` and is used as the backing for `Object::Array`. Worth it if we ever do concurrent GC (item 5 invariant change). Defer.

---

## Nice-to-have (debug aids)

### 10. Debug-build race detector

**Source**: ThreadSanitizer / Go's race detector. Instrument every `Value` read/write under `cfg(debug_assertions)` with a check that detects accesses without a happens-before edge.

**Why**: even with the runtime hardened, users will write racing code. A race detector helps them debug. Existing tools (`cargo +nightly test -Z sanitizer=thread`) are heavy and require nightly Rust.

**Cost**: significant implementation. Lots of false positives in BAML's `unsafe` patterns. Defer until users actually ask for it.

---

### 11. User-facing "Concurrency model" doc

**What**: a doc in `BEPS/` or BAML's docs site explaining the runtime's promise under spawn races: what won't crash, what might silently corrupt, when to use `Mutex` / `Channel` (when those exist). Modeled on .NET's threading remarks.

**Cost**: writing time. Should land alongside the canary PR for items 1-3.

---

## Explicitly NOT doing (researched and rejected)

### Version counter / `modCount` fail-fast

**Source**: JVM `modCount`, .NET `_version`.

**Why not**: both runtimes' docs explicitly say "best-effort, not a safety mechanism, do not rely on this for correctness." It catches some concurrent-modification-during-iteration bugs but misses many other races. Adds a field to every container for diagnostic-only value. If we want race detection we'd rather invest in a debug-build TSan equivalent (item 10).

### Go-style `fatal("concurrent X")`

**Source**: Go's `runtime.mapassign` `fatal("concurrent map writes")`.

**Why not**: uncatchable runtime crash. Converts user bugs into process death. Worse UX than .NET's recoverable `ConcurrentOperationsNotSupported` throw (item 7), which users can `catch` and react to.

### NaN-boxing instead of tagged pointer

**Source**: V8, SpiderMonkey, LuaJIT.

**Why not**: more complex implementation (IEEE 754 bit games, NaN canonicalization for user-produced NaNs). Tagged pointer is simpler and the float-arithmetic regression of boxed floats is acceptable for BAML's workload (mostly integer and object).

---

## Suggested PR sequence (when picking back up)

1. **`bex_heap`: STW GC invariant doc** (item 5). One-line comment, no risk, lands in 5 minutes. Just sets the contract.
2. **`Object::Map` iteration backstop** (items 2 + 7). Stand-alone correctness improvement. New stdlib panic class.
3. **Tagged-pointer `Value`** (item 1). The big one. Already in progress on `perf/tagged-ptr-value`. Benchmark vs `spawn-baseline-c4ed637cf`.
4. **`Relaxed` atomics on shareable `Value` slots** (item 4). After tagged-pointer lands.
5. **Lazy biased mutex per container** (item 3). After atomics; this provides the structural protection that makes the whole thing work for racing `push` etc.
6. **Re-enable `spawn_array_race.rs` test** as a regression guard.
7. **User-facing concurrency model doc** (item 11). Ships alongside the canary PR.
8. **(Optional, defer)** items 8-10.

## Reference

- `spawn_value_tearing.md` — Q1 deep dive on the `Value`-tearing failure mode.
- `vm_safety_under_races.md` — synthesis of JVM / .NET / Go research with code-level references into the upstream projects (jdk / runtime / go).
- `lazy_biased_mutex_for_containers.md` — Vaibhav's per-container mutex pattern, expanded.
- `~/Desktop/github/baml-perf/canary-baseline-338e5f2a1/` — canary perf baseline (hw counters, divan, real-world vs python/bun).
- `~/Desktop/github/baml-perf/spawn-baseline-c4ed637cf/` — spawn-branch perf baseline. Use as the reference for measuring any subsequent perf work; spawn alone added ~1-2% over canary.
- `~/Desktop/github/baml-perf/darwin-kperf-events-fork/` — local patched darwin-kperf for M5 Max (`as5-1`) recognition. The patched workspace Cargo.toml has `[patch.crates-io]` pointing here.
