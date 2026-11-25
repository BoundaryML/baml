# Control

Loop polls, the stack check, throw and catch, and how a task's stack is sized. Numbers are instructions retired; method in `README.md`. Native totals in "Loop poll" and "Throw and catch" are counted at `881e4c1dfc`; "before" is the layout before two-word values, at `39d3ad4c25`. Everything else here, the single-step trace and every Go and Node number are from `39d3ad4c25` and were not run again.

## Loop poll

A loop back edge loads a 32-bit word of the worker and branches to a register-preserving stub when it is not zero. `codegen.rs`, `poll`:

```rust
let requested = self.builder.ins().uload32(mutator_access(), mutator, offset(layout::MUTATOR_POLL));
self.builder.set_cold_block(slow);
self.builder.ins().brif(requested, slow, &[], resume, &[]);
// slow: call baml_rt_poll_slow (preserves every register), jump resume
```

```asm
  ldr  w0, [x21, #24]
  cbnz x0, poll_slow          ; cold
```

Two instructions, never taken. The empty loop `s = (s ^ i) + 3` is 7.00 instructions per iteration (85,726,747 at 10 M; 435,760,649 at 60 M), as before; 2 are the poll. Go has none and preempts with a signal: 5.00 (at `39d3ad4c25`).

Where it is emitted: at the latches of a loop that contains no direct call to a function with an entry stack check (`polled_blocks`). A loop that makes such a call gets no poll, on the grounds that the callee's entry check "asks the same question". Loops whose calls are indirect (interface dispatch, closures), calls to leaf functions and calls to natives keep the poll.

State of the mechanism:

- Nothing sets the poll word. `Mutator::poll` is `self.poll = 0` (`heap.rs`), with the comment "Nothing sets it yet". The 2 instructions buy nothing today.
- The stack check cannot stand in for a poll. Its slow path only throws `StackOverflow` (`check_stack`, `unwind::panic_stack_overflow`). Nothing lowers `stack_limit` to request a stop and nothing tells such a trip from a real overflow. So a loop of direct calls has no safepoint a second worker could use to stop it.
- There is no poll at function entry. Unbounded recursion is bounded only by the stack.

