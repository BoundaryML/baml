# Calls

How compiled BAML calls compiled BAML: direct calls, interface dispatch,
function values. Numbers are instructions retired per operation on arm64
macOS; see `README.md` for the method. Every number is measured unless it
says estimated.

## Registers and convention

Compiled functions use Cranelift's `tail` convention. Arguments and results
start at `x2`; `x0`/`x1` are left for what a handler lands with (the error
and its context). `x21` is pinned: it holds the address of the worker (the
`Mutator`), and every access to the worker addresses off it directly.
`x29` frame records are kept in every function, because the collector and
the unwinder walk them.

`crates/baml_compiler2_native/src/codegen.rs`, `Backend::new`:

```rust
flags.set("opt_level", "speed")?;
flags.set("enable_pinned_reg", "true")?;        // x21 = worker
flags.set("fold_pinned_reg_reads", "true")?;    // vendored patch: [x21, #off] addressing
flags.set("preserve_frame_pointers", "true")?;  // collector walks frame records
isa.set("sign_return_address", "false")?;       // no PAC to strip in the stack walk
```

```rust
// Backend::signature
let mut clif = ir::Signature::new(CallConv::Tail);
```

The first words of the worker are what compiled code reads through `x21`
(`src/layout.rs`):

| offset | field | read by |
|---|---|---|
| 0 | `alloc_ptr` | inline allocation |
| 8 | `alloc_limit` | inline allocation |
| 16 | `stack_limit` | function entry check |
| 24 | `poll` (u32) | loop back edge |
| 32 | `scratch` | slow paths, throw context |
| 40 | `hash_seed` | map probes |
| 48 | `cur_task` | switch stubs |
| 56 | `tele_node` | reserved, unused |
| 64 | `g0_sp` | switch stubs |
| 72 | `anchor` (sp, pc, fp) | anchor stubs |

## Direct call

A call of a known function is a `bl`. The callee's entry makes a frame
record and, unless it is a leaf, compares `sp` with the worker's stack
limit. There is no poll at entry and no other bookkeeping. Small callees
are inlined in MIR before codegen (`MAX_CALLEE_SIZE = 24` statements,
`baml_compiler2_mir/src/optimize/inline.rs`), so the probe uses a
self-recursive callee, which the inliner leaves alone.

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

Emitted (`otool`/`objdump` of the linked probe):

```asm
; main, loop body
  mov  x4, #0                 ; arg d
  mov  x3, x19                ; arg b = i   (a = s is already in x2)
  bl   _baml.user.leaf.1
  add  x19, x19, #1           ; i += 1     (result s came back in x2)
  cmp  x19, #N
  b.lt loop

; leaf
  stp  x29, x30, [sp, #-16]!  ; frame record
  mov  x29, sp
  ldr  x8, [x21, #16]         ; worker.stack_limit
  cmp  sp, x8
  b.lo overflow               ; cold: baml_rt_panic_stack_overflow
  cmp  x4, #0
  b.eq base
  ...
base:
  add  x2, x2, x3
  ldp  x29, x30, [sp], #16
  ret
```

The loop has no poll: a loop that calls a function with an entry check is
not given one (see `control.md`, and the weakness noted there).

Cost, 16 instructions per iteration: 3 for the loop, 3 at the call site,
10 in the callee (2 frame, 3 stack check, 2 test, 1 add, 2 return).

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native | 10 M | 60 M | 174,477,945 | 974,567,770 | 16.00 |
| Go | 10 M | 60 M | 229,234,092 | 1,280,754,945 | 21.03 |
| Node | 20 M | 120 M | 919,780,180 | 3,523,869,304 | 26.04 |
| native, empty loop | 10 M | 60 M | 84,429,491 | 434,551,492 | 7.00 |
| Go, empty loop | 10 M | 60 M | 69,041,225 | 319,479,554 | 5.00 |

The empty loop is `s = (s ^ i) + 3`: 5 instructions of work plus the 2 of
the poll. Go has no loop poll.

Go equivalent:

```go
//go:noinline
func leaf(a, b, d int) int {
	if d == 0 { return a + b }
	return leaf(a, b, d-1)
}
for i := 0; i < n; i++ { s = leaf(s, i, 0) }
```

A pure call and return chain, `down(n) = n == 0 ? 0 : down(n - 1) + 1`
called at depth 1000: 12.09 instructions per level native
(135,296,680 at 10,000 outer iterations, 739,649,868 at 60,000), 13.09 Go
(150,646,375 and 805,331,534).

What dominates: the frame record and the stack check, 5 of the 10 callee
instructions. Leaf functions skip the check but still make the record.

## Interface dispatch

Programs are compiled whole. A call whose receiver type is known is
resolved to a direct call in MIR (`optimize/devirtualize.rs`). Otherwise
the code comes from one table for the whole program: the receiver's type id
picks the row, the method the column
(`baml_compiler2_native/src/dispatch.rs`, `codegen.rs` `dispatch`).

