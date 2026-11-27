# Bridges: calling compiled BAML from Python, Node and Rust

A bridge lets a program in another language call a BAML function and get a typed result back. This note says how that works when BAML is compiled to machine code: what is decided, what is built, what it costs, and what comes next.

Status, on branch `antoniosarosi/native-bridges-poc`:

| Part | State |
|---|---|
| The compiled program as a shared library, and the C functions it exports | Built, behind the runtime feature `host-lib` |
| Reading any exported value out and building any argument in, checked | Built |
| The library's interface as data, with a typed tree of every type (`native_pack --lib --describe`) | Built |
| A support library per language | Built: `baml_native_host` (Rust; its core layer is what the other two build on), `baml_native_python` (pyo3), `baml_native_node` (Node-API) |
| Bindings for the type-shapes fixture, in all three languages | Built by hand, in the shape today's generators emit. The generators are not changed |
| Generators, LLM functions, generics, streaming, cancellation, calls from BAML into the host | Next |

## The decision

**Results are copied.** When a call returns, its result is copied once, directly from BAML memory, into an ordinary host object: a pydantic model, a plain TypeScript object, a Rust struct. There is no protobuf and no other wire format in between.

User code does not get references into BAML memory. That idea, and why it waits, is in [ideas/host-handles-and-refs.md](ideas/host-handles-and-refs.md). In short:

- For the usual case, where the host reads most of a result, a copy is as fast or faster in Python and Node.
- Users keep the objects they have today, and nobody has to choose between two kinds of result on every call.
- The cost of today's bridges is converting twice through protobuf, not the copy. This design converts once.

What travels through the C functions is a **handle**: a token for an object that stays inside the runtime, with functions to read it, as in CPython's C API or SQLite's `sqlite3_column_*`. Handles are used only inside the bindings; user code never sees one. The two other styles do not fit:

