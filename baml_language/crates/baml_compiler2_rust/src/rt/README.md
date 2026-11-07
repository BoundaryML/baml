# BAML runtime emitter

Closed-world MIR-to-Rust emission targeting `baml_rt`. Unsupported input is a
pack error; generated functions have no bytecode fallback.

`mod.rs` holds shared state and exports `compile_rt`. `driver.rs` orders
admission, descriptors, functions, source sites and output writes.
`layout.rs` and `repr.rs` define the shared storage ABI.

Feature modules implement `registry::Feature` and register in the ordered
`FEATURES` list. Admission checks run in registry order; the first error wins.
`prepare` registers additional MIR and roots before reachability. `reach`
provides call targets, builtin adapters or JSON specializations. Lowering hooks
handle values, statements, terminators and calls.

`None` means unhandled; `Some(Err(...))` is final. Container iteration precedes
interface dispatch; specialized calls precede ordinary calls. Admission and
lowering must both support a construct. Keep helpers private to `rt` or
`pub(super)` where siblings need them.

From `baml_language`, with `CARGO_TARGET_DIR` inside your worktree:

```sh
cargo test -p baml_compiler2_rust
cargo test -p baml_tests --test rust_backend
BAML_RUST_DIFF_PROBES='programs::richards 50,memory::binary trees 14,programs::agent loop 300' cargo test -p baml_tests --test rust_differential -- --ignored
```

`rt_source_bytes_and_diagnostics` pins generated source hashes and sampled
admission diagnostics. Inspect generated source changes before updating hashes.
Use packed workloads with `--jobs`/`--warmup` for runtime benchmarks.
