# Bridges: calling compiled BAML from Python, Node and Rust

A bridge lets a program in another language call a BAML function and get a typed result back. This note says how that works when BAML is compiled to machine code: what is decided, what is built, and what comes next.

Status, on branch `antoniosarosi/native-bridges-poc`, step 1:

| Part | State |
|---|---|
| The compiled program as a shared library, and the C functions it exports | Built, behind the runtime feature `host-lib` |
| A Rust host that loads the library and awaits calls | Built, as an example |
| Reading any exported value out and building any argument in, checked | Built |
| The library's interface as data: numbers, fields, variants, hash (`native_pack --lib --describe`) | Built |
| Copying results into host objects | Built for Rust: the crate `baml_native_host`, and bindings for the type-shapes fixture written by hand as a generator would emit them |
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
 │ small support library (per language)  │  built for Rust: baml_native_host
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

`baml.panics.Exit` does not end the host's process: from the call's own task it is status 3, from a task the call spawned a failure like any other. Two things in BAML code still can end it; they are in the limits table.

A task the call spawned that fails, with nothing awaiting it, is reported through `on_unhandled` once **the last task of its call has ended**, because nothing can await it after that, or in the turn that reports the call stuck. The task is then freed. A sibling that awaits the failed task after the call returned takes the error, and nothing is reported. Each call has a record of its live tasks, found from the task without hashing; nothing scans every task per turn.

## The exported functions (built)

In `src/sched/host.rs` and `src/sched/host/`, feature `host-lib`. The library exports these and nothing else. Every function that takes a handle checks it: in range, holding an object, of the right kind. A wrong one gets the function's error value, never undefined behaviour, and `baml_lib_last_error` says why. No function consumes a handle: the host frees every handle it was given. Called before `baml_lib_init`, a function returns its error value.

A freed handle's slot is reused. A handle kept after it was freed may then name another object: the checks keep that safe (the kind and type checks still run), but they do not detect it. There are no generations.

| Function | Does | May collect |
|---|---|---|
| `baml_lib_abi_version()` | The version of this interface (2) | no |
| `baml_lib_init(callbacks)` | Builds the worker, the handle table and the interface. `callbacks` starts with its own size; a struct too small is refused | no |
| `baml_lib_find(name, len)` | The number of an exported function (`complex_models.round_trip_complex_profile`), or -1 | no |
| `baml_lib_export_count()`, `baml_lib_export_info(...)` | Per export: arity, signature hash, signature text | no |
| `baml_lib_interface_hash()` | One hash for the whole exported interface | no |
| `baml_lib_interface_json(buf, cap)` | The interface as JSON, as `--describe` prints it; readable before init | no |
| `baml_lib_call_start(fn, args, n)` | Checks the arguments against the parameters, queues a task for the call; returns the call id, or 0 | yes |
| `baml_lib_turn()` | Runs every runnable task, then reports what ended | yes |
| `baml_lib_last_error(buf, cap)` | Why the last refusal refused | no |
| `baml_lib_string_new`, `_string_len`, `_string_copy` | Make a string; read one by copying it out | `new`: yes |
| `baml_lib_class_of(h)` | The number of an instance's class, or of an enum value's enum, in the interface | no |
| `baml_lib_field_get(h, field, out)` | A field by its position in declaration order | no |
| `baml_lib_array_len(h)`, `_array_get(h, i, out)` | An array's length and elements | no |
| `baml_lib_map_len(h)`, `_map_next(h, cursor, key, value)` | A map's length; its entries in insertion order | no |
| `baml_lib_variant_index(h)`, `_variant(enum, index, out)` | An enum value's position; the value at a position | no |
| `baml_lib_instance_new(class, fields, n, bad_field)` | An instance, all fields at once, each checked against its field's type | yes |
| `baml_lib_array_new(values, n)` | An array, all elements at once | yes |
| `baml_lib_map_new(keys, values, n)` | A map from strings the host made, all entries at once | yes |
| `baml_lib_handle_tag`, `_describe`, `_handle_free` | An object's tag; its text form; release a handle | no |
| `baml_lib_collect`, `_debug_address`, `_debug_task_count`, `_debug_handle_count` | Test aids | yes / no / no / no |

Reads return objects as new handles: reading a field that holds an instance gives a handle to it, which the reader frees.

## Values across the boundary (built)

A value is two words, `{tag, payload}`, the same 16 bytes a heap slot holds.

