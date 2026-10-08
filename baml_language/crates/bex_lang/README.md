# bex_lang

Language semantics that every BAML execution backend must agree on: the
interpreter in `bex_vm` and the ahead-of-time native code emitted by the Rust
backend. The native binary links no VM crate, so anything both sides need to
produce byte-identical behaviour lives here.

## What belongs here

- `int` arithmetic over `baml_type::Int63` that fails with the language's
  catchable panics (`int::add`, `int::div`, `int::shl`, ...). The panic
  messages are the single source of truth; the VM's cold error paths call the
  same formatting functions.
- `float`: the total order (`cmp`, `eq`, `lt`, ...; NaN is one value above
  every number, `-0.0 == 0.0`), `to_string` formatting (`format`), and the
  fallible conversions to `int` (`itrunc`, `ifloor`, ...) with the VM's
  `InvalidArgument` messages. `bex_vm_types::float_order` re-exports the order.
- `Str`, a newtype over `bex_str::BexStr` (the VM's string) with `Display`,
  `Debug`, `From<String>` and serde support, and the `string` helpers that
  pin down the language's semantics: code-point `length`, byte-order `cmp`,
  rope `concat`.
- `handle::Shared<T>`, a newtype over `Rc<RefCell<T>>` for reference-semantics
  values (derefs to the `RefCell`, serde through the pointee), and `array`:
  subscripts with negative indexes and `IndexOutOfBounds`, `push`, the
  primitive sorts behind `_rust_sort`, and the for-in cursor `Iter<T>` that
  re-reads the array's length each step.
- `render::ToBaml`: the structural `to_string` walk (`_to_string_default`),
  with `render::class` for generated classes.
- `json`: `to_string` / `deserialize<T>` over serde's traits through
  `serde_json` with the workspace's `preserve_order` + `arbitrary_precision`
  features, `serialize_f64` for `float` fields (NaN → `null`), and the
  `ParseError` / `DecodeError` classes. `Int63`'s serde impls live in
  `baml_type` behind its `serde` feature, which this crate enables.
- `Panic`: the backend-neutral payload for a `baml.panics.*` class, with its
  fully qualified class name, the readable rendering the engine prints for an
  uncaught panic, and the process exit code it maps to.
- `Thrown` / `ErrorObject`: everything a `throw` can unwind with (a panic or
  a class instance), plus the stdlib error classes this crate raises itself
  (`errors::InvalidArgument`, `json::{ParseError, DecodeError}`).
- `clamp_exit_code`: narrowing a `baml.sys.exit(code)` `int` to a host `i32`.

Everything must stay `wasm32`-safe (no `std::time`, no threads, no I/O).

## What does not belong here

- No VM: no object layout, no `Value`, no heap pointers. Materializing a
  panic or error as a BAML heap object is the backend's job.
- No scheduler, futures, or cancellation.
- No printing or logging. `print_stdout` / `print_stderr` are denied lints.
- No dependency beyond `baml_type` (with `serde`), `bex_str`, `serde` and
  `serde_json`; `serde` and `serde_json` are re-exported so generated crates
  need no dependency of their own.
