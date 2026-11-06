# Runtime v2 integration and first typed backend

`Instance` owns a heap and task loop. Generated parameterless integer entries
run as root tasks with scoped TLS bindings. `alloc(desc, bytes)` and safepoints
obtain roots through the loop-owned `IntegrationRoots` API; `TaskCx` does not
implement `RootSource`. The loop retains responsibility for running-register
capture, parked stacks and precise roots. TLS bindings restore on return/error.
Only one root body is exposed at a time; child spawning needs task-switch-aware
bindings before extending this adapter. Heap capacity survives successive jobs.

The binary-trees emission mode supports classes with nullable class links,
i63 arithmetic and shifts, direct calls, loops and null tests. Descriptors use
pointer-first fields and stores call `write_ptr`. Errors return boxed `Result`s,
allocate the corresponding message-bearing BAML panic class in the v2 heap,
and append source-side-table frames on failure. Shared `Int63`, `VmPanic`, and
`format_traceback` retain the existing scalar and diagnostic conventions.
This is not yet the general builtin error conversion API or catch support.

Generated functions execute the observation-word branch on every call. This
phase defaults explicitly to telemetry off and rejects any other value of
`BAML_TELEMETRY`; a telemetry producer is not implemented. `BAML_GC_STRESS=1`
enables collection at heap stress points. Root fibers reserve 8 MiB. Generated
prologues check the loop-provided stack bound with 64 KiB reserved for error
construction/unwinding; bodies exceeding 1024 locals are rejected.

The experimental driver prints an integer for a plain invocation. With
`--jobs N --warmup N` it emits the speedtest native JSON protocol, checking every
return. It retains an instance across jobs and excludes startup from timing.
Select it with `baml pack --rust --runtime v2 ... main`. Other entry signatures,
containers, catches, virtual calls, generic classes, initializers and unsupported
MIR fail at pack time; there is no fallback to the VM.

Managed pointers must remain on the fiber stack or in traced heap objects.
Rust vectors and boxes are not implicit roots. The current uncaught-error path
performs no managed allocation after storing the error in its host box; catches
will need explicit exception rooting. Heap-valued host results, finalizers,
opaque ownership and OOM translation remain unimplemented. The heap currently
panics on unrecoverable allocation failure; this is an interface integration gap,
not claimed language-error parity. See `architecture/runtime-v2/PHASE1-REPORT.md`.

From the worktree root:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime --release
```
