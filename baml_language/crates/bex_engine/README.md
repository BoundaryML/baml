# BEX engine

`bex_engine` is the async embedding layer that drives `BexVm`, dispatches system operations, coordinates futures and child threads, converts boundary values, emits runtime events, and coordinates garbage collection.

## Execution model

`BexEngine::call_function(&self, ...)` can run concurrently. Each root call gets a `BexThread`/`BexVm`, an evaluation stack, and a private TLAB while sharing the immutable program metadata, packages, globals, system-operation provider, future manager, and `Arc<BexHeap>`.

The engine repeatedly resumes the VM and handles its `VmExecState`:

| VM state | Engine action |
|---|---|
| `Complete(Value)` | Convert or return the result and finish the call |
| `SysOp { operation, args }` | Run the `sys_ops::SysOps` operation, race it with cancellation, push the result, and resume |
| `Spawn(HeapPtr)` | Create a pending future, start the closure on a child `BexThread`, and push the future |
| `Await(FutureId)` | Release heap access while waiting, then resume after settlement |
| `AwaitAny(Vec<FutureId>)` | Wait until one input settles, then resume so the VM can select it |
| `Event { ... }` | Convert the payload and emit a custom runtime event |
| `EarlyYield` | Cooperatively yield, honor GC parking if requested, and resume |

System operations use the dedicated single-yield `SysOp` path. The former `ScheduleFuture` state and its two-yield system-operation sequence have been removed. Heap futures remain the representation for explicit concurrency such as `spawn`.

## Garbage collection coordination

The engine uses `HeapPermitManager`:

1. Every VM thread and other heap-root holder registers a permit. Heap access occurs only while that permit is active.
2. `collect_garbage` requests parking and drains active permits. Running VMs release at normal async yields or forced early-yield checks; new holders cannot enter during collection.
3. `HeapGuard` calls `RootHaver::collect_roots` for registered holders and unions those roots with opaque handle roots.
4. `BexHeap::collect_garbage_generational` performs a minor or major collection and returns forwarding information.
5. The guard calls `RootHaver::forward_roots`; VM implementations rewrite stacks/frames/continuations and invalidate their TLABs.
6. Dropping the guard releases parked holders.

This replaces the older epoch-slot coordination model. The heap itself is generational (`Gen0`, `Gen1`, `Gen2`, plus permanent compile-time objects), not a two-space-only runtime heap.

## Boundary values

- `BexExternalValue` is fully owned and safe outside the heap.
- `Handle` is an opaque rooted reference to a heap object.
- Internal `Value`/`HeapPtr` objects are used only while heap access is proven by a permit.

Conversion code in `conversion.rs` moves between external values and VM values while preserving type metadata, object identity where handles are used, and GC safety.

## Ownership boundaries

- `bex_vm` interprets bytecode and yields descriptions of engine work.
- `sys_ops` implements filesystem, network, LLM, process, and related operations using contexts from `sys_types`.
- `bex_engine` is the mediator that knows about both layers.
- `bex_heap` owns allocation, handles, permits, and collection.

Focused concurrency, cancellation, future, early-yield, identity, and GC regression tests live under `crates/bex_engine/tests/`.
