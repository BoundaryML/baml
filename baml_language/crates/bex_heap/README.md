# BEX Heap

Shared memory management for the BEX runtime: a copying collector with
pinning over 32 KiB blocks, bump allocation for the mutator, behind the
engine's heap handles, permits and root registry.

## Layout

```
compile_time: Vec<Object>         permanent: functions, classes, enums, literals

blocks (32 KiB, block-aligned, carved from 1 MiB slabs), bump-filled once
┌──────────────┬──────┬────────────────┬──────────┬─────┬──────────────┐
│ header       │ cell │ cell           │ cell     │ ... │ free tail    │
│  starts      │      │                │          │     │  ▲ cursor    │
│  marks, pins │      │                │          │     │              │
│  hooked      │      │                │          │     │              │
└──────────────┴──────┴────────────────┴──────────┴─────┴──────────────┘

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

`space/blocks.rs`: a block is handed out empty, once, and filled front to
back with cells of mixed sizes (`Bump`). Each mutator owns a `Tlab` holding
one current block and bumps into it without locking; only taking a block
locks. No size classes and no free lists: a freed cell is never reused, only
a whole emptied block is, and the pool hands out the lowest free address
first. The collector's to-space is the same `Bump` over blocks from the same
pool. Cells above 8 KiB go to the large-object space. Every cell spends its
(granule-aligned) size of the GC budget.

A `Tlab`'s block is from-space at a collection like any other, so a `Tlab`
keys its block by the heap's full-collection count: the allocation fast path
compares that epoch (one relaxed load) and the slow path takes a fresh block
when it is stale. The block it lost was freed, or retained by a pin, and may
already be another mutator's; the stale `Tlab` never writes into it.

## Collection

Stop-the-world under the `HeapGuard` pause, copying:

1. Every block holding a cell is from-space. Roots: the handle table, every
   holder in the root registry (`collect_paused`), and any `RootSource`.
   Conservative word ranges go first and *pin* the cell each word lands in
   (an interior pointer keeps its whole object; a word outside every cell is
   ignored).
2. Mark and copy: every unpinned live small cell is copied into to-space
   (bump-filled blocks from the pool) and each pointer to it is repointed as
   it is visited; a pinned, large or to-space cell is marked in place.
3. Keep alive: unobserved spawn errors and dead finalizable instances, with
   everything they reach.
4. Forward the outside, while the old cells still carry their forwarding
   words: the heap's weak entries (function lookup, finalizable list) are
   dropped or repointed, so are the handles, and every holder in the root
   registry is walked through its `RootVisitor` (strong roots repointed,
   weak references repointed or reported dead). Nothing is left for the
   engine to repoint after the collection.
5. Reclaim: drop every dead cell; free dead large objects; a from-space
   block with no pin goes back to the pool empty, whole. One with a pin is
   *retained*: its dead and moved cells lose their start bits, its pinned
   survivors stay, nothing is allocated into it, and the first later
   collection that finds no pin in it copies its survivors out and frees it.
   Under `heap_debug` or `BAML_GC_STRESS=1` every reclaimed from-space cell
   is poisoned.

The budget for the next collection is `max(32 MiB, 4 × live bytes)`, live
bytes being the survivors' cells plus what they keep alive outside them.
`HeapStats::heap_bytes` over `HeapStats::live_bytes` is the fragmentation
ratio; `BexHeap::block_occupancy` counts blocks, retained blocks and cells.

## Handles

External code holds opaque handles; the handle table is always a root, and
a collection repoints each handle whose object moved, in the pause. The
`Restamp` map the explicit-root entry points (`collect_garbage`,
`collect_with_holder`, `collect`) return names only the precise roots that
moved; a cell reached through a root is repointed where it is stored.

## Concurrency

| Component | Mechanism |
|-----------|-----------|
| Cells and tables of a mutator's current block | Written only by that mutator, or by the collector while all mutators are parked |
| Block pool, large objects | `parking_lot::Mutex`, taken per block or large allocation |
| Handles | `RwLock<HashMap>` |
| Collection | `HeapPermitManager` pause: every permit drained, registry locked |

Global variables are written only by `$init`, then frozen into
`SharedGlobals`, which the collector traces as a root holder. Their mutation
goes through the same synchronized containers (`LazyBiasedMutex` arrays and
maps, atomic instance field slots) as values shared with a `spawn`ed task.