| Style | What crosses | For BAML |
|---|---|---|
| Message | One generic call carrying a serialized blob (today's bridges, protobuf) | Two conversions per value: BAML to bytes, bytes to host object |
| Struct | A C struct per type, described in a header | The collector moves objects, a field is two words (tag and payload), a string is not a `char*`, a list is not a C array; each object would be copied into a C struct first |
| **Handle** | A token, and functions to read and build | One conversion: read the object where it is, build the host object |

## The layers

```
 your Python / TypeScript / Rust code         unchanged: today's classes and functions
        │   greet(person: Person) -> str
 ┌──────▼────────────────────────────────┐
 │ bindings (per program)                │  today's generated code; only the runtime it imports changes
 ├───────────────────────────────────────┤
 │ support library (per language)        │  baml_native_host · baml_native_python · baml_native_node
 ├───────────────────────────────────────┤
 │ baml_native_host core                 │  Library, Session, BamlValue, Interface
 ├───────────────────────────────────────┤
 │ C functions exported by the library   │  baml_lib_*
 ├───────────────────────────────────────┤
 │ compiled BAML + the Rust runtime      │  one .dylib
 └───────────────────────────────────────┘
```

What the user writes does not change:

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

## How a call works

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

## The C functions

### What the library exports

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

### Values across the boundary

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

### Checked writes

Compiled code trusts static types: a slot typed `int` that held a string would be read as a number, and a slot typed as a class that held an `int` would be read as an address. So a value from the host is checked where it lands in a typed place, and refused if it does not fit:

- `call_start` checks each argument against its parameter's type, and `instance_new` each field against its field's type.
- The check is deep, against the type descriptions the compiler leaves in the library (`json_layout.rs`), read once at init: scalars by tag, string literals by text, enum values by their enum, optionals and unions by trying each member, arrays and maps element by element.
- An instance is checked by its class alone. Every instance in memory was made by compiled code or by a checked `instance_new`, so its fields are already right. Generic classes are refused when packing, because `Box<string>` and `Box<int>` have one class.
- **An array or a map is accepted once**, right after `array_new` or `map_new` made it, and never again. One passed twice in the same call, instance or array is refused. A refused call takes nothing: its containers can still be passed. BAML arrays and maps are invariant (`int[]` is not a `(int | string)[]`), but one array landing in two places could still be written through one place's type and read through the other's. The host cannot change a container after making it.

Every build function checks all its inputs first, then allocates once, then turns handles into addresses: an allocation can move every object, and the handles are where the collector leaves the new addresses.

### The interface as data

`interface.rs` in the compiler turns the exported functions into plain data: their numbers, names, parameters (name, type, whether it has a default), result and throws type, and every class and enum the signatures reach, numbered, with fields in slot order and variants in order. One hash covers all of it. The table of exports is emitted from it, and `native_pack --lib --describe` prints it as JSON for bindings.

- `native_pack --lib <dir> <out>` takes a directory as the source root and exports every function that can be exported. Each one that cannot is printed with the reason, for example `a generic class 'generics.Box'` or `a bigint`. A list of names exports just those, in that order.
- A type the boundary cannot carry is refused when packing, naming the function and the type: bigint, uint8array, tuples, functions, generic classes, classes of the standard library, maps whose keys are not strings, recursive aliases, `json`, and classes the library cannot describe (fields with `@json.flatten` or `@json.fallback`, a class's own `to_json` or `from_json`).
- The interface's JSON is in the library too, read with `baml_lib_interface_json`. Beside each type's text it carries the same type as a tree (`"int"`, `{"literal": 42}`, `{"class": 3}`, `{"enum": 1, "variant": 0}`, `{"list": T}`, `{"map": T}`, `{"union": [...]}`), so no host parses type text.
- A thrown value is only read, never built, so a throws type that cannot cross does not block a function: its errors arrive described, without a type.
- The table records each function's and each class's number of type parameters, zero for now. Type arguments are to travel first in the values a call or an instance takes.

### Inside the library

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

## Rust (built)

**What the user writes.** Today's generated Rust SDK: plain structs, enums and union enums, one async function per export.

```rust
let profile = complex_models::round_trip_complex_profile(&profile).await?;
```

**What is underneath.** The crate `baml_native_host`, in two layers.

**The core** is what any host builds on: a Python or Node module written in Rust drives it from its own event loop. It has no executor types and no thread of its own.

| Piece | What it is |
|---|---|
| `Library::load(path, hash, wake)` | Opens the library, refuses one whose interface hash is not the bindings', builds its worker. `wake` is called from any thread when the library wants a turn. One library per process: the same path again returns it, another path is an error |
| `Library::interface()` | The interface as a typed tree, parsed once at load: `Interface` (functions, types), `Function` (params, result, throws), `TypeDef` (class fields or enum variants), `Type` (scalars, literals, class or enum by number, enum variant, list, map, union). Bindings build their plans from it |
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

The bindings for the type-shapes fixture are in `baml_native_host/tests/type_shapes/bindings.rs`: the names `sdkgen_rust` emits, arguments taken as it takes them. Each class is copied by straight-line code: read each field once, build the struct.

**What differs from today's SDK.**

- No `_sync` twin.
- Errors are `baml_native_host::Error<E>`: `Thrown` (the declared type), `Runtime` (anything else thrown, described), `Panic`, `Stuck`, `Sdk`.
- No per-call options, cancellation, streams or generics.

## Python (built)

**What the user writes.** Today's generated package, unchanged: pydantic classes, enums, `f(...)` and `f_sync(...)`.

```python
o = await round_trip_outer(Outer(inner=Inner(value=9)))
p = round_trip_complex_profile_sync(profile)
```

**What is underneath.** The crate `baml_native_python`: a pyo3 module, `baml_native._native`, on the core alone, and a small package `baml_native` that gives generated code `load`, `set_type_map` and `define_function` with the signatures `baml_bridge` has.

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

The bindings are today's generated package for the type-shapes fixture plus `bridge_test`, with 81 lines changed, all of them imports or the init call. `tests/type_shapes/setup.sh` makes them and lists them. Today's 136 tests for the fixture run against them unchanged: 79 pass and 57 are skipped. 51 of the skips are tests that call a function not in the library's interface, whatever they catch. 6 need standard-library handles (4), media (1), or the wire's 32-level limit (1). The crate's own 50 tests pass too (10 of them run against today's SDK as well, with the same answers). All of it passes again under `BAML_GC_STRESS=1`, and with models made by `model_validate`.

**What differs from today's SDK.**

- `baml.panics.Exit` raises `BamlPanic`. It does not `os._exit`.
- A stuck call raises `BamlPanic` with `baml.panics.Stuck`. Today's VM has no such error: such a call never ends.
- Arguments nest at most 200 levels; today's wire refuses at 33.
- No `_baml=` options, cancellation of the BAML call, streams, generics, or instance methods.
- Argument rules are today's, checked against today's SDK by the same tests (`test_arguments.py` runs against both). Argument errors come at `await`, as today.
- The standard library's handles (`baml.fs`, `baml.http`) and media classes still come from today's `baml_bridge`, which stays installed for them, for the error classes and for the type map.

## Node (built)

**What the user writes.** Today's generated TypeScript package, unchanged, `_sync` twins included.

```ts
const o = new Outer({ inner: new Inner({ value: 9 }) });
const r = await round_trip_outer(o);       // an Outer, as today
const s = round_trip_outer_sync(o);
```

**What is underneath.** The crate `baml_native_node`, a Node-API addon on the core, and `baml_native_node/js`, the runtime generated SDKs import as `@boundaryml/baml-bridge`.

| Piece | What it is |
|---|---|
| The addon (`src/lib.rs`) | A Node-API module written against the C functions directly (`napi-sys`, Node-API version 10). It loads the library through `Library::load`, checks the interface hash, and holds the plans. It belongs to the first JavaScript environment that loads the library |
| Plans (`src/copy.rs`) | Built at the first call, from `Library::interface()`: per export its parameter types and result plan; per class its field names (kept as JavaScript strings), field plans and generated class; per enum its generated members. Built again after `setTypeMap`, deleting the old references |
| Arguments (`src/args.rs`) | Builds a call's arguments by the parameters' declared types, by today's encoder's rules (below) |
| The runtime (`js/index.js`) | Exports the names `@boundaryml/baml-bridge` exports. `defineFunction` (async and sync), the type map and the error classes are real. The rest (instance methods, media, streams, tracing, `$baml` options) are stubs that throw when used, so a whole generated SDK imports |
| The bindings | Today's generated package, unchanged except `_inlinedbaml.ts`, which names the library and its hash instead of embedding bytecode, and two lines of `index.ts` (the initializer's import and call: `initializeNativeLibrary(_inlinedbaml.LIBRARY, _inlinedbaml.INTERFACE_HASH)`). Its `package.json` points `@boundaryml/baml-bridge` at the native runtime |

