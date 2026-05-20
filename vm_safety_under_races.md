# VM safety under user data races — research findings

Companion to `spawn_value_tearing.md`. This document captures what JVM,
.NET CLR, and Go do for shared mutable containers (`ArrayList` / `List<T>`
/ slice, `HashMap` / `Dictionary` / map) under unsynchronized concurrent
access, with a focus on what BAML can adopt to support BEP-034 spawn
without crashing the runtime when user code races.

Goal we're designing toward: **user data races may produce wrong
user-visible values, but the runtime itself does not segfault, leak
pointers, infinite-loop, or violate memory safety.**

This is the JVM/CLR contract. Go partially honors it (map writes) and
partially does not (slice/interface tearing is documented as "arbitrary
memory corruption").

The full per-runtime walkthroughs (with code line refs) live in
git history attached to this commit. What follows is the synthesized
distillation: patterns, tradeoffs, and what's directly applicable to BAML.

## Three patterns at a glance

| Concern | **JVM (ArrayList / HashMap)** | **.NET (List / Dictionary)** | **Go (slice / map)** |
|---|---|---|---|
| Container hot-path atomicity | Zero. Plain field reads/writes. | Zero. Plain field reads/writes. | Zero. Plain field reads/writes. |
| Backing-buffer swap on grow | Ref write to `elementData` — atomic per JLS §17.7 | Ref write to `_items` — atomic per ECMA + release-fence per `Memory-model.md §134` | `s = append(...)` writes 3-word slice header — **NOT atomic, can tear**, can corrupt runtime |
| Lost writes under race? | Yes, accepted. | Yes, accepted. | Yes, accepted. |
| Bounds checks read live state? | Yes — every access re-reads `size` / `length`. Re-fetches per call. | Yes, but inline path may snapshot the buffer reference first into a local. | Yes for slice index; map lookup walks current bucket array. |
| Infinite-loop backstop on hash chains | No explicit backstop. Java 8 mitigated the Java 7 deterministic cycle bug by switching to tail-insertion + treeify-on-collision. Racing puts can still form cycles in theory; the treeify gives O(log n) bounds on degenerate bins. | **Yes** — Dictionary counts collisions every chain walk; throws `ConcurrentOperationsNotSupported` if exceeds `entries.Length`. (`Dictionary.cs:437-441, 468-472, 558-562, 590-594, 1358-1362`) | **Yes** — single XOR-toggled `writing` byte on the map; on detected concurrent write throws `fatal("concurrent map writes")` (uncatchable). |
| Concurrent-modification detection during iteration | `modCount` snapshot in iterator, throws CME on `next()` — explicitly best-effort. | `_version` field, same shape, throws `InvalidOperationException` — best-effort. | Iterator checks `writing` byte → fatal. |
| Multi-word values (slice / interface / 16-byte enum) | All user-visible refs are pointer-sized. `long`/`double` can tear per JLS §17.7 unless `volatile`. | All refs pointer-sized; primitives larger than pointer can tear. | **3-word slice headers, 2-word interface and string headers explicitly allowed to tear** — Go docs state this leads to "arbitrary memory corruption." |
| GC safety vs mutating threads | Concurrent GCs (G1, ZGC) use barriers. Object headers atomic by construction (immutable klass post-alloc; mark word via CAS). | Same as JVM in spirit — refs atomic, object construction release-fenced so freshly allocated array observed valid before published. | Concurrent GC. Torn interface read in GC trace → segfault. Go does not formally promise runtime safety against user races. |
| Per-op overhead in single-threaded code | ~zero | ~zero (plus collision counter increment per probe) | Map write: 1 byte-load + 1 branch + 1 XOR-store on entry, same on exit. ~3-5 cycles. Slice: zero. |

## Three observations that drive the BAML design

### 1. No production VM pays for full atomicity on container hot paths

All three runtimes plain-load, plain-store, and accept lost writes. The
cheap stuff (single ref write atomicity) gets atomicity because the
hardware delivers it for free on aligned word stores. The expensive stuff
(multi-word containers) doesn't. Adopting this pattern in BAML costs
nothing in the steady state — `Vec::push` becomes "look up live len,
check capacity, store, increment len" with no extra synchronization.

### 2. The iteration-backstop on chain walks is the load-bearing safety mechanism

JVM had the Java-7 `HashMap` cycle bug — a racing put could form a `Node`
chain that loops back on itself, and the next `get()` would CPU-spin
forever. Java 8 mostly fixed it via treeify + tail-insertion, but the
guarantee is structural ("we won't build cycles") not reactive ("if a
cycle forms, we'll bail out"). .NET went further: every `Dictionary`
chain walk counts steps and throws `ConcurrentOperationsNotSupported`
if it exceeds the bucket array length. Go's `fatal("concurrent map
writes")` is the loudest version of the same idea, applied preemptively.

**Both .NET's and Go's mechanisms are debug-style assertions, not full
race detection.** They catch the cases that would otherwise hang the
runtime. They miss many other races (lost writes, ghost values, wrong
values) which they explicitly accept.

For BAML: **adopt the .NET approach** — collision counter on every chain
walk in the Map implementation, throw `ConcurrentOperationsNotSupported`
(a BAML-level user-catchable error, not a `fatal`). This is the single
most valuable change for preventing the worst runtime failure mode.

### 3. The "version counter" stuff (modCount, _version) is purely diagnostic

Both JVM and CLR docs explicitly state: "fail-fast iteration is best-effort,
do not rely on it for correctness." It's a debug aid that catches some
concurrent-modification-during-iteration bugs but is not a safety
mechanism — it can both miss real races and fire spuriously.

For BAML: **skip the version counter.** It's not on the safety-critical
path; if we want race detection we'd rather invest in a debug-build
TSan-equivalent than a permanent runtime field that adds cost and
diagnostic noise.

## Where BAML differs from all three references

| | BAML today | JVM | .NET | Go |
|---|---|---|---|---|
| GC model | **STW** (via `HeapPermitManager`) | concurrent | concurrent | concurrent |
| `Value` size | **16 bytes** (1-byte tag + 8-byte payload + padding) | 4 or 8 bytes (compressed oops) | 8 bytes (pointer) | 8 bytes (slot fits a pointer or boxed value) |
| Container backing | `Vec<Value>` (Rust Vec) — racy capacity grow under user races | `Object[]` (atomic ref + bounds-checked) | `T[]` (atomic ref + bounds-checked) | `[]T` slice (no atomics, torn header risk) |
| Heap shape | `ChunkedVec<Object>`, chunks never move once allocated | various; objects can move | various; objects can move | objects can move; per-P allocator |

Two things stand out:

**STW GC is our killer simplification.** We don't have to worry about
mutator-vs-GC races on container internals — the GC parks all mutators
before scanning. JVM/CLR/Go all engineer against this case. We don't have
to. The card-table write barrier we already maintain is for generational
promotion tracking, not for concurrent collection. As long as we stay
STW, GC-vs-mutator races are not a concern.

**The 16-byte `Value` is our killer problem.** This is the same shape as
Go's slice header / interface header — a tagged composite wider than a
machine word. Two threads writing different-variant `Value`s to the same
heap slot can produce a torn `(tag, payload)` pair. The next reader
matching on the tag and accessing the payload as that variant is the
canonical "race a Value::Object over a Value::Int and segfault on the
next deref" failure mode. Full walkthrough in `spawn_value_tearing.md`.

JVM and .NET avoid this because all user-visible values are
pointer-sized. We can't avoid it without either accepting Go's posture
or shrinking `Value` to one word.

## The applicable adoptions for BAML

In order of priority and effort:

### 1. Shrink `Value` to one machine word (NaN-boxing or tagged pointer) — **necessary** if we want "no SIGSEGV from user races"

- **What:** make `Value` fit in 8 bytes via NaN-boxing (V8 / SpiderMonkey / LuaJIT pattern) or tagged pointer (OCaml / Erlang / V8 Smi).
- **Why:** every aligned 8-byte load/store is hardware-atomic on x86-64, ARM64, ARMv7, RISC-V. Torn reads of `Value` become impossible. A reader always sees exactly one writer's full `Value`, never a Frankenstein.
- **Runtime cost in steady state:** zero. Same `mov` instruction. Tag/untag is a single masked-shift, which the optimizer typically folds into address computation.
- **Engineering cost:** multi-week refactor. ~200 sites that construct or `match` on `Value` need to change. `derive(PartialEq, Copy, Debug)` either needs to be replaced with manual impls or kept on a private representation.
- **Bonus:** smaller cache footprint may improve perf on benchmarks. The user noted Vaibhav's benchmarks brought VM perf close to Python's; this could close more of the gap.

### 2. `Relaxed`-atomic field stores on cross-thread-shareable `Value` slots — **recommended** after (1) is done

- **What:** mark the `Value` slots in `Object::{Array, Map, Instance, Cell}` as `AtomicU64`. Use `AtomicU64::store(bits, Relaxed)` / `AtomicU64::load(Relaxed)` for cross-thread reads/writes. Stack / frame / TLAB-private slots stay plain.
- **Why:** even though the hardware delivers atomicity for free on aligned 8-byte stores, the Rust language model requires explicit atomic ordering for cross-thread accesses to be defined behavior. Without it, future compiler optimizations are formally allowed to break us.
- **Runtime cost on x86-64 / ARM64:** zero — `Relaxed` lowers to the same `mov` as plain stores.
- **Engineering cost:** localized to four `Object` variants. Maybe ~50 sites.

### 3. Iteration backstop on hash chain walks — **necessary** in our Map implementation

- **What:** every `next`-pointer walk in `Object::Map` operations carries a counter; if it exceeds the bucket array length (which means the chain has formed a cycle), throw `baml.panics.ConcurrentMutation` (catchable from BAML; not a runtime `fatal`).
- **Why:** prevents the worst failure mode (CPU-spinning a BAML thread permanently) under racing map writes. .NET's collision-counter pattern. Cheap.
- **Runtime cost:** one increment + branch per probe step. Negligible vs the cost of the hash lookup itself.
- **Engineering cost:** small. ~3 sites in `Map.{insert, remove, get}`.

### 4. Container ops bounds-check against live `len`, not a cached snapshot — **necessary** (already partially true)

- **What:** `Vec::push` and `Vec` indexing in `Object::Array` always read `self.len()` at the access site, never cache the value across a potential-grow point.
- **Why:** prevents out-of-bounds writes under racing grow. JVM/CLR pattern.
- **Status:** already true for our `Vec<Value>` access through `as_array_mut`. The risk is introducing micro-optimizations later that cache `len`.
- **Action:** add a doc comment to `as_array_mut` enforcing the invariant. Possibly add a clippy lint or a test that exercises a racing grow.

### 5. Atomic reference-pointer swap on grow — **necessary** if we ever migrate off `Vec<Value>`

- **What:** when an array grows, the swap of `old_buffer → new_buffer` is an aligned-pointer atomic store. Readers that hold a stale `old_buffer` ref complete their access on the old buffer (which the GC keeps alive), then re-resolve on next access.
- **Why:** prevents use-after-free where one thread reallocates while another holds a pointer to the old buffer. JVM/CLR pattern.
- **Status:** Rust's `Vec` does not do this — its grow swap is internal and racing it is UB. If we want this property we'd need to replace `Vec<Value>` with a custom container whose backing pointer is an `AtomicPtr<[AtomicU64]>` and whose grow path atomically swaps it. Significant work, but unavoidable for the strong "no use-after-free" guarantee.
- **Pragmatic alternative:** keep using `Vec<Value>` but document that racing array grow is UB; rely on the iteration backstop and the user-visible behavior being bounded. We can defer the custom container until we have evidence the use-after-free is a real production hazard.

### 6. Document STW GC as a load-bearing invariant — **free, do it now**

- **What:** add a comment to `bex_heap/src/heap_guard.rs` and to the spawn-runtime docs stating that the safety story relies on STW GC.
- **Why:** the moment anyone proposes concurrent / incremental GC, the entire VM-safety-under-races analysis needs to be re-done. Most of the simplifications we get are because GC stops the world.
- **Cost:** zero.

## What we should NOT adopt

- **Version counter + `ConcurrentModificationException` on iteration.** JVM/CLR ship it, both docs say it's best-effort and not a safety mechanism. We don't need it; if we want diagnostic race detection later, ship a debug-build TSan equivalent.
- **Go-style `fatal("concurrent X")`.** Uncatchable runtime crash on detected concurrent write. Converts user bugs into process death. We prefer .NET's recoverable `ConcurrentOperationsNotSupported` throw, which the user can `catch` and react to.

## Mapping to PR sequence

The order I'd ship these:

1. **Doc-only:** add the "STW GC is load-bearing" comment to `heap_guard.rs`. One-line, zero risk.
2. **Map iteration backstop:** add collision counter to `Object::Map` chain walks; declare `baml.panics.ConcurrentMutation`; throw on cycle detection. Stand-alone correctness improvement, no Value-layout dependency.
3. **NaN-box `Value` (or tagged pointer)** — the big one. Measure perf vs Vaibhav's benchmark suite as we go. If perf is unchanged or better, ship; if it regresses meaningfully, pause and rethink.
4. **`AtomicU64::Relaxed` on shareable Value slots** — done after (3), cheap addendum. Validates the design under miri / loom.
5. **(Optional) Custom container with atomic-pointer-swap grow** — only if we observe a real production use-after-free. Otherwise leave `Vec<Value>` with documented "racing grow is UB."

Items (3) and (4) together are what the user wants to package as a canary
PR: closer-to-Python perf + reduced UB surface under spawn. Item (2) can
ship independently as a smaller correctness PR.

## Open questions for the BAML design

- **Tag encoding for NaN-boxing vs tagged pointer.** Both work. Tagged pointer is conceptually simpler (low bits encode the tag, aligned pointers have zero low bits). NaN-boxing fits more variants and is more battle-tested. Pick one based on what's easiest to refactor *into* given our current code shape.
- **What does `baml.panics.ConcurrentMutation` look like in the BAML stdlib?** A new panic class in `ns_panics/`, mirroring `Cancelled`. Should it have a `field: string` describing which container? Probably yes, for debuggability.
- **Should we add a debug-build race detector?** Even with the runtime hardened, users will still write racing code and get wrong answers. A TSan-equivalent (instrument `Value` writes with a check) would help debug. Defer.
- **Threading-model docs.** We should write a user-facing "Concurrency model and safety guarantees" doc explaining what the runtime promises under spawn races. Modeled on the .NET threading remarks. Defer until after the implementation lands.

## References

- JLS §17.7 (reference / long / double atomicity)
- ECMA-335 §I.12.6.6 (CLR atomicity)
- `/Users/antonio/Desktop/github/runtime/docs/design/specs/Memory-model.md` §18-25, §134-146 (CLR memory model)
- `/Users/antonio/Desktop/github/go/doc/go_mem.html` §"Implementation restrictions" (Go memory model on multi-word values)
- OpenJDK source: `/Users/antonio/Desktop/github/jdk/src/java.base/share/classes/java/util/{ArrayList,HashMap}.java`
- .NET source: `/Users/antonio/Desktop/github/runtime/src/libraries/System.Private.CoreLib/src/System/Collections/Generic/{List,Dictionary}.cs`
- Go source: `/Users/antonio/Desktop/github/go/src/{runtime/slice.go,runtime/map.go,internal/runtime/maps/runtime.go}`
- BAML context: `spawn_value_tearing.md` (the specific Value-tearing failure mode)
