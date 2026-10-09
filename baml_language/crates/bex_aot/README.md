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
| `T[]`, class `C` | `Shared<Vec<T>>`, `Shared<C>` | `handle`: `Rc<RefCell<T>>` with reference semantics; `array`: checked `get`/`set`, `push`, primitive sorts, the for-in cursor `Iter<T>` |
| `map<K, V>` | `Map<K, V>` | `map`: a handle over an insertion-ordered `IndexMap`, keyed by `string`, `int` or `bool`; `index` raises `MapKeyNotFound`; JSON needs string keys, as on the VM |
| `T \| null` | `Option<T>` | |
| `(A) -> R throws E` | `Rc<dyn Fn(A) -> Result<R, Thrown>>` | a counted pointer to a closure, as `Shared` is to an object; the `throws` clause has no form of its own |
| a captured local | `cell::Cell<T>` | `cell`: the VM's `Object::Cell`, a counted pointer to the value a closure and its creator share; `fresh` / `carry` for a binding's new cell, `get` / `set` through it |
| `map`, `filter`, `sort_by`, .. | `array::map` etc. | `array`: the methods that call back into a function value, each over a snapshot of the array with no borrow held while the callback runs; `sort_by` is the VM's bottom-up merge sort, written back only on success |
| `A \| B` | a generated enum | one variant per member; `json::deserialize_union` decodes the first member the text fits, in the union's member order, as the VM's typed decode does |
| `==` | `BamlEq` | `eq`: the VM's broad `==` for primitives, `null`, nullable values and (through generated impls) unions: different kinds are never equal, `float` by the reflexive order |
| `to_string()` | `ToBaml` | `render`: the structural walk, `render::class` for generated structs |
| `json.*` | serde through `serde_json` | `json`: `to_string`, `deserialize<T>`, the `ParseError` / `DecodeError` classes |
| `throw`, panics | `Result<T, Thrown>` | `thrown`: a `Panic` or a boxed `ErrorObject` (class name plus rendered fields); `Clone`, so a caught error can be tested, bound and thrown on; `is_class` / `class_fqn` / `is_panic` are what a `catch` arm asks, `downcast` recovers the handle a generated class (`ErrorClass`) was thrown as; `errors::InvalidArgument` and `errors::ParseError` |
| fields of an uncaught throw | `Readable` | `readable`: the engine's `render_readable` for every value above (strings quoted, floats with `.0`, classes by fully qualified name) |
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

`serde` and `serde_json` are re-exported so a generated crate depends on
this crate alone.