How Node's loop is joined:

```
JavaScript (main thread)                      the library
------------------------                      -----------
f(args)  ── start(export, args) ─▶ build args by declared type, call_start
queueMicrotask ── turn() ───────▶ run BAML
         ◀── [delay, pending, id, status, value, ...] ── results copied in the turn
setTimeout(delay) ── turn() ─────▶ a BAML timer is due
onWake() ── turn() ─────────────▶ ◀── wake() from the I/O thread, through a thread-safe function
f_sync(args) ── start + drive(id) ▶ turns until call id ends, sleeping on the timer or a wake in between
```

- BAML runs on Node's main thread, inside `call`, `turn` or `drive`. A call schedules one turn by `queueMicrotask`, so calls started together share a turn. With `BAML_NATIVE_TURN=eager` it runs a turn at once instead (`call`); the two cost the same per call (below).
- The thread-safe function is referenced only while calls are pending, and the timer for the next BAML timer is unreferenced when none is: a script whose last call has ended exits, even with a BAML task still sleeping.
- `drive` hands back every event of its turns, so calls that end while a `_sync` call waits are settled too.
- An entry point entered while another holds the library (a constructor run while a result is copied, or a getter run while arguments are read, that calls a binding) throws instead of waiting for the core's lock. The async and the `_sync` form throw the same `BamlClientError`.
- The addon belongs to the first environment that loads the library. A `worker_threads` worker, or a second copy of the addon, gets a `BamlClientError` from every entry point and touches no shared state; a second copy of the JavaScript runtime in the same thread is refused at load, so it cannot take the first one's events.
- When that environment is torn down, a cleanup hook added after the thread-safe function (so it runs before Node frees it) releases it, under the lock a wake takes: a wake after that does nothing. A process exits cleanly with a spawned task's I/O or timer in flight.

Copying: a turn copies each result in the same native call, by its export's plan, through the core's reads (`field`, `elements`, `entries`, `variant_index`, `string`). A class instance is built by one call into a small per-class JavaScript function made at bind, `(v0, v1, ..) => { const o = new C(E); o.f0 = v0; ..; return o }`; that was the fastest of three ways measured. An `int` becomes a number, a map a null-prototype object, an enum value its generated member, a union by its value's tag.

