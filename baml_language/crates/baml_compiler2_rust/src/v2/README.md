# Runtime v2 emitter

Closed-world MIR-to-Rust emission; unsupported input remains a pack error.
`mod.rs` holds shared state and the public `compile_v2` re-export. `driver.rs`
orders the emission stages. Ordering of admission, descriptors, functions, source
sites and output writes is intentional: the phase-one source bytes are preserved.

## Module map / ownership

| Owner | Modules and responsibilities |
| --- | --- |
| Initializers and globals | `globals.rs`: project admission; future initialization and global lowering |
| Lists and maps | `containers.rs`: literals, lengths, iteration, container calls |
| Closures and generics | `closures.rs`: receiver-storage inference, closure/capture/generic-class admission; future specialization |
| Interfaces, switches, aliases | `dispatch.rs`: implementors, virtual reachability and calls; future switch/alias lowering |
| Tasks and cleanup | `tasks.rs`: shielding/cleanup admission; future spawn/await/cancellation/defer lowering on `bex_v2_loop` |
| Shared planning | `admission.rs`: ordered reachability walk; `layout.rs`: layout planning and descriptors; `repr.rs`: shared storage ABI |
| Shared lowering | `body.rs`: function/structured-body core; `statements.rs`: block, statement and basic terminator core; `expressions.rs`, `places.rs`: scalar/class expressions, operands, coercions and stores; `calls.rs`: direct-call routing and reachability |
| Errors | `errors.rs`: line/PC tables, raises and fallible expressions; `diagnostics.rs`: unsupported prefix and diagnostic markers |
| Telemetry / reflection | `telemetry.rs`: observation, metadata and hook admission; `capture_codegen.rs`: capture descriptors |
| Adapters | `builtins.rs`: generated builtin adapters; `strings.rs`: string expressions; `json.rs`: typed JSON decoding; `host.rs`: executable entry |

## Registration contract

**A feature job edits its own module plus registration lines.**

Implement `registry::Feature` in the owning module. The single ordered
`FEATURES` list in `registry.rs` registers each area once. Hooks are:

- `admit`: checks at the original project/function/local/block/statement/class
  boundaries. All checks run in registry order; the first error wins.
- `prepare`: register initializer/lambda/specialized MIR and extra roots before
  the reachability walk.
- `reach`: claim a terminator and return direct targets, a builtin adapter, or
  a JSON specialization. The walker owns queuing and deterministic sets.
- `value`, `statement`, `terminator`: claim MIR lowering in the owning module.
- `call`: claim a direct call after argument layout and operand lowering.

`None` means unhandled; `Some(Err(...))` is final, never fallback. Container
iteration precedes interface dispatch; specialized calls precede ordinary calls.
Admission and lowering must both support a new construct. Keep default rejection
text and ordering intact. These seams add no new supported features.

For generated helper definitions, add a one-line stage call in `driver.rs`
(next to JSON/builtin emission), keeping its implementation in the feature
module. Later I/O/host entry, tool adapters, trace hooks and runtime-compilation
islands follow the same pattern. Shared representation/layout ABI changes need
coordination; do not duplicate that state in feature modules. Helpers are private
to `v2`, with sibling-facing methods restricted to `pub(super)`.

The `baml_tests` test `v2_phase1_source_bytes_and_diagnostics` pins pre-split
SHA-256 source bytes for binary-trees, richards and agent-loop, plus exact sampled
unsupported diagnostics. For an intentional future emitter change, inspect the
source diff before updating the hashes. Run the three-workload differential
subset documented in `baml_tests/rust_diff/README.md` as the execution gate.
