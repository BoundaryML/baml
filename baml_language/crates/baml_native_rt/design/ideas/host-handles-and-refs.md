# Later: handing the host references instead of copies

Status: an idea, not built and not scheduled. What is built copies results into host objects (`../bridges.md`). This note records the alternative, what it would look like, and why it waits. The costs marked "estimate" are not measured.

## The idea

Instead of copying a result out of BAML memory, give the host something that points at it:

- a **handle**: a reference that stays valid for as long as the host keeps it; or
- a **borrowed pointer**: the object's raw address, valid for a short time.

The host's `Person` would then be a thin wrapper, and reading `person.name` would read BAML memory.

## Why it is tempting

| Case | With copies | With references |
|---|---|---|
| A large result of which the host reads a little | The whole thing is copied | Only what is read is touched |
| A result passed straight into another BAML call | Copied out, then copied back in | Nothing is converted |
| Raw bytes (a file, an image) | Copied | Could be read in place |
| Changing an object BAML still holds | Not possible; the copy is separate | A write goes to the real object |

## Three things a host could hold

| | Owned copy | Handle | Borrowed pointer |
|---|---|---|---|
| What it is | An ordinary host object | A slot the collector keeps updated | The object's address |
| Valid until | The host drops it | The host releases it | BAML runs again |
| Steps to reach a field | 0 | 2 (slot, then object) | 1 |
| Sees later changes made by BAML | No | Yes | Yes |
| Built today | Next (`../bridges.md`) | Inside the bindings only | No |

## Why a raw pointer does not last

The collector copies every live object to a new place (`../gc.md`). It updates the places it knows about. It does not know about a pointer in a Python variable.

```
 before a collection            after
 ┌──────────────┐               ┌──────────────┐
 │ old space    │               │ new space    │
 │   Person ◀───┼── host ptr    │   Person     │   the object moved;
 └──────────────┘               └──────────────┘   the host's pointer did not
```

- The collector never runs on its own. It runs when BAML allocates, which only happens when the host calls in. So a raw pointer is safe **until the next call into the library**.
- The obvious fix, telling the collector "do not move this one" (pinning), is not possible with a collector that works by abandoning a whole space (`gc.md`, section 5). It becomes possible with the block-based design (`gc.md`, section 13).
- Large byte blocks are planned to live in memory that never moves (`gc.md`, section 6). Those could be handed out by address.

A handle avoids the problem: it is the address of a slot that never moves, and the collector rewrites the slot.

```
 host ──▶ handle slot ──▶ Person
```

## What the API would look like

The generated class becomes the wrapper. Field names stay the same. A copy is one call away.

```python
r = await round_trip_outer(o)     # r is a reference; nothing was copied
r.inner.value                     # read from the BAML object
r.inner.value = 10                # written to the BAML object, type-checked
d = r.snapshot()                  # a detached pydantic model
```

```ts
const r = await round_trip_outer(o);
r.inner.value;                    // a generated getter
r.inner.value = 10;               // a generated setter
const plain = r.snapshot();       // a detached plain object
```

```rust
let r = round_trip_outer(&o).await;
r.inner().value();                // getter
r.inner().set_value(10);          // setter
let owned: OuterData = r.snapshot();
```

Notes on the Rust form:

- No `field_mut()`. A `&mut` needs a stable address holding a plain Rust value. A BAML field moves with its object, and it is a tag and a payload that must be written together and checked.
- Setters take `&self`. Other references may point at the same object, so `&mut self` would promise an exclusivity that does not exist.
- `clone()` means "another reference to the same object", as for `Rc`. The detached copy needs its own name, hence `snapshot()`.
- Rust would have three types per class if everything were exposed: the owned struct, the reference, and a borrowed view tied to a lifetime.

## Does the host need to call the runtime to read a field?

Not always. With the address in hand, the host can read the memory itself, because generated code knows the layout.

