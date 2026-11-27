# Calls

How compiled BAML calls compiled BAML: direct calls, interface dispatch, function values, and the thunks between call layouts. Numbers are instructions retired per operation on arm64 macOS; see `README.md` for the method. Native numbers are counted at `881e4c1dfc` unless they say otherwise. "Before" is the layout before two-word values, as these notes had it at `39d3ad4c25`. Go and Node numbers are the ones measured then and were not run again.

## Registers and convention

Compiled functions use Cranelift's `tail` convention, unpatched in this respect: integer arguments in `x2`..`x7`, the rest on the stack, which the callee pops. `x0`/`x1` are left for what a handler lands with (the thrown value's tag and payload). `x21` is pinned: it holds the address of the worker (the `Mutator`), and every access to the worker addresses off it directly. `x29` frame records are kept in every function, because the collector and the unwinder walk them.

Every argument and every result is two words, a tag and a payload, whatever its type (`codegen.rs`, `Backend::clif_signature`):

```rust
let (closure, receiver) = match entry {
    Entry::Direct => (0, 0),
    Entry::Value => (1 + usize::from(checks), 0),   // the closure word; with checks, the caller's layout id
    Entry::Bound => (1 + usize::from(checks), 1),
};
clif.params = repeat_n(AbiParam::new(WORD), closure)
    .chain(repeat_n(AbiParam::new(WORD), type_args))          // one type object address per type parameter the function needs
    .chain(repeat_n(AbiParam::new(WORD), 2 * (values - receiver)))  // tag, payload, tag, payload, ...
    .collect();
clif.returns = vec![AbiParam::new(WORD); 2];                  // x2 = tag, x3 = payload
```

So three values travel in registers, two after a closure word. A `float` travels as its bits in an integer register. An argument left to its default is the tag `OMITTED` with payload 0. A function with type parameters is compiled once and holds every value of a type parameter as a pair; it takes a type object word only for a parameter it tests, hands to `baml.json`, or hands on.

Inside a function a value whose static type fixes its tag (`int`, `float`, `bool`, a class, an array, a map, a `string`, an enum, a future) is its payload alone; the tag is made as a constant, or read from the object's header for a `string` (which may be a slice), right before the call or the store that needs it. A union, `unknown`, an optional, a type parameter and a function value are held as the pair.

The first words of the worker are what compiled code reads through `x21` (`src/layout.rs`):

| offset | field | read by |
|---|---|---|
| 0 | `alloc_ptr` | inline allocation |
| 8 | `alloc_limit` | inline allocation |
| 16 | `stack_limit` | function entry check |
| 24 | `poll` (u32) | loop back edge |
| 32 | `scratch` | slow paths, the context of a throw at its landing |
| 40 | `hash_seed` | map probes |
| 48 | `cur_task` | switch stubs |
| 56 | `tele_node` | reserved, unused |
| 64 | `g0_sp` | switch stubs |
| 72 | `anchor` (sp, pc, fp) | anchor stubs |

## Direct call

A call of a known function is a `bl`. The callee's entry makes a frame record and, unless it is a leaf, compares `sp` with the worker's stack limit. Small callees are inlined in MIR before codegen, so the probe uses a self-recursive callee.

```baml
function leaf(a: int, b: int, d: int) -> int {
  if (d == 0) { return a + b; }
  return leaf(a, b, d - 1);
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < N; i += 1) { s = leaf(s, i, 0); };
  return s;
}
```

Emitted (`otool -tvV` of the linked probe):

```asm
; main, loop body
  mov  x6, #0x2               ; INT, the tag of all three arguments
  mov  x7, #0x0               ; d
  mov  x5, x20                ; b = i      (a = s is already in x3: the last result's payload)
  mov  x2, x6                 ; a.tag
  mov  x4, x6                 ; b.tag
  bl   _baml.user.leaf.1
  add  x20, x20, #0x1
  cmp  x20, x19
  b.lt loop

; leaf
  stp  x29, x30, [sp, #-0x10]!
  mov  x29, sp
  ldr  x12, [x21, #0x10]      ; worker.stack_limit
  cmp  sp, x12
  b.lo overflow
  cmp  x7, #0x0               ; the callee reads no tag: its parameters are int
  b.eq base
  ...
base:
  mov  x2, #0x2               ; result tag
  add  x3, x3, x5
  ldp  x29, x30, [sp], #0x10
  ret
```