```rust
let type_id = self.builder.ins().uload32(slot_access(), receiver, 0);
let methods = self.data_address(methods);
let row = self.builder.ins().ishl_imm(type_id, i64::from(program.column_shift));
let row = self.builder.ins().iadd(methods, row);
let code = self.builder.ins().load(WORD, MemFlags::trusted().with_readonly(),
                                   row, offset(slot.column * layout::WORD));
```

The table has one row per 8 bytes of the type table, so a type id (a byte
offset into the type table) indexes it with a shift. It is sparse: most
rows are zero.

```baml
interface Shape { function area(self, k: int) -> int throws never }
class Square { side int  implements Shape { function area(self, k: int) -> int { return self.side + k } } }
class Rect   { w int h int implements Shape { function area(self, k: int) -> int { return self.w - k } } }
function main() -> int {
  let shapes: Shape[] = [Square { side: 3 }, Rect { w: 5, h: 3 }];
  let s = 0;
  for (let i = 0; i < N; i += 1) { s = shapes[i & 1].area(s); };
  return s;
}
```

```asm
  ldr  x2, [x15, x7, lsl #3]   ; receiver = shapes[i & 1]
  ldr  w0, [x2]                ; type id: low 32 bits of the header
  adrp x1, methods@PAGE        ; table address, column folded in
  add  x1, x1, #0x200
  ldr  x0, [x1, x0]            ; row = type id (shift is 0 here)
  blr  x0
  ldr  w0, [x21, #24]          ; poll: the callee is not known to check the stack
  cbnz x0, poll_slow

; Square.area: a leaf, no stack check
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  ldr  x5, [x2, #8]            ; self.side
  add  x2, x5, x3
  ldp  x29, x30, [sp], #16
  ret
```

Cost: 5 instructions to dispatch (type id, two for the table address, row
load, `blr`), 6 in this callee. The table address is not hoisted out of the
loop.

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native, dispatch loop | 10 M | 60 M | 304,479,009 | 1,754,753,822 | 29.00 |
| native, same loop reading a field instead | 10 M | 60 M | 194,480,465 | 1,094,581,802 | 18.00 |
| Go, dispatch loop | 10 M | 60 M | 189,190,339 | 1,040,680,601 | 17.02 |
| Go, field loop | 10 M | 60 M | 98,914,748 | 499,394,163 | 8.00 |

So a dispatched call of a 2-instruction method is 11 instructions over a
field read here and 9 in Go. The gap in the loop total (29 against 17) is
not the dispatch. It is the loop: the array is reloaded from its stack
slot and its length and data pointer re-read on every iteration (3
instructions), the index is bounds checked (2), and there is a poll (2).
The reload is because the array is a root across a call that can collect:
the stack map names a slot, and the value is read back from it after every
call. Go keeps the slice in registers.

Weak points: one global table costs a relocation pair per call site and a
data cache line per receiver type per method column, with rows spaced by
type size. Go's itab is one load from the interface value. No inline cache
and no guarded devirtualization exist.

## Function values

A function value is a closure object: header, code address, captures
(`layout.rs`, `CLOSURE_CODE = 8`, `CLOSURE_CAPTURES = 16`). The code takes
the closure as its first argument. A captured local lives in a cell
object (header, value) and the closure's captures word points at it.

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
  ldr  x2, [x4, x5, lsl #3]    ; closure = fs[i & 1]
  ldr  x0, [x2, #8]            ; closure.code
  blr  x0                      ; x2 = closure, x3 = x
  ldr  w0, [x21, #24]          ; poll
  cbnz x0, poll_slow

; the lambda: a leaf
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  ldr  x6, [x2, #16]           ; closure.captures -> cell of k
  ldr  x6, [x6, #8]            ; cell.value
  add  x2, x3, x6
  ldp  x29, x30, [sp], #16
  ret
```

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native | 10 M | 60 M | 284,454,244 | 1,634,777,174 | 27.00 |
| Go | 10 M | 60 M | 179,126,425 | 980,708,703 | 16.03 |
| Node | 20 M | 120 M | 2,417,841,812 | 12,525,096,567 | 101.07 |

The call is 2 instructions (load code, `blr`), the callee 7. The rest of
the 27 is the same loop overhead as the dispatch loop. Go's closure call is
the same shape (load code from the closure, `blr`, context in a register).
Node's number includes JIT tiers and is approximate.

Weak point: `k` is never assigned after capture, yet it is read through a
separate object (two loads). Storing an unassigned capture in the closure
itself would save one load and one allocation.

A closure passed to a function that MIR inlines is itself inlined: the
`closure-apply-1m` workload compiles to a loop with no call.

## Not built

- Tail calls between BAML functions. The convention supports them; the
  collector's stack walk assumes none ("no function here makes a tail
  call", `heap.rs` `Copy::stack`).
- Any inline cache or profile-guided devirtualization.