Known alternatives: poison the stack limit to request a stop (Go's cooperative path) and make the overflow stub check for it first; or drop loop polls and use signals with stack maps at every instruction, which Cranelift does not provide.

## Stack check

Every function that is not a leaf compares `sp` with the worker's limit at entry. `codegen.rs`, `check_stack`:

```rust
let limit = load(mutator + MUTATOR_STACK_LIMIT);
let stack = self.builder.ins().get_stack_pointer(WORD);
brif(icmp(UnsignedLessThan, stack, limit), slow, resume);
// slow (cold): baml_rt_panic_stack_overflow; trap
```

```asm
  ldr  x8, [x21, #16]
  cmp  sp, x8
  b.lo overflow               ; cold
```

Three instructions. The check is against `sp` after the frame is pushed, with no per-function frame size folded in; the 16 KB reserve below the limit covers that. Go's is the same three instructions for a small frame.

## Stacks

`src/sched.rs`:

```rust
const TASK_STACK: usize = 64 << 10;     // every spawned task
const MAIN_STACK: usize = 8 << 20;      // the task that runs the entry function
const STACK_GUARD: usize = 16 << 10;    // PROT_NONE below the stack
const RUNTIME_STACK: usize = 16 << 10;  // kept for runtime functions; the limit is lo + this
```

A stack is one `mmap` of guard plus size and one `mprotect` of the guard (`map_stack`). It is never grown, moved or shrunk. A finished task's record and stack go on a free list and are reused; nothing is ever unmapped. `switch_in` copies the task's limit into the worker on every switch (`ldr x9, [task, #8]; str x9, [x21, #16]`).

Rust runs on the task's stack: a native is called on the stack of the task that calls it, inside the 16 KB reserve. A runtime function that recurses further (the structural walks) moves to the worker's own stack with `sched::on_stack`.

A function entered below the limit throws `baml.panics.StackOverflow` from its caller. A spawned task therefore has 48 KB for BAML frames: about 3,000 frames of the 16-byte minimum, fewer with locals.

Compared with Go: a goroutine starts with a few kilobytes and grows by copying, so depth is bounded by memory. Here depth in a spawned task is bounded at 48 KB and the cost of a task that never recurses is a 64 KB reservation (one touched page, 16 KB on this machine).

Weak points: the fixed size is both too large (address space and a page per idle task; `idle-tasks-8k` holds 16,383 of them) and too small (a recursive parser in a spawned task overflows where the same code in `main` does not). Growing by copying needs every pointer into the stack to be known; the stack maps describe heap references in frames, not pointers to frames. Segmented stacks or a larger lazily committed reservation are the alternatives. Not built.

## Throw and catch

Compiled code throws by calling a stub that never returns to its caller. A thrown value is two words: the stub is called with `x0` = tag, `x1` = payload (`baml_rt_rethrow` also takes the context in `x2`). It makes a frame record and calls `unwind::throw`, which returns where to resume; the stub installs `sp`, `fp`, `x0` (tag), `x1` (payload), `x21` (worker) and branches. The context (the trace, or a `baml.errors.Context`) does not fit the two registers a landing gets: the unwinder stores it in `worker.scratch` as the last thing it does, and a landing that reads it loads it before anything else. `scratch` is not a root, and nothing allocates between the store and the load. No Rust frame is unwound and no register is restored, so nothing is read from the frames in between.

`src/stubs.rs`:

```asm
baml_rt_throw:                ; throwing_stub!
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  sub  sp, sp, #48            ; room for a Resume: sp, fp, pc, tag, payload, worker
  mov  x3, x21 ; mov x4, x29 ; mov x5, sp
  bl   unwind::throw
  ldp  x2, x3, [sp, #0]       ; handler sp, fp
  ldp  x4, x0, [sp, #16]      ; handler pc, tag
  ldp  x1, x21, [sp, #32]     ; payload, worker
  mov  sp, x2 ; mov x29, x3
  br   x4
```

The search walks frame records outward and looks each return address up, by binary search, in a table of call sites that have handlers. Each handler says what it takes (a list, every error, every panic, `null`), so a handler that would only pass the error on is skipped rather than landed in. A list is 64-bit words in the unwind table (version 3), each a tag (`INT`, `STRING`, ...) or the address of a class object: a handler takes a thrown value whose tag is in its list (a slice reads as `STRING`), or which is an instance or an enum variant whose class object is. Whether the value is a panic is bit 62 of the object's header, read only when the tag is a pointer's. `src/unwind.rs`, `unwind`:

```rust
'frames: for (pc, fp) in frames(mutator) {
    if wanted != Some(true) && let Some((handlers, frame_offset)) = mutator.unwind.handlers(pc) {
        for handler in handlers {
            if !handler.takes(thrown) { continue; }
            if found.is_none() { found = Some((handler.address, fp - frame_offset, fp)); }
            if wanted.is_some() { break 'frames; }
            if handler.flags & HANDLER_BINDS_CONTEXT != 0 { wanted = Some(true); break; }
            if handler.flags & HANDLER_MAY_RETHROW == 0 { wanted = Some(false); break 'frames; }
        }
    }
    trace.push(pc);
}
```

A stack trace is built only when some handler on the way will read it (it binds the context, or the error reaches the top). Then the walk continues to the outermost frame and allocates an `int[]` of return addresses. A handler that takes the error and neither binds the context nor can rethrow ends the search with no trace.

The handler table is decoded from the program's data on the first throw (`Unwinder::tables`, a `OnceCell`).

```baml
class E { code: int }
function thrower(d: int, x: int) -> int throws E {
  if (d == 0) { throw E { code: x }; }
  return thrower(d - 1, x) + 1;
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < N; i += 1) {
    s += thrower(M, i) catch (e) { E => e.code & 1 };
  };
  return s;
}
```

The handler, in `main`:

```asm
  bl   _baml.user.thrower.1   ; a call with an exception edge
  ...                         ; normal return: the result's payload in x3
landing:                      ; x0 = tag, x1 = payload. This handler does not read the context
  sub  w3, w0, #0x7           ; an instance (7) or an enum variant (8)?
  cmp  w3, #0x1
  b.ls by_class
  mov  w3, #0x0               ; anything else is not an E
  b    tested
by_class:
  ldr  x3, [x1, #0x8]         ; its class object
  ldur x4, [sp, #0x10]        ; E's, kept in a frame slot
  cmp  x3, x4
  cset x3, eq
tested:
  uxtb w5, w3
  cbnz x5, take
  ...                         ; not ours: thrown again
take:
  add  x7, x1, #0x18
  ldar x7, [x7]               ; e.code
```

`x is E` on a value is the same six instructions wherever it is compiled: a tag test, a load and a pointer compare. The landing also has a dead arm, behind a branch on a constant zero that Cranelift does not fold, which would read the header of address 0; it is never taken.

Cost per iteration, N 100,000 and 600,000:

| frames between throw and catch | native | raw (lo, hi) | native, before | Go `panic`/`recover` | Go error return |
|---|---|---|---|---|---|
| 1 (thrown in the callee) | 314.42 | 47,155,334; 204,367,125 | 249.75 | 3,072.77 | 181.23 |
| 2 | 373.47 | 53,233,414; 239,972,030 | 297.79 | 3,428.42 | 194.94 |
| 12 | 963.99 | 112,308,373; 594,306,193 | 777.67 | 6,334.35 | 335.05 |
| 102 | 6,277.74 | 643,392,010; 3,782,262,875 | 5,100.56 | 31,855.93 | 1,604.13 |

Go columns are from `39d3ad4c25`. Per extra frame: native 59 (48 before). A throw caught one frame up is 65 more than before. It was not traced, so the split is not known; from the listings, `thrower` allocates a 32-byte `E` in 12 instructions where it took 9, each level of the way down passes and returns two more tag constants, and the landing's type test is a tag test, a load and a compare where it was a load and a compare. The rest is in `unwind`, which now decides by tag and class object.

Go panic 285 per extra frame, Go error return 14. Node `try { throw new P(i) } catch` is about 21,500 (22,962,114,278 at 1 M; 130,450,599,868 at 6 M), approximate.

Single-step trace of one throw caught one frame up on the old layout, 241 instructions (not traced again):

| part | instructions |
|---|---|
| `main`: loop, call, landing, type test, read the field | 22 |
| `thrower`: entry, allocate `E`, call the stub | 19 |
| `baml_rt_throw` stub | 13 |
| `unwind::throw` | 12 |
| `unwind::unwind`: classify the error, walk one frame, one table search, fill `Resume` | 175 |

At 12 frames `unwind` is 604: 39 per frame, each a frame-record load and a binary search that misses. The other 9 per frame are the calls on the way down.

What dominates: the fixed 175 in `unwind` for a table lookup that hits on the first frame. A throw is 10 times cheaper than a Go panic and 1.7 times a Go error return at depth 1, and 3.9 times an error return at depth 102 (Go at `39d3ad4c25`), because each frame costs a search instead of a compare and branch. An obvious improvement is to cache the last handler per throw site.

Go equivalents:

```go
//go:noinline
func thrower(d, x int) int { if d == 0 { panic(&E{x}) }; return thrower(d-1, x) + 1 }
//go:noinline
func catcher(d, x int) (r int) {
	defer func() { if e := recover(); e != nil { r = e.(*E).code & 1 } }()
	return thrower(d, x)
}
//go:noinline
func errer(d, x int) (int, error) {
	if d == 0 { return 0, &E{x} }
	v, err := errer(d-1, x)
	if err != nil { return 0, err }
	return v + 1, nil
}
```

## Entering a catch region when nothing throws

The numbers in this section are from `39d3ad4c25` and were not measured again. Nothing is registered at run time: the handler is a row in a static table. But the call inside the region has an exception edge, and a landing arrives with no register restored. Cranelift therefore treats every register as clobbered across that call.

```baml
s += maybe(0, i) catch (e) { E => e.code & 1 };   // maybe never throws here
```

```asm
main:
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  str  x28, [sp, #-16]!       ; every callee-saved register but the pinned one:
  stp  x26, x27, [sp, #-16]!  ; x19, x20, x22-x28, d8-d15
  ...                         ; 9 pushes, 9 pops on the way out
loop:
  ldur x3, [sp]               ; i lives in a stack slot
  bl   _baml.user.maybe.1
  ...
  stur x9, [sp]
```

| | native | Go |
|---|---|---|
| call inside a handler, per iteration | 28.02 (294,646,361 at 10 M; 1,696,032,805 at 60 M) | 78.07 with a `defer`+`recover` in the callee (799,554,039; 4,703,386,455) |
| same call, handler outside the loop | 26.00 (274,713,037; 1,574,736,420) | 25.01 with no `defer` (269,242,766; 1,519,989,813) |

Two instructions per iteration (one store, one load of the loop state), against 53 for a Go `defer`. The hidden cost is per function, not per iteration: a function that contains a handler saves and restores all 17 callee-saved registers, 9 instructions each way, whether or not it uses them, and keeps every value that lives across the call in memory. A small function with a `catch` that is called in a hot loop pays about 20 instructions for it (counted from the prologue, not measured separately).

## Not built

- Anything that sets the poll word.
- Stack growth.
- A per-site handler cache.
- Telemetry on popped frames (the comment in `unwind` marks the place).
