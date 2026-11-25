# Uniform values: convention, stack maps and the open items

Status: built through `52abac7468`, except ruling 11's use of type arguments in objects (below). The as-built description, with emitted code and measured costs, is in `../memory.md`, `../calls.md`, `../natives.md`, `../gc.md`, `../control.md` and `../tasks.md`. The sections after "The busy bit protocol" are the analysis as proposed, kept for the reasoning; its instruction counts are estimates made before building, and the measured ones are in those notes.

Built: rulings 1 to 10 and 12 to 15; of ruling 11, type arguments as raw type-object addresses, a function with type parameters compiled once, and a union of type parameters built and interned when the program runs.

Not built: the words in objects that say their type arguments. An instance's type parameter slots are `(NULL, 0)`, an array's `ty` and a map's `key_ty` and `value_ty` are zero. So `x is Box<int>` on `unknown` or between two instantiations of one class, and `x is int[]`, are refused when compiled ("type test: between instantiations of one class", "between array types"), and so are two different implementations for one class at different type arguments. The first of these was accepted for scalar arguments before this change, when each instantiation had its own type id: of the ten functions of `baml_tests/baml_src` that compiled then and are refused now, seven are refused for this (`../completeness.md`, census). Writing the words in some makers and testing by them would give wrong answers, so none is written. Three questions for the lead stand in the way:

1. Where a closure and a method entered through a row get their type arguments: closure captures of types and `BoundMethodObject.type_args` as `../memory.md` has them, and a method's from its receiver's slots.
2. What `Self.Item` is at run time: the standard library's adaptor classes make instances such as `Map<Self, Self.Item, U, E>`, whose `Self.Item` is not a type parameter. It needs an associated type looked up on a type object.
3. Whether every native that makes an array or a map is told the type or copies it: the natives' ABI passes types to seven natives today.

