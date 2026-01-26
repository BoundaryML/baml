# BEX VM

Stack-based bytecode interpreter for the BAML language, inspired by CPython and Lox.

## Architecture Overview

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│                                        BexVm                                          │
├───────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                       │
│  ┌─────────────────────────────────────────────────────────────────────────────────┐  │
│  │                             Call Stack (frames)                                 │  │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐                                            │  │
│  │  │ Frame 0 │ │ Frame 1 │ │ Frame 2 │ ...  (MAX_FRAMES = 256)                    │  │
│  │  │ main()  │ │ foo()   │ │ bar()   │                                            │  │
│  │  └─────────┘ └─────────┘ └─────────┘                                            │  │
│  │                                                                                 │  │
│  │  Each Frame contains:                                                           │  │
│  │  • function: ObjectIndex      ← which function is running                       │  │
│  │  • instruction_ptr: isize     ← next instruction to execute                     │  │
│  │  • locals_offset: StackIndex  ← where locals start in eval stack                │  │
│  └─────────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                       │
│  ┌─────────────────────────────────────────────────────────────────────────────────┐  │
│  │                             Evaluation Stack                                    │  │
│  │                                                                                 │  │
│  │    ┌────┬─────┬─────┬─────┬─────┬────────┬─────────────────┐                    │  │
│  │    │ fn │arg1 │arg2 │loc1 │loc2 │ Int(3) │ HeapPtr("hello")│ ← values flow here │  │
│  │    └────┴─────┴─────┴─────┴─────┴────────┴─────────────────┘                    │  │
│  │    ▲                            ▲                          ▲                    │  │
│  │    │                            │                          │                    │  │
│  │ locals_offset               locals_end                  stack_top               │  │
│  │                                                                                 │  │
│  └─────────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                       │
│  ┌────────────────────────┐  ┌────────────────────────┐  ┌────────────────────────┐   │
│  │  tlab: Tlab            │  │  heap: Arc<BexHeap>    │  │  watch: Watch          │   │
│  │  • alloc_ptr: usize    │  │                        │  │  (watch graph)         │   │
│  │  • alloc_limit: usize  │  │                        │  │                        │   │
│  └───────────┬────────────┘  └───────────┬────────────┘  └────────────────────────┘   │
│              │                           │                                            │
└──────────────│───────────────────────────│────────────────────────────────────────────┘
               │                           │
               │    ┌──────────────────────┘
               │    │
               ▼    ▼
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                              BexHeap (shared across VMs)                                │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                         │
│  ┌───────────────────────────────────────────────────────────────────────────────────┐  │
│  │  runtime spaces[2]: ChunkedVec<Object>  (semi-space GC, one active at a time)     │  │
│  │                                                                                   │  │
│  │  alloc_ptr                                                                        │  │
│  │      │    alloc_limit                                                             │  │
│  │      ▼        ▼                                                                   │  │
│  │    ┌─────┬─────┬─────┬─────┐   ┌─────┬─────┬─────┬─────┐   ┌─────┬─────┬─────┐    │  │
│  │    │ obj │ obj │ obj │     │   │ obj │ obj │     │     │   │     │     │     │    │  │
│  │    └─────┴─────┴─────┴─────┘   └─────┴─────┴─────┴─────┘   └─────┴─────┴─────┘    │  │
│  │    ├──── TLAB chunk ───────┤   ├──── TLAB chunk ───────┤   ├──── chunk ──────┤    │  │
│  │          (VM 1)                      (VM 2)                   (available)         │  │
│  │                                                                                   │  │
│  │  Allocation: VM bumps alloc_ptr, writes object directly, no locks                 │  │
│  │  When full: atomic fetch_add reserves next chunk                                  │  │
│  │                                                                                   │  │
│  └───────────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                         │
│  ┌───────────────────────────────────────────────────────────────────────────────────┐  │
│  │  compile_time: Vec<Object>  (permanent, never collected)                          │  │
│  │                                                                                   │  │
│  │ ┌───────────────────┬───────────────────┬───────────────────┬───────────────────┐ │  │
│  │ │     Function      │       Class       │        Enum       │       String      │ │  │
│  │ └──────────┬────────┴──────────┬────────┴──────────┬────────┴──────────┬────────┘ │  │
│  │            │                   │                   │                   │          │  │
│  │            ▼                   ▼                   ▼                   ▼          │  │
│  │  ┌───────────────────┐ ┌───────────────────┐ ┌───────────────────┐ ┌───────────┐  │  │
│  │  │ name: "add"       │ │ name: "Point"     │ │ name: "Status"    │ │  "hello"  │  │  │
│  │  │ bytecode:         │ │ field_names:      │ │ variant_names:    │ └───────────┘  │  │
│  │  │   LOAD_VAR 1      │ │   ["x", "y"]      │ │   ["Success",     │                │  │
│  │  │   LOAD_VAR 2      │ └───────────────────┘ │    "Failure"]     │                │  │
│  │  │   ADD             │                       └───────────────────┘                │  │
│  │  │   RETURN          │                                                            │  │
│  │  └───────────────────┘                                                            │  │
│  │                                                                                   │  │
│  └───────────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                         │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

