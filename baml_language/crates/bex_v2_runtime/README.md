# Runtime v2 integration seam (incomplete)

This crate connects the independently verified heap and task loop. It is **not a
BAML execution backend**. No compiler emits calls to it yet, and it does not
implement language values, builtins, errors, telemetry, or a pack protocol.

`Instance` owns a heap and loop. `run` executes an integer-returning Rust entry
as a root task with scoped TLS bindings; the heap is retained between calls.
`alloc` uses the heap's allocator and a synchronous adapter from the loop's
`RootSource` to the heap's trait. `safepoint` polls the task and invokes the heap
budget/stress check. `BAML_GC_STRESS=1` enables heap stress; tests can also call
`set_stress`. Stack reservations are explicitly 8 MiB for this integration
probe, pending validation of generated frame sizes.

All managed-pointer APIs are unsafe. Entry code must keep roots on its fiber
stack or in descriptor-traced objects, initialize objects before another
collecting operation, and use `write_ptr` for pointer stores. Host Rust vectors
and boxed captures are not implicitly roots. A result may only be an owned int;
no managed pointer may escape. Child-task spawning, finalizer dispatch, host
handles and heap-valued result hydration are not exposed. The loop's placeholder
error enum is preserved only for testing this seam; it is not the required
BAML exception representation. The scheduler's `LoopHeap` is `()` because all
heap collections in this restricted adapter occur explicitly via `alloc` or
`safepoint`, with the current `TaskCx` as the actual root source.

The new `RootSource for TaskCx` delegates to the same Core implementation as
`TaskLoop`, including running-register capture and parked-stack visits. It
closes the allocation API gap without collecting on every fast-path allocation
or changing either root trait. TLS is restored by a drop guard on success,
error, and unwind. Only one root body is exposed at a time; supporting child
fibers requires task-switch-aware bindings before extending this API.

From the worktree root:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime --release
CARGO_TARGET_DIR="$PWD/target" cargo clippy --manifest-path baml_language/Cargo.toml -p bex_v2_runtime --all-targets --no-deps -- -D warnings
```

The recursive-tree stress test retains the first child across construction of
the second, retains a completed tree across further allocation, checks every
node count, and runs three jobs through a reused instance. It requires at least
1,146 actual collections. This is a Rust integration probe, **not** the BAML
binary-trees workload or bytecode differential evidence.
