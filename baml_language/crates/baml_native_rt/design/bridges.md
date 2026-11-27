# Bridges: calling compiled BAML from Python, Node and Rust

A bridge lets a program in another language call a BAML function and get a typed result back. This note says how that works when BAML is compiled to machine code: what is decided, what is built, and what comes next.

Status, on branch `antoniosarosi/native-bridges-poc` after the review fixes of step 1:

| Part | State |
|---|---|
| The compiled program as a shared library, and the C functions it exports | Built as a spike, behind the runtime feature `host-lib` |
| A Rust host that loads the library and awaits calls | Built, as an example |
| Copying results into host objects | Next. Not built |
| Generated bindings for Rust, Python and Node | Next. Not built |
| Calls from BAML into the host, streaming, cancellation | Not designed |

## The decision

**Results are copied.** When a call returns, its result is copied once, directly from BAML memory, into an ordinary host object: a pydantic model, a plain TypeScript object, a Rust struct. There is no protobuf and no other wire format in between.

User code does not get references into BAML memory. That idea, and why it waits, is in [ideas/host-handles-and-refs.md](ideas/host-handles-and-refs.md). In short:

- For the usual case, where the host reads most of a result, a copy is as fast or faster in Python and Node.
- Users keep the objects they have today, and nobody has to choose between two kinds of result on every call.
- The cost of the current bridges is converting twice through protobuf, not the copy. This design converts once.

## The layers

```
 your Python / TypeScript / Rust code
        │   greet(person: Person) -> str            typed, as today
 ┌──────▼────────────────────────────────┐
 │ generated bindings (per program)      │  next
 ├───────────────────────────────────────┤
 │ small support library (per language)  │  next: event loop glue, copying values
 ├───────────────────────────────────────┤
 │ C functions exported by the library   │  built (spike)
 ├───────────────────────────────────────┤
 │ compiled BAML + the Rust runtime      │  one .dylib
 └───────────────────────────────────────┘
```

## What the user writes does not change

```python
from baml_sdk.class_refs import Inner, Outer, round_trip_outer
o = Outer(inner=Inner(value=9))          # a pydantic model
r = await round_trip_outer(o)            # a pydantic model
```

```ts
const o = new Outer({ inner: new Inner({ value: 9 }) });
const r = await round_trip_outer(o);
```

```rust
let o = Outer { inner: Inner { value: 9 } };
let r = round_trip_outer(o).await;
```

What changes is underneath: the library that is loaded, and how a value gets from BAML memory into the host object.

## Three ways to cross a C boundary

Every design uses C functions to get in and out of a library. The question is what travels through them.

| Style | What crosses | Who uses it | For BAML |
|---|---|---|---|
| **Message** | One generic call carrying a serialized blob | The VM's bridges (protobuf) | Two conversions per value: BAML to bytes, bytes to host object |
| **Struct** | A C struct per type, described in a header file | Ordinary C libraries. Usually what "just use C FFI" means | BAML objects are not C structs, so each one would be copied into a C struct first, then into the host object |
| **Handle** | A token for an object that stays inside the runtime, plus functions to read it | CPython's C API, JNI, SQLite's `sqlite3_column_*` | One conversion: read the object where it is, build the host object |

This design is the handle style, used only inside the bindings. User code never sees a handle.

Why not the struct style:

- The collector **moves** objects (`gc.md`). A `Person*` given to the host points at garbage after the next collection.
- A field is not a C field: every value is two words, a tag and a payload (`memory.md`).
- A string is not a `char*`, a list is not a C array, and a union is told apart by its tag.
- A C function returns when it is done. A BAML function may wait for the network, so a call must be able to finish later.

## How a call works (built)

A call does not return its result. It starts, and finishes later.

```
host                                 the library
----                                 -----------
load, init(callbacks)      ──────▶   create the worker; no main, no thread of its own
call_start(fn, args)       ──────▶   queue a task for the call        ──▶ a call id
turn()                     ──────▶   run BAML until every task is done or waiting
      ◀────── on_done(call id, status, value)   a call finished
      ◀────── on_unhandled(call id, error)      a task it spawned failed, unawaited
      ◀────── wake()                            from the I/O thread: "turn me again"
```

- BAML runs on the host's own thread, inside `turn()`. Nothing runs in between.
- `turn()` never sleeps. It returns how long until the next BAML timer, and the host arms its own timer for that.
- `on_done` and `on_unhandled` are called after the run, inside `turn()`, on the caller's thread and stack, never on a task's stack. `wake` may come from any thread. A callback must not call into the library, and the host frees every handle a callback gives it.
- A generated function wraps this in the host's async type: an `async def`, a `Promise`, a `Future`.

How a call can end, as `on_done` says:

| Status | Meaning | Value |
|---|---|---|
| 0 returned | The function returned | Its result |
| 1 threw | It threw, and nothing in it caught the throw | What it threw |
| 2 stuck | It can never finish: nothing can run, no timer is set, no I/O can complete | `null`. The call is over for the host; a result that came later would be dropped |
| 3 panicked | A `baml.panics` error reached the top of the call, `baml.panics.Exit` included | The panic |

No BAML code can end the host's process. `baml.panics.Exit` from the call's own task is status 3. From a task the call spawned, it is a failure like any other.

A task the call spawned that fails, with nothing awaiting it, is reported through `on_unhandled` once the call has finished, because nothing can await it after that. The task is then freed.

## The exported functions (built)