| Operation | Needs a call into the runtime? |
|---|---|
| Read an `int`, `float`, `bool` or pointer field | No |
| Read a string or an array element | No |
| Look up a key in a map | Yes, or the host reimplements the hash table |
| Create an object or a string | Yes: allocation can collect |
| Write a field | Yes if the collector ever needs to hear about writes (a generational one would) |
| Call a function | Yes |

What it costs: the memory layout becomes a contract with every set of bindings, and the layout is still changing (values became two words recently; `type-header-fast-path.md` would change headers again).

How cheap a direct read is depends on the language:

- **Rust** reads memory inline.
- **Node** could, by viewing the heap as a typed array, but the heap moves and grows.
- **Python** cannot do it cheaply: any raw read goes through the interpreter or a native helper, which is a call anyway.

## Is not copying actually faster?

Not in general. One field read (estimates):

| | Ordinary host object | Through a reference |
|---|---|---|
| Rust | under 1 ns | 1 to 3 ns |
| Node | about 1 ns | 30 to 100 ns |
| Python | 20 to 40 ns | 60 to 300 ns |

Two more costs of a reference:

- Every read takes two steps through memory, and the object is likely not in the cache.
- BAML fields are 16 bytes each. An `int[]` read in place is half as dense as a host array of 8-byte integers.

So:

| What the host does with a result | Better |
|---|---|
| Reads most of it, perhaps more than once | Copy |
| Passes it on to another BAML call | Reference |
| Reads a small part of something large | Reference |
| Loops over a large numeric array many times | Copy, into a packed host array |

Copying once and reading at native speed wins the usual case outside Rust. That is why copy is the default.

## Threads, and BAML running in parallel

Today the runtime has **one worker**. All BAML code runs on it.

- Several host threads can use the library only one at a time, behind a lock.
- A borrowed pointer is valid only while the holder has that lock. Another thread calling in may collect.
- A handle is always valid, but reading through it needs the lock too.
- In Rust this means references could probably not be sent between threads at first, which rules out holding one across a spawn on a multi-threaded executor.

If BAML later runs on several workers at once (`../threads.md`):

- A slot is written under a lock bit so its two words are never seen half-written (`../memory.md`). A host reading memory directly would have to follow that protocol.
- A collection stops every worker at a safe point. A host thread reading BAML memory would have to be known to the collector and stop too.

Direct reads from the host are therefore tied to the single-worker design. Calls into the runtime could hide that change; direct reads could not.

## Ways to offer both, and why none was chosen

| Option | Problem |
|---|---|
| References by default, `snapshot()` for a copy | Slower for the usual case in Python and Node. A result that changes under you is new behaviour. Other libraries need the snapshot anyway |
| A second variant of every function that returns a reference | Doubles the generated surface. Every caller must choose, on every call |
| Copy on first touch: a reference until read, then a copy | Whether a write reaches BAML depends on whether the object was read before. Hard to reason about |
| A different default per language | The same BAML function would behave differently in Rust and Python |

## Why not yet

- **Not measured.** The cost tables above are estimates. The honest test is four read paths (the VM's bridge, a direct copy, a reference through a call, a reference by direct read) in each language.
- **It changes what a result is.** Today a result is a value. A reference is shared and can change.
- **The ground is moving.** The value layout, the collector and the threading model are all still being designed. Each changes the trade-off: pinning would allow raw pointers, parallel workers would restrict them.
- **Copies are needed regardless.** Even with references, `snapshot()` must exist. Building the copy path first loses nothing.

## What would bring this back

- Measurements showing the copy dominates a real workload: results chained between BAML calls, or large arrays.
- The non-moving region for bytes landing (`gc.md`, section 6), which makes zero-copy bytes straightforward.
- A collector that can pin (`gc.md`, section 13).
- A user who needs to mutate BAML state from the host.

The most likely first step is narrow: handles for bytes and for results passed directly into another BAML call, with everything else still copied.