## Stack-Based Execution

Unlike register-based VMs, a stack VM operates by pushing and popping values from an evaluation stack. All operations consume operands from the top of the stack and push results back.

Example for the expression `result = (a + b) * c`:

```
Register VM (2 instructions):          Stack VM (6 instructions):    Eval Stack:

  ADD r2, r1, r0   ; r2 = r1 + r0         LOAD_VAR a                   [a]
  MUL r4, r2, r3   ; r4 = r2 * r3         LOAD_VAR b                   [a, b]
                                          ADD                          [a+b]
                                          LOAD_VAR c                   [a+b, c]
                                          MUL                          [(a+b)*c]
                                          STORE_VAR result             []
```

Trade-off: More instructions to execute, but simpler implementation (no register allocation).

## Value Types

The VM operates on a small set of value types:

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                     Value                                        │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌────────┬──────────┬────────────┬────────────┬─────────────────────┐           │
│  │  Null  │ Int(i64) │ Float(f64) │ Bool(bool) │ Object(ObjectIndex) │           │
│  └────────┴──────────┴────────────┴────────────┴──────────┬──────────┘           │
│                                                           │                      │
└───────────────────────────────────────────────────────────│──────────────────────┘
                                                            │
                                                            ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                      Heap                                        │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  String, Array, Map, Instance, Variant, Function, Class, Enum, Media, Future     │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

## Memory Access Patterns

The VM accesses heap memory through two mechanisms:

| Access Type | Method | When Used |
|-------------|--------|-----------|
| Read | `get_object(idx)` | Type checks, field reads, method dispatch |
| Write | `get_object_mut(idx)` | Field writes, array/map mutations |
| Allocate | `tlab.alloc(obj)` | Creating new objects (strings, arrays, instances) |

Safety invariants:
- Single-threaded execution within a VM instance
- TLAB provides exclusive write access to allocated regions
- Only runtime objects (not compile-time) can be mutated
- GC only runs when VM is yielded at safepoints

## Execution Loop

The main `exec()` loop fetches and executes one instruction per cycle:

```
┌───────────────────────────────────────────────────────────────────────┐
│                            exec() loop                                │
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  1. Read instruction_ptr from current frame                     │  │
│  │  2. Increment instruction_ptr (for next cycle)                  │  │
│  │  3. Decode instruction at bytecode[instruction_ptr]             │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                              │                                        │
│                              ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  match instruction {                                            │  │
│  │      Push(val)      → stack.push(val)                           │  │
│  │      Pop            → stack.pop()                               │  │
│  │      Add            → a,b = pop2(); push(a+b)                   │  │
│  │      LoadVar(i)     → push(stack[locals_offset + i])            │  │
│  │      StoreVar(i)    → stack[locals_offset + i] = pop()          │  │
│  │      Call(n)        → push new Frame, jump to function          │  │
│  │      Return         → pop Frame, push return value              │  │
│  │      Jump(offset)   → instruction_ptr += offset                 │  │
│  │      DispatchFuture → return ScheduleFuture(idx)                │  │
│  │      Await          → return Await(idx) if pending              │  │
│  │      ...                                                        │  │
│  │  }                                                              │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                              │                                        │
│               ┌──────────────┼──────────────┐                         │
│               │              │              │                         │
│               ▼              ▼              ▼                         │
│        ┌───────────┐  ┌───────────┐  ┌───────────┐                    │
│        │ frames    │  │ Schedule  │  │  Await    │                    │
│        │ empty?    │  │ Future    │  │ (pending) │                    │
│        └─────┬─────┘  └─────┬─────┘  └─────┬─────┘                    │
│          yes │ no           │              │                          │
│              │              └──────────────┴────────────────┐         │
│   ┌──────────┴──────────┐                                   │         │
│   ▼                     ▼                                   ▼         │
│ Complete(val)    (continue loop)                       VmExecState    │
│                                                        returned to    │
│                                                          engine       │
└───────────────────────────────────────────────────────────────────────┘
```

