# BEX Heap

Shared memory management for the BEX runtime: a non-moving mark-sweep
collector with size-class allocation, behind the engine's heap handles,
permits and root registry.

## Layout

```
compile_time: Vec<Object>         permanent: functions, classes, enums, literals

blocks (32 KiB, block-aligned, carved from 1 MiB slabs), one size class each
┌──────────────┬──────────┬──────────┬──────────┬─────┬──────────────┐
│ header       │ cell     │ free ──┐ │ cell     │ ... │ unused tail  │
│  starts      │          │ (next) │ │          │     │              │
│  marks       │          │        ▼ │          │     │              │
│  class, free │          │          │          │     │              │
└──────────────┴──────────┴──────────┴──────────┴─────┴──────────────┘

large objects (cells > 8 KiB): one block-aligned allocation each, prefix + cell
```

Every allocation is a *cell*: an 8-byte `Header` naming a `Descriptor`,
then the object. A pointer to an object names a byte in its cell's first
granule, so masking the low four bits finds the cell and masking to the block
size finds the side tables: an object-start bitmap and a mark bitmap, one bit
per 16-byte granule.

The VM's 64-byte `Object` enum is one descriptor (`OBJECT_DESC`), an 80-byte
cell whose tracing calls the enum-specific code in `object_trace.rs`. Other
descriptors describe objects by pointer-slot and tagged-slot offsets and
repeated elements, the shared object format the next PRs move the VM onto.

## Allocation

`space/small.rs` is the small-object policy, behind one seam: Go-style size
classes (`SIZE_CLASSES`, 16 bytes to 8 KiB). A block holds one class; its
empty slots form a free list threaded through them. Each mutator owns a
`Tlab` holding one current block per class and takes cells from it without
locking; only taking or giving back a block locks. Cells above 8 KiB go to
the large-object space. Every cell spends its size of the GC budget.

## Collection

Stop-the-world under the `HeapGuard` pause, never moving anything:

1. Roots: the handle table, every holder in the root registry
   (`collect_paused`), and any `RootSource`: precise pointers, or word ranges
   scanned conservatively (an interior pointer keeps its whole object; a word
   outside every cell is ignored).
2. Mark: set the side-table mark bit of every reachable cell.
3. Keep alive: unobserved spawn errors and dead finalizable instances, with
   everything they reach.
4. Weak entries (function lookup, finalizable list) of dead objects are
   dropped; the engine then forwards the registry, which drops weak caches.
5. Sweep: drop every unmarked cell and clear its start bit; free dead large
   objects; rebuild each block's free list; return empty blocks to the
   shared pool, where any class can take them.

The budget for the next collection is `max(32 MiB, 4 × live bytes)`, live
bytes being the survivors' cells plus what they keep alive outside them.
`HeapStats::heap_bytes` over `HeapStats::live_bytes` is the fragmentation
ratio; `BexHeap::class_occupancy` breaks blocks and cells down by class.

## Handles

External code holds opaque handles; the handle table is always a root, and
since objects never move a handle's pointer never changes. Under
`heap_debug`, each collection re-stamps surviving pointers with a new epoch
(the `Restamp` map), so a pointer the collection never saw still fails the
epoch check.

## Concurrency

| Component | Mechanism |
|-----------|-----------|
| Cells and free list of an owned block | Written only by the owning mutator, or by the collector while all mutators are parked |
| Block pool and class lists, large objects | `parking_lot::Mutex`, taken per block or large allocation |
| Handles | `RwLock<HashMap>` |
| Collection | `HeapPermitManager` pause: every permit drained, registry locked |

Global variables are written only by `$init`, then frozen into
`SharedGlobals`, which the collector traces as a root holder. Their mutation
goes through the same synchronized containers (`LazyBiasedMutex` arrays and
maps, atomic instance field slots) as values shared with a `spawn`ed task.