| What | Tag | Payload |
|---|---|---|
| `int`, `float`, `bool`, `null` | As in memory, for example `INT` = 2 | The value itself |
| Any object | Its real tag (`STRING`, `INSTANCE`, ...) with bit 61 set | A **handle** |
| An argument | As above. A scalar's tag must have no other bit set; a `bool` is 0 or 1; `null` and `OMITTED` carry 0. `OMITTED` is taken only for a parameter with a default | |

A handle is a slot in a table that grows by chunks of 4096 slots. A chunk never moves and never goes away. The collector walks every chunk (`heap.rs`, `copy_into`) and rewrites a slot when its object moves. So a handle stays valid until it is released, across any number of collections. Slot zero is never handed out: zero is what every function returns when it refuses.

```
 bindings ──▶ handle slot ──▶ Person      (the slot is rewritten when Person moves)
```

## Checked writes (built)

Compiled code trusts static types: a slot typed `int` that held a string would be read as a number, and a slot typed as a class that held an `int` would be read as an address. So a value from the host is checked where it lands in a typed place, and refused if it does not fit:

- `call_start` checks each argument against its parameter's type, and `instance_new` each field against its field's type.
- The check is deep, against the type descriptions the compiler leaves in the library (`json_layout.rs`), read once at init: scalars by tag, string literals by text, enum values by their enum, optionals and unions by trying each member, arrays and maps element by element.
- An instance is checked by its class alone. Every instance in memory was made by compiled code or by a checked `instance_new`, so its fields are already right. Generic classes are refused when packing, because `Box<string>` and `Box<int>` have one class.
- **An array or a map is accepted once**, right after `array_new` or `map_new` made it, and never again. One passed twice in the same call, instance or array is refused. A refused call takes nothing: its containers can still be passed. BAML arrays and maps are invariant (`int[]` is not a `(int | string)[]`), but one array landing in two places could still be written through one place's type and read through the other's. The host cannot change a container after making it.

Every build function checks all its inputs first, then allocates once, then turns handles into addresses: an allocation can move every object, and the handles are where the collector leaves the new addresses.

## The interface as data (built)

`interface.rs` in the compiler turns the exported functions into plain data: their numbers, names, parameters (name, type, whether it has a default), result and throws type, and every class and enum the signatures reach, numbered, with fields in slot order and variants in order. One hash covers all of it. The table of exports is emitted from it, and `native_pack --lib --describe` prints it as JSON for bindings.

