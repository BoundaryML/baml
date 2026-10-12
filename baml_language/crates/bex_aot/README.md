# bex_aot

The runtime a BAML program compiled ahead of time to Rust links. The Rust
backend (`baml_compiler2_rust`) emits ordinary Rust over these types; the
language rules come from `bex_lang`, and this crate adds only
what native code needs and the VM never does. The binary links no VM; its
only collector is the cycle collector for tracked handles (`cc`).

| BAML | Rust | Here |
|---|---|---|
| `int` | `Int63` (from `baml_type`) | re-exported; arithmetic is `bex_lang::int` |
| `float` | `f64` | `bex_lang::float` re-exported |
| `bigint` | `BigInt` over `num_bigint::BigInt` | `bigint`: a counted pointer; the VM's checks and messages (`AllocFailure` past `MAX_BIGINT_BITS`, `DivisionByZero`, `NegativeBitShift`), `to_int`, `parse`, `pow`, `isqrt`, `ilog`, JSON as a number with every digit |
| `string` | `Str` over `bex_str::BexStr` | `string`: literals, `concat`, byte-order `cmp`, code-point `length` |
| `T[]`, class `C` | `Shared<Vec<T>>`, `Shared<C>` (or `Cc<..>`) | `handle`: `Rc<RefCell<T>>` (or, for a type that can be on a cycle, the cycle collector's `Cc` box) with reference semantics, reached only through `shared` / `cc` / `borrow` / `borrow_mut` / `ptr_eq` / `id`; `array`: checked `get`/`set`, `push`, primitive sorts, the for-in cursor `Iter<T>` |
| `map<K, V>` | `Map<K, V>` | `map`: a handle over an insertion-ordered `IndexMap`, keyed by `string`, `int` or `bool`; `index` raises `MapKeyNotFound`; JSON needs string keys, as on the VM |
| `T \| null` | `Option<T>` | |
| `(A) -> R throws E` | `func::Func<(A,), R>` | `func`: a counted handle to a `Callable` object, the closure's generated environment (`<lambda>__env`, its captured cells as fields) or a declared function (`func::item`); called with `call((a,))`; the `throws` clause has no form of its own |
| a captured local | `cell::Cell<T>` | `cell`: the VM's `Object::Cell`, a `Shared` handle to the value a closure and its creator share; `fresh` / `carry` for a binding's new cell, `get` / `set` through it |
| `map`, `filter`, `sort_by`, .. | `array::map` etc. | `array`: the methods that call back into a function value, each over a snapshot of the array with no borrow held while the callback runs; `sort_by` is the VM's bottom-up merge sort, written back only on success |
| `A \| B` | a generated enum | one variant per member; `json::deserialize_union` decodes the first member the text fits, in the union's member order, as the VM's typed decode does |
| `==` | `BamlEq` | `eq`: the VM's broad `==` for primitives, `null`, nullable values and (through generated impls) unions: different kinds are never equal, `float` by the reflexive order |
| `to_string()` | `ToBaml` | `render`: the structural walk, `render::class` for generated structs |
| `json.*` | serde through `serde_json` | `json`: `to_string`, `deserialize<T>`, the `ParseError` / `DecodeError` classes |
| `throw`, panics | `Result<T, Thrown>` | `thrown`: a `Panic` or a boxed `ErrorObject` (class name plus rendered fields); `Clone`, so a caught error can be tested, bound and thrown on; `is_class` / `class_fqn` / `is_panic` are what a `catch` arm asks, `downcast` recovers the handle a generated class (`ErrorClass`) was thrown as; `errors::InvalidArgument` and `errors::ParseError` |
| fields of an uncaught throw | `Readable` | `readable`: the engine's `render_readable` for every value above (strings quoted, floats with `.0`, classes by fully qualified name) |
| heap edges | `mem::Trace` | `mem`: every value a handle can hold reports the handles it owns (generated for every class, union, interface and closure environment); `mem-stats` feature: allocation counters, a weak registry of live objects, `BAML_MEM_TORTURE=1` |
| recursion | `depth::Guard` | counts frames on call cycles and throws `StackOverflow` at the VM's limit |

The program runs on one OS thread, so a handle takes no lock; borrows are
short because lowering evaluates operands into temporaries before a store or
a call, and a callback runs over a snapshot with no borrow held. Rust panics
(a `RefCell` conflict, an index past a `Vec`'s length) are bugs in the
backend or this crate, never BAML semantics, which is why generated
projects build with `panic = "abort"`.

`serde` and `serde_json` are re-exported so a generated crate depends on
this crate alone.

## Memory

Every heap object (array, map, class instance, function value, captured
local's cell) is reference counted, and the types that can form a cycle
also get a cycle collector, as in Nim's ORC.

- **Handle kinds.** Every handle type takes a kind, `handle::Kind`.
  `Untracked` (the default) is a plain `Rc`. `Tracked` (`Cc<T>`, `CcMap`,
  `CcFunc`, `CcCell`) is a counted box in `cc` with a 16-byte header (the
  count, a color, a buffered bit, and a per-type ops table). The backend
  tracks exactly the types whose type graph has a cycle through them
  (`baml_compiler2_rust`'s `memory.rs`); in the test corpus that is about 1%
  of heap types, and a program with none runs exactly as under `Rc`.
- **Counting.** A decrement to zero frees the object. Frees of tracked
  objects are iterative (a queue, not recursion), so dropping a
  million-node list does not overflow the stack. A decrement of a tracked
  object to a nonzero count buffers it as a possible root of a garbage
  cycle.
- **Collecting.** `cc::collect` (also `mem::collect`) runs synchronous
  trial deletion (Bacon and Rajan) over the buffered roots: subtract the
  counts the objects reachable from them give each other, restore whatever
  is still referenced from outside, free the rest. It runs itself when the
  buffer reaches the threshold (`BAML_CC_THRESHOLD`, default
  `cc::DEFAULT_THRESHOLD` = 10,000; it doubles while collections mostly
  retrace live data, `BAML_CC_ADAPTIVE=0` fixes it). Roots freed by their
  counts while buffered are swept out between collections. A host may also
  call `mem::collect()` at an idle point; generated code never does.
- **The seam.** Generated code allocates and reads heap objects only
  through `handle::shared` / `cc`, `Shared::borrow` / `borrow_mut`,
  `array::new` / `new_cc`, `map::new` / `new_cc`, `cell::fresh` / `with` /
  `carry`, `func::alloc` / `alloc_acyclic` / `item` and `Func::call` (the
  table at the top of `src/handle.rs`). Every heap object's contents
  implement `mem::Trace`, which visits the handles a value owns without
  following them; that is all the collector reads.
- **Invariants.** `Trace` reports each owned handle exactly once: a missing
  edge leaks a cycle, an extra one would free a live object. The collector
  reads values under a shared borrow and treats a mutably borrowed object
  as having no edges (which can only keep objects alive). Collection never
  nests inside a free or another collection. `cc.rs` holds the crate's
  `unsafe` code (plus one block in the `mem-stats` registry); its module
  docs list what the blocks rely on.
- **Observability.** `cc::stats()` and `BAML_CC_STATS=1` (printed to stderr
  when the thread exits) give collections, roots, objects traced, freed and
  swept, and the pause total, maximum and p99 (pauses read as 0 on
  wasm32-unknown-unknown, which has no clock). With the `mem-stats` feature
  (tests only), `mem::stats()` counts allocations and frees per thread,
  `mem::unreachable()` counts live objects no outside handle reaches (trial
  deletion over a registry of every object, which also checks that `Trace`
  misses no edge), and `BAML_MEM_TORTURE=1` runs `mem::collect` at every
  allocation and every buffered decrement. Without the feature an untracked
  handle is exactly `Rc<RefCell<T>>`.

## Miri

The unit tests run under Miri, with and without `mem-stats`:

    cargo +nightly miri test -p bex_aot --features mem-stats --target x86_64-unknown-linux-gnu

Any nightly with the `miri` and `rust-src` components works (checked with
nightly-2026-05-27 and nightly-2026-10-10). On an aarch64 host pass the
x86_64 target: `bex_str` counts code points with a NEON intrinsic
(`llvm.aarch64.neon.uaddlv.i32.v16i8`) that Miri does not implement, while
its x86_64 path interprets. Two bigint tests at the size cap take minutes
under Miri and two `mem` tests leak an untracked cycle on purpose (Miri's
leak check would fail them); all four are `#[cfg_attr(miri, ignore)]`. The
collector's tests (`cc::tests`) run under Miri at reduced sizes; `cc.rs`
holds the crate's `unsafe` code.
