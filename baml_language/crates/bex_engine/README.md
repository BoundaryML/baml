# BEX Engine

Async runtime for the BEX virtual machine, coordinating concurrent execution and garbage collection.

## Architecture Overview

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                                 ARCHITECTURE                                  │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐  │
│  │                              BexEngine                                  │  │
│  │  • Owns: BexHeap (contains Arc internally)                              │  │
│  │  • Owns: BytecodeProgram (Arc-shared)                                   │  │
│  │  • Owns: OpContext (Arc-shared, contains ResourceRegistry)              │  │
│  │  • Owns: env_vars                                                       │  │
│  │  • Responsibility: Event loop, Handle↔Value conversion, VM↔Sys mediate  │  │
│  │  • call_function(&self, ...) ← Note: &self, enables concurrency!        │  │
│  └───────────────┬───────────────────────────────────────┬─────────────────┘  │
│                  │ clones heap Arc, creates VM           │ calls              │
│                  ▼                                       ▼                    │
│  ┌──────────────────────────────────┐    ┌──────────────────────────────────┐ │
│  │              BexVm               │    │             bex_sys              │ │
│  │  • Has: BexHeap (cloned Arc)     │    │  • Provides: ops::fs, ops::net,  │ │
│  │  • Owns: EvalStack               │    │              ops::sys, ops::llm  │ │
│  │  • Owns: frames: Vec<Frame>      │    │  • Receives: BexValue args       │ │
│  │  • Owns: globals: GlobalPool     │    │  • Returns: BexValue             │ │
│  │  • Uses: ObjectIndex internally  │    │  • Uses: OpContext for resources │ │
│  │  • Yields: VmExecState to engine │    │                                  │ │
│  │                                  │    │  ResourceRegistry (in OpCtx):    │ │
│  │  NO DEPENDENCY ON bex_sys!       │    │  • Files, Network, Shell         │ │
│  │  (doesn't know about sys ops)    │    │                                  │ │
│  └──────────────────────────────────┘    └──────────────────────────────────┘ │
│                  │                                       │                    │
│                  │ uses types from                       │ uses types from    │
│                  ▼                                       ▼                    │
│  ┌─────────────────────────────────────────────────────────────────────────┐  │
│  │                              bex_vm_types                               │  │
│  │  • Defines: ObjectIndex, Value, Object                                  │  │
│  │  • Defines: ExternalOp, SysOp enums (operation descriptors)             │  │
│  │  • No dependencies (leaf crate)                                         │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                               │
│  KEY INSIGHT: bex_vm and bex_sys are SIBLINGS - they never depend on each     │
│               other. BexEngine is the ONLY component that talks to both.      │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

## Async Execution Model

The engine uses a Deno-inspired event loop where the VM executes synchronously until I/O:

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                      EVENT LOOP                                         │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                         │
│  call_function(&self, name, args)                                                       │
│        │                                                                                │
│        ▼                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────────────┐     │
│  │  Create VM with cloned heap Arc + TLAB, register with current epoch            │     │
│  └────────────────────────────────────────────────────────────────────────────────┘     │
│        │                                                                                │
│        ▼                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────────────┐     │
│  │  VM executes bytecode synchronously                                         ◄──┼──┐  │
│  └────────────────────────────────────────────────────────────────────────────────┘  │  │
│        │                                                                             │  │
│        ├───────────────────┬───────────────────┬───────────────────┐                 │  │
│        ▼                   ▼                   ▼                   ▼                 │  │
│  ┌───────────┐      ┌────────────┐      ┌────────────┐      ┌────────────┐           │  │
│  │ Complete  │      │ Schedule   │      │   Await    │      │  Notify    │           │  │
│  │ (Value)   │      │ Future     │      │ (pending)  │      │  (watch)   │           │  │
│  └─────┬─────┘      └─────┬──────┘      └─────┬──────┘      └─────┬──────┘           │  │
│        │                  │                   │                   │                  │  │
│        │                  ▼                   ▼                   ▼                  │  │
│        │           ┌────────────┐      ┌────────────┐      ┌─────────────┐           │  │
│        │           │ Spawn task │      │ Wait for   │      │ Call        │           │  │
│        │           │ (tokio)    │      │ completion │      │ notification│           │  │
│        │           └─────┬──────┘      └─────┬──────┘      │ callback    │           │  │
│        │                 │                   │             └─────┬───────┘           │  │
│        │                 │                   ▼                   │                   │  │
│        │                 │             ┌────────────┐            │                   │  │
│        │                 │             │ Fulfill    │            │                   │  │
│        │                 │             │ future     │            │                   │  │
│        │                 │             └─────┬──────┘            │                   │  │
│        │                 │                   │                   │                   │  │
│        │                 └───────────────────┴───────────────────┘───────────────────┘  │
│        │                                                                                │
│        ▼                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────────────┐     │
│  │  Unregister from epoch, return BexValue                                        │     │
│  └────────────────────────────────────────────────────────────────────────────────┘     │
│                                                                                         │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

### VmExecState Variants

| State | Meaning |
|-------|---------|
| `Complete(Value)` | Execution finished, return result |
| `ScheduleFuture(ObjectIndex)` | Spawn async op, immediately resume VM |
| `Await(ObjectIndex)` | Wait for future completion, fulfill, then resume VM |
| `Notify(WatchNotification)` | Call notification callback, then resume VM |

## Epoch-Based GC Coordination

VMs register with an epoch at call start. GC advances the epoch, causing old-epoch VMs to park at safepoints:

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                         EPOCH-BASED SAFEPOINT COORDINATION                    │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  Engine State:                                                                │
│  ┌─────────────────────────────────────────────────────────────────────────┐  │
│  │  current_epoch: AtomicU64 = 5                                           │  │
│  │                                                                         │  │
│  │  epoch_states: [EpochState; 2]                                          │  │
│  │    [0]: { active: 0, parked: 0 }  ← slot for even epochs                │  │
│  │    [1]: { active: 3, parked: 0 }  ← slot for odd epochs (epoch 5)       │  │
│  │                                                                         │  │
│  │  epoch_drained: Notify     ← VMs signal when they park                  │  │
│  │  gc_complete: Notify       ← GC signals when done                       │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                               │
│  TIMELINE: 3 VMs in epoch 5, GC requested                                     │
│  ┌─────────────────────────────────────────────────────────────────────────┐  │
│  │  T0: VM-A, VM-B, VM-C running (epoch 5)                                 │  │
│  │                                                                         │  │
│  │  T1: Engine calls collect_garbage()                                     │  │
│  │      └─ current_epoch.fetch_add(1) → epoch becomes 6                    │  │
│  │                                                                         │  │
│  │  T2: New call VM-D starts (gets epoch 6, unaffected by GC)              │  │
│  │                                                                         │  │
│  │  T3: VM-A reaches await point                                           │  │
│  │      ├─ Sees current_epoch(6) > my_epoch(5)                             │  │
│  │      ├─ parked += 1, notify epoch_drained                               │  │
│  │      └─ Waits on gc_complete                                            │  │
│  │                                                                         │  │
│  │  T4-T5: VM-B, VM-C reach await points and park                          │  │
│  │         parked(3) >= active(3) → all old-epoch VMs parked               │  │
│  │                                                                         │  │
│  │  T6: GC runs (safe - all old-epoch VMs frozen)                          │  │
│  │      ├─ Collect roots from handles + parked VM stacks                   │  │
│  │      ├─ Run semi-space collection                                       │  │
│  │      └─ gc_complete.notify_waiters()                                    │  │
│  │                                                                         │  │
│  │  T7: VM-A, VM-B, VM-C wake up and resume with forwarded references      │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

### VM State Machine at Safepoints

```
┌───────────┐    await point    ┌─────────────────┐
│  RUNNING  │ ─────────────────►│  CHECK EPOCH    │
└───────────┘                   └────────┬────────┘
     ▲                                   │
     │                          ┌────────┴────────┐
     │                    my_epoch ==        my_epoch <
     │                    current            current
     │                          │                 │
     │                          ▼                 ▼
     │                   ┌────────────┐   ┌─────────────────────┐
     │                   │ maybe_gc() │   │ PARK                │
     │                   │ if needed  │   │ ├─ incr parked      │
     │                   └──────┬─────┘   │ ├─ notify drained   │
     │                          │         │ └─ wait gc_complete │
     │                          │         └──────────┬──────────┘
     │                          │                    │
     │                          └──────┬─────────────┘
     │                                 ▼
     └────────────────────────  CONTINUE EXECUTION
```

## Value Boundary Crossing

Two value systems: VM-internal and external boundary:

```
┌───────────────────────────────────────────────────────────────────────────────────┐
│                                VALUE TYPE HIERARCHY                               │
├───────────────────────────────────────────────────────────────────────────────────┤
│                                                                                   │
│  VM Internal                           FFI / External Boundary                    │
│  ┌──────────────────────┐             ┌────────────────────────────────────────┐  │
│  │   Value              │             │ BexValue (unified boundary type)       │  │
│  │ ┌──────────────────┐ │             │ ┌────────────────────────────────────┐ │  │
│  │ │ Null             │ │             │ │                                    │ │  │
│  │ │ Int(i64)         │ │             │ │ Opaque(Handle)                     │ │  │
│  │ │ Float(f64)       │ │             │ │   └─ live reference to heap        │ │  │
│  │ │ Bool(bool)       │ │             │ │      resolve via to_bex_external() │ │  │
│  │ │ Object(Index) ───┼─┼────────────►│ │                                    │ │  │
│  │ └──────────────────┘ │             │ │ External(BexExternalValue)         │ │  │
│  │                      │             │ │   └─ owned data:                   │ │  │
│  │ Fast, uses indices   │             │ │      Null, Int, Float, Bool,       │ │  │
│  │ into shared heap     │             │ │      String, Array, Map,           │ │  │
│  │                      │             │ │      Instance, Variant, Union      │ │  │
│  │                      │             │ │                                    │ │  │
│  │                      │             │ └────────────────────────────────────┘ │  │
│  └──────────────────────┘             └────────────────────────────────────────┘  │
│                                                                                   │
└───────────────────────────────────────────────────────────────────────────────────┘
```

### BexValue Variants

- **`Opaque(Handle)`**: A live reference to a heap-allocated object. Use this when you want
  to keep a reference without copying data. The handle acts as a GC root, keeping the object
  alive. Resolve to owned data via `BexEngine::to_bex_external()` when needed.

- **`External(BexExternalValue)`**: Fully owned data that doesn't reference the heap.
  Use this when passing arguments to functions or when you've already converted a handle.
  Contains all concrete value types: primitives (Null, Int, Float, Bool), collections
  (String, Array, Map), and typed values (Instance, Variant, Union).

### Conversion Flow

```
Arguments:   BexValue → resolve to Value (allocation in VM heap if needed)
Returns:     Value → BexValue (Opaque for heap objects, External for primitives)
External Op: BexValue args → perform I/O → BexValue result
```

## Concurrency Scenarios

### Scenario 1: Independent Concurrent Calls

```
┌──────────────────────────────────────────────────────────────────┐
│  Thread 1                          Thread 2                      │
│  ┌─────────────────────────┐      ┌─────────────────────────┐    │
│  │ engine.call("foo", [])  │      │ engine.call("bar", [])  │    │
│  │   │                     │      │   │                     │    │
│  │   ▼                     │      │   ▼                     │    │
│  │ VM-1 (TLAB chunk 0-1023)│      │ VM-2 (TLAB chunk 1024+) │    │
│  │   │                     │      │   │                     │    │
│  │   │  No contention!     │      │   │  No contention!     │    │
│  │   │  Exclusive TLAB     │      │   │  Exclusive TLAB     │    │
│  │   ▼                     │      │   ▼                     │    │
│  │ Result-1                │      │ Result-2                │    │
│  └─────────────────────────┘      └─────────────────────────┘    │
│                                                                  │
│  Objects never shared between independent calls (no races)       │
└──────────────────────────────────────────────────────────────────┘
```

### Scenario 2: GC During Concurrent Execution

```
┌──────────────────────────────────────────────────────────────────┐
│  VM-1 (epoch 5)     VM-2 (epoch 5)     VM-3 (epoch 6, new)       │
│  ┌───────────┐      ┌───────────┐      ┌───────────┐             │
│  │ executing │      │ executing │      │ executing │             │
│  └─────┬─────┘      └─────┬─────┘      └─────┬─────┘             │
│        │                  │                  │                   │
│        │  GC requested (epoch → 6)           │                   │
│        │                  │                  │                   │
│        ▼                  ▼                  │                   │
│  ┌───────────┐      ┌───────────┐            │                   │
│  │ await →   │      │ await →   │            │ continues         │
│  │ PARK      │      │ PARK      │            │ normally          │
│  └─────┬─────┘      └─────┬─────┘            │                   │
│        │                  │                  │                   │
│        └──────────┬───────┘                  │                   │
│                   ▼                          │                   │
│            ┌─────────────┐                   │                   │
│            │ GC runs     │                   │                   │
│            │ (safe)      │                   │                   │
│            └──────┬──────┘                   │                   │
│                   │                          │                   │
│        ┌──────────┴───────┐                  │                   │
│        ▼                  ▼                  │                   │
│  ┌───────────┐      ┌───────────┐            │                   │
│  │ RESUME    │      │ RESUME    │            │                   │
│  │ (stacks   │      │ (stacks   │            │                   │
│  │ updated)  │      │ updated)  │            │                   │
│  └───────────┘      └───────────┘            │                   │
│                                                                  │
│  New-epoch calls proceed without waiting for old GC              │
└──────────────────────────────────────────────────────────────────┘
```

## Thread Safety Summary

| Component | Mechanism | Notes |
|-----------|-----------|-------|
| Heap sharing | `Arc<BexHeap>` | Cheap clones for each VM |
| Object writes | TLAB exclusivity | Lock-free within reserved region |
| GC coordination | Epoch-based safepoints | VMs park at await points |
| Handle access | `RwLock<HashMap>` | Concurrent reads, exclusive GC updates |
| External ops | `Mutex<ResourceRegistry>` | Clone Arc before release |

## Crate Dependencies

```
bex_vm_types (no deps, defines Value/Object/ObjectIndex)
      │
      ├──────────────┐
      ▼              ▼
  bex_heap       bex_sys (leaf, external operations)
      │              │
      ▼              │
   bex_vm            │
      │              │
      └──────┬───────┘
             ▼
        bex_engine (orchestrates everything)
```
