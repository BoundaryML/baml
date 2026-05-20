# `Value` tearing under BEP-034 spawn

## TL;DR

BAML's `Value` enum is 16 bytes (`bex_vm_types/src/types.rs:579`). A write to
a `Value` field lowers to two 8-byte stores. With BEP-034 `spawn`, two BAML
threads can mutate the same heap object's `Value`-typed slot concurrently;
a third thread reading that slot mid-write can observe a **tag word from
one write paired with a payload word from another**, producing a malformed
`Value::Object(some_integer_we_treat_as_a_pointer)`. The next dereference of
that fake pointer segfaults the runtime — at best — or, worse, succeeds and
treats arbitrary memory as a heap `Object`, with consequent silent heap
corruption.

This is the same shape as Go's torn-`interface{}` problem documented in the
[Go memory model](https://go.dev/ref/mem) §"Implementation restrictions":

> When the values depend on the consistency of internal (pointer, length)
> or (pointer, type) pairs, as can be the case for interface values, maps,
> slices, and strings in most Go implementations, such races can in turn
> lead to arbitrary memory corruption.

The JVM and CLR avoid this because their pointer-sized references are
hardware-atomic; they never have a multi-word value type that a user can
write to a shared field.

## The BAML example

```baml
class Box {
  v (int | int[])
}

function child_writes_array(b: Box) -> null {
  let xs = [1, 2, 3];
  b.v = xs;        // writes Value::Object(HeapPtr → Array)
  null
}

function main() -> int {
  let b = Box { v: 42 };               // b.v = Value::Int(42)
  let _ = spawn { child_writes_array(b) };
  // ↑ child concurrently writes b.v as Value::Object(...)

  // Parent reads b.v while the child is mid-write.
  // Pattern-match dispatch decides what to do based on the tag byte.
  let result = match b.v {
    let n: int    => n,
    let a: int[]  => a.length(),    // ← if a is a torn HeapPtr, this
                                    //   dereferences garbage
  };

  result
}
```

A shorter form, if the runtime ever supports indexed assignment to a union
array element:

```baml
function main() -> int {
  let arr = [42];                       // arr[0] = Value::Int(42)
  let _ = spawn { arr[0] = [1,2,3]; };  // writes Value::Object(...)
  arr[0]                                // racy read; may segfault on use
}
```

What the two examples have in common: **two BAML threads writing different
`Value` variants to the same heap-resident slot, and a reader observing
that slot mid-write.**

## Why this is dangerous: the memory hazard

### `Value` layout

The enum (`bex_vm_types/src/types.rs:579`):

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    OmittedArg,
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Object(HeapPtr),
}
```

`HeapPtr` is `*mut Object` (8 bytes; 16 with the `heap_debug` feature flag
adding an epoch). Rust lays the enum out as a tag word followed by the
payload word, aligned to 8 bytes:

```
byte offset:  0           8        16
              ┌───────────┬────────┐
              │    tag    │payload │     16 bytes total, 8-byte aligned
              │  (1B+pad) │  (8B)  │
              └───────────┴────────┘
```

- `Value::Int(42)`     → tag=`Int`, payload=`42`
- `Value::Object(p)`   → tag=`Object`, payload=`0x7fff_1234_5678`

### A `Value` write is two `mov` instructions

A field write `b.v = new_value` lowers to two 8-byte `mov`s — one for the
tag word, one for the payload word. On x86-64 you might think `movdqa`
could do it as one 16-byte op, but **`movdqa` is not guaranteed atomic
across the 16 bytes**; only aligned 8-byte stores are atomic on standard
x86. The only 16-byte atomic on x86 is `lock cmpxchg16b`, which LLVM does
not emit for plain stores.

So a write of `Value` is *literally* two writes, in some order.

### The interleaving that crashes

```
Parent thread (P)                      Child thread (C)
─────────────────────────              ─────────────────────────
   (b.v is Value::Int(42),
    so memory at &b.v is:
    [tag=Int | payload=42])

                                       store tag = Object
                                       (about to store payload =
                                        0x7fff_1234_5678 next)
