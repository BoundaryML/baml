# BEX heap

`bex_heap` provides the shared object heap, per-VM allocation buffers, handles, and garbage collector used by the BEX runtime.

## Layout and allocation

The heap is shared as `Arc<BexHeap>`. Objects live in fixed-address `ChunkedVec<Object>` storage divided into these generations:

| Generation | Purpose |
|---|---|
| `CompileTime` | Permanent functions, classes, enums, types, and other program objects |
| `Gen0` | Nursery for every new TLAB allocation |
| `Gen1` | Intermediate generation for Gen0 survivors |
| `Gen2` | Old generation for long-lived objects |
| inactive | Copy destination used during collection |

Each VM owns a `Tlab`. The fast allocation path bumps its private cursor and writes directly into its reserved Gen0 region. Refilling a TLAB atomically reserves another chunk; the default reservation is 1,024 object slots. The backing `ChunkedVec` uses 4,096-element chunks so growth does not move object storage held by other VMs.

Heap references use `HeapPtr`/`ObjectIndex`, which encode enough information to locate the correct compile-time or runtime generation. Runtime writes must use the heap's mutation APIs so the generational write barrier can mark old-to-young references in the card table.

## Garbage collection

The collector is safepoint-based, generational, copying, and compacting:

- `CollectionLevel::Minor` traces Gen0 and Gen1. Gen0 survivors move to a new Gen1 and Gen1 survivors move to Gen2.
- `CollectionLevel::Major` traces Gen0, Gen1, and Gen2 and compacts all survivors into Gen2.
- Compile-time objects are permanent and identity-map during tracing. They must not contain runtime `HeapPtr` fields.
- The forwarding map updates VM roots, other permit-holder roots, and opaque FFI handles. Each forwarded VM also invalidates its TLAB so its next allocation refills from the reset Gen0 cursor.

Collection itself is coordinated by `HeapPermitManager`, not by epochs. VM threads and other root holders register permits. An active permit proves heap access is safe; GC parks active holders, gathers roots through `RootHaver`, runs collection under `HeapGuard`, forwards roots, and resumes holders when the guard drops.

## Handles

`Handle` is the opaque external boundary for heap-owned values. The heap's `RwLock<HashMap<usize, HeapPtr>>` keeps handled objects rooted and lets GC rewrite their pointers atomically. `BexValue` is a borrowed heap view used while a permit is active; `BexExternalValue` is owned and independent of the heap.

## Safety invariants

1. A TLAB region has one writer.
2. Heap access that can race collection requires an active permit and `PermitProof`.
3. Old-to-young writes go through the write barrier.
4. GC runs only after all active permit holders are parked.
5. `ChunkedVec` growth never moves existing object slots.

The focused collector tests live in `crates/bex_heap/tests/generational.rs`; TLAB and pointer-stability tests also live alongside their implementations.
