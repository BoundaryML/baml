# Idea: the object header as a fast path for types

Status: not built, and nothing below is decided. The principle is settled; the bit layout, the kind encoding and the field widths are one candidate, to be chosen by measuring real code (see "Decide by measuring").

## The problem

The native header's type id describes a layout, not a language type. `int[]` and `bool[]` share an id, `string[]` and `Foo[]` share an id, every map shares one. Only classes have an identity, and no object records its type arguments. So the lowering refuses array type tests, and reflection has nothing to read.

The interpreter stores the full declared type on every value that has arguments (`bex_vm_types/src/types/`):

```rust
pub struct Instance { class: HeapPtr, class_type_args: Box<[RealizedTy]>, fields: ... }
pub struct Array    { element_ty: Box<RealizedTy>, data: ... }
pub struct Map      { key_ty: Box<RealizedTy>, value_ty: Box<RealizedTy>, data: ... }
pub struct Tuple    { element_tys: Box<[RealizedTy]>, slots: ... }
pub struct Closure  { function, captures, captured_type_args: Box<[RealizedTy]> }
pub struct Future   { ..., types: Box<FutureOutputTypes> }
```

Type tests compare that declared type, with arguments invariant (`bex_vm/src/type_match.rs`). So types cannot be erased and recovered by walking contents:

```baml
let a: string[] = [];
a is int[]                              // false: the array says string[]

let b = Box<int | string> { value: 3 };
b is Box<int>                           // false: declared Box<int | string>

class Handler<T> { f: (T) -> int }      // T is never stored, yet Handler<int> != Handler<string>
```

Reflection needs the same data: `reflect.Type.of_value(x)`, `reflect.Type.of<T>()` inside generic code, and types created at run time (`Package.compile`, `Package.with_types`, `Session.eval`).

## The principle

The type descriptor is the truth. The header is a cache of the questions asked most. Anything the header cannot answer, a slow path can, by following a pointer and comparing descriptors. So `type(x)` is O(1) for most values and a short walk for the rest.

The header names concrete types only. Interfaces and unions are never in a header.

## Decide by measuring

Two kinds of evidence, both from real programs (the stdlib, the conformance cases, the speedtest programs, user projects).

Static, from the compiler:

- How many declarations a program has, and how many concrete instantiations. Sets the id width and whether `WIDE` is ever reached.
- Generic classes by arity: none, one, more. Sets whether an inline single argument is worth its bits.
- How often a type argument is itself composite (`Box<Foo[]>`). Decides whether nested types need interned indexes.
- Type tests by shape: exact class, generic class, array, map, "any class", "any enum". Shows which questions the header must answer in one compare.
- Interface dispatch sites against type-test sites. Shows what an extra instruction on either costs in total.

Dynamic, from running programs:

- Objects allocated by kind, and by argument form (none, inline, list). Shows what an extra word per generic instance costs in memory.
- Instructions for each candidate encoding on dispatch, a type test and a kind test, measured the way `design/README.md` describes.

Candidates to compare:

- Kind as bits in the high word (below), as bits in the low word next to the id, or as id ranges with no bits at all.
- Argument form folded into the kind (below) or as its own field.
- Full type per instantiation as its own id, against declaration id plus arguments (below).

## Candidate layout

One 64-bit word. The id stays where it is, as a byte offset, so interface dispatch keeps its current sequence (`ldr w0, [obj]; ldr x0, [table, x0]`). Speed wins over having every type bit in one 32-bit word.

```
bits 0..2    collector (bit 0 forwarded; fixed by the forwarding address)
bits 3..31   declaration id: byte offset into the type table
bits 32..59  payload, meaning set by the kind
bits 60..62  kind
bit  63      panic class
```

### Declaration id

Names the declaration, never the instantiation: `Foo`, `Box`, "array", "map". Dispatch, the collector and field access use only this, so there is one table row per declaration and the table is sized at compile time.

Three ranges:

```
id < K            fixed constants, the same in every binary
K <= id < WIDE    a declaration of this program
id == WIDE        did not fit: word 1 of the object points at the descriptor
```

Fixed constants are whole concrete types with nothing more to read: `null`, `bool`, `int`, `float`, `string`, `bigint`, `uint8array`, `int[]`, `float[]`, `bool[]`, `string[]`, `map<string, string>`, and so on. Rust natives can `match` on them without the program's table. `float[]` being its own id frees the "float elements" flag bit.

### Kind

Three bits. Says what sort of thing this is and where its type arguments are, so no separate arity field is needed and a string keeps all 28 payload bits for its hash.

```
0  other, no arguments      string, boxed scalar, bigint, bytes, cell
1  class, no parameters     the id is the whole type
2  class, one argument      argument inline in the payload
3  class, argument list     word 1 of the object points at the list
4  enum
5  array                    element type inline in the payload
6  map                      argument list (the payload is the lock word)
7  other, argument list     tuple, closure, future
```

"Any class" is `1 <= kind <= 3`. A `match` over kinds is a jump table on three bits.

```asm
ldr  x0, [x2]              ; whole header
ubfx x1, x0, #60, #3       ; kind
```

### Payload

- Kind 2 and 5: the one type argument, as a type index. An argument whose index does not fit in 28 bits uses the list form instead (kind 3 for a class; an array would need a wide form).
- Strings: the cached hash, as today.
- Maps: the lock word, as today.

### Argument list

A pointer in the object to a list of type descriptors. A null list reads as "arguments unknown", so an opaque `Box` is representable.

## What each question costs

```baml
x is Foo             // 32-bit load, compare with a constant
x is Box<int>        // 64-bit load, compare with a constant (id + kind + inline argument)
x is Foo[]           // same
x is map<string, Foo>  // compare the id, then compare the argument list
```

- Dispatch: unchanged, by declaration id.
- Collector: unchanged, plus it can switch on fixed ids without reading the table.
- Allocation in generic code: `Box<T> { ... }` writes the `T` it was passed into the payload. No lookup.
- `reflect.Type.of_value(x)`: build the type from the id, the kind and the arguments.

## Open

1. Nested arguments. In `Box<Foo[]>` the inline argument must be a single index, so `Foo[]` needs one. Either composite types used as arguments are interned (statically at compile time, by lookup at run time), or anything nested uses the list form.
2. Argument lists: interned (the slow path is a pointer compare) or compared structurally. Likely intern the static ones and compare the rest.
3. Run-time declarations (`Session.eval`, `with_types`): whether a packed binary supports them at all. If yes, they are the users of `WIDE`, or the table has to grow.
4. How the native backend passes type arguments to generic functions today. The "types loaded once" refusals in `baml_compiler2_native/src/check.rs` suggest only the static case exists.
5. Where the argument-list word goes in an array (3 words today) if its element type ever needs the list form.
