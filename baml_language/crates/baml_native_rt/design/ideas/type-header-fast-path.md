# Later: the header word as a fast path for types

Status: an idea to come back to, not built and not needed before native has parity with the interpreter. `../memory.md` is the model: every question about a value's type is answered correctly there by following a pointer to its class or type object. This note is about answering the common questions from the header word alone, without the pointer. The bit layout, the kind encoding and the field widths are one candidate, to be chosen by measuring real code (see "Decide by measuring"). The header that is built (`../memory.md`, "Values") fixes part of it: bits 0..31 are a dense type id with no shift, one `INSTANCE` id for every class, the collector's bits are 59 and 60, 61 is free, 62 is the panic class and 63 the busy bit, and there is no type table. Where this note still reasons from a per-class id that is a byte offset into a type table, it describes the layout before that.

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

## One word format for values and objects

Every value is two words wherever it is: a tag, then a payload (`../memory.md`, "Values"). The tag has exactly the format of an object's header. So this note defines one word that appears in two places:

```
an object      [ header | ... ]            the first word of anything on the heap
a value        ( tag, payload )            an array element, a field, a map entry, an argument, a result
```

```
let xs: (int | string | Foo)[] = [5, "a", Foo { n: 1 }];
storage:  ( INT, 5 )   ( STRING, ptr )   ( Foo, ptr )
                              |               |
                              v               v
                       [ STRING hdr | ..]  [ Foo hdr | (INT, 1) ]
```

Decided: the tag of a reference is the object's own header, not a generic "reference" mark. So the tag of any value is its type as far as a header can say it, with no case split:

- `x is Foo`, "is this an int", and interface dispatch read the tag and never touch the object.
- Dispatch on an `int` and on an instance are the same sequence.
- It costs no load. A value always travels with its tag (in memory, across a call), and inside one function the compiler has it as a constant or as the word it loaded.

The tag is a cache, like the header itself. The truth about an instance is its class pointer and its type arguments, which are in the object (`../memory.md`, `InstanceObject`); for the scalars the tag is the truth, since there is nothing else. An object never changes type, so the copy cannot go stale. The collector bits stay in the object only. A string's cached hash is copied into the tag too, and is replaced with the pointer when a string slot is assigned.

### Why every value is two words

Recorded here because nothing else says it. Native used to pick a representation from the static type: a bare word for an `int`, a value and a tag for an `int?`, a pointer to a box for an `int | string`, and storage to match. A conversion can be inserted where a value flows, but not for a view:

```baml
function total(xs: readonly (int | string)[]) -> int { ... }
let a: int[] = [1, 2];
total(a)        // planned: the same array seen at a wider type, and total sees later writes to a
```

With typed storage the two sides disagree about what an element is, and the only ways out are a copy or a branch on the real element kind at every read. The same hazard exists today for functions (`w(x: int | string)` passed as `(int) -> int`). Rulings, 2026-11-24:

1. Two words everywhere: array elements, map values, class fields, cells, futures, and also arguments and results. "Two words in memory, typed in registers" was rejected: inside a function it is the same code, at a call it saves about one instruction per exactly-typed argument, and it keeps two shapes per type, thunks for function variance, and per-type copies of generic code.
2. Planned variance: `readonly T[]`, `readonly map<K, V>`, covariant class type parameters. A `readonly` view is the same object as the mutable one, never a copy.
3. This is the model until native has parity with the interpreter. No dense typed layouts before then. Twice the memory for scalars on the heap is accepted.
4. `uint8array` and strings stay packed bytes.
5. One word format for a tag and a header. Header bits may change if that is what it takes.
6. `omitted` exists only in arguments.
7. Built as one change, before defaulted arguments and globals. Instruction counts before and after are reported against `../README.md`, not used as a gate.

Inside one function the compiler still knows the tag of a value whose type is exact and keeps only the payload in a register. Tags become real at a call, a return, or a store.

### Immediates

A value whose payload is bits and not an address is an immediate. Each has a fixed id, the same in every binary:

| tag | payload |
|---|---|
| `null` | unused |
| `bool` | 0 or 1 |
| `int` | the 64 bits |
| `float` | the 64 bits |
| `omitted` | unused |

`null` is a tag, so `T?` needs no representation of its own and an optional scalar is an ordinary value. Boxes go away: no object is ever made to hold an `int`, `float` or `bool`.

`omitted` is the one tag that is not a type. A call passes it in the slot of a parameter left to its default and the callee's prologue replaces it before anything else reads the parameter (`default-arguments.md`). It exists only in arguments: no program can hold it and it is never stored to memory. A type test against it is false for every type.