read tag      → Object
read payload  → 42 (still old!)
                                       store payload = 0x7fff_1234_5678

Parent now has:
  Value::Object(HeapPtr(0x000000000000002a))
                              ^^^^^^^^^^^^^
                              the integer 42, treated as a pointer
```

The parent's `match b.v` arm sees tag=`Object` and binds `a` as
`Value::Object(0x2a)`. Calling `.length()` on `a` dispatches to
`array_length`, which:

1. Reads the `HeapPtr` out of the `Value`.
2. Calls `unsafe { ptr.get() }` → `&*(0x2a as *mut Object)`.
3. Loads bytes at virtual address `0x2a` to read the `Object` discriminant.

Address `0x2a` (= 42) is in the kernel's reserved low-memory region on every
modern OS. The CPU faults. The process gets `SIGSEGV`. The BAML runtime is
dead.

The symmetric interleaving (parent reads old tag with new payload) is also
possible and equally bad: `Value::Int(0x7fff_1234_5678)` is a "valid-looking"
int that downstream arithmetic will silently use as a number.

## Why this is qualitatively worse than the `Vec::push` race

The `array.push(4)` / `array.push(5)` race I demonstrated separately
(`spawn_array_race.rs` reproducer) produced **lost pushes** and an
occasional `SIGTRAP` from a `Vec` internal `debug_assert`. Those are bad
but *bounded*: the user wrote racy code, they got a hung array or a crash
they can attribute to their racy code.

The `Value`-tearing race is qualitatively worse because:

1. **The user has not done anything explicit-feeling-racy.** They wrote
   `b.v = 42` and then `b.v = xs` from another thread. Reading `b.v` looks
   like it should just give them one or the other. They have no intuition
   that a single field *read* can produce something that's neither.

2. **The crash happens far from the race.** The torn read produces a
   malformed `Value` that flows through the program until some downstream
   method dispatch dereferences it. The stack trace at the crash points at
   `array_length` (or wherever the dereference happens) — not at the `b.v`
   read where the actual bug occurred. Almost impossible to debug.

3. **It's a true memory-safety violation in the runtime.** The Rust
   `unsafe` contract on `HeapPtr::get` (`bex_vm_types/src/heap_ptr.rs:118`)
   is "the pointer must still be valid (object not collected by GC)." A
   torn `Value` violates that contract by manufacturing a fake `HeapPtr`
   from an integer like 42 — an address that was never produced by the
   allocator and will never have been a valid heap object. Every downstream
   `unsafe` block becomes a potential exploit primitive: an attacker who
   can race `b.v` can plant arbitrary pointer values in the heap.

4. **It can corrupt heap state silently if the integer happens to land
   in mapped memory.** If the int payload is, say, `0x7fff_dead_beef` and
   that address happens to fall inside a `ChunkedVec` chunk, the
   dereference succeeds and reads garbage as an `Object`. The `match`
   arm in `HeapPtr::get` callers reads the `Object` discriminant byte
   (`Function`, `Class`, `Array`, ...). If the garbage byte happens to
   match `Array` (a common variant), we now have a `&Vec<Value>` whose
   internal `len` / `cap` / `ptr` are arbitrary garbage. Iterating that
   "array" can scribble across the entire heap.

## Equivalent in production runtimes

| Runtime | Has the same problem? | How they avoid it |
|---|---|---|
| **JVM** | No | All user-visible reference values are pointer-sized and aligned (compressed oops on 64-bit). Per JLS §17.7, reference writes are atomic. No multi-word user-mutable value type exists. `long`/`double` *can* tear under racy non-volatile access, but those are scalars — no follow-on memory-safety issue. |
| **CLR (.NET)** | No | Same shape as JVM. Per the CLR's `Memory-model.md` §18-25: "Memory accesses to properly aligned data of primitive and Enum types with size with sizes up to the platform pointer size are always atomic. Managed references are always aligned to their size on the given platform and accesses are atomic." Plus object assignment is release-fenced, so a freshly-allocated array is always observed valid. |
| **Go** | **Yes — formally allowed and documented.** | None. The Go memory model spec (linked above) explicitly states that races on multi-word values can lead to "arbitrary memory corruption." `interface{}` = `(type_descriptor, data)` — two words — and a torn read pairing a *new* type descriptor with *old* data and dispatching through the new type's vtable is the canonical Go segfault pattern. Slice headers `(ptr, len, cap)` have the same problem. Go ships with this as a known and documented limitation, with the race detector as the user-facing diagnostic. |

We're in Go's position by default. The question is whether we want JVM's
position instead.

## How to fix it

Two answers. The choice is binary.

### Answer A — make `Value` fit in one machine word

If `Value` is 8 bytes and 8-byte aligned, every read/write is one
hardware-atomic `mov`. Torn reads become impossible. Two techniques:

- **NaN-boxing.** Store everything in a `f64` slot. IEEE-754 doubles have
  ~2^51 free NaN payload bits; you tag integers, booleans, null,
  `OmittedArg`, and heap pointers (47-bit virtual addresses on x86-64)
  into the NaN payload. V8, SpiderMonkey, LuaJIT all use this.

- **Tagged pointer.** Use the low 3 bits of an aligned 8-byte pointer as
  the tag. Aligned heap addresses have low bits = 0, so we can repurpose
  them. Small integers / booleans / null get encoded as
  non-pointer-shaped values where the low bits are nonzero. Standard
  technique in OCaml, Erlang, V8 (Smi).

Per-op cost in the steady state: **zero**. Reads and writes become single
aligned `mov`s. Heap allocation rate may drop slightly because boxed-int
allocations disappear. The cost is engineering effort: every site that
constructs or matches on `Value` (probably ~200 places in `bex_vm` /
`bex_engine` / `bex_vm_types`) has to switch from `match value { … }` to
calls on a `Value` API that masks-and-tags. Multi-week refactor.

### Answer B — accept tearing, document it

Keep the 16-byte `Value`. Accept that user code can corrupt the runtime
under spawn races on shared heap objects. Document this as a known
limitation, equivalent to Go's interface-tearing posture. Recommend that
users either:

- Avoid sharing mutable heap objects across spawns. Capture by deep copy
  if they must.
- Use a future stdlib `Mutex` / `Channel` for cross-spawn data exchange.

Implementation cost: zero. Documentation cost: real (this is a sharp edge
in the language model). Production-quality cost: bad — sporadic SIGSEGV
in production from racy user code is hard to support, and the failure mode
is far from the cause.

## Recommendation

If we're committing to BEP-034's "spawn is normal BAML, accessible to any
user," we should pay the cost of Answer A. The reproducer for a torn
`Value` is small (a few lines of BAML) and the failure mode is a segfault
inside the runtime — exactly the kind of thing the BEP-034 PR description
promises won't happen.

If we're treating BEP-034 v1 as "advanced users only, here are the rules,
don't share mutable state," Answer B is defensible and ships today. We
should be explicit about it in the BEP and the stdlib docs.

The decision blocks any further VM-safety design work, because every other
mitigation we considered (iteration backstops, bounds-checks-on-live-len,
write-barrier hardening) operates on individual `Object` variants and is
orthogonal to whether reads of the top-level `Value` slot are atomic. If
`Value` reads can tear, no amount of container-level safety prevents the
crash; if they can't, the container-level work suffices.

## Related

- BEP-034 PR: https://github.com/BoundaryML/baml/pull/3520
- Reproducer for the related but distinct `Vec::push` race:
  `baml_language/crates/baml_tests/tests/spawn_array_race.rs` (currently
  `#[ignore]`d; runs lose pushes and occasionally `SIGTRAP`).
- `HeapPtr::get` safety contract:
  `baml_language/crates/bex_vm_types/src/heap_ptr.rs:118-134`.
- Go memory model on multi-word value races:
  https://go.dev/ref/mem (§"Implementation restrictions").
- JLS §17.7 on reference atomicity (and the explicit `long`/`double`
  carve-out).
- CLR memory model:
  `/Users/antonio/Desktop/github/runtime/docs/design/specs/Memory-model.md`
  §18-25 (atomic aligned access) and §134-146 (object assignment release
  semantics).