20 instructions per iteration: 3 for the loop, 6 at the call site, 11 in the callee. The four more than before are tag constants nobody reads: three argument tags and the result tag.

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native | 10 M | 60 M | 215,764,189 | 1,215,889,280 | 20.00 |
| native, before | 10 M | 60 M | 174,477,945 | 974,567,770 | 16.00 |
| Go (at `39d3ad4c25`) | 10 M | 60 M | 229,234,092 | 1,280,754,945 | 21.03 |
| Node (at `39d3ad4c25`) | 20 M | 120 M | 919,780,180 | 3,523,869,304 | 26.04 |
| native, empty loop | 10 M | 60 M | 85,726,747 | 435,760,649 | 7.00 |
| Go, empty loop (at `39d3ad4c25`) | 10 M | 60 M | 69,041,225 | 319,479,554 | 5.00 |

The empty loop is `s = (s ^ i) + 3`: 5 instructions of work plus the 2 of the poll, unchanged.

Go equivalent:

```go
//go:noinline
func leaf(a, b, d int) int {
	if d == 0 { return a + b }
	return leaf(a, b, d-1)
}
for i := 0; i < n; i++ { s = leaf(s, i, 0) }
```

A pure call and return chain, `down(n) = n == 0 ? 0 : down(n - 1) + 1` called at depth 1000: 14.03 instructions per level (155,991,116 at 10,000 outer iterations, 857,247,289 at 60,000), 12.09 before, 13.09 Go (at `39d3ad4c25`). The two more per level are `mov x2, #2` before the call and before the return.

A call with more than three values passes the rest on the stack: per call one `add sp` in the callee's return and one `sub sp` in the caller after the `bl`, plus a store and a load per word (read in the listings of `tests/conformance/calls.baml`; not counted here).

A direct call whose site has fewer or differently ordered operands than the callee has parameters is laid over the callee's parameters when it is compiled (`check::call_slots`): a slot the site leaves out is passed as `(OMITTED, 0)`. No check and no thunk.

## Interface dispatch

A call whose receiver type is known is resolved to a direct call in MIR. Otherwise the code address comes from the receiver. An instance and an enum variant have the address of their class object at offset 8, and the class object holds one code address per method column of the program (`CLASS_METHODS`, offset 56). Any other receiver (an `int`, a `string`, an array) is looked up by its tag in a table the compiler emits per column, `TAGS_END` (31) words.

```baml
interface Shape { function area(self, k: int) -> int throws never }
class Square { side: int  implements Shape { function area(self, k: int) -> int throws never { return self.side + k } } }
class Rect   { w: int h: int implements Shape { function area(self, k: int) -> int throws never { return self.w - k } } }
function main() -> int {
  let shapes: Shape[] = [Square { side: 3 }, Rect { w: 5, h: 3 }];
  let s = 0;
  for (let i = 0; i < N; i += 1) { s = shapes[i & 1].area(s); };
  return s;
}
```

```asm
  add     x0, x2, x3, lsl #4      ; the element's slot is at storage + 16 + 16 * index
  mov     x1, #-0x8000000000000000
  add     x2, x0, #0x10
  ldsetal x1, x2, [x2]            ; lock the slot: x2 = its tag
  tbnz    x2, #0x3f, <the ldsetal>
  add     x1, x0, #0x18
  ldar    x3, [x1]                ; payload: the receiver
  add     x0, x0, #0x10
  stlr    x2, [x0]                ; the tag back: release
  sub     w0, w2, #0x7            ; INSTANCE is 7, ENUM_VARIANT 8
  cmp     w0, #0x1
  b.ls    by_class
  adrp    x0, tag_row@PAGE        ; any other tag: the column's table
  add     x0, x0, tag_row@PAGEOFF
  ldr     x0, [x0, w2, uxtw #3]
  b       call
by_class:
  ldr     x0, [x3, #0x8]          ; class object
  ldr     x0, [x0, #0x38]         ; its row, this column
call:
  mov     x4, #0x2                ; k.tag (receiver in x2, x3; k in x5)
  blr     x0
  ldr     w0, [x21, #0x18]        ; poll
  cbnz    x0, poll_slow

; Square.area: a leaf, no stack check
  stp  x29, x30, [sp, #-0x10]!
  mov  x29, sp
  add  x7, x3, #0x18
  ldar x7, [x7]                   ; self.side: an atomic load takes no offset
  mov  x2, #0x2
  add  x3, x7, x5
  ldp  x29, x30, [sp], #0x10
  ret
```

