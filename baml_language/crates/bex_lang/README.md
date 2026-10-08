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
- `Panic`: the backend-neutral payload for a `baml.panics.*` class, with its
  fully qualified class name, the readable rendering the engine prints for an
  uncaught panic, and the process exit code it maps to.
- `clamp_exit_code`: narrowing a `baml.sys.exit(code)` `int` to a host `i32`.

Everything must stay `wasm32`-safe (no `std::time`, no threads, no I/O).

## What does not belong here

- No heap, values, or object layout: this crate only knows `Int63` and plain
  Rust strings. Materializing a panic as a BAML object is the backend's job.
- No scheduler, futures, or cancellation.
- No printing or logging. `print_stdout` / `print_stderr` are denied lints.
- No dependency other than `baml_type`.
