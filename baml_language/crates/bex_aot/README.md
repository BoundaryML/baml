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
| `string` | `Str` over `bex_str::BexStr` | `string`: literals, `concat`, byte-order `cmp`, code-point `length` |
| `T[]`, class `C` | `Shared<Vec<T>>`, `Shared<C>` | `handle`: `Rc<RefCell<T>>` with reference semantics; `array`: checked `get`/`set`, `push`, primitive sorts, the for-in cursor `Iter<T>` |
| `map<K, V>` | `Map<K, V>` | `map`: a handle over an insertion-ordered `IndexMap`, keyed by `string`, `int` or `bool`; `index` raises `MapKeyNotFound`; JSON needs string keys, as on the VM |
| `T \| null` | `Option<T>` | |
| `to_string()` | `ToBaml` | `render`: the structural walk, `render::class` for generated structs |
| `json.*` | serde through `serde_json` | `json`: `to_string`, `deserialize<T>`, the `ParseError` / `DecodeError` classes |
| `throw`, panics | `Result<T, Thrown>` | `thrown`: a `Panic` or a boxed `ErrorObject` (class name plus rendered fields), `errors::InvalidArgument` |
| recursion | `depth::Guard` | counts frames on call cycles and throws `StackOverflow` at the VM's limit |

Handles are reference counted, not collected: a cycle of handles is never
freed. The program runs on one OS thread, so a handle takes no lock; borrows
are short because lowering evaluates operands into temporaries before a
store or a call. Rust panics (a `RefCell` conflict, an index past a `Vec`'s
length) are bugs in the backend or this crate, never BAML semantics, which
is why generated projects build with `panic = "abort"`.

`serde` and `serde_json` are re-exported so a generated crate depends on
this crate alone.