The dispatch is 6 instructions for an instance or a variant (tag test 3, two loads, `blr`) and 8 for any other receiver. Before it was 5 for every receiver, from one table for the whole program. A program whose implementors are all classes still emits the tag test.

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native, dispatch loop | 10 M | 60 M | 425,763,703 | 2,476,125,616 | 41.00 |
| native, a loop reading a field of `Square[]` instead | 10 M | 60 M | 225,756,360 | 1,275,850,821 | 21.00 |
| native, before, dispatch loop | 10 M | 60 M | 304,479,009 | 1,754,753,822 | 29.00 |
| native, before, field loop | 10 M | 60 M | 194,480,465 | 1,094,581,802 | 18.00 |
| Go, dispatch loop (at `39d3ad4c25`) | 10 M | 60 M | 189,190,339 | 1,040,680,601 | 17.02 |
| Go, field loop (at `39d3ad4c25`) | 10 M | 60 M | 98,914,748 | 499,394,163 | 8.00 |

The field-loop probe of the earlier measurement is lost; this one is `s = shapes[i & 1].side + s` over a `Square[]`, written again from its description.

Where the 12 more in the dispatch loop come from, counted in the listing: the element of a `Shape[]` is a pair read under the slot's lock, 8 instructions where one `ldr` was; the dispatch is 6 where 5 were; the callee is 8 where 6 were (`add` and `ldar` for the field, and the result tag); one argument tag at the site; and the scaled index is an `add` of its own. The array is still reloaded from its stack slot and its length and storage address re-read on every iteration, and the loop bound, which no longer fits an immediate at 60 M, is rebuilt each time (2).

Weak points: every read of an interface-typed slot takes the lock. No inline cache and no guarded devirtualization exist. A method an implementation declares with more parameters than the interface is entered through a thunk (below).

## Function values

A function value is a callable object: its header holds the tag and, in bits 32..58, its call layout id; the code address is at offset 8 of every callable, and the object itself is the closure word (`layout.rs`, `CLOSURE_CODE = 8`, `CLOSURE_FUNCTION`, captures as values from offset 32). A function named as a value is its function object, in the executable's data; a closure and a bound method point at theirs, which says how many type objects and cells they hold. A function value is always held as a pair, and its tag carries the layout id, so the caller has it in a register. A captured local lives in a cell object (header, one value) and the closure's capture is a value pointing at it.

Every call through a function value checks that the callee's layout is the site's (`codegen/layouts.rs`, `call_value`). Layout ids count from 1, per distinct parameter layout in the program.

```baml
function main() -> int {
  let k = 3;
  let fs: ((int) -> int throws never)[] = [(x: int) -> int { x + k }, (x: int) -> int { x - k }];
  let s = 0;
  for (let i = 0; i < N; i += 1) { s = fs[i & 1](s); };
  return s;
}
```

```asm
  ...                          ; locked load of fs[i & 1] into x14 (tag), x2 (address): 8, as above
  lsr  x0, x14, #32            ; the callee's layout id, from the tag
  cmp  x0, #0x1                ; the site's
  cset x0, eq
  uxtb w0, w0
  cbz  x0, trap                ; no other layout can arrive here: the cold arm is a trap
  ldr  x0, [x2, #0x8]          ; code
  mov  x3, #0x2                ; x.tag    (x2 = closure, x3/x4 = x)
  blr  x0
  ldr  w0, [x21, #0x18]        ; poll
  cbnz x0, poll_slow

; the lambda: a leaf
  stp  x29, x30, [sp, #-0x10]!
  mov  x29, sp
  add  x9, x2, #0x28
  ldar x9, [x9]                ; the capture's payload: the cell of k
  add  x9, x9, #0x18
  ldar x9, [x9]                ; the cell's payload
  mov  x2, #0x2
  add  x3, x4, x9
  ldp  x29, x30, [sp], #0x10
  ret
```

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native | 10 M | 60 M | 455,791,839 | 2,656,044,972 | 44.00 |
| native, before | 10 M | 60 M | 284,454,244 | 1,634,777,174 | 27.00 |
| Go (at `39d3ad4c25`) | 10 M | 60 M | 179,126,425 | 980,708,703 | 16.03 |
| Node (at `39d3ad4c25`) | 20 M | 120 M | 2,417,841,812 | 12,525,096,567 | 101.07 |