Arguments are built by the parameter's declared type, by the rules of today's encoder (`bridge_encode`'s planner with its JavaScript rules), and `js/parity` checks the same cases against both bridges:

- a class takes an instance of its own generated class (found by a symbol `bind` sets on each generated prototype) or a plain object; only own properties are read, an unknown key is refused (`class user.bridge_test.Point has no field \`z\``), and a missing nullable field is `null`;
- a map takes a plain object, not a `Map`; an `int` takes a whole number or a `bigint` in range;
- a union ranks its members by how the value stands to each: its own type, then its structural type (an array as a list, a plain object as a map), then a conversion (a whole number as a `float`, a string as an enum value, a plain object as a class). Every member of the best rank that takes the value is built. A tie goes to the value's natural type, then to the member that read the fewest fields as `null`, else the value is refused as ambiguous. So a string stays a string under `Mood | string`, `1` is the literal under `float | 1 | 2`, and `{text}` is a `Tag` under `Note { text, meta? } | Tag { text }`;
- arguments and results nest at most 200 levels of classes, lists and maps. Deeper, or cyclic, is refused: ``the value is nested too deeply for the boundary, which carries 200 levels``. Today's bridge refuses earlier, at its wire's limit: a chain of 33 classes.

**What differs from today's SDK.**

How a call ends, in JavaScript:

| End | Error | As today's SDK |
|---|---|---|
| returned | the promise resolves, the `_sync` function returns | yes |
| threw | `BamlError` (`.value` the thrown value, a declared class as its generated class; `.className`; message `baml error: <class>[: <message>]`) | yes, without `.bamlTrace` (traces are dropped) |
| panicked | `BamlPanic`, message `baml panic: <class>[: <message>]` | the same message and `.className`; `.value` is the value's text (`baml.panics.DivisionByZero {dividend: 1}`), not an object, because the standard library's classes are not in the interface |
| stuck | `BamlPanic`, `baml panic: the call can never finish: ...` | today's SDK never settles such a call |
| refused, by the bindings or the library | a plain `Error` with the code `InvalidArg`: `user.optional.round_trip_resume: argument \`r\`: class user.optional.Resume requires field \`name\``; the value is rendered by today's `describe` | yes, the same message |
| a result nests deeper than 200 levels | `BamlError`, `the result is nested too deeply for the boundary, which carries 200 levels` | today's bridge refuses deep results at its wire's limit |
| called from a worker, or from a second copy of the runtime | `BamlClientError`, `the native BAML library belongs to another JavaScript environment ...` | today's bridge runs in workers; this one does not |
| a constructor threw while copying, or a getter while arguments were read | that exception, as it is, from both forms | yes |
| a task failed and nothing awaits it | thrown on Node's loop | yes |

Also: arguments and results nest at most 200 levels (today's wire refuses at 33); `worker_threads` are not supported (the addon belongs to the first environment that loads it); instance methods, media, streams, tracing and `$baml` options are stubs that throw; an `int` beyond 2^53 loses precision, as today.

## Measured

One session on one Mac (Apple M5 Max, 18 cores), through `perfq --bench`, every binary run once untimed first. "Today's" is the generated SDK of the same sources (the type-shapes fixture plus `bridge_test`) over today's VM, release builds, called by the same script. Time is the median of interleaved rounds; instructions per call come from two sizes and their difference. For the arrays, time and instructions are per element; for arguments ("in") the instructions include building the argument, the same on both sides, and Python's are net of it.

**Rust** (#1264, outside load 0.46 cores; 5 rounds; `crates/baml_native_host/scripts/bench.sh`, `bench_run.py`; today's: `sdkgen_rust` over `bridge_cffi`, current-thread tokio):

| One call after another | Today's | Native | Native vs today |
|---|---|---|---|
| `round_trip_int` | 21.3 µs · 213,294 instr. | 90.7 ns · 2,579 | 234× |
| `round_trip_inner` (one-`int` class) | 26.2 µs · 263,545 | 161 ns · 4,629 | 163× |
| `round_trip_complex_profile` | 214 µs · 4,210,191 | 10.1 µs · 235,332 | 21× |
| `int[]` of 100,000 out, per element | 218 ns · 2,991 | 71.3 ns · 213 | 3.1× |
| `int[]` of 100,000 in | 198 ns · 2,226 | 9.1 ns · 209 | 22× |
| `Point[]` of 10,000 out | 1,576 ns · 27,287 | 115 ns · 1,085 | 14× |
| `Point[]` of 10,000 in | 2,063 ns · 43,672 | 67.8 ns · 1,809 | 30× |

**Python** (#1265 to #1271, outside load 0.33 to 0.50 cores; 3 or 5 rounds, each the median of 3 runs; `crates/baml_native_python/tests/type_shapes/bench_run.py`; today's: `baml_bridge`; native: models by the native construct, GIL released):

| One call after another | Today's, async | Native, async | Today's, sync | Native, sync | Native vs today, sync |
|---|---|---|---|---|---|
| `round_trip_int` | 43.9 µs · 368,312 | 309 ns · 8,256 | 29.2 µs · 321,119 | 204 ns · 5,686 | 143× |
| `round_trip_inner` | 55.9 µs · 535,448 | 739 ns · 20,393 | 38.6 µs · 488,610 | 643 ns · 17,618 | 60× |
| `round_trip_complex_profile` | 478 µs · 9,523,193 | 19.9 µs · 467,248 | 458 µs · 9,460,427 | 19.4 µs · 466,022 | 24× |

| Arrays, sync, per element | Today's | Native | Pure Python, same objects | Native vs today |
|---|---|---|---|---|
| `int[]` of 100,000 out | 326 ns | 78 ns · 512 instr. | 13 ns | 4.2× |
| `int[]` of 100,000 in | 514 ns · 9,010 net | 12 ns · 122 net | 7 ns | 43× |
| `Point[]` of 10,000 out | 3,406 ns | 347 ns · 6,698 | 333 ns | 9.8× |
| `Point[]` of 10,000 in | 5,294 ns · 138,998 net | 99 ns · 1,795 net | 37 ns | 53× |

Today's SDK validates every model. Like for like, sync (the same jobs), with the native bridge making models three ways and pure Python building the same objects each way:

| | Today's | Native: `model_validate` | Native: `model_construct` | Native: native construct | Python: validate | Python: `model_construct` | Python: native construct |
|---|---|---|---|---|---|---|---|
| `round_trip_inner` | 38.6 µs | 851 ns | 1,060 ns | 643 ns | 324 ns | 549 ns | 224 ns |
| `round_trip_complex_profile` | 458 µs | 27.3 µs | 33.9 µs | 19.4 µs | 27.5 µs | 35.4 µs | 21.1 µs |
| `Point[]` out, per element | 3,406 ns | 563 ns | 827 ns | 347 ns | 438 ns | 707 ns | 333 ns |

- With validation kept, the bridge alone takes the profile from 458 µs to 27.3 µs (17×), the cost of pydantic validation with no bridge at all (27.5 µs). Skipping validation takes it to 19.4 µs.
- The native construct sets `__dict__`, `__pydantic_fields_set__`, `__pydantic_extra__` and `__pydantic_private__` with the generic setter after `object.__new__`; it depends on those four pydantic v2 slots. Models it would build wrong (extra fields allowed, private attributes, validators, `model_post_init`) are made with `model_validate`.
- Releasing the GIL during a turn costs 27 to 30 ns on `round_trip_int` and nothing measurable on long calls, while another Python thread gets about 14 times more done during them (#1181 to #1185, before the merge).

**Node** (#1272 time, outside load 0.32 cores, 3 rounds, each the median of nine batches; #1273 instructions, 0.25; `crates/baml_native_node/scripts/bench.sh`; today's: `bridge_typescript`; native: microtask turn, the default):

| Async, one `await f()` after another | Today's | Native | Plain JavaScript, no bridge | Native vs today |
|---|---|---|---|---|
| `round_trip_int` | 28.1 µs · 371,748 | 488 ns · 18,078 | 0 ns | 58× |
| `round_trip_inner` | 35.9 µs · 494,649 | 774 ns · 27,264 | 10 ns | 46× |
| `round_trip_complex_profile` | 325 µs · 8,711,567 | 31.9 µs · 1,012,843 | 407 ns | 10× |
| `int[]` of 100,000 out, per element | 344 ns | 131 ns | 0.9 ns | 2.6× |
| `int[]` of 100,000 in | 413 ns | 30 ns | 0.4 ns | 14× |
| `Point[]` of 10,000 out | 1,210 ns | 196 ns | 12.5 ns | 6.2× |
| `Point[]` of 10,000 in | 3,689 ns | 254 ns | 0.9 ns | 15× |

- Sync against sync: `round_trip_int` 11.1 µs today, 399 ns native (28×); `round_trip_inner` 17.5 µs and 673 ns (26×); the profile 313 µs and 32.5 µs (9.6×).
- The eager turn costs the same as the microtask turn (512 against 488 ns for `round_trip_int`).
- Instances are built by one call into a per-class function taking the values in order: 788 ns for `round_trip_inner`, against 830 ns setting each property from native code and 798 ns for `new C({..})`.

Where the time goes:

- A call through the C functions alone costs about 1,000 instructions; Rust's bindings add the lock, the map of waiting calls and the executor (2,579 for `round_trip_int`). Python adds the coroutine object and the GIL release (5,686 sync); Node the promise, the generated wrapper and about ten Node-API calls (18,078).
- Reading costs one C call per field and per element, and a string read allocates; that is most of `int[]` out in every language.
- Arguments in Python and Node are read the way today's encoders read them (an instance or a plain object, unknown keys refused, unions by rank), which costs more than reading declared fields only.

## Limits today

| Limit | Consequence |
|---|---|
| One library per process, one thread or loop at a time | Two BAML libraries cannot be loaded together. Rust: the core's lock serializes calls. Python: the loading thread owns the library, other threads get `RuntimeError`, one asyncio loop at a time. Node: the first JavaScript environment owns it; workers get `BamlClientError` |
| A turn runs on the host's thread | A long BAML computation blocks that thread (Python releases the GIL during it) |
| Generic classes and functions with type parameters are refused when packing | Type arguments would have to cross; the table leaves room for them |
| bigint, uint8array, tuples, functions, `json`, media, maps whose keys are not strings, classes of the standard library do not cross | Refused when packing, with the reason. About 60% of the type-shapes fixture packs |
| Values of classes outside the interface cannot be read | A standard-library panic arrives as text (`.value`), a thrown tuple cannot be read |
| An array does not record its element type | A result typed `int[] \| string[]` cannot be told apart when it is empty. A type test between array types does not compile natively yet |
| An array or a map from the host can be passed once | Bindings build a fresh one for each place, which is what copying does anyway |
| The trace of a throw is dropped | Errors arrive as a value and its text, without a BAML traceback |
| `baml.panics.Exit` | Raises a panic in the host; today's SDKs end the process |
| A stuck call | Raises a panic; today's SDKs never settle it. Its tasks stay parked for the life of the process |
| Nesting | Arguments and results nest at most 200 levels; today's wire refuses at 33 |
| Not built in the bindings | Per-call options, cancellation of the BAML call (a dropped or cancelled caller only frees the result), streams, instance methods (Python's `define_function` does not bind `self`), media, tracing, Node workers |
| Python, sync calls only | A task spawned by a call that returned advances during later calls; one waiting only on I/O advances when a loop is joined or at the next call |
| Two calls cannot share state through the program | A global is a `const`, and reading one is not compiled natively yet |
| A `spawn` with semaphores or cancel tokens ends the host's process | It reaches `baml_rt_spawn_unsupported`, which exits |
| A stack overflow in a frame too large to throw from ends the host's process | As in an executable (`unwind.rs`) |
| A handle kept after it is freed may name another object | Safe, but not detected: no generations |
| Describing a value stops 32 objects deep | A value that holds itself is written with a marker, `<nested too deep>` |
| Timers end about 3 ms late | Executables do too |
| arm64 macOS only | As for all native code today |

## Next

- What both languages asked of the core: a string read without extra copies; bulk reads of scalar arrays; builders that take slices; a turn that hands its events back after it returns (so Python can release the GIL for the C turn alone); values of classes outside the interface (a structured description of a thrown value's class).
- Generics: type arguments travel first in the values a call or an instance takes.
- LLM functions: the standard library's LLM code does not compile natively yet.
- The generators: emit what the hand-written bindings show (Rust, Python, TypeScript).
- Per-call options, cancellation, streaming.
- Calls from BAML into the host.

## Try it

```sh
cd baml_language
# Rust: the futures layer over a small library
cargo build --release -p baml_compiler2_native --example native_pack --example native_host
target/release/examples/native_pack --lib \
  crates/baml_compiler2_native/examples/native_host/bridge.baml libbridge.dylib \
  add nap env_or greet fail
target/release/examples/native_host libbridge.dylib
# Python: pack the fixture, generate both packages, build both runtimes, test
crates/baml_native_python/tests/type_shapes/setup.sh
(cd target/native_python/tests && PYTHONPATH=../native ../venv/bin/python -m pytest)
```

Try it, from `baml_language`, after building `native_pack`, the addon and today's bridge in release:

```sh
crates/baml_native_node/scripts/stage.sh    # pack the fixture, generate both packages
crates/baml_native_node/scripts/test.sh     # today's tests, parity, the bridge's own: default, eager turn, the two other builds, BAML_GC_STRESS=1
crates/baml_native_node/scripts/bench.sh prepare && crates/baml_native_node/scripts/bench.sh time
```