## Function Call Convention

When bytecode calls a function, it first loads the function reference, then pushes arguments.

Illustration with source code `let result = add(x, y)`:

```
Bytecode:                       Stack after each instruction:
  LOAD_GLOBAL "add"               [add_fn]
  LOAD_VAR x                      [add_fn, x]
  LOAD_VAR y                      [add_fn, x, y]
  CALL 2                          → new frame created

After CALL(2):
┌─────────────────────────────────────────────────────┐
│  ... │ add_fn │   x   │   y   │                     │
└─────────────────────────────────────────────────────┘
       ▲                        ▲
       │                        │
   locals_offset            stack_top
   (new frame)

Inside add(), locals are accessed relative to locals_offset:
  LOAD_VAR 0  → add_fn (the function itself)
  LOAD_VAR 1  → x (first argument)
  LOAD_VAR 2  → y (second argument)
```

After `RETURN`, the frame is popped and result replaces the call site on stack.

## Native Functions vs Bytecode Functions

**Case 1: Calling a bytecode function** (stays inside VM)

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                        VM                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  Instruction:               Eval Stack:                     Frames:                 │
│                                                                                     │
│  ┌──────────────────┐       ┌────────┐                      ┌──────┐                │
│  │ LOAD_GLOBAL add  │       │ add_fn │                      │ main │                │
│  │                  │       └────────┘                      └──────┘                │
│  │                  │       ┌────────┬───┐                  ┌──────┐                │
│  │ LOAD_VAR x       │       │ add_fn │ x │                  │ main │                │
│  │                  │       └────────┴───┘                  └──────┘                │
│  │                  │       ┌────────┬───┬───┐              ┌──────┐                │
│  │ LOAD_VAR y       │       │ add_fn │ x │ y │              │ main │                │
│  │                  │       └────────┴───┴───┘              └──────┘                │
│  │                  │       ┌────────┬───┬───┐              ┌──────┬─────┐          │
│  │ CALL 2           │       │ add_fn │ x │ y │              │ main │ add │          │
│  │                  │       └────────┴───┴───┘              └──────┴─────┘          │
│  └──────────────────┘                                                               │
│           │                                                                         │
│           │                 ... add() executes                                      │
│           ▼                                                                         │
│  ┌──────────────────┐       ┌────────┬───┬───┬───┐          ┌──────┬─────┐          │
│  │ LOAD_VAR 1       │       │ add_fn │ x │ y │ x │          │ main │ add │          │
│  │                  │       └────────┴───┴───┴───┘          └──────┴─────┘          │
│  │                  │       ┌────────┬───┬───┬───┬───┐      ┌──────┬─────┐          │
│  │ LOAD_VAR 2       │       │ add_fn │ x │ y │ x │ y │      │ main │ add │          │
│  │                  │       └────────┴───┴───┴───┴───┘      └──────┴─────┘          │
│  │                  │       ┌────────┬───┬───┬─────┐        ┌──────┬─────┐          │
│  │ ADD              │       │ add_fn │ x │ y │ x+y │        │ main │ add │          │
│  │                  │       └────────┴───┴───┴─────┘        └──────┴─────┘          │
│  │                  │       ┌─────┐                         ┌──────┐                │
│  │ RETURN           │       │ x+y │                         │ main │                │
│  │                  │       └─────┘                         └──────┘                │
│  └──────────────────┘                                                               │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Case 2: Calling a native function** (calls out to Rust)

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                        VM                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  Instruction:               Eval Stack:                     Frames:                 │
│                                                                                     │
│  ┌──────────────────┐       ┌───┐                           ┌──────┐                │
│  │ LOAD_CONST 1     │       │ 1 │                           │ main │                │
│  │                  │       └───┘                           └──────┘                │
│  │                  │       ┌───┬───┐                       ┌──────┐                │
│  │ LOAD_CONST 2     │       │ 1 │ 2 │                       │ main │                │
│  │                  │       └───┴───┘                       └──────┘                │
│  │                  │       ┌───┬───┬───┐                   ┌──────┐                │
│  │ LOAD_CONST 3     │       │ 1 │ 2 │ 3 │                   │ main │                │
│  │                  │       └───┴───┴───┘                   └──────┘                │
│  │                  │       ┌─────────────┐                 ┌──────┐                │
│  │ ALLOC_ARRAY 3    │       │ [1, 2, 3]   │                 │ main │                │
│  │                  │       └─────────────┘                 └──────┘                │
│  │                  │                                       ┌──────┐                │
│  │ STORE_VAR arr    │       (arr stored at locals[0])       │ main │                │
│  │                  │                                       └──────┘                │
│  │                  │       ┌────────┐                      ┌──────┐                │
│  │ LOAD_GLOBAL len  │       │ len_fn │                      │ main │                │
│  │                  │       └────────┘                      └──────┘                │
│  │                  │       ┌────────┬─────────────┐        ┌──────┐                │
│  │ LOAD_VAR arr     │       │ len_fn │ [1, 2, 3]   │        │ main │                │
│  │                  │       └────────┴─────────────┘        └──────┘                │
│  │                  │                                                               │
│  │ CALL 1           │                                                               │
│  │                  │                                                               │
│  │                  │       ┌───┐                           ┌──────┐                │
│  │ ...              │       │ 3 │                           │ main │ (no new frame) │
│  │                  │       └─▲─┘                           └──────┘                │
│  └────────┬─────────┘         │                                                     │
│           │                   │                                                     │
└───────────│───────────────────│─────────────────────────────────────────────────────┘
            │                   │
            │                   │
            ▼                   │
   ┌─────────────────────────────────────────────────────────────────┐
   │  fn rust_native_len(vm: &mut BexVm, args: &[Value]) -> Value {  │
   │      let ptr = &args[0];                                        │
   │      let Object::Array(arr) = vm.get_object(ptr);               │
   │      Value::Int(arr.len())  // returns 3                        │
   │  }                                                              │
   └─────────────────────────────────────────────────────────────────┘
