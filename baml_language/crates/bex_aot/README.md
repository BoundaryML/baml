# bex_aot

The runtime a BAML program compiled ahead of time to Rust links. The Rust
backend (`baml_compiler2_rust`) emits ordinary Rust over these types; the
language rules come from `bex_lang`, and this crate adds only
what native code needs and the VM never does. The binary links no VM and no
garbage collector.

| BAML | Rust | Here |
|---|---|---|
| `int` | `Int63` (from `baml_type`) | re-exported; arithmetic is `bex_lang::int` |
| `float` | `f64` | `bex_lang::float` re-exported |
| `bigint` | `BigInt` over `num_bigint::BigInt` | `bigint`: a counted pointer; the VM's checks and messages (`AllocFailure` past `MAX_BIGINT_BITS`, `DivisionByZero`, `NegativeBitShift`), `to_int`, `parse`, `pow`, `isqrt`, `ilog`, JSON as a number with every digit |
| `string` | `Str` over `bex_str::BexStr` | `string`: literals, `concat`, byte-order `cmp`, code-point `length` |
| `T[]`, class `C` | `Shared<Vec<T>>`, `Shared<C>` | `handle`: `Rc<RefCell<T>>` with reference semantics, reached only through `shared` / `borrow` / `borrow_mut` / `ptr_eq` / `id`; `array`: checked `get`/`set`, `push`, primitive sorts, the for-in cursor `Iter<T>` |
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

Handles are reference counted, not collected: a cycle of handles is never
freed, whether it runs through class fields or through a closure (a closure
stored in a cell it captures, or in a field of an object it captures, keeps
itself alive). That is the VM's behaviour without its collector, accepted
for now; cycle collection is a roadmap item. The program runs on one OS
thread, so a handle takes no lock; borrows are short because lowering
evaluates operands into temporaries before a store or a call, and a
callback runs over a snapshot with no borrow held. Rust panics (a `RefCell` conflict, an index past a `Vec`'s
length) are bugs in the backend or this crate, never BAML semantics, which
is why generated projects build with `panic = "abort"`.

Memory management is behind a seam: generated code allocates and reads
heap objects only through `handle::shared`, `Shared::borrow` /
`borrow_mut`, `array::new`, `map::new`, `cell::fresh` / `with` / `carry`,
`func::alloc` / `item` and `Func::call` (the table at the top of
`src/handle.rs`), and every heap object's contents implement `mem::Trace`,
which visits the handles a value owns without following them. That is
what a cycle collector or a tracing collector needs to see; today nothing
calls one (`mem::collect` is a no-op). With the `mem-stats` feature,
`mem::stats()` counts allocations and frees per thread,
`mem::unreachable()` counts live objects no outside handle reaches (trial
deletion over the registry, which also checks that `Trace` misses no
edge), and `BAML_MEM_TORTURE=1` runs `mem::collect` at every
allocation. Without the feature a handle is exactly `Rc<RefCell<T>>`.

`serde` and `serde_json` are re-exported so a generated crate depends on
this crate alone.

## Miri

The unit tests run under Miri, with and without `mem-stats`:

    cargo +nightly miri test -p bex_aot --features mem-stats --target x86_64-unknown-linux-gnu

Any nightly with the `miri` and `rust-src` components works (checked with
nightly-2026-05-27 and nightly-2026-10-10). On an aarch64 host pass the
x86_64 target: `bex_str` counts code points with a NEON intrinsic
(`llvm.aarch64.neon.uaddlv.i32.v16i8`) that Miri does not implement, while
its x86_64 path interprets. Two bigint tests at the size cap take minutes
under Miri and two `mem` tests leak a cycle on purpose (Miri's leak check
would fail them); all four are `#[cfg_attr(miri, ignore)]`.
