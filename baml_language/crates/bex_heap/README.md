# BEX Heap

Shared memory management with semi-space copying garbage collection for the BEX runtime.

## Architecture Overview

```
┌───────────────────────────────────────────────────────────────────────────┐
│                              BexEngine                                    │
│  • Owns BexHeap via Arc                                                   │
│  • Coordinates GC via epoch-based safepoints                              │
│  • call_function(&self) enables concurrent execution                      │
└───────────────────────────────────────────────────────────────────────────┘
                  │ clones heap Arc, creates VM per call
                  ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                               BexVm (per-call)                            │
│  • Receives: BexHeap (cloned Arc)                                         │
│  • Owns: TLAB for lock-free allocation                                    │
│  • Uses: ObjectIndex internally                                           │
└───────────────────────────────────────────────────────────────────────────┘
                  │ allocates via TLAB
                  ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                          BexHeap (Shared via Arc)                         │
├───────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │              compile_time: Vec<Object>                              │  │
│  │  [0] [1] [2] ... [N-1]                                              │  │
│  │   │   │   │       │                                                 │  │
│  │   ↓   ↓   ↓       ↓                                                 │  │
│  │  Fn  Cls Enum   ...     ← PERMANENT (functions, classes, enums)     │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                               ▲                                           │
│                    compile_time_boundary = N                              │
│                               ▼                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │          spaces[active]: ChunkedVec<Object>                         │  │
│  │  [N] [N+1] [N+2] ...                                                │  │
│  │   │    │     │                                                      │  │
│  │   ↓    ↓     ↓                                                      │  │
│  │  Arr  Map  Inst   ...   ← RUNTIME (arrays, maps, instances)         │  │
│  │                                                                     │  │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐                                │  │
│  │  │ TLAB 1  │ │ TLAB 2  │ │ TLAB 3  │  ← Per-VM allocation chunks    │  │
│  │  └─────────┘ └─────────┘ └─────────┘                                │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │          spaces[inactive]: ChunkedVec<Object>                       │  │
│  │                   (empty until GC copies here)                      │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  handles: RwLock<HashMap<key, ObjectIndex>>  ← FFI boundary / GC roots    │
│  active_space: AtomicUsize (0 or 1)                                       │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

## Object Indexing

Objects are referenced via `ObjectIndex` (a `usize` with optional epoch for debug):

```
ObjectIndex(raw)
      │
      ▼
┌─────────────────┐
│ raw < ct_len?   │
└────────┬────────┘
    YES  │  NO
    ↓    │  ↓
compile_time[raw]  │  spaces[active][raw - ct_len]
```

## TLAB Allocation (Fast Path)

Each VM gets exclusive access to a TLAB chunk for lock-free allocation:

```
tlab.alloc(obj)
      │
      ▼
┌───────────────────────┐
│ alloc_ptr >= limit?   │
└───────────┬───────────┘
      YES   │   NO
      ↓     │   ↓
  refill()  │   1. global_idx = alloc_ptr
  (atomic   │   2. alloc_ptr += 1
   chunk    │   3. write object to heap
   reserve) │   4. return ObjectIndex
```

- **Fast path**: Single pointer bump, no locks
- **Cold path**: Atomic `fetch_add` to reserve next chunk (~every 1024 allocations)

## Semi-Space Garbage Collection

Cheney-style copying collector that alternates between two spaces:

```
BEFORE GC:                           AFTER GC:
┌─────────────────────┐              ┌─────────────────────┐
│ spaces[0] (ACTIVE)  │              │ spaces[0] (INACTIVE)│
│ [A][B][C][D][E][F]  │              │      (cleared)      │
│  ↑       ↑          │              │                     │
│  └───────┴── roots  │              │                     │
└─────────────────────┘              └─────────────────────┘
┌─────────────────────┐              ┌─────────────────────┐
│ spaces[1] (INACTIVE)│              │ spaces[1] (ACTIVE)  │
│      (empty)        │              │ [A'][C']            │
│                     │              │   ↑                 │
│                     │              │   └── compacted     │
└─────────────────────┘              └─────────────────────┘

B, D, E, F collected (garbage)
```

### Collection Algorithm

1. **Initialize**: Clear inactive space, create forwarding map
2. **BFS Trace**: Starting from roots (VM stacks + handles)
   - Skip if already forwarded or compile-time
   - Copy live object to inactive space
   - Record `forwarding[old_idx] = new_idx`
   - Add object's references to worklist
3. **Fix References**: Update all ObjectIndex fields in copied objects
4. **Swap Spaces**: Atomic `active_space.store(to_space)`
5. **Update Handles**: Remap handle table using forwarding map
6. **Invalidate TLABs**: Force VMs to refill from new space

## ChunkedVec: Stable Pointer Storage

ChunkedVec stores objects in fixed-size chunks (4096 elements) to prevent pointer invalidation during concurrent growth:

```
Vec (problematic):              ChunkedVec (safe):
┌──────────────────┐            Chunk 0: ┌──────────────┐
│ [obj0][obj1]...  │                     │ [obj0][obj1] │ ← never moves
└──────────────────┘                     └──────────────┘
        │                       Chunk 1: ┌──────────────┐
        ↓ resize                         │ [obj4096]... │ ← appended
┌───────────────────────┐                └──────────────┘
│ [obj0][obj1]... (NEW) │
└───────────────────────┘
  ↑ old pointers invalid!       Index N → chunk[N/4096][N%4096]
```

This fixed a race condition where concurrent TLAB refills could invalidate pointers held by other VMs.

## Handle-Based FFI

External code interacts through opaque handles that serve as GC roots:

```
External Code                        Heap
┌─────────────────┐                 ┌──────────────────────────┐
│  Handle         │  slab_key       │  handles: HashMap        │
│  ┌───────────┐  │                 │  ┌────────────────────┐  │
│  │ key: 42   │──┼────────────────►│  │ 42: ObjectIndex(N) │  │
│  │ heap: Arc │  │                 │  └────────┬───────────┘  │
│  └───────────┘  │                 │           │              │
│                 │                 │           ▼              │
│                 │                 │  [N]: Array([...])       │
└─────────────────┘                 │       ↑                  │
                                    │       └── protected      │
                                    │           from GC        │
                                    └──────────────────────────┘
```

After GC, handles are updated via the forwarding map to point to new locations.

## Concurrency Model

| Component | Mechanism | Purpose |
|-----------|-----------|---------|
| Object storage | `UnsafeCell<ChunkedVec>` + TLAB exclusivity | Lock-free writes within reserved regions |
| Active space | `AtomicUsize` | Atomic space swaps during GC |
| TLAB chunks | `AtomicUsize::fetch_add` | Lock-free chunk reservation |
| Handles | `RwLock<HashMap>` | Concurrent reads, exclusive GC updates |
| GC coordination | Epoch-based safepoints | VMs park at await points for collection |

### Safety Invariants

1. **TLAB exclusivity**: Each VM's TLAB region is never accessed by other VMs
2. **Safepoint GC**: Collection only runs when all old-epoch VMs are parked
3. **ChunkedVec stability**: Growing never moves existing elements
4. **No global mutable state**: BAML has no global variables
