# BEX VM

`bex_vm` is the synchronous stack-based bytecode interpreter for BAML. It knows how to execute bytecode and native VM builtins, but the async runtime and system-operation implementations live in `bex_engine` and `sys_ops`.

## Per-thread state

Each `BexVm` owns:

- an evaluation stack of `Value`s;
- a frame stack capped by `MAX_FRAMES` (256);
- a `Tlab` for allocation into the shared `Arc<BexHeap>`;
- globals, package metadata, error-class metadata, and tracing/cancellation context supplied by the engine.

`Value` stores inline primitives and `HeapPtr` object references. Functions, classes, enums, strings, arrays, maps, instances, variants, futures, and other compound values live in the heap.

Bytecode calls leave the callee and arguments on the evaluation stack and push a frame whose local indices refer into that stack. Returning pops the frame and replaces the call area with the result. Rust-native VM builtins are resolved through the generated `package_baml` dispatch and can complete immediately, continue through a VM continuation, or throw.

## Execution and engine yields

`BexVm::exec` runs synchronously until the program completes or reaches work that the embedding engine must perform. The current `VmExecState` variants include:

| State | Engine responsibility |
|---|---|
| `Complete(Value)` | Return the final VM value |
| `Await(FutureId)` | Wait for one future to settle, then resume |
| `AwaitAny(Vec<FutureId>)` | Park until at least one listed future settles, then re-execute the opcode |
| `Spawn(HeapPtr)` | Turn an `UnscheduledFuture` into a scheduled child thread and push its future |
| `SysOp { operation, args }` | Run a system operation and push its result; no heap future is allocated for the op itself |
| `Event { ... }` | Convert and emit a custom event |
| `EarlyYield` | Cooperatively return control so other work or a pending GC park request can proceed |

The old `ScheduleFuture` → `Await` sequence is no longer the system-operation path. `SysOp` is a single engine yield, while explicit `spawn` uses `Spawn` and later `Await`/`AwaitAny` as needed.

## Heap safety

A VM executes on one thread at a time and accesses the heap while its holder has an active heap permit. Its TLAB provides an exclusive allocation region. At async and forced safepoints the engine may release the permit so GC can park all holders. After collection, `RootHaver::forward_roots` rewrites pointers in the evaluation stack, frames, globals, and continuations and invalidates the TLAB.

Runtime mutations must use APIs that maintain the heap write barrier. Compile-time objects are immutable.

## Dependency direction

```text
bex_vm_types -> bex_external_types -> bex_heap -> bex_vm -> bex_engine
```

`bex_vm` does not implement network, filesystem, LLM, or other system I/O. It only describes that work in `VmExecState::SysOp` for the engine.
