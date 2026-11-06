# Corosensei test support

`corosensei/` is crates.io corosensei **0.3.4**, MIT OR Apache-2.0,
with the `saved_sp()` accessor used by `perf/v2-gc`. Compared against the
published 0.3.4 crate: the only source difference is that accessor in
`src/coroutine.rs`. It exposes the existing `stack_ptr` as `Option<usize>`.
No Rust struct-layout guessing or scans through guard pages are used.

Packaging changes: renamed to `bex-v2-corosensei` to coexist with the workspace's
unmodified corosensei; omitted examples, benchmark targets, development-only
dependencies and upstream profile settings; disabled the vendored package's own
unit/doctest targets (its integration is tested by `tests/fibers.rs`). The build script has a narrow
Clippy allowance for reading Cargo's target environment variables. Runtime
source and licenses are retained. This is only a dev-dependency of the heap.

The current `bex_vm` fiber does not expose its saved SP or scan its native stack
for roots; it retains the interpreter's rooting protocol. The future v2 task
loop needs this accessor upstreamed or a similarly reviewed fork. Stack base
comes from `Stack::base()` before moving the stack into `Coroutine`; `limit()`
includes guard pages and is not a scan bound. Completed coroutines are removed
from the root ranges. Reading a parked context requires keeping it parked.

Use `cargo clippy -p bex_v2_heap --all-targets --no-deps -- -D warnings` to lint
the heap and its tests without imposing new Clippy style/MSRV lints on the
vendored upstream implementation.