```

## Async Operations: DispatchFuture + Await

The VM cannot perform I/O directly. External operations (LLM calls, file I/O) use a two-phase pattern.

Example: `let content = fetch("http://example.com")`

```
┌──────────────────────────────────────────────────┐      ┌───────────────────────────────────────┐
│                       VM                         │      │               Engine                  │
├──────────────────────────────────────────────────┤      ├───────────────────────────────────────┤
│                                                  │      │                                       │
│  Instruction:          Eval Stack:               │      │                                       │
│                                                  │      │  ┌───────────────────────────────┐    │
│  ┌──────────────────┐  ┌──────────┐              │      │  │ tokio::spawn {                │    │
│  │ LOAD_GLOBAL      │  │ fetch_fn │              │      │  │     SysOp::Fetch(url)         │    │
│  │   fetch          │  └──────────┘              │      │  │ }                             │    │
│  │                  │  ┌──────────┬────────────┐ │      │  │                               │    │
│  │ LOAD_CONST url   │  │ fetch_fn │ "http://." │ │      │  │ // task running in background │    │
│  │                  │  └──────────┴────────────┘ │      │  └───────────────────────────────┘    │
│  │                  │  ┌────────────┐            │      │                 ▲                     │
│  │ DISPATCH_FUTURE ─│──│ future_idx │ ───────────│──────│─────────────────┘                     │
│  │                  │  └────────────┘            │      │                                       │
│  │ ...              │  (vm continues working)    │      │                                       │
│  │                  │                            │      │                                       │
│  │                  │  ┌────────────┐            │      │                                       │
│  │ AWAIT ───────────│──│ future_idx │            │      │                                       │
│  │                  │  └────────────┘            │      │                                       │
│  │                  │        │                   │      │                                       │
│  └──────────────────┘        │ pending?          │      │  ┌───────────────────────────────┐    │
│                              ▼                   │      │  │ let result = task.await;      │    │
│                     VmExecState::Await ──────────│──────│─►│                               │    │
│                                                  │      │  │ heap[idx] = Ready(result)     │    │
│                        ┌───────────────┐         │      │  │ vm.exec()                     │    │
│                        │ "<html>..."   │◄────────│──────│──│                               │    │
│                        └───────────────┘         │      │  └───────────────────────────────┘    │
│                                                  │      │                                       │
└──────────────────────────────────────────────────┘      └───────────────────────────────────────┘
```

Key insight: DISPATCH_FUTURE returns immediately, allowing the VM to continue other work.
AWAIT blocks only if the future is still pending.

## Watch System

The `watch` keyword enables reactive change notifications. The Watch module maintains a
dependency graph tracking which heap objects are reachable from watched variables. When a
watched variable or its nested fields change, the VM yields to the engine which calls a
notification handler callback.

Example: `watch let user = getUser(); ... user.email = "new@..."`

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│                                          VM                                          │
├──────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│  Instruction:               Eval Stack:                     Frames:                  │
│                                                                                      │
│  ┌──────────────────────┐   ┌──────┐                        ┌──────┐                 │
│  │ CALL getUser         │   │ user │                        │ main │                 │
│  │                      │   └──────┘                        └──────┘                 │
│  │ WATCH 0, "user"      │   (registers root)                                         │
│  │ ...                  │                                   ┌──────┐                 │
│  │                      │   ┌───────────┐                   │ main │                 │
│  │ LOAD_CONST "new@..." │   │ "new@..." │                   └──────┘                 │
│  │ LOAD_VAR user        │   └───────────┘                                            │
│  │                      │                                   ┌──────┐                 │
│  │ STORE_FIELD email    │   (triggers filter)               │ main │                 │
│  │                      │                                   └──────┘                 │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  ┌─────────────────────────────────────────────────────────────────────────────────┐ │
│  │ match filter:                                                                   │ │
│  │   Default       → deep_equals(last_assigned, value)?                            │ │
│  │   Function(f)   → interrupt(f, [value])                                         │ │
│  │   Manual/Paused → skip                                                          │ │
│  └─────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                      │
│  ─── interrupt(f, [value]) ────────────────────────────────────────────────────────  │
│                                                                                      │
│  ┌──────────────────────┐   ┌───────────┐                   ┌──────┬────────┐        │
│  │ (filter bytecode)    │   │ filter_fn │                   │ main │ filter │        │
│  │ LOAD_VAR 1           │   │ value     │                   └──────┴────────┘        │
│  │ ...                  │   └───────────┘                                            │
│  │                      │   ┌──────┐                        ┌──────┐                 │
│  │ RETURN               │   │ bool │  ← should notify?      │ main │                 │
│  │                      │   └──────┘                        └──────┘                 │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  ─── if should_notify ─────────────────────────────────────────────────────────────  │
│                                                                                      │
│                                        VmExecState::Notify                           │
│                                               │                                      │
│                                               │                          ▲           │
│                                               │                 continues│           │
└───────────────────────────────────────────────│──────────────────────────│───────────┘
                                                │                          │
                                                ▼                          │
┌───────────────────────────────────┐      ┌───────────────────────────┐   │
│ Python (Host Lang):               │      │ Engine:                   │   │
│   def handle_watch_notification() │◄─────│   watch_handlers(roots)   │   │
└───────────────────────────────────┘      │   vm.exec() ──────────────│───┘
                                           └───────────────────────────┘
```

**Filters** control when notifications fire:
- `Default`: deep_equals(last_assigned, value) - notify only if actually changed
- `Function(f)`: calls `interrupt(f, [value])` which runs filter bytecode inline, returns bool
- `Manual`: never auto-notifies, requires explicit NOTIFY instruction
- `Paused`: disabled, never notifies

## Crate Dependencies

```
bex_vm_types (defines Instruction, Value, Object, bytecode)
      │
      ▼
  bex_heap (provides BexHeap, Tlab for allocation)
      │
      ▼
   bex_vm (this crate - the interpreter)
      │
      ▼
  bex_engine (orchestrates VM + async operations)
```