Where the 17 more come from, counted in the listing: the locked load of the function value (8 where one `ldr` was), the layout check (5), the argument and result tags (2), two more in the callee (atomic loads take no offset), and the loop bound and index as in the dispatch loop.

The check is three instructions (`lsr`, `cmp`, `b.eq`) where another layout can arrive and the cold arm is a chain of compares, as in the listing under "Thunks". Where no other layout can arrive the cold arm is a trap and Cranelift emits five (`lsr`, `cmp`, `cset`, `uxtb`, `cbz`), as above: a compare-and-trap that is not fused. That is the common site, so this is worth fixing.

Weak point: `k` is never assigned after capture, yet it is read through a separate object (two loads), and each is an atomic load.

## Thunks between layouts

A function with defaulted parameters can stand where a function type with fewer parameters is expected (`ideas/default-arguments.md`). Two kinds of thunk close the gap, both compiled functions with a frame record, an ordinary call and no name, so a traceback has no frame for them (`codegen/layouts.rs`).

A remap thunk is per pair of layouts. A site whose check fails compares the callee's id with each layout the site can be laid over and calls the thunk for it, which passes the site's arguments on with `(OMITTED, 0)` in each slot the site does not have, through the same function value:

```baml
function f(a: int, extra: int = count(2)) -> int throws never { ... }
function apply(g: (int) -> int throws never, x: int) -> int throws never { ... g(x) }
apply(f, count(5))
```

```asm
; apply: x2 = g's tag, x3 = g, x4/x5 = x
  lsr  x0, x2, #32
  cmp  x0, #2                 ; the site's layout (_)
  b.eq enter
  cmp  x0, #1                 ; f's layout (_, extra?)
  cset x0, eq
  uxtb w0, w0
  cbz  x0, trap
  mov  x3, #2 ; mov x2, x1 ; mov x4, x5
  bl   remap                  ; the thunk for (_, extra?) from (_)
  b    done
enter:
  mov  x2, x1 ; mov x4, x5
  ldr  x0, [x2, #8]
  mov  x3, #2
  blr  x0

; remap (_, extra?) from (_)
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  ldr  x8, [x2, #8]           ; the callee's code
  mov  x5, #4                 ; extra = (OMITTED, 0)
  mov  x6, #0
  blr  x8
  ldp  x29, x30, [sp], #16
  ret
```

The callee tests `cmp x5, #4` for `OMITTED` at its entry and computes its default.

A method-table thunk is in a class object's row, or a tag row, where the implementation declares more parameters than the interface's method: it has the interface method's layout and calls the implementation directly with `(OMITTED, 0)` for the extras. Nothing is checked at a dispatched call. The interface method's layout is taken from the dispatching sites' `argument_layout`.

Over the conformance programs at the commit that built them (`8362bb03c7`, a counter since removed): 20 remap thunks in 9 programs and 10 method-table thunks in 2, all in the cases written for them. Not counted again at this commit.

Code compiled with checks (`CompileOptions::checks`, `native_pack --checked`) passes the site's layout id after the closure word, and every entry reached through a value ends the program when it is not its own.

A closure passed to a function that MIR inlines is itself inlined: the `closure-apply-1m` workload compiled to a loop with no call (at `39d3ad4c25`; not looked at again).

## Not built

- Tail calls between BAML functions. The convention supports them; the collector's stack walk assumes none ("no function here makes a tail call", `heap.rs` `Copy::stack`).
- Any inline cache or profile-guided devirtualization.
- Skipping the layout check at a site no other layout can reach, or fusing its compare and trap.
- Widening the argument registers past `x7`.
- A native or an intrinsic taking an omitted operand, and a class's own `to_json`, `from_json` or `eq` with extra defaulted parameters (refused when checked).