- `native_pack --lib <dir> <out>` takes a directory as the source root and exports every function that can be exported. Each one that cannot is printed with the reason, for example `a generic class 'generics.Box'` or `a bigint`. A list of names exports just those, in that order.
- A type the boundary cannot carry is refused when packing, naming the function and the type: bigint, uint8array, tuples, functions, generic classes, classes of the standard library, maps whose keys are not strings, recursive aliases, `json`, and classes the library cannot describe (fields with `@json.flatten` or `@json.fallback`, a class's own `to_json` or `from_json`).
- The interface's JSON is in the library too, read with `baml_lib_interface_json`.
- A thrown value is only read, never built, so a throws type that cannot cross does not block a function: its errors arrive described, without a type.
- The table records each function's and each class's number of type parameters, zero for now. Type arguments are to travel first in the values a call or an instance takes.

## Inside the library (built)

| Piece | Where | What it is |
|---|---|---|
| Library form | `baml_compiler2_native`: `native_pack --lib`, `link.rs` | A `.dylib` linked against a runtime archive without `main` |
| Export entry | `codegen/host.rs` | Per exported function, an entry with the C convention. It reads the arguments, calls the function under a catch-all, and returns the result or what was thrown |
| Host shim | `codegen/host.rs` | Compiled code that every host-started task begins in, so no Rust frame sits under a parked task |
| Export table | `codegen/host.rs` | Per function its entry, hash, name, signature and the descriptions of its parameters, result and throws type; per class or enum its class object and description |
| The interface | `interface.rs` | The numbers and the hash, as plain data |
| Values and checks | `sched/host/values.rs`, `types.rs`, `table.rs` | Reads, builds, the check against descriptions, the handle table |
| The turn | `sched/host.rs` | `serve`'s loop, returning where `serve` would sleep |

How one call runs:

1. `call_start` puts the arguments in a BAML array held by the new task, where the collector can see them.
2. The task starts in the host shim, which calls the export entry.
3. When the function ends, the result is recorded. Nothing calls the host from a task's stack.
4. After the run, `turn` reports what ended: each finished call, each call found stuck, each failure nobody awaits.

The call id lives in the task (`wait_value`), and every task the call spawns inherits it. The entry's address waits in the first slot of the argument array. So starting a call allocates nothing but the array and, when none is free, a task.

## Copying a result (built for Rust)

Generated code knows each type, so the copy is straight-line code: read each field once, build the host object. The numbers it uses come from `--describe`; the interface hash makes a library and bindings of different versions refuse each other at load. Arguments go the other way, bottom-up, each part a handle until it is built into its parent.

The crate `baml_native_host` is in two layers.

**The core** is what any host builds on: a Python or Node module written in Rust drives it from its own event loop. It has no executor types and no thread of its own.

| Piece | What it is |
|---|---|
| `Library::load(path, hash, wake)` | Opens the library, refuses one whose interface hash is not the bindings', builds its worker. `wake` is called from any thread when the library wants a turn. One library per process: the same path again returns it, another path is an error |
| `Library::exports()`, `interface_json()`, `find(name)` | The interface as data: the exports, and the JSON `--describe` prints |
| `Library::session()` | The library's lock, held by this thread: a `Session`. Neither `Send` nor `Sync`, nor is a `BamlValue` |
| `Session::start(export, args)` | Starts a call from values built in the session; its id |
| `Session::turn(each)` | Runs a turn, then hands `each` every `Event` that ended (a call with its `Outcome` and value, or an unhandled failure) while the session still holds the library, so the values are copied on the spot. Returns how long until the next BAML timer. Allocates nothing when nothing ended |
| `BamlValue`, `ToBaml`, `FromBaml`, `Session::instance`, `array`, `map`, `string`, `variant` | Reading and building values. A `BamlValue` frees its handle when dropped |
| `BamlValue::detach`, `Session::attach` | Carry a value past its session; dropped without attach, its handle is freed at the next session |

**The futures layer**, `baml_native_host::futures`, is built on the core's public functions alone, which shows the core is enough.

| Piece | What it is |
|---|---|
| `futures::load(path, hash)` | `Library::load` with this layer's `wake`, and its timer thread (one per process) |
| `futures::call(library, export, args)` | Builds the arguments and starts the call. Returns a `Call<T, E>`, the future of `Result<T, Error<E>>` |
| `futures::set_on_unhandled(hook)` | What is done with an unhandled failure, given as text. The default prints it to stderr |
| `Error<E>` | `Thrown` (the declared type), `Runtime` (anything else thrown, described), `Panic`, `Stuck`, `Sdk` |

How calls are driven: a `Call` that is polled gives the library a turn when one is owed (a call started, `wake` came, a timer fired), on the polling thread, under the library's lock. So any executor drives the calls, a multi-thread tokio runtime included, and BAML runs on whichever thread polls, one at a time. The runtime has no state tied to a thread: no thread-locals, no signal stack, and the worker's register and stack are set on every resume.

- When a turn becomes owed, **one** waiting call is woken. If that call is dropped, or ends, while the turn is still owed, it wakes another in its place.
- Nothing of the user's runs while the library is held. Wakers are woken, and the unhandled hook is called, after the session is dropped. The hook gets the error as text, may call any binding, and a panic in it is caught.
- A `Call` that is dropped does not cancel the BAML call: its result is freed when it arrives.

The bindings for the type-shapes fixture are in `baml_native_host/tests/type_shapes/bindings.rs`: plain structs, enums and union enums with the names `sdkgen_rust` emits, and one async function per export under its own name, with its arguments taken as `sdkgen_rust` takes them (today's SDK also emits a `_sync` twin, which these leave out).

## Python (built)

The crate `baml_native_python` is a pyo3 module, `baml_native._native`, built on the core alone, with a small Python package `baml_native` in front of it. A generated package uses it in place of today's runtime: it gives `load`, `set_type_map` and `define_function` with the signatures `baml_bridge` has.

```
 baml_sdk (today's generated package)      classes, enums, signatures unchanged
        │   define_function(...) from baml_native instead of baml_bridge
 ┌──────▼─────────────────────────────────┐
 │ baml_native._native (pyo3)            │  plans, the loop, calls, errors
 ├───────────────────────────────────────┤
 │ baml_native_host core                 │  Library, Session, BamlValue, Interface
 └───────────────────────────────────────┘
```

| Piece | What it is |
|---|---|
| `load(path, hash)` | `Library::load`, then a plan for each type of `Library::interface()`, the interface as a typed tree. The thread that loads the library is the only one that may call it |
| `set_type_map(tm)` | The generated type map. A class or enum finds its Python class through it the first time it is used. Fields are matched to pydantic attributes by alias, then name |
| `define_function(fqn, mode, ...)` | Today's signature. Async gives a coroutine object, whose arguments are read when it is first awaited, as an `async def` reads them; never awaited, it warns as a coroutine does. Sync gives a blocking call. A function not in the library's interface still imports, and raises `NotImplementedError`. It does not bind `self`: methods installed on a class are not supported (none packs today) |
| Results | One native call per result: a walk over the core's reads that builds `int`, `str`, `list`, `dict`, enum members and models directly. A union picks its member by tag when one member alone has it, otherwise by `class_of` or by every element of a list or map. A model that allows extra fields, or has private attributes, validators or a `model_post_init`, is made with `model_validate` |
| Arguments | Built by the parameter's declared type, by today's encoder's rules (`bridge_encode`, Python): `bool` is not an `int`; an `int` fits a `float` only when exact; an enum member is a variant of its own enum only, told apart before the `str` it mixes in; a `str` names a variant; a `dict` whose keys are fields stands in for a class (a missing field is null where null is allowed). Under a union: rank by shape (identity, natural, conversion), then one type, then the value's own type, then fewest omitted fields, else `TypeError` ("fits more than one"). A `dict` argument is walked from a snapshot of its items |
| Errors | Today's classes, from `baml_bridge.errors`. A throw becomes `BamlError(value, class_name="user.<class>")`, typed when the declared throws type fits. A panic becomes `BamlPanic`. A refusal becomes `TypeError` with the library's reason |
| `set_unhandled_spawn_error_handler` | Today's contract and default (print, then exit). It is called after the turn that found the failure, with nothing held |

How it joins asyncio:

- The library's `wake` comes from its I/O thread, without the GIL. It sets a flag and writes one byte to a socket pair, as today's bridge does. The loop watches the socket (`add_reader`), and a turn runs on the loop's thread. The flag is cleared before a turn, so a wake that comes during one asks for another.
- A BAML timer becomes a `call_later` timer of the joined loop, armed from every turn that returns one, so a task spawned by a call that already returned still runs when it sleeps. A call that ends in its first turn and leaves a timer set joins the running loop for it. With no loop at all (sync calls only), such tasks advance during later calls; a returned call's task that waits only on I/O, with no timer, advances when a loop is joined or at the next call.
- An async call is a coroutine object (`__await__`, `send`, `throw`, `close`), so `await`, `asyncio.run`, `create_task`, `gather` and `wait_for` take it. Its first step starts the call and runs a turn. A call that ends in that turn returns there, with no future and no trip around the loop. Otherwise it waits on a future of the joined loop.
- Cancelling the awaiting task does not stop the BAML call: its result is freed unread when it arrives.
- A sync call turns until its call ends. Between turns it waits for `wake` or the next timer with the GIL released, in slices of 50 ms that look for signals, so Ctrl-C ends it; the call is then abandoned, and no wake is lost. It delivers the results of other calls that end meanwhile. A call's result belongs to its call id from the start, whichever turn delivers it (a sync call made from the unhandled-error handler, say).
- A panic while a result is built becomes that call's exception; the other results of the turn are still delivered and every handle is freed. A future that cannot take its result (cancelled meanwhile, or of a closed loop) stops nothing else.
- One loop at a time. A call that must wait on another loop moves the reader there when the old loop is closed or has nothing in flight. Otherwise it is a `RuntimeError`. A call from another thread is a `RuntimeError`, as is a call made from Python code run while a result is being built. Neither is ever a hang.
- The GIL is released while BAML runs in a turn, and taken again for each result copied.

What differs from today's SDK:

- `baml.panics.Exit` raises `BamlPanic`. It does not `os._exit`.
- A stuck call raises `BamlPanic` with `baml.panics.Stuck`. Today's VM has no such error: such a call never ends.
- Arguments may nest 200 levels; the protobuf wire allowed 32.
- No `_baml=` options, cancellation of the BAML call, streams, or generics.
- Argument rules are today's, and checked against today's SDK by the same tests (`test_arguments.py` runs against both). Argument errors come at `await`, as today: a missing argument is `BamlError` with `baml.errors.InvalidArgument`, too many positional arguments a `TypeError`.

The bindings are today's generated package for the type-shapes fixture plus `bridge_test`, with 81 lines changed, all of them imports or the init call. `tests/type_shapes/setup.sh` makes them and lists them. Today's 136 tests for the fixture run against them unchanged: 79 pass and 57 are skipped. 51 of the skips are tests that call a function not in the library's interface, whatever they catch. 6 need standard-library handles (4), media (1), or the wire's 32-level limit (1). The crate's own 50 tests pass too (10 of them run against today's SDK as well, with the same answers). All of it passes again under `BAML_GC_STRESS=1`, and with models made by `model_validate`.

On Apple M5 Max, Python 3.13, through `perfq --bench` (#1170, #1172 to #1177). Each number is the median of 3 or 5 interleaved rounds, each round the median of 3 runs. Outside load was 0.33 to 0.48 cores. "Today's" is the generated Python SDK of the same sources over the VM (`baml_bridge`, release). Native models are made by the native construct (below), and the GIL is released.

| One call, one after another | Today's, async | Native, async | Today's, sync | Native, sync |
|---|---|---|---|---|
| `round_trip_int` | 43.7 µs, 365,662 instr. | 291 ns, 8,151 | 29.0 µs, 321,451 | 201 ns, 5,700 |
| `round_trip_inner` | 56.3 µs, 535,869 | 713 ns, 20,157 | 38.5 µs, 487,947 | 621 ns, 17,509 |
| `round_trip_complex_profile` | 478 µs, 9,547,442 | 19.6 µs, 465,552 | 456 µs, 9,477,652 | 19.1 µs, 461,686 |

| Arrays, sync, per element | Today's | Native | Pure Python, same objects |
|---|---|---|---|
| `int[]` of 100,000 out | 326 ns | 79 ns, 502 instr. | 13 ns |
| `int[]` of 100,000 in | 534 ns, 9,359 instr. | 13 ns, 128 | 7 ns |
| `Point[]` of 10,000 out | 3,350 ns | 343 ns, 6,488 | 326 ns |
| `Point[]` of 10,000 in | 5,316 ns, 136,673 | 97 ns, 1,682 | 37 ns |

Instructions come from running each case at n and 2n. For the "in" rows they are net of building the argument, which the pure-Python run does too.

After the review's fixes (#1237 to #1245) the Mac's outside load was 0.74 to 1.17 cores, too much to quote times. Instruction counts, which do not depend on it, are within about 1% of the table above (profile 467,007 sync, `Point[]` in 13,074 gross). The times agree with it too, and so do their ratios to today's SDK, which each job measures interleaved: sync `round_trip_int` 209 ns, inner 638 ns, profile 19.2 µs, `Point[]` in 98 ns.

How a model is made decides most of what is left. Today's SDK validates, so the table below shows like for like: the native bridge making models three ways, beside pure Python building the same objects each way with no bridge (#1172, #1175, #1177, sync).

| | Today's (validates) | Native: validate | Native: `model_construct` | Native: native construct | Python: validate | Python: `model_construct` | Python: native construct |
|---|---|---|---|---|---|---|---|
| `round_trip_inner` | 38.5 µs | 822 ns | 1,067 ns | 621 ns | 317 ns | 537 ns | 223 ns |
| `round_trip_complex_profile` | 456 µs | 26.9 µs | 34.0 µs | 19.1 µs | 27.2 µs | 35.0 µs | 20.7 µs |
| `Point[]` out, per element | 3,350 ns | 550 ns | 800 ns | 343 ns | 428 ns | 695 ns | 326 ns |

- With validation kept, the profile goes from 456 µs to 26.9 µs. That is the bridge alone, 17×, and it lands on what pydantic validation costs with no bridge at all (27.2 µs). Skipping validation takes it to 19.1 µs.
- `model_construct` is slower than validating: it is Python code. The native construct does what it does, from Rust: `object.__new__(cls)`, then `__dict__`, `__pydantic_fields_set__`, `__pydantic_extra__` and `__pydantic_private__` set with the generic setter. It depends on those four pydantic v2 slots, and on the generated models having no private attributes and no validators that change values. It is the default.
- The GIL (#1185, #1183, #1184, #1181): releasing it costs 27 to 30 ns, about 1,200 instructions, on a call that ends at once, and nothing measurable on `make_ints(1,000,000)` or on the profile. While a thread made long calls, another Python thread ran about 14 times as many iterations with the GIL released (37 M held, 505 to 538 M released). It is released by default.

## Measured

On the Rust host, Apple M5 Max, through `perfq --bench` (#1092): the median of eight interleaved rounds, outside load 0.33 cores. Instructions per call come from running each case at two sizes. "Today's SDK" is the generated Rust SDK of the same fixture over the VM (`bridge_cffi`, release), called the same way.

| One call, one after another | Native: instructions | Native: time | Today's SDK: instructions | Today's SDK: time |
|---|---|---|---|---|
| `add(int, int)` through the C functions alone | 1,037 | 39 ns | | |
| `add(int, int)` through the bindings | 2,772 | 95 ns | | |
| `round_trip_int(int)` | 2,619 | 89 ns | 212,053 | 21.4 µs |
| `round_trip_inner(Inner)`, a class of one `int` | 4,646 | 160 ns | 265,468 | 26.0 µs |
| `round_trip_complex_profile`, the SDK tests' nested profile | 238,303 | 9.3 µs | 4,214,833 | 213 µs |

| Arrays, per element | C calls per element | Instructions | Time |
|---|---|---|---|
| `int[]` out of BAML, 100,000 to 600,000 | 1 (`array_get`) | 240 | 99 ns, mostly BAML building the array and first touches of new memory |
| `int[]` into BAML | 0 (one `array_new`) | 203 | 8.8 ns |
| `int[]` round trip | 1 | 331 | 14 ns; today's SDK 4,281 and 342 ns |
| `Point[]` out, 10,000 to 60,000 | 5 (`array_get`, `class_of`, two `field_get`, a free) | 1,066 | 88 ns |
| `Point[]` in | 2 (`instance_new`, a free) | 1,942 | 63 ns |

- The C surface costs 1,037 instructions per `add`: 1,680 in the spike, 953 once the per-call map went, 84 more for the per-call record of live tasks that makes the unhandled rule exact.
- The bindings add about 1,700 more: the lock, the map of waiting calls, the executor. Splitting the crate first cost 4,752; keeping the events in a cell under the lock, skipping the deferred frees when there are none, passing few arguments from the stack, hashing call ids by a multiplication and waking one call without a vector took it to 2,772, under the 3,910 before the split.
- Reading costs one C call per field and per element, with a handle for each object read. Bulk reads are not built; the per-element numbers above are what they would save.
- With 12,000 calls started before the first turn, each costs about 20,000 instructions: each waiting task maps a fresh stack.

## Limits today

| Limit | Consequence |
|---|---|
| One worker per process | Two BAML libraries cannot be loaded together. Calls into the library must come one at a time; the Rust crate's lock enforces it, the C functions do not |
| A turn runs on the thread that polls, holding the lock | A long BAML computation blocks that executor thread, and every other call waits for the lock |
| Generic classes and functions with type parameters are refused when packing | Type arguments would have to cross; the table leaves room for them |
| bigint, uint8array, tuples, functions, `json`, maps whose keys are not strings, classes of the standard library do not cross | Refused when packing, with the reason |
| An array does not record its element type | A result typed `int[] \| string[]` cannot be told apart when it is empty |
| An array or a map from the host can be passed once | Bindings build a fresh one for each place, which is what copying does anyway |
| The trace of a throw is dropped | Errors arrive as a value and its text |
| A stuck call's tasks stay parked for the life of the process | A task parked on I/O may still be named by the job it waits for, so it is not freed |
| Two calls cannot share state through the program | A global is a `const`, and reading one is not compiled natively yet. When it is, a semaphore shared by two calls could release a call reported stuck, and its result would be dropped |
| A `spawn` with semaphores or cancel tokens ends the host's process | It reaches `baml_rt_spawn_unsupported`, which exits; making it fail the call needs a throw from that stub, not built |
| A stack overflow in a frame too large to throw from ends the host's process | As in an executable (`unwind.rs`) |
| A handle kept after it is freed may name another object | Safe, but not detected: no generations |
| Describing a value stops 32 objects deep | A value that holds itself is written with a marker, `<nested too deep>`; executables print uncaught throws the same way |
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