An enum value is not an immediate. It is the address of its variant, an object, whether the enum is declared in the program or at run time (`Package.compile`, `with_types`, `Session.eval`). Two values of one variant are the same address, so `==` is a pointer compare.

The collector has to tell an immediate from an address by the tag alone, for every slot it scans. In the candidate below that is one test: the kind is immediate or it is not.

## Type arguments are descriptors, not ids

A type argument can be any type, not only a concrete one with a table row:

```baml
Box<int | string>            // a union: never in a header, has no id
Box<(int) -> string>         // a function type
Box<"a" | "b">               // literal types
type Tree = Node<Tree>[]     // a recursive alias: the descriptor points back at itself
```

So the general form of a type argument is a pointer to a type descriptor: a small graph that may contain unions, function types, literals and cycles. Nothing requires every type in that graph to have an id, and the full graph is only built for the types that need it. An id, an inline argument or a fixed constant is an optimization for the arguments that happen to be simple.

## Values of type `type`

A type is also a value: `reflect.Type.of<T>()`, `User.type()`, `reflect.Type.of_value(x)`. The interpreter has `Object::Type(Box<TypeValue>)` for it, and `==` on two of them is type equivalence.

```baml
let t: type = reflect.Type.of<Box<int>>();
t == reflect.Type.of_value(Box<int> { value: 1 })    // true
```

So `type` needs its own fixed id, and a value of it is (or points at) a descriptor. The natural shape: a descriptor starts with an object header whose id is `type`, so a descriptor is already a valid `type` value.

- Descriptors the compiler emits live in the program's data and are never moved; the collector already skips addresses outside its space.
- Descriptors built at run time (`Box<T>` in generic code, `with_types`) are heap objects of the same shape.
- Then the type argument of a generic function, the descriptor pointer in an object, and a `type` value are all the same thing, and `reflect.Type.of_value(x)` for a generic instance is one load.
- `==` must be structural unless descriptors are interned.

Other values the interpreter has that also need an id and a shape here: function and generic-function values (with their type arguments), bound methods, enum variants, packages, and opaque Rust data (media types). List them with `type` when phase 1 is designed.

## Phases

Correct first, fast later. Each later phase only adds a shortcut for a case that phase 1 already answers.

0. Uniform values (`../memory.md`). Every slot is a tag and a payload, immediates have fixed ids, boxes and the per-array element flags are gone. A tag answers "which scalar" and "which declaration", nothing about type arguments. It is built as one change.
1. Correct. The header keeps a declaration id as today. Every object whose type has arguments (generic instance, array, map, tuple, closure, future) carries one pointer to its full type descriptor. Type tests and `reflect.Type.of_value` compare or return descriptors. Slow, uniform, and equal to the interpreter in every case.
2. Fixed constants for the common whole types (`int[]`, `float[]`, `string[]`, `map<string, string>`, ...), so those objects need no descriptor pointer and test in one compare.
3. Kind bits, for "any class / enum / array / map" without touching the table.
4. Inline single argument, when the argument is a type that has an index. Anything else keeps the pointer from phase 1.
5. Whatever the measurements below show is worth it: more fixed constants, interned descriptors so equality is a pointer compare, wider fields.

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

This is where phases 2 to 4 could end up, not phase 1. Wherever it says "argument list", read "pointer to a type descriptor" in the general case; an inline argument applies only when the argument has an index.

One 64-bit word. The low half and bits 59..63 are as built; what a candidate can still choose is the meaning of the payload.

```
bits 0..31   type id, dense, no shift (as built)
bits 32..58  payload, meaning set by the kind
bit  59      collector: forwarded. The word is then `address | 1 << 59` (as built)
bit  60      collector: a mark used only while slices are settled (as built)
bit  61      free
bit  62      panic class
bit  63      busy: set while a slot's two words are being changed (`../memory.md`)
```

An earlier draft of this layout had the collector in bits 0..2, the id in bits 3..31 as a byte offset into a type table, and a three-bit kind in 59..61. None of that is built: the kind would now have to come out of the payload, the id, or bit 61, and dispatch reads the method row from the class object, not a table indexed by id.

The busy bit is the sign bit, so testing it is a sign test and taking it is one atomic OR. It is the same bit in a value's tag and in an object's header, since they are one format. The kind sits one bit lower than in earlier drafts to make room, and the payload is 27 bits, not 28.

### Declaration id

Names the declaration, never the instantiation: `Foo`, `Box`, "array", "map". Dispatch, the collector and field access use only this, so there is one table row per declaration and the table is sized at compile time.

Three ranges:

```
id < K            fixed constants, the same in every binary
K <= id < WIDE    a declaration of this program
id == WIDE        did not fit, or declared at run time: word 1 of the object points at the descriptor
```

Fixed constants are whole concrete types with nothing more to read: `null`, `bool`, `int`, `float`, `string`, `bigint`, `uint8array`, `int[]`, `float[]`, `bool[]`, `string[]`, `map<string, string>`, and so on. Rust natives can `match` on them without the program's table. The immediates and `omitted` are in this range too (see "Immediates").

With uniform values no flag has to say what an array's elements are: each element carries its tag. The "float elements" flag and the int and bool flags added beside it go away. A fixed id such as `int[]` is then only a shortcut for the type test `x is int[]`, not something the collector or a native needs in order to read the array.

### Kind

Three bits. Says what sort of thing this is and where its type arguments are, so no separate arity field is needed and a string keeps all 27 payload bits for its hash (one fewer than the 28 it has today).

```
0  immediate                null, bool, int, float, omitted: the value's payload is bits
1  class, no parameters     the id is the whole type
2  class, one argument      argument inline in the payload
3  class, argument list     word 1 of the object points at the list
4  enum variant             an object; the value's payload is its address
5  array                    element type inline in the payload
6  map                      argument list (the payload is the lock word)
7  other object             string, bigint, bytes, cell, tuple, closure, future; the id says whether it has an argument list
```

"Any class" is `1 <= kind <= 3`. "The payload is an address" is `kind != 0`, which is what the collector asks of every slot. A `match` over kinds is a jump table on three bits.

Kind 0 used to be "other, no arguments" and held strings and boxed scalars. Boxes no longer exist, and strings move to kind 7 so that kind 0 can mean immediate. That costs kind 7 its single meaning: whether a kind-7 object has an argument list is now a property of its id (a tuple, a closure and a future do; a string, a bigint, bytes and a cell do not).

```asm
ldr  x0, [x2]              ; whole header
ubfx x1, x0, #59, #3       ; kind
```

### Payload

- Kind 2 and 5: the one type argument, as a type index. An argument whose index does not fit in 27 bits uses the list form instead (kind 3 for a class; an array would need a wide form).
- Strings: the cached hash, as today.
- Maps: the lock word, as today.
- Closures: the call layout id. A function value says here which parameter layout its code expects (required slots, then named optional ones), so an indirect call can compare it with the layout it pushes in one load and one compare (`default-arguments.md`). It is not a type: two functions of different types can share a layout, and it answers no type test. A closure is kind 7, so its type arguments are in its argument list and the payload is free for this. The id is dense per program and needs far fewer than 27 bits.

### Argument list

A pointer in the object to a list of type descriptors. A null list reads as "arguments unknown", so an opaque `Box` is representable.

## What each question costs

```baml
x is Foo             // 32-bit load, compare with a constant
x is Box<int>        // 64-bit load, compare with a constant (id + kind + inline argument)
x is Foo[]           // same
x is map<string, Foo>  // compare the id, then compare the argument list
```

- Dispatch: by declaration id as today, read from the value's tag instead of from the object, so one load fewer and the same sequence for an `int` receiver as for an instance.
- Collector: a slot is scanned by its tag (immediate or address). An object's size and slot count still come from the table by its id.
- Allocation in generic code: `Box<T> { ... }` writes the `T` it was passed into the payload. No lookup.
- `reflect.Type.of_value(x)`: build the type from the id, the kind and the arguments.

## Open

1. Nested arguments. In `Box<Foo[]>` the inline argument must be a single index, so `Foo[]` needs one. Either composite types used as arguments are interned (statically at compile time, by lookup at run time), or anything nested uses the list form.
2. Argument lists: interned (the slow path is a pointer compare) or compared structurally. Likely intern the static ones and compare the rest.
3. Run-time declarations (`Session.eval`, `with_types`): whether a packed binary supports them at all. If yes, they are the users of `WIDE`, or the table has to grow.
4. How the native backend passes type arguments to generic functions today. The "types loaded once" refusals in `baml_compiler2_native/src/check.rs` suggest only the static case exists.
5. Where the argument-list word goes in an array (3 words today) if its element type ever needs the list form.
6. Say exactly which bits of a tag must equal the object's header (the collector bits and the busy bit do not), so the rule can be checked in a debug build.
7. Arguments and results are two words each (ruling 1). The calling convention for that is not written anywhere: how many registers, how a float payload travels, what the task shim, the method table and the unwinder's landing registers pass.
8. What the collector's stack maps record for a value in a frame whose tag is only known at run time: it needs the tag to know whether the payload is an address.