In `src/sched/host.rs`, feature `host-lib`. The library exports these and nothing else.

| Function | Does | May collect |
|---|---|---|
| `baml_lib_abi_version()` | The version of this interface (2) | no |
| `baml_lib_init(callbacks)` | Builds the worker and the handle table. `callbacks` starts with its own size; a struct too small is refused | no |
| `baml_lib_find(name, len)` | The number of an exported function, or -1 | no |
| `baml_lib_export_count()`, `baml_lib_export_info(...)` | Per export: arity, signature hash, signature text | no |
| `baml_lib_interface_hash()` | One hash for the whole exported interface | no |
| `baml_lib_call_start(fn, args, n)` | Queues a task for the call; returns the call id, or 0 | yes |
| `baml_lib_turn()` | Runs every runnable task, then calls `on_done` for each finished call | yes |
| `baml_lib_string_new`, `_string_len`, `_string_copy` | Make a string; read one by copying it out | `new`: yes |
| `baml_lib_handle_tag`, `_describe`, `_handle_free` | An object's tag; its text form; release a handle | no |
| `baml_lib_collect`, `_debug_address`, `_debug_task_count` | Test aids | yes / no / no |

## Values across the boundary (built)

A value is two words, `{tag, payload}`, the same 16 bytes a heap slot holds.

| What | Tag | Payload |
|---|---|---|
| `int`, `float`, `bool`, `null` | As in memory, for example `INT` = 2 | The value itself |
| Any object | Its real tag (`STRING`, `INSTANCE`, ...) with bit 61 set | A **handle** |
| An argument | As above. A scalar's tag must have no other bit set, and `OMITTED` is refused | |

A handle is a slot in a fixed table (4096 slots) that never moves. Every slot is registered with the collector, which rewrites it when the object moves. So a handle stays valid until it is released, across any number of collections.

```
 bindings ──▶ handle slot ──▶ Person      (the slot is rewritten when Person moves)
```

## Inside the library (built)

| Piece | Where | What it is |
|---|---|---|
| Library form | `baml_compiler2_native`: `native_pack --lib`, `link.rs` | A `.dylib` linked against a runtime archive without `main` |
| Export entry | `codegen/host.rs` | Per exported function, an entry with the C convention. It reads the arguments, calls the function under a catch-all, and returns the result or what was thrown |
| Host shim | `codegen/host.rs` | Compiled code that every host-started task begins in, so no Rust frame sits under a parked task |
| Export table | `codegen/host.rs` | Names, arities and hashes, read by `find` and `export_info` |
| The turn | `sched/host.rs` | `serve`'s loop, returning where `serve` would sleep |

How one call runs:

1. `call_start` puts the arguments in a BAML array held by the new task, where the collector can see them.
2. The task starts in the host shim, which calls the export entry.
3. When the function ends, the result is recorded. Nothing calls the host from a task's stack.
4. After the run, `turn` reports what ended: each finished call, each call found stuck, each failure nobody awaits.

The call id lives in the task (`wait_value`), and every task the call spawns inherits it. The entry's address waits in the first slot of the argument array. So starting a call allocates nothing but the array and, when none is free, a task.

## Copying a result (next)

Generated code knows each type, so the copy is straight-line code: read each field once, build the host object.

- A field's position in an instance is its declaration order, after any type parameters (`codegen.rs`).
- The compiler writes, next to the library, the facts bindings need: the number of each function, the position of each field in its class, the order of each enum's variants.
- Bindings and library both store a hash of the program's interface. At load, a mismatch is refused.
- In Python and Node the copy should be one native call per result, not one call per field.

Arguments go the other way: the bindings build the BAML value bottom-up, and everything under construction is held by a handle, because any allocation may move what was built before.

## Measured

On the Rust host, Apple M5 Max:

| | Instructions |
|---|---|
| One `add(int, int)`: start, one turn, completion | 1,680 |
| The same, with 12,000 calls started before the first turn | about 20,000 per call (each waiting task maps a fresh stack) |

The first number was estimated at 330 before it was measured. The likely extras are bookkeeping in the spike (a hash map per call, a vector reallocated per turn), not yet profiled.

## Limits today

| Limit | Consequence |
|---|---|
| One worker per process | Two BAML libraries cannot be loaded together. Calls into the library must come one at a time; nothing enforces it |
| Argument types are not checked by the library | Compiled code trusts static types. Generated bindings and the interface hash are the guard |
| Only `int` and `string` cross | No accessors yet for instances, arrays or maps |
| A class exists at run time only if compiled code uses it | Library mode must include every class in an exported signature. Not done |
| An array does not record its element type | A result typed `int[] \| string[]` cannot be told apart by its tag |
| The trace of a throw is dropped | Errors arrive as a value and its text |
| A stuck call's tasks stay parked for the life of the process | A task parked on I/O may still be named by the job it waits for, so it is not freed |
| Timers end about 3 ms late | Executables do too; not a bridge problem |
| arm64 macOS only | As for all native code today |

## Try it

```sh
cd baml_language
cargo build --release -p baml_compiler2_native --example native_pack --example native_host
target/release/examples/native_pack --lib \
  crates/baml_compiler2_native/examples/native_host/bridge.baml libbridge.dylib \
  add nap env_or greet fail
target/release/examples/native_host libbridge.dylib
```

The host starts `add`, a sleep and two environment reads together, and awaits them as Rust futures. It then passes a string in, gets a string handle back, forces two collections and reads the string again.