Also open, from building: a store of a pointer whose static type fixes its tag takes the lock and writes both words (an exempt class would make it one atomic store); mutable covariance would put a tag in a slot the exemption for `int`, `float` and `bool` slots assumes never changes; nothing above one slot is safe under several workers (array growth and append, map insert and growth, a string's lazy hash). It settles on paper the nine open items of the uniform-values handoff, on top of `../memory.md` ("Values", "Tags", "Objects") and `default-arguments.md`.

## Rulings

The lead ruled on section 14 on 2026-11-24. These override the recommendations below wherever they differ; the analysis is kept, and each superseded recommendation is marked where it stands. The principle, in the lead's words: bias toward correctness and simplicity over optimizations; optimization passes come after it all works. Where the simple choice skips an optimization, the skipped one is named.

1. Tag rule. A tag's type id (bits 0..31, ruling 15) is always exact. A callable's layout id (bits 32..58) is always exact, in the object header and in every tag of a function value. Nothing else is in a tag: code that needs a string's cached hash or the panic bit reads the object, and bits 59..63 of a tag are zero (ruling 15). Consequences: `STRING` never stands for `STRING_SLICE`, so the collector rewrites the tag of every slot that points at a slice it makes a string of; a string's tag is made from the low half of the object's header where it is not already in a register. Supersedes the per-field table of section 1.
2. Every store of a value to memory writes both words. No payload-only stores, with the one exception ruling 13 makes for a slot whose header cannot change. Supersedes 2.6.
3. Argument registers stay as the vendored `tail` convention has them (`x2`..`x7`): closure word, then type arguments, then tag and payload as two `I64` per value; the rest on the stack. Cranelift is not patched. Result in `x2`, `x3`. Supersedes 2.3 (skipped: widening to `x15`).
4. One `INSTANCE` id for every class; the method row lives in the class object. `class_ptr` is traced by the collector: classes and enum variants cannot be made at run time today but will be, so nothing may assume they live outside the heap. An enum key in a map is hashed by a stable field of the variant object (its index), never by its address.
5. A value whose tag is only known at run time is one 16-byte unit, tag low, with a pair entry in the stack map. `NULL`'s payload is always 0, but an optional is an ordinary two-word value: `T?` is never held as one word. Supersedes the `T?` row of 2.5 (skipped: one-word optional pointers).
6. The code address is at offset 8 of every callable, so one call sequence serves all of them.
7. A map record is `{ key: Value, value: Value }`, with the hashes in the parallel array.
8. Every indirect call through a function value does the layout check. The whole-program rule for skipping it is not built. Supersedes that bullet of 2.7.
9. A throw lands with `x0` tag, `x1` payload, and the context through `worker.scratch` as 2.9 describes, taking care that scratch is not a root.
10. A throwing native returns a `Value` with one bit of the tag meaning "thrown". The bit is 59 (`layout::THROWN`), not bit 0, which ruling 15 made part of the type id: 59 is the collector's forwarded bit, which only an object header ever has, so it is zero in every tag, and a tag with it is never stored. The caller's test is one instruction (`tbnz x0, #59`). Exact-typed scalar parameters of natives stay unwrapped at the Rust boundary.
11. Type arguments are raw type-object pointers, one word each, interned when built at run time.
12. `OWNED` and `PLAIN` stay in this change; `RustInstanceObject` is later.
13. The busy bit (63) protocol is built in this change, not merely reserved: several workers are coming. The sequences are in "The busy bit protocol" below. Panic class is bit 62. Supersedes section 8.
14. Inside a function the compiler may drop a tag it knows statically, but only where that is the simple thing; never at the cost of a second representation in memory or across a call.
15. The header format (ruled later, overrides every `id << 3` and "bits 0..2" below). Bits 0..31 are the type id, dense, with no shift: `NULL` 0, `BOOL` 1, `INT` 2, `FLOAT` 3, `OMITTED` 4, then the object tags in order. Bits 32..58 are the header payload (a string's hash, a callable's call layout id, a class object's column count). Bit 59 is the collector's forwarded bit, bit 60 its mark while slices are settled, bit 61 is free, 62 is the panic class, 63 the busy bit. A forwarding word is `address | 1 << 59`; a word is 64 bits on every target. A 32-bit read of a header or a tag is the type id with no mask. A table indexed by tag scales the id in the load's addressing mode. Which bits a tag keeps: a callable's tag is bits 0..58 of its header, so a value whose static type can be a callable gets its tag from the 64-bit header word; every other pointer's tag is bits 0..31, read as 32 bits, which drops a string's hash and the panic bit; an immediate's is the id. No mask is applied for the collector's bits, which are zero whenever compiled code runs. For an object whose type is only known at run time the header is read whole and bits 32..58 kept only when the id is a callable's. One helper on each side makes a tag of a header: `tag_of_object` in the compiler, `layout::tag_of` (under `Value::reference`) in the runtime; the collector's checked-build assertion compares a slot's tag with exactly that.

Scope and order: the value model first. The indirect-call layout check, remap thunks, compile-time mapping of direct calls from `argument_layout`, method-table thunks, the checked-build entry assertion and defaulted arguments through `OMITTED` are the last phase, each its own commit. The layout id is in the header from the first commit that touches the header.

## The busy bit protocol

A slot is two words that several workers may read and write. Bit 63 of the header word is a lock on the slot. There is one rule: **whoever reads or writes a slot's header word holds the bit while it touches the payload**. Readers take it too. That is the simplest protocol that is correct, and it is what is built.

Why a reader cannot merely wait for the bit and re-check the header. Take a slot holding `(STRING, p1)`. A reader loads the header, sees `STRING`. Writer A stores `(INT, 7)`. Writer B starts storing `(STRING, p2)` and has not yet written the payload. The reader loads the payload, 7. Writer B finishes. The reader loads the header again: `STRING`, equal to its first read. It returns `(STRING, 7)`. A re-check detects a change only if the header never returns to a word it held before, and a busy bit carries no count. So the reader locks.

Write, of `(tag, payload)` to a slot whose header may change:

```
retry:
  old = atomic_or(slot.header, BUSY)     ; SeqCst. arm64 with LSE: ldsetal
  if old & BUSY: spin hint; goto retry   ; someone else holds it; the or changed nothing (compiled code: no hint)
  atomic_store(slot.payload, payload)    ; SeqCst
  atomic_store(slot.header, tag)         ; SeqCst, bit clear: this is the release
```

Read, of a slot whose header may change:

```
retry:
  old = atomic_or(slot.header, BUSY)
  if old & BUSY: spin hint; goto retry
  payload = atomic_load(slot.payload)
  atomic_store(slot.header, old)         ; release, header unchanged
  value = (old, payload)
```

Orderings. Every operation is sequentially consistent, which is what Cranelift's atomic instructions give and the strongest there is; nothing weaker was reasoned about. It is sufficient because the bit gives mutual exclusion: `atomic_or` returns the old word and sets the bit in one step, so exactly one worker sees the bit clear, and the payload and the header are written only by the holder. A reader therefore sees a pair some writer wrote whole. The release store publishes the payload written before it. A tag in a register never has the bit (the reader keeps `old`, which had it clear).

Exempt slots, and why:

- A slot whose declared type is exactly `int`, `float` or `bool`. Its header is written once, when the slot is made, and never changes, so a store is one atomic store of the payload and a read is one atomic load of it; either order of two racing stores leaves a valid value. Generic code that reaches such a slot at a type it does not know runs the full protocol, which is also correct here: it releases with the same header.
- A read that needs only the payload because the static type fixes the type id (a class, an array, a map, a `string`, a future, and so on, not optional). One atomic load of the payload: whatever writer wins, the word is the address of an object of that type, and what else the tag would say (slice or not, layout id) is read from the object. A store to such a slot still runs the full protocol and writes both words.
- A slot of an object no other worker can reach yet: every slot written between an allocation and the first store of the object's address anywhere. Plain stores of both words.
- A future's value. It is written once, before the state word is stored with release ordering, and read only after the state is loaded settled. The state word is its lock.
- Frames and registers. A spilled pair in a stack slot belongs to one worker.
- The collector. The world is stopped at safepoints and no protocol sequence contains one, so no slot has the bit set while it runs. It reads and writes slots with plain loads and stores, never sets the bit, and in checked builds asserts the bit is clear on every slot it scans.

One place each. In the compiler (`codegen.rs`): `slot_lock` is the acquire, `slot_store_words` the locked store, `slot_load_words` the locked load; `slot_store` picks between the locked store and the one atomic payload store of an `int`, `float` or `bool` slot, `slot_load` between the locked load and the one atomic payload load, and `slot_init` is the plain store of both words into an object not yet reachable (a new instance, tuple, cell, closure, bound method, array literal). In the runtime (`heap.rs`): `Value::lock`, `Value::store`, `Value::load`, `Value::payload_at` (the one atomic payload load), `Value::init` (plain, unreachable objects and a future's value) and `Value::unshared` (plain read of a settled future's value). The runtime does not know a slot's declared type, so everything it does to a reachable slot is the locked form. Nothing else touches a shared slot's header. The hand-written map probes read only the payload of a record's key, whose type id the map's kind fixes, with one `ldar`; they return the address of the value's slot, which the caller reads or writes through the functions above.

As built, on arm64 with LSE (the host ISA is detected by `cranelift_native`, which enables `has_lse` on Apple silicon; without it Cranelift emits a load-exclusive loop through fixed registers). Taken from a build of a store and a load of a field `v: int | string` at offset 16 of the instance in `x3` (store) or `x1` (load):

```asm
; locked store of (x1, x5)
  mov     x0, #0x8000000000000000   ; BUSY, once per function
  add     x4, x3, #16               ; the slot
  ldsetal x0, x6, [x4]              ; x6 = header, header |= BUSY
  tbnz    x6, #63, <the ldsetal>    ; held by another worker: again
  add     x0, x3, #24
  stlr    x5, [x0]                  ; payload
  stlr    x1, [x4]                  ; tag: the release

; locked load into (x2, x3)
  add     x0, x1, #16
  mov     x5, #0x8000000000000000
  ldsetal x5, x2, [x0]
  tbnz    x2, #63, <the ldsetal>
  add     x1, x1, #24
  ldar    x3, [x1]                  ; payload
  stlr    x2, [x0]                  ; the header back: the release
```

Six instructions each and one for the constant, where two plain stores or loads were. An `int` field store is `add; stlr` and a `string` field load `add; ldar`: the atomic instructions take no offset. Differences from the sequences above: compiled code retries with no spin hint, since Cranelift has no instruction for one and a call would put a safepoint candidate in every function that stores (the runtime's `Value::lock` uses `std::hint::spin_loop`).

What the `int`, `float`, `bool` exemption needs, and what was changed for it: a store of the payload alone leaves whatever tag the slot had, so such a slot must have its tag from the moment it can be stored to. Three places did not give it one. A cell made without a value was `null`; it is now the zero of its content type. `baml.Array._results<U>`, which makes the array `map` fills by index, made `null` slots; it is now passed its types and makes zeros when `U` is `int`, `float` or `bool`. An append in place writes a slot past the length, which is `null`; it always runs the locked store of both words.

Read-only data. A locked load writes the header, so no slot it reaches may be in read-only memory. None is: the objects the compiler places in the executable (strings, bigints, class, variant, package and type objects, closures that capture nothing) have no slots, and the runtime's one static object with slots, the entries every empty map shares, has no record a search can match.

What this does not give. It makes one slot safe. It does not make an array's growth, append or removal, a map's insert, delete, growth or compaction, or a string's lazy hash safe under several workers; those need their own rules (`../threads.md`). In particular a removal that clears a slot to `null` beside a payload-only store to it can leave `null` with a payload. What is tested with one worker: the sequences run on every access of the conformance suite, checked builds assert the bit is clear on every slot the collector scans (object slots, pair entries of stack maps, the runtime's root values), and two runtime tests run `Value::store`, `Value::load` and payload-only stores against each other from six threads on one slot. What is not: contention in compiled code, that is its retry path, and any interleaving of compiled sequences. That waits for several workers.

Cheaper variants, not built: no lock for a slot nothing stores to after it is made (a closure's captures, a bound method's receiver); a 128-bit compare-and-swap (`casp`) where the target has one, which makes a reader a single load pair that retries; a reader that takes no lock where the static type rules out the header returning to an old word; an exempt class for exact pointer slots whose tag is a constant (a store would be one atomic payload store).

How to read it. **Fact** means read in the code or measured, with the place. **Proposal** means a choice made here. **Unverified** means not checked by building or running. All assembly is hand-written to show the shape, not compiler output. "Today" counts are the measured ones from the as-built notes; "new" counts are estimates counted from the hand-written sequences.

## 0. Summary

| # | item | recommendation |
|---|---|---|
| 1 | instance id | One `INSTANCE` id. Method row inside the class object. |
| 2 | calling convention | Closure word, type arguments, then (tag, payload) pairs as separate 64-bit integer parameters. Widen `tail` to `x2`..`x15` (7 values; ruled out, ruling 3). Result in `x2`, `x3`. Thrown value lands in `x0`, `x1`, context in `worker.scratch`. |
| 3 | stack maps | A value of run-time tag is one `I128` with the tag in the low half, in a 16-byte slot; the table entry's low bit says "pair". Exact pointers stay one-word entries. |
| 4 | map keys | Record is `{ key: Value, value: Value }`, 32 bytes, hashes stay in the parallel `u32` array. Probes keep their shape. |
| 5 | builtin interface | Exact parameters unwrapped; others as two `u64` parameters; a throwing native returns a `Value` in `x0`, `x1` with bit 0 of the tag meaning "thrown". |
| 6 | type arguments | One word each, a raw `*TypeObject`. Static types' objects live in program data. |
| 7 | busy bit | Reserve bit 63, emit plain stores. (Ruled out, ruling 13: the protocol is built.) |
| 8 | missing structs | Section 9. Every callable keeps its code address at offset 8. |
| 9 | well-known classes | A table of class object addresses, same order as `WELL_KNOWN` then `NAMED`. |

Instruction counts, common cases (new is estimated):

| operation | today | new | why |
|---|---|---|---|
| direct call, 3 `int` arguments | 13 | 17 | one `mov` per argument tag, one for the result tag |
| interface dispatch, receivers all classes | 5 | 3 | row is in the class object |
| interface dispatch, receivers include `int`/`string` | not possible unboxed | 5 instance path, 8 other | tag test, per-tag row table |
| closure call, no layout check | 2 | 2 | needs code at offset 8 (section 9) |
| closure call, with layout check | n/a | 5 | check is 3, on the tag, no load |
| allocation of `P { x: int }` | 9 emitted, 10 per object | 12 emitted, about 13 to 14 per object | 32 bytes, 4 words stored |
| field load, exact type | 1 | 1 | payload word only |
| field load, run-time tag | 1 (a box pointer) | 1 (`ldp`) | |
| field store, exact non-optional | 1 | 1 | payload only, proposal 2.6 |
| field store, optional pointer | 1 | 4 | tag rebuilt from the payload |
| array index | 3 to 5 | 4 to 6 | no `lsl #4` addressing mode |
| map get, string key | 37 to 39 | 37 to 39 | probe keeps its instruction count |

## 1. Tag rules (decide these first, the rest depends on them)

**Proposal**, numbering: `NULL` = 0, `BOOL` = 1, `INT` = 2, `FLOAT` = 3, `OMITTED` = 4, then the object layouts. A tag word is `id << 3` (superseded by ruling 15: the id is not shifted, and the collector's bits are 59 and 60). So a zeroed 16 bytes is a `null` value, "is a pointer" is `cmp w, #(4 << 3); b.hi` (now `cmp w, #4; b.hi`), and "is null" is `cbz w`. **Fact**: today no type id is zero (`layout.rs`, `RUNTIME_TYPES`), so nothing relies on a zero header meaning anything.

**Proposal**: a `NULL` value's payload is always 0. `memory.md` says "unused"; this is a change to that table. It lets an optional pointer of exact type be one word inside a function. (The first half stands; the one-word optional is superseded by ruling 5.)

### Can a tag be a strict copy of the header? No.

Ruling 8 says a pointer's tag is a copy of the object's header. Three facts from the code make a strict copy impossible or costly:

1. The handoff's own model keeps only the payload of an exact-typed value in a register and makes the tag "as a constant". A constant cannot carry a string's hash.
2. **Fact**: a string's hash is written into its header lazily (`baml_rt_str_hash`, `map.rs`). Any copy taken before that is stale.
3. **Fact**: the collector turns a slice into a string of its own (`heap.rs`, `settle_slices`). The object's id changes from `STRING_SLICE` to `STRING` under every slot that points at it.

Cost of the two readings, per pointer whose tag must be made (an argument, a result, a store):

```asm
; strict copy: load the header
  ldr  x2, [x3]               ; 1 load per pointer, per call or store
; strict copy, optional pointer: cannot load through null
  mov  x2, #0
  cbz  x3, 1f
  ldr  x2, [x3]               ; 3 instructions, 1 load, 1 branch
1:
; canonical tag: a constant
  mov  x2, #INSTANCE          ; 1 instruction, no load
; canonical tag, optional pointer
  cmp  x3, #0
  mov  w2, #INSTANCE
  csel x2, x2, xzr, ne        ; 3 instructions, no load, no branch
```

Strict copy also makes the collector rewrite the tag of every slot that points at a flattened slice.

**Proposal**, the tag invariant, per group of bits (superseded by ruling 1: the id is exact with no `STRING` exception, the hash and the panic bit are not in a tag; and by ruling 15: the id is bits 0..31, the collector's bits are 59 and 60):

| bits | in a value's tag |
|---|---|
| 0..2 | always zero. The collector's bits belong to object headers. A mutator never sees them set: collection stops the world. |
| 3..31 | the object's id, exactly. One exception: `STRING` may stand for `STRING_SLICE`. Code that cares reads the object's header, as it does today. |
| 32..58 | per kind, below |
| 62 | panic class: exact. Known statically for an exact class, copied otherwise. |
| 63 | zero in a register. In memory, only what the busy protocol puts there. |

Bits 32..58 mean different things by kind, and the two uses never meet:

| kind | bits 32..58 | in a tag |
|---|---|---|
| callable (`CLOSURE`, `BOUND_METHOD`, `REALIZED_FUNCTION`, `HOST_CLOSURE`, a `RAW_FUNCTION` used as a value) | call layout id, written when the object is made, never changed | must be present and exact. The indirect-call check reads it from the tag. |
| `STRING`, `STRING_SLICE` | hash cache, 27 bits, zero while not computed | zero or the hash. Nobody reads a hash from a tag; the probe reads the object's header as today. |
| everything else | zero | zero |

**Fact**: the hash mask today is 28 bits (`STRING_HASH_MASK = 0x0FFF_FFFF`); it loses one bit.

Consequence for callables: a function type has five layouts and a layout id per function, so its tag is never a compile-time constant. **Proposal**: a function value is never "exact". It is always carried as a full pair, in registers and in frames. Its tag starts as a constant where the closure is made (the compiler knows the id and the layout) or as the header word of the static function object, and is copied from then on. No load is ever needed to have it.

## 2. Calling convention (OPEN 2)

### 2.1 Facts about the vendored `tail` convention

- `abi.rs:126`: integer arguments start at `x2`. `abi.rs:148,301`: `max_per_class_reg_vals = 8` and the test is `next_reg < 8`, so `x2`..`x7`: six words, three values. The same code assigns results, so a result pair is `x2`, `x3`.
- The comment says `x0` is kept for a return-area pointer. The code would put one in `x8` (`abi.rs:153`). We return two words and never have one. `x0` and `x1` are simply unused by the convention, which is why a landing can use them (`exception_payload_regs`, `abi.rs:1189`, exactly `[x0, x1]`).
- An `I128` parameter that would start at an odd register skips one (`abi.rs:260`, the skip applies to every convention but Apple's), and goes whole to the stack when fewer than two registers remain. Today's optional scalars are passed this way (`clif_type` returns `I128` for a pair).
- Stack arguments are popped by the callee: the callee's epilogue adds `tail_args_size` to `sp` (`abi.rs:658`) and the caller subtracts it again after the call (`emit.rs:2963`). Two instructions per call that has any stack argument, on top of one store and one load per word.
- Incoming stack arguments are loaded into virtual registers by explicit instructions at entry (`machinst/abi.rs:1595`).
- Cranelift's own use of `x8`..`x17`: `x16`, `x17` are its spill temporaries and are not allocatable (`regs.rs:122,136`); the anchor stubs use `x16`. `x8` is only the C struct-return register, and struct return asserts under `tail` (`abi.rs:189`). `x9` is a temporary only for results carried on the stack (`machinst/abi.rs:2499`), which we never have. `x8`..`x15` are otherwise ordinary caller-saved allocatable registers (`create_reg_env`). Stack probes on arm64 use `x16`, `x17` and are not enabled.

### 2.2 Proposal: the order and the registers

```
x2            closure word          only for a function entered through a value; a raw pointer, not a value
then          type arguments        one word each, *TypeObject (section 7)
then          values                tag, payload, tag, payload, ...  each a separate I64 parameter
result        x2 = tag, x3 = payload
x0, x1        unused by calls; a landing's thrown value
x21           worker
```

Separate `I64` parameters, not one `I128` per value: no register is skipped after an odd closure word, and a value may straddle the last register and the stack.

A float payload travels as bits in an integer register. It cannot use a `d` register: under function variance the callee may take `int | float` there. Cost: one `fmov` on each side that has the float in a `d` register.

A receiver is a value like any other (`x2`, `x3` for a direct or dispatched method), so an implementation for `int` and one for a class have the same signature and need no boxing thunk.

### 2.3 Three values or seven

| | A: keep `x2`..`x7` | B: widen to `x2`..`x15` |
|---|---|---|
| values in registers | 3 (2 after a closure word and one type argument) | 7 |
| patch | none | `compute_arg_locs`: the integer limit becomes 16 under `tail`; float limit stays 8. A few lines. |
| each value past the registers | 2 stores, 2 loads, and once per call `add sp` in the callee and `sub sp` in the caller | same, but rare |
| risk | none | a seventh vendored patch; register pressure at calls is unchanged since `x8`..`x15` are already clobbered by every call |

**Recommendation: B.** (Superseded by ruling 3: A.) Three values is `self` plus two arguments; a closure call with three arguments already spills. Nothing in the backend, the stubs or the runtime names these registers: the preserving stub already saves `x0`..`x17`, the anchor stub touches only `x16`, natives use the C convention. **Unverified**: that the patch is as small as it reads (regalloc2 fixed-register constraints on `x8`..`x15`, `return_call` paths we do not use). It needs one build and the conformance run. I have no arity histogram for the corpus; the census could give one and would settle this with data.

### 2.4 Direct call

`s = leaf(s, i, 0)` from `calls.md`, all three parameters `int`:

```asm
; hand-written. today: 3 at the site, 10 in the callee = 13
  mov  x2, #INT               ; a.tag
  ; x3 = s is already there: the last result's payload came back in x3
  mov  x4, #INT               ; b.tag
  mov  x5, x19                ; b = i
  mov  x6, #INT               ; d.tag
  mov  x7, #0                 ; d
  bl   _baml.user.leaf.1      ; 6 at the site

leaf:
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  ldr  x8, [x21, #16]
  cmp  sp, x8
  b.lo overflow
  cmp  x7, #0                 ; the callee does not read x2, x4, x6: its parameters are int
  b.eq base
  ...
base:
  add  x3, x3, x5
  mov  x2, #INT               ; result tag
  ldp  x29, x30, [sp], #16
  ret                         ; 11 in the callee
```

17 against 13. The four extra are tag constants nobody reads. That is the price of ruling 1; the rejected "typed in registers" would have saved exactly these. They are register moves of constants with no dependency, so the cost in time is small, but it is in every call.

A callee whose parameter is `int | string` reads the tag (`cmp w4, #INT`). A callee with a defaulted parameter tests `cmp w4, #OMITTED` as in `default-arguments.md`.

### 2.5 Inside a function

| static type | held as | Cranelift type | in stack maps |
|---|---|---|---|
| `int`, `bool`, enum pointer excluded | payload | `I64` | no |
| `float` | payload in a `d` register | `F64` | no |
| class, array, map, string, bigint, bytes, tuple, enum, future, cell | payload (address) | `I64` | word entry |
| `T?` of one of the line above | payload, 0 for `null` | `I64` | word entry |
| `int?`, `float?`, `bool?`, union, `unknown`, generic `T`, interface, any function type | tag and payload | `I128`, tag low | pair entry |

(Superseded by ruling 5: `T?` is a pair.) `T?` as one word checks against the rules: it needs "`NULL`'s payload is 0" (section 1) and nothing else. The tag is rebuilt with `cmp`, `csel` where a tag is needed.

### 2.6 Stores and loads

(Superseded by rulings 2 and 13: every store writes both words under the busy protocol, except to an exact `int`, `float` or `bool` slot.) **Proposal**: a slot whose declared type is exact and not optional always holds that tag, written when the slot is first written (allocation, `push`). A later store writes only the payload. This is the same case `memory.md` already gives for `int` under the busy protocol. It is sound for a `readonly` view at a wider type because that view cannot write.

```asm
; hand-written
  ldr  x1, [x0, #24]          ; load p.x, x: int             (slot 0 at 16, payload at 24)   today 1, new 1
  ldp  x1, x2, [x0, #16]      ; load p.v, v: int | string    tag, payload                    today 1, new 1
  str  x1, [x0, #24]          ; store p.x = ..., exact                                       today 1, new 1
  stp  x1, x2, [x0, #16]      ; store p.v = ..., run-time tag                                today 1, new 1
  cmp  x1, #0                 ; store p.next = q, next: Node?
  mov  w9, #INSTANCE
  csel x9, x9, xzr, ne
  stp  x9, x1, [x0, #32]      ;                                                              today 1, new 4
```

Alternative: always write both words (`mov` tag, `stp`), 2 for every exact store. Simpler rule, one more instruction on the commonest store. Not recommended.

### 2.7 Function values and call layouts

This part of `default-arguments.md` stands; only the encoding of "omitted" changed (it is the `OMITTED` tag on each argument).

- Every callable object's header holds its call layout id in bits 32..58. Static function objects in program data have it too.
- A function value is always a pair (section 1), so its tag, with the layout id, is in a register at the call. No load.
- A direct call is mapped onto the callee's layout at compile time with `CallLayout::map_to`; missing optional slots are passed as `OMITTED`. No check, no thunk.
- An indirect site checks that the callee's layout is exactly the site's. Unequal goes to a remap thunk for that pair of layouts, which re-lays the arguments with `OMITTED` in every slot the site does not have and calls through the same function value.
- A method-table row whose implementation has extra or reordered optionals holds a thunk with the interface method's layout. Nothing is checked at a dispatched call.
- (Superseded by ruling 8: every site checks.) Whole-program rule: a site needs the check only if some function value in the program has a different layout that is compatible with the site's. Otherwise the site is the bare 2-instruction call.
- Checked builds: an entry reached through a value takes one extra word after the closure word, the site's layout id, and traps if it is not its own. Release builds do not pass it.

```asm
; hand-written. x4 = the function value's tag, x5 = its address. site layout L_SITE
  ubfx x9, x4, #32, #27       ; callee's layout id, from the tag
  cmp  w9, #L_SITE
  b.ne remap                  ; cold
  ldr  x9, [x5, #8]           ; code, same offset in every callable (section 9)
  mov  x2, x5                 ; closure word
  blr  x9                     ; 5 with the check (3 + 2), 2 without. today 2
resume:
  ...
remap:                        ; one compare per layout that can arrive here, x9 still holds the id
  cmp  w9, #L_FOO
  b.eq via_foo
  udf
via_foo:
  bl   remap$L_FOO$from$L_SITE
  b    resume
```

The check in `default-arguments.md` section 4 compares the whole word with `CLOSURE | L_SITE << 32`. Two notes on that form. The constant does not fit a compare immediate, so it is `mov`, `movk`, `cmp`, `b.ne` (the `and` can go: bits 0..2 and 63 are zero in a register tag by section 1, and a callable never has bit 62). And it sends a `BOUND_METHOD` of the right layout to the cold path. The static type already says the value is a callable, so comparing only the layout field is enough and is one instruction shorter. Either is correct. **Recommendation**: the layout-only form above.

### 2.8 Interface dispatch

**Proposal**: the class object holds its method row at a fixed offset, one code address per column, zero where the class has no code. The compiler knows the whole program, so it knows whether every implementor of the interface is a class.

```asm
; hand-written. receiver in x2 (tag), x3 (address)
; (a) every implementor is a class. today 5, new 3
  ldr  x9, [x3, #8]           ; class_ptr
  ldr  x9, [x9, #ROW + 8*col]
  blr  x9

; (b) some implementor is not an instance (int, string, an array)
  cmp  w2, #INSTANCE
  b.ne by_tag
  ldr  x9, [x3, #8]
  ldr  x9, [x9, #ROW + 8*col]
call:
  blr  x9                     ; 5 on the instance path
by_tag:
  adrp x10, tag_rows$col@PAGE
  add  x10, x10, tag_rows$col@PAGEOFF
  ldr  x9, [x10, w2, uxtw]    ; the tag word is id << 3: already a byte offset (ruling 15: `uxtw #3`, the id is dense)
  b    call                   ; 8 on the other path
```

`tag_rows$col` is one table per column with one code address per tag id (about 31 entries, 248 bytes). `STRING_SLICE` has `STRING`'s entry. An enum variant and a Rust instance keep their class-like pointer at offset 8 with the row at the same offset (section 9); if their ids sit next to `INSTANCE`, the test in (b) becomes `sub w9, w2, #INSTANCE; cmp w9, #(2 << 3); b.hi by_tag`, one more instruction.

The `to_string` fallback (`slot.falls_back`, a zero in the row) is unchanged.

### 2.9 Throw, landing, shims, hooks, stubs

**Facts**. The throwing stubs take the error in `x0` and the context in `x1`, pass worker, frame and a 48-byte `Resume` in `x2`..`x4`, and land with `x0` = error, `x1` = context (`stubs.rs`, `throwing_stub!`; `unwind.rs`, `Resume`). Cranelift gives a landing exactly two payload registers. `worker.scratch` carries a trace today only on the async-native paths (`STEP_THREW`, the hook entries); a landing does not read it. The starting point said scratch is "already used for the throw context"; that is true only for those paths.

**Proposal**:

- A thrown value is two words. The stubs take `x0` = tag, `x1` = payload, `x2` = context; worker, frame and `Resume` move to `x3`..`x5`. `Resume` grows to 56 bytes, 64 on the stack.
- A landing has `x0` = tag, `x1` = payload. The unwinder leaves the context in `worker.scratch`; the landing block's first instruction loads it, and only in a handler that binds the context or may rethrow.
- `scratch` is not a root. This is safe because nothing can allocate between the unwinder's store and that load, and the allocation slow path's own use of `scratch` comes later. It must stay the first instruction of the block.
- Alternative: widen `exception_payload_regs` to three registers. Another vendored patch for one load on a cold path. Not recommended.
- `Handler::takes` reads the tag; for `INSTANCE` it compares `class_ptr` with the handler's list, which becomes class object addresses (relocated words in the unwind table). The panic test is bit 62 of the tag, no load.

```asm
; hand-written landing. today: x0 = error, x1 = context
landing:
  ldr  x9, [x21, #32]         ; context, only if this handler reads it
  cmp  w0, #INSTANCE
  b.ne no_match
  ldr  x10, [x1, #8]          ; class_ptr
  adrp x11, class$E@PAGE
  add  x11, x11, class$E@PAGEOFF
  cmp  x10, x11
  b.ne no_match
  ldr  x4, [x1, #24]          ; e.code
```

- Task shim. **Fact**: one shim per machine result type today, chosen at the `spawn` site and passed to `baml_rt_spawn` with a "value is a reference" flag (`codegen/spawn.rs`). **Proposal**: one shim. It calls `baml_rt_task_finish(worker, tag, payload)`, which stores the pair into `FutureObject.value`. `baml_rt_spawn` loses the shim and flags arguments, and `FUTURE_VALUE_IS_REF` goes: the slot's tag says it. The shim's uncaught block gets `x0`, `x1` and passes tag, payload and the trace from `scratch` to `baml_rt_task_fail`.
- `await`. The ready path is `ldar` of the state, then `ldp` of the value (or `ldr` of the payload word at exact type). Same count as today.
- Switch frame result word, `task.future`, `task.entry`: each is always a future or a closure address. They stay single-word roots.
- `scope.call_baml` hooks. **Fact**: an entry takes values "as a union holds them" in four words (`NATIVE_CALL_ARGS = 4`) and returns two words, the value and whether it threw. **Proposal**: the four words are two values (an `eq` takes exactly two); the entry returns the value in `x0`, `x1` with bit 0 of the tag set when it threw (section 6), the trace in `scratch` as today.
- `native_poll`. **Fact**: it returns `Step { kind, word }` in two registers, and a done native's value is the word. A value is now two words, and three words do not come back in registers (measured, section 6). **Proposal**: on `STEP_DONE` the value is left in the block's first two argument words and the driver stub does `ldp x2, x3, [x26]`: one more instruction on the done path.
- Anchor stubs: unchanged, four instructions, arguments pass through untouched. Preserving stubs: unchanged.

## 3. Stack maps (OPEN 3)

### 3.1 Facts

- `declare_value_needs_stack_map` asserts only `size <= 16` and a power of two (`frontend.rs:554`). `declare_var_needs_stack_map` asserts `<= 16`.
- The spiller makes one explicit slot per value, sized and aligned to the value's type (`safepoints.rs:554`: `I128` gets `explicit_slot 16, align = 16`), stores at the definition, reloads after each safepoint, and records `UserStackMapEntry { ty, slot, offset }`. Its own test covers `I128` (`needs_stack_map_and_heterogeneous_types`).
- The emitted map keeps the type: `UserStackMap::entries()` yields `(ir::Type, offset from sp)`. Our `define` throws the type away today (`codegen.rs:1350`, `|(_, slot)| slot`).
- An `I128` store on arm64 is `stp lo, hi` and a load is `ldp` (`lower.isle:2667,2779`). `iconcat(lo, hi)` puts `lo` at the lower address.
- Today's pair is `iconcat(bits, present)`: payload low, flag high (`codegen.rs:1776`). The starting point said the new pair is "as optional scalars already are"; the mechanism is the same, the order is the reverse.
- The collector walks frame records, binary-searches the return address, and calls `forward(sp + offset)` on each listed slot (`heap.rs`, `Copy::stack`). `forward` ignores a word that is not an address in the space being copied, so zero and program data are skipped.

### 3.2 Proposal

- A value whose tag is known only at run time is one `I128` SSA value, `iconcat(tag, payload)`, declared as needing a stack map. `isplit` and `iconcat` cost no instructions. In a frame it is 16 bytes, tag at the lower address: the same bytes as a `Value` in the heap.
- An exact pointer, and an optional exact pointer, stays an `I64` with a one-word entry, as today. An exact immediate is not in the map.
- The table keeps its format. A slot word is the offset from `sp`, a multiple of 8, with bit 0 set for a pair. `define` sets the bit when the entry's type is `I128`.

```
table:    functions, then per function: address, sites, then per site: return offset, n, n slot words
slot word:  offset            a word that is null or an address      (ty = I64)
            offset | 1        a Value: tag at offset, payload at offset + 8   (ty = I128)
```

```rust
// Copy::stack, per frame
for &entry in maps.slots(pc) {
    let slot = sp + (entry & !7) as usize;
    if entry & 1 == 0 {
        self.forward(slot);
    } else if load(slot) as u32 > LAST_IMMEDIATE_TAG {
        self.forward(slot + WORD);
    }
}
```

```asm
; hand-written: a union-typed local live across a call. x8 = tag, x9 = payload
  stp  x8, x9, [sp, #32]      ; one instruction, as for a reference today (str)
  bl   callee                 ; stack map of this site: 33  (32 | pair)
  ldp  x8, x9, [sp, #32]      ; one instruction (ldr today)
```

Under the tag rules of section 1 the collector never rewrites a tag. Under a strict-copy reading it would have to, for every pair slot that points at a flattened slice, and `forward` would need to know which of its callers has a tag beside the word.

### 3.3 Cost

Per frame today: 75 (57 in the walk and the search, the rest in `forward`). The walk and the search do not change. Per entry, estimated: a word entry adds 2 (test of the pair bit); a pair entry adds about 5 (the bit, the tag load, the compare) and then either calls `forward` as today or, for an immediate, skips it, which is about 25 fewer than a slot that holds an already-copied reference. So a frame holding one reference is about 77, and one holding a union value is about 80 if it is a pointer and about 55 if it is an `int`. Decode cost per table word is unchanged.

Frame size: a pair slot is 16 bytes and 16-aligned where a box pointer was 8.

### 3.4 Incoming stack arguments

They need nothing. The callee loads each into a register at entry (fact, 2.1); a reference among them that is live across a call is spilled to the callee's own slot and listed there. The words left in the argument area are never read again, by the callee or the caller. The caller's own copy, if it still needs one, is in the caller's slot. The frame-record walk still holds: the callee's record is pushed at its entry `sp`, which is the caller's `sp` at the call, so `sp = fp + 16` is right with stack arguments too. **Unverified** by a test with a collection inside a callee that has stack arguments holding the only reference; add one conformance case under `BAML_GC_STRESS`.

A throw past a callee with stack arguments is also fine: the handler's `sp` comes from the unwind table (`fp - frame_offset`), not from the popped state.

## 4. Instance id (OPEN 1)

| | A: one `INSTANCE` id | B: an id per class (today) |
|---|---|---|
| tags | closed Rust `enum`, about 31 | open set, one per class per program |
| type table | gone for classes; size and description come from the class object | a row per class and per generic instantiation |
| collector | `match` on the tag; an instance's slot count is one load from its class object | size and reference offsets from the type table, as today |
| `Box<int>` against `Box<string>` | same class object, generics in the first slots | two ids, as today |

```asm
; hand-written. x is a run-time value: tag x2, address x3
; x is Foo, A:                       5 and the branch on the result
  cmp  w2, #INSTANCE
  b.ne no
  ldr  x9, [x3, #8]
  adrp x10, class$Foo@PAGE
  add  x10, x10, class$Foo@PAGEOFF
  cmp  x9, x10
; x is Foo, B:                       1, no load: the tag is already in a register
  cmp  w2, #FOO
; today:                             2: ldr w, [x]; cmp

; dispatch, A: section 2.8, 3 (two dependent loads) or 5 / 8 with non-instances
; dispatch, B: 4, one load, no tag test, int and string and classes alike
  adrp x10, methods$col@PAGE
  add  x10, x10, methods$col@PAGEOFF
  ldr  x9, [x10, w2, uxtw]
  blr  x9
```

So B is not slower. With tags in registers it is faster on type tests and uniform on dispatch. What it costs is everything ruling 14 defers: an open tag set, per-program type tables, the collector going through a table, an id per instantiation.

Does the code give a reason not to take A? No blocker. Four places key on the header id today and each changes:

1. `switch_on_tag` (`codegen.rs:2010`) builds a Cranelift `Switch` over class ids for a `match` on a union. With one id the key becomes a dense class index loaded from the class object (two loads), or a chain of pointer compares.
2. Handler lists in the unwind table are type ids (`unwind.rs`, `Handler::takes`). They become class object addresses.
3. `x is SomeInterface` is a list of implementor ids (`passing_ids`, `TestKind::Implements`). It becomes one test of the interface's column in the class object's row: `ldr class; ldr; cbnz`, constant in the number of implementors.
4. A class's description (field names, used by JSON and by the uncaught report) hangs off its type table row (`TYPE_DESCRIBED`). It moves to a pointer in the class object.

**Recommendation: A.** One thing to decide with it: if classes can be created at run time (run-time enums suggest it), a class object can be in the heap, `class_ptr` is a traced pointer, and the collector must read the slot count through a possibly forwarded class object. Static class objects are program data and `forward` already ignores them.

## 5. Map keys (OPEN 4)

**Facts**. Key types handled today: `string` (by text), `float` (canonical bits), and `int`, `bool`, enum as bits (`map.rs`, `KeyKind`). Entries are records of 16 bytes followed by a parallel array of `u32` hashes; a small map's probe compares all eight hashes with two NEON compares (`ldp q0, q1, [x3, #144]`). The collector scans an entries object on its own, with no map in hand, which is why there are four entries types today.

Two problems with `MapRecord { key: ValuePayload, hash: u32, value: Value }` in `memory.md`:

- With the hash inside the record the eight hashes are no longer contiguous, and the small-map probe loses its vector compare.
- With no tag on the key the collector cannot tell, from the entries object alone, whether keys are addresses. It would need a flag in the entries header or a back pointer to the map.

**Proposal**: `MapRecord { key: Value, value: Value }`, 32 bytes, and the hashes stay in the parallel array. Same record size as `memory.md`'s (which pads to 32), 4 more bytes per slot for the hash, no special case in the collector, and the key is in the one format.

- `baml_rt_map_find_str` and `baml_rt_map_find_int` keep their shape and instruction count. Offsets change: record stride 32 (`lsl #5`), key payload at +8, value at +16, hashes at `16 + 32 * 8 = 272` for a small map. They return the address of the value's `Value`; compiled code reads `[x0, #8]` at exact type or `ldp` otherwise.
- An enum key is now an address. A variant object is one per variant and never copied by value, so equality is the address. The hash must not be the address (objects move): hash the variant's index, one load. So an enum key uses a third entry point or `find_int` with the index hashed and the address compared. **Unverified**: whether variant objects can live in the heap; if all are program data the address is stable and `find_int` works unchanged.
- `float` keeps going through `baml_rt_map_find`.
- Map get, string key: 37 to 39 today, the same estimated.

## 6. Builtin interface (OPEN 5)

**Measured** (`rustc 1.88`, `aarch64-apple-darwin`, a 20-line file compiled to assembly):

| case | result |
|---|---|
| `extern "C" fn(w: u64, v: Value)`, `Value` is `#[repr(C, align(16))]`, two `u64` | `v` arrives in `x1`, `x2`. No skip to an even register on Apple. |
| seven `u64` then `v: Value` | `v` goes whole to the stack; `x7` is left unused |
| `-> Value` | `x0`, `x1` |
| `-> ` a struct of three `u64` | through `x8`, in memory |

Standard AAPCS64 (Linux) would skip to an even register for a 16-aligned struct. **Unverified** there; it is not the target.

**Proposal**:

- A parameter of exact type stays unwrapped, as today: `i64`, `f64` in a `d` register, `bool`, a raw address for a string, array, map, class or Rust value.
- A parameter whose type is not exact (`Elem`, `Any`, `BoxedElem`, `Typed`, an optional scalar) is two `u64` parameters, tag then payload, in the generated `extern "C"` signature. The wrapper builds the `Value`. Two scalars, not a struct: the same on every target, no skipped register, and the pair can straddle `x7`.
- A native that cannot throw returns its exact result in one register, or a `Value` in `x0`, `x1`.
- A native that can throw always returns a `Value` in `x0`, `x1`. Bit 0 of the tag set means "this is the thrown value". The caller tests `tbnz w0, #0, raise`: one instruction, as `cbnz` on the thrown word is today. Bit 0 is free for this: it is zero in every value's tag (section 1) and this word is never stored. (Superseded by ruling 15: bit 0 is part of the type id; the bit is 59, see ruling 10.) Alternative: a thrown slot in the worker and a flag, which costs a store and a load. `RtResult` and `OptionalScalar` go.
- `TypeId` parameters become `*const TypeObject` (section 7). Most disappear: `elem_raw(word, type)` needed the id only to know whether the word was a reference.
- `RootSlots` become 16 `Value` slots (256 bytes); a root's tag says whether to forward it. `Elem` loses `is_ref`.
- The lists in `natives/*.txt` and the three kinds of call do not change.

## 7. Type arguments at run time (OPEN 6)

**Fact**: a generic function that must know its type arguments takes 32-bit type ids after the closure word (`clif_signature`), and `x is T` compares the header id with the passed id (`type_test`, `TestKind::Argument`).

**Proposal**:

- A type argument is one word, a raw `*TypeObject`, in the same position. Not a value: no tag.
- The `TypeObject` of every type the program names statically is program data, emitted once per distinct type, never moved. `forward` skips it.
- A type built at run time (a generic function making a `T[]` needs the type object of `T[]`) is found or made by a runtime call that interns it, so that "is `x` an `int[]`" stays a pointer compare of `x.ty`. Such an object may be in the heap, so a type argument held across a call is a one-word stack map entry.
- For `x is T` inline, give `TypeObject` a fixed prefix: the tag id its values have (zero when it takes more than a tag to decide) and the class object to compare (zero when none).

```asm
; hand-written. x is T: T in x4, x in x2 (tag), x3
  ldr  w9, [x4, #8]           ; T.tag
  cbz  w9, slow               ; a union, an interface, a generic instance: runtime call
  cmp  w9, w2
  b.ne no
  ldr  x10, [x4, #16]         ; T.class, zero unless T is a class
  cbz  x10, yes
  ldr  x11, [x3, #8]
  cmp  x10, x11
```

**Unverified**: what `RealizedTy` holds and whether it can be read from compiled code at fixed offsets; the prefix above assumes it can be laid out that way.

## 8. Busy bit (OPEN 7)

(Superseded by ruling 13.) **Recommendation**: reserve bit 63 and emit plain stores (`str`, `stp`). There is one worker and collection stops it at a call. Two things to keep true so that the protocol can be added later without touching layouts: a tag in a register never has bit 63 (section 1), and exact slots are written payload-only (2.6), which is the case the protocol already treats as needing no bit.

## 9. Structs missing from `memory.md` (OPEN 8)

**Proposals**, in that file's style. Sizes round up to 16.

```rust
/// Tag `BIGINT`. As today: a count and that many limbs, two's complement, least significant first.
#[repr(C, align(16))]
pub struct BigintObject {
    pub header: ObjectHeader,
    pub limb_count: u64,
    pub limbs: [u64],
}

/// Tag `BYTES`. As today: the length never changes, the bytes are in a storage object.
#[repr(C, align(16))]
pub struct BytesObject {
    pub header: ObjectHeader,
    pub length: u64,
    pub ptr: NonNull<BytesStorageObject>,
}

/// Tag `BYTES_STORAGE`. Packed bytes, no slots.
#[repr(C, align(16))]
pub struct BytesStorageObject {
    pub header: ObjectHeader,
    pub capacity: u64,
    pub data: [u8],
}

/// Tag `TUPLE`.
#[repr(C, align(16))]
pub struct TupleObject {
    pub header: ObjectHeader,
    pub count: u64,
    pub slots: [Value],
}

/// Tag `ENUM_VARIANT`. One object per variant; an enum value is its address, and equality is the address.
#[repr(C, align(16))]
pub struct EnumVariantObject {
    pub header: ObjectHeader,
    /// At offset 8, where an instance has `class_ptr`: dispatch reads the method row the same way.
    pub enum_ptr: NonNull<ClassObject>,
    pub index: u64,
    pub name: NonNull<StringObject>,
}

/// Tag `CLASS`. Also describes an enum (`kind`).
#[repr(C, align(16))]
pub struct ClassObject {
    pub header: ObjectHeader,
    pub package: NonNull<PackageObject>,
    pub name: NonNull<StringObject>,
    /// Generics then fields: an instance is `16 + 16 * slot_count` bytes.
    pub slot_count: u32,
    pub generic_count: u32,
    /// Class, enum, Rust-backed; panic class.
    pub kind: u32,
    /// Dense, for a `match` over classes.
    pub index: u32,
    /// Field names, places and types: what JSON and the uncaught report read. Program data.
    pub description: *const u8,
    /// An enum's variants, or null.
    pub variants: *const NonNull<EnumVariantObject>,
    /// One code address per method column of the program, zero where the class has none. At offset 56.
    pub methods: [usize],
}

/// Tag `PACKAGE`.
#[repr(C, align(16))]
pub struct PackageObject {
    pub header: ObjectHeader,
    pub name: NonNull<StringObject>,
}

/// Tag `OWNED`. As today: a Rust value with a destructor, dropped when the object is not copied.
#[repr(C, align(16))]
pub struct OwnedObject {
    pub header: ObjectHeader,
    pub value: *const (),
    pub kind: &'static RustKind,
}

/// Tag `PLAIN`. As today: `Copy` Rust data held inline.
#[repr(C, align(16))]
pub struct PlainObject {
    pub header: ObjectHeader,
    pub words: u64,
    pub kind: &'static RustKind,
    pub data: [u64],
}
```

Notes:

- `OWNED` and `PLAIN` are kept as two tags, as the runtime has them. `RustInstanceObject` can replace them later; doing it now is a redesign of every `$rust_type` native inside a change that is already large.
- `PackageObject`: I do not know what the interpreter's holds beyond a name. **Unverified**.
- Every callable needs its code address at offset 8, with the object itself as the closure word, as `CLOSURE_CODE = 8` is today. `memory.md`'s `ClosureObject` has `function: NonNull<RawFunctionObject>` there, which makes a closure call three instructions (`ldr function; ldr code; blr`) against two. **Proposal**:

```rust
/// Tag `CLOSURE`. Header payload: the call layout id.
#[repr(C, align(16))]
pub struct ClosureObject {
    pub header: ObjectHeader,
    pub code: usize,
    pub function: NonNull<RawFunctionObject>,
    pub capture_count: u64,
    /// [generics ++ variable cells]
    pub captures: [Value],
}

/// Tag `BOUND_METHOD`. Header payload: the layout of the method without its receiver.
#[repr(C, align(16))]
pub struct BoundMethodObject {
    pub header: ObjectHeader,
    pub code: usize,
    pub function: NonNull<RawFunctionObject>,
    pub type_arg_count: u64,
    pub receiver: Value,
    pub type_args: [NonNull<TypeObject>],
}
```

`REALIZED_FUNCTION`, `HOST_CLOSURE` and `RAW_FUNCTION` get the same first two words when they are built. This change needs only `CLOSURE` and `BOUND_METHOD`: those are the two entries that exist today (`Entry::Value`, `Entry::Bound`).

## 10. Well-known classes (OPEN 9)

**Fact**: the compiler emits the type id of each `WELL_KNOWN` class, then of each `NAMED` class (zero when the program names none), as `u32`s after a count; the runtime finds a class's fields through the description its type table row points at.

**Proposal**: the same table, with the address of each class object in place of its id: 25 words, then 35, zero for an absent `NAMED` class. Relocated words in program data, in the order of the two arrays in `layout.rs`, which stay. The runtime indexes it as today (`WELL_KNOWN_DIVISION_BY_ZERO` and so on), reads `slot_count` and `description` from the class object, and sets bit 62 in the header of an instance of the first 15. Alternative: class objects owned by the runtime as Rust statics. Not possible: a class object's method row has one entry per column of the program.

## 11. Costs up front (estimates)

Bytes:

| object | today | new |
|---|---|---|
| instance, n slots | 8 + 8n | 16 + 16n |
| `P { x: int }` | 16 | 32 |
| `int[]` of n (capacity n) | 24 + 16 + 8n | 32 + 16 + 16n |
| `[i, i]` | 56 | 80 |
| an `int` in an `(int \| string)[]` | 8 + a 16-byte box | 16 |
| map object | 40 | 64 |
| `map<string, int>`, per slot of capacity | 20 | 36 (32 with the hash in the record) |
| cell | 16, or 24 for an optional scalar | 32 |
| future | 48 | 64 |
| closure, one capture | 24 | 48 (32 with `memory.md`'s layout and a 3-instruction call) |
| string of b bytes | 16 + b, to 8 | 16 + b, to 16 |

Allocation of `P { x: i }`:

```asm
; hand-written. today 9 emitted
  ldr  x4,  [x21]
  ldr  x14, [x21, #8]
  add  x13, x4, #32
  cmp  x13, x14
  b.hi alloc_slow
  str  x13, [x21]
  mov  x0, #INSTANCE
  adrp x1, class$P@PAGE
  add  x1, x1, class$P@PAGEOFF
  stp  x0, x1, [x4]           ; header, class_ptr
  mov  x2, #INT
  stp  x2, x3, [x4, #16]      ; x: tag, payload            12 emitted
```

Per object with collections: 10 today. The space fills twice as fast, so twice the collections at the same fixed cost each: about 13 to 14.

`keep[i & 7] = i`, `keep: int[]`:

```asm
; hand-written. today: ldr length; ldr data; and; cmp; b.hs; str x3, [x15, x0, lsl #3]
  ldr  x1,  [x0, #8]          ; length
  ldr  x15, [x0, #24]         ; storage (ptr is the fourth word now)
  and  x0, x19, #7
  cmp  x0, x1
  b.hs slow
  add  x9, x15, x0, lsl #4    ; 16-byte elements: no addressing mode scales by 16
  str  x3, [x9, #24]          ; payload of element: 16 header and capacity, + 8
```

One more instruction: 16.00 per iteration today, about 17. The same `add` is in every index, read or write, exact or not; a read of a run-time-typed element is `add`, `ldp`.

Collector, per live object: 122 today for a 24-byte `Node`. The object is 48 bytes, and the scan reads a tag per slot where it read a table of offsets. Roughly 130 to 145. Per element of a large `int[]`: 2.96 today for a storage that is copied and not scanned; it is now scanned (every slot has a tag to test), so expect about 2 to 3 times that.

## 12. Changes needed in `memory.md`

Not made here; listed for whoever owns the file.

1. "Tags": `NULL`'s payload is 0, not "unused". Give the ids, with `NULL` = 0.
2. "Tags": say which bits a value's tag carries (section 1): that `STRING` may stand for a slice, that a string's hash in a tag is zero-or-right, that a callable's layout id is always there.
3. "Values", the busy rules: add that an exact slot is written payload-only after its first write.
4. `MapRecord`: `{ key: Value, value: Value }` and the hashes in a parallel array.
5. `ClosureObject`, `BoundMethodObject`, `RealizedFunctionObject`, `HostClosureObject`: code address at offset 8.
6. Add the structs of section 9, and the tags `BYTES_STORAGE` and (if an enum is not described by a `ClassObject`) `ENUM`.
7. `TypeObject`: the fixed prefix of section 7.
8. `FutureObject`: `flags` loses "value is a reference".
9. The string hash is 27 bits; `layout.rs` has 28.

## 13. Where the starting points were wrong or incomplete

1. "A pointer's tag is a copy of the header" cannot hold strictly: exact types make the tag a constant, a string's hash is written late, and the collector changes a slice into a string. Section 1 replaces it with a per-field rule.
2. A function value cannot be held as one word: its tag carries the layout id. It is always a pair.
3. The new pair is not "as optional scalars already are": today's is `iconcat(bits, present)`, payload low, and it is passed as one `I128` parameter, which skips an odd register and cannot straddle the stack. The new one is tag low and is passed as two `I64` parameters.
4. `x0` is not used for a return-area pointer by the code, only by the comment; a return area would go in `x8`. `x0` and `x1` are free, which is what the landing relies on.
5. `worker.scratch` does not carry a landing's context today, only the async-native trace. Using it for landings is new, and correct only while the load is the landing's first instruction.
6. Dispatch in three instructions holds only when every implementor is a class. And an id per class is not the slower option: with tags in registers it gives a 1-instruction `x is Foo` and a 4-instruction dispatch with no tag test. The case for one id is closedness and ruling 14, not speed.
7. `memory.md`'s map record breaks the small-map vector compare and leaves the collector unable to scan keys.
8. `memory.md`'s closure layout turns the 2-instruction closure call into 3.
9. A `Value` struct is passed without an even-register skip on Apple arm64 (measured), so that fear is unfounded here, but a native's value plus a thrown flag is three words and does not come back in registers. Hence the thrown bit.
10. Three values in registers is right, but the cost of the fourth was understated: besides the store and load per word there is an `add sp` in the callee and a `sub sp` in the caller on every such call.

## 14. Decisions for the lead

Most consequential first.

1. Tag of a pointer: (A) a strict copy of the header, loaded at each call, return and store, or (B) the rule of section 1 (id exact, callable layout id exact, string hash zero-or-right, `STRING` may stand for a slice)? Recommendation: B.
2. May a store to an exact, non-optional slot write only the payload? Recommendation: yes.
3. Widen `tail` to `x2`..`x15` in the vendored Cranelift (7 values) or keep `x2`..`x7` (3 values)? Recommendation: widen, after one build confirms the patch is small.
4. One `INSTANCE` id with the method row in the class object: yes or no? Recommendation: yes.
5. Values of run-time tag as one `I128`, tag low, with "pair" entries in the stack map: yes or no? Recommendation: yes.
6. Is `NULL`'s payload always 0, so an optional pointer of exact type is one word? Recommendation: yes.
7. Code address at offset 8 of every callable object (closure call stays 2 instructions, objects grow by a word): yes or no? Recommendation: yes.
8. Map record: (A) `{ key: Value, value: Value }` with hashes in a parallel array, or (B) `memory.md`'s `{ key payload, hash, value }`? Recommendation: A.
9. Indirect-call check: (A) the layout field only, 3 instructions, or (B) the whole tag word against `CLOSURE | L << 32`, 4 instructions? Recommendation: A.
10. Thrown value lands in `x0`, `x1` with the context in `worker.scratch`, or widen Cranelift's landing registers to three? Recommendation: `scratch`.
11. A throwing native returns a `Value` with bit 0 of the tag as "thrown": yes or no? Recommendation: yes.
12. Type arguments as raw `*TypeObject` words, interned when built at run time: yes or no? Recommendation: yes.
13. Keep `OWNED` and `PLAIN` in this change and leave `RustInstanceObject` for later: yes or no? Recommendation: yes.
14. Busy bit reserved, plain stores now: yes or no? Recommendation: yes.
15. Can a class or an enum variant object be created at run time and live in the heap? This is a question, not a choice: it decides whether `class_ptr` is traced and whether an enum key can be hashed by address.
