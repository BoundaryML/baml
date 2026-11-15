# MIR optimizer ledger: redundancy and loops

Scope: the O2 passes in `baml_language/crates/baml_compiler2_mir/src/optimize/redundancy.rs`
and `optimize/loops.rs`, plus one O2 lowering rule for `== null` in `lower.rs`.
Metric: bytecode instructions executed per loop iteration on `bex_vm`, confirmed by wall time.

## Survey

Method: a throwaway test compiled each workload at O2 and printed MIR (`pretty::display_function`)
and bytecode (`baml_tests::engine::display_user_functions`). Counts below are instructions on the
hot path of one iteration, read off those listings.

| Loop | Before | After | What changed |
| --- | --- | --- | --- |
| nested-loops inner | 14 | 13 | rotation |
| collatz `while` (even / odd) | 19 / 22 | 18 / 21 | rotation |
| fib loop | 15 | 14 | rotation |
| array-traversal `indexed` | 15 | 14 | rotation |
| array-traversal `rebuild` | 15 | 14 | rotation |
| array-traversal `iterate` (for-in) | 9 | 9 | none |
| bubble-sort inner (no swap / swap) | 22 / 38 | 21 / 37 | rotation |
| merge-sort merge loop (compare path) | 29 | 28 | rotation |
| quick-sort partition (`<` / `>` / `==`) | 27 / 28 / 18 | 26 / 26 / 17 | rotation, reuse of `a[i]` |
| nbody `advance` inner | 100 | 99 | rotation |
| mixed-ops loop (`i <= 2500`) | 40 | 39 | rotation |
| guard-clauses `validate` (last guard) | 23 | 22 | return threading |
| guard-clauses, divide-guard, if-else `main` loop | 14 to 15 | 13 to 14 | rotation |
| if-else `classify` (mean of the five arms) | 19.6 | 16.2 | reuse of `x % 5`, return threading |
| divide-guard `maybe_divide` | 8 | 7 | return threading |
| splay `splay` loop, per null test | 3 and a native call | 2 | null tag test |
| richards, per null test | 3 or 4 and a native call | 2 | null tag test, negated branch |

What the listings showed, in order of cost:

1. `x == null` and `x != null` compiled to a call of the native `baml.ops.equals_equals` driver.
   Splay and richards each had 19 of them, most inside their hot paths. One call costs far more
   than one instruction: it builds a driver with a work list on every comparison.
2. Every `while` and C-style `for` paid a `jump` back to a test at the top of the loop.
3. Multi-exit functions paid `jump L; L: return` on every exit but the last.
4. `!= null` paid a `unary_op !` before its conditional jump.
5. Repeated pure work was rare. `x % 5` four times in if-else-dispatch. `a[i]` up to three times
   per partition step in quick-sort. `j + 1` three times in the bubble-sort swap.
6. No `Truthy` of a known bool, no `x * 1`, no `x + 0`, no repeated `%` beyond item 5, and nothing
   loop-invariant that is not a heap read. So there is no strength-reduction or algebraic pass.

Two facts about this VM shaped the passes. A value used once stays on the operand stack for free,
and a value used twice needs a store and a load. So reusing a three-instruction computation once
saves nothing, and reusing it on a path the program does not take loses. Second, a `jump` is the
cheapest instruction the VM has, so removing one moves wall time less than its share of the count.

## Passes

The five MIR passes run once, from one call placed right after the cleanup loop in
`optimize_body`. They do not run inside the loop because rotation must come after dead-code
elimination. `effects.rs` proves a dead evaluation cannot trap from integer ranges, and it bounds
a loop counter from the test at the top of the loop. With the test at the bottom the bound is
lost to widening, and dead code in loop bodies stopped being removed (a test in
`tests/optimizer_effects.rs` caught it). Running last keeps every proof as strong as before.
The call sweeps its own leftovers with `eliminate_dead_blocks` and `eliminate_dead_locals`.

### Null literal comparison (`lower.rs`, O2 only)

`e == null` lowers to `is_type_tag(e, NULL)`, and `e != null` to its negation, when one side is
the `null` literal. The driver compares runtime kinds first and never consults an `Equals` impl
for operands of different kinds, so the tag test is the same answer. This is a lowering rule
because the optimizer cannot name `equals_equals` without a database.

Blocked when: neither side is the literal (the general driver stays); both sides are the literal
(the primitive opcode already handles it); the level is below O2.

One difference from the driver was checked and accepted. The driver looks through a raw cell
value. A MIR operand is never a raw cell: `verify_mir` rejects a bare read of a captured local.

### Invariant hoisting (`loops.rs`, `hoist_invariants`)

Runs just before rotation. Statements of a loop's test that compute the same value on every trip
move to a new block in front of the header: `len` of a container, a field read, a unary or binary
operator, when every operand is a constant, a local the loop never writes, or a result hoisted
with it. Entry edges go to the new block. Back edges still go to the header.

Only the test is looked at: the header and the single-entry blocks its `goto`s lead through.
Those statements run every time the loop is reached, with or without a trip, so nothing is
evaluated that the program did not evaluate, and a zero-trip loop does what its test did. The
loop body is not looked at. A read there may sit behind a check the loop makes.

Blocked when:
- The object read is not thread-local. `escape::analyze_local` decides, per local. It is the
  escape analysis with callee summaries off: a summary reads lowered MIR, and this body is still
  being lowered, so asking would cycle. Every call to a source function is opaque and its
  arguments count as shared. Builtins keep their known effects, so `push` does not share an array.
  A parameter is shared.
- The loop stores the same field index through any object, or makes a call, await or spawn.
- For `len`: the loop writes an element other than through an array index. An indexed array
  store is bounds-checked and never changes the length.
- An operand is a local the loop writes, a `move`, an index read, or goes through a cell.
- The destination is named, captured, a parameter, or has another definition.
- The statement can trap and the test runs a store, or another evaluation that can trap, before
  it and leaves that one behind. The first error the test raises stays the first.
- The statement is a bare copy that no hoisted statement reads. Moving a load saves nothing.
- The header is the entry block or a handler.

### Loop rotation (`loops.rs`)

The header's test is copied into one new block. Every predecessor of the header inside the loop
enters the copy instead. The copy branches back to the body. This is tail duplication only: each
execution runs the same statements in the same order under the same handler. The original header
still runs the test on entry, so a loop that runs zero times still runs zero times.

A test may make calls. A `for` over an iterator has a header that calls `next`, and the block the
call returns to branches on the result. `while f(x)` has the same shape. The copy is then one
block per call: each copied call returns to the next copied block, and the last one branches. A
handler the call unwinds to is entered from the copy as from the original. Escape analysis and
both backends recognise an array walk per `next` site, so a walked array stays an index walk
with two sites (`array_iter_next` twice in the bytecode).

Blocked when:
- No back edge is an unconditional `goto`. There is no jump to save. This also stops a rotated
  loop from being rotated again.
- The test does not end in a `branch` with one target in the loop and one out.
- A block of the test ends in anything but a `goto`, a call, a virtual call or that branch. An
  await, a sys-op or a spawn in the test leaves the loop alone.
- The test spans blocks with different `unwind`, `handling` or `shielded`, or a block of it has
  another predecessor. One copy could not run under both handlers.
- The header is a handler. The VM lands there with its error slots.
- The test has more than 8 statements, a call counting as one. This bounds code growth.

A temporary that lives entirely inside the test gets a fresh local in the copy. Without that it
would have two definitions and the emitter would keep it in a slot, which costs more than the
jump saved. A local read outside the test stays shared. The result of a copied call is such a
temporary when only the test reads it (`while f(x)`), and is shared when the body reads it (the
item of a `for`). The first version shared every call result, and `while is_digit(current(p))`
in json-parser then paid `store_var; load_var` on each test to save one `jump`.

Rotation runs on a function before its calls are inlined, so a test that calls two small
functions is copied as two calls and both copies are inlined afterwards. That is the growth in
json-parser's listing (2244 to 2373 instructions, 6%). The inliner's own budgets bound it.

### Value reuse (`redundancy.rs`, `reuse_available_values`)

Forward available-expressions analysis. Expressions: binary and unary operators over locals and
scalar constants, reads through field and index projections, and `len`. An assignment whose
expression a local already holds becomes a copy of that local. Nothing moves. The evaluation that
is removed has the same operands as one that already finished, so it could not have trapped.

Lowering reads a named local through a fresh temporary at every use. The analysis therefore
tracks which locals hold the same value and compares expressions after seeing through them.

Blocked when:
- Any operand local, or the holding local, was written since. A write to an operand makes the
  second evaluation a new value that may trap on its own.
- A store to the same field index through any object, or any element store or `len`-changing
  write, happened since. The model is `memory.rs`: field sensitive, alias insensitive.
- A call, virtual call, sys-op, await or spawn happened since, for any heap read.
- The block is a loop header, for any heap read. Every read available on entry is dropped
  there, so a read is reused within one trip and never carried around the back edge. Carried
  around, one read would stand for the reads of every later trip, which is hoisting a shared
  read by another route. The first version of the pass allowed it whenever the cost model
  agreed, which a header with four or more reads of one field is enough for.
- A join has a predecessor where the expression is missing or held in another local. The
  evaluation, and its trap, has not happened on that path.
- The block is a handler. It starts with nothing: it is entered before the faulting store.
- The place goes through a cell. A closure sharing the cell is not visible to the body.
- An operand is a `move`.
- The reuse does not pay. Saved instructions are `(cost - 1)` per reuse, counted in full in the
  holder's block and by half in a later block. The holder costs 1 instruction if its value is
  consumed straight off the stack and 3 otherwise. Reuse must save more than that.

A reuse that does not pay is still rewritten to a copy when nothing reads its result any more,
because the reuses that fed on it were taken. Otherwise a dead evaluation that can trap would
have to stay and be popped.

The analysis recomputes each block's entry from its predecessors until nothing changes, at most
16 passes, and gives up without rewriting if it has not settled. Recomputing is needed because
the name an expression is compared under depends on the copy facts, which settle as it runs.

### Negated branch (`fold_negated_branches`)

`_t = !c; branch _t -> [a, b]` becomes `branch c -> [b, a]`.

Blocked when: the negation is not the last statement of the block; `_t` has another reader or
another definition or a name; `c` is not a local of type `bool`.

### Return threading (`thread_returns`)

A `goto` to a block that holds only `return` becomes `return`.

Blocked when: the return block has statements, is a handler, or differs from the jumping block
in `unwind`, `handling` or `shielded`.

## Heap reads and other tasks

A heap value can change at any call, await or spawn, and another thread can write it at any time.

Reuse of a heap read is allowed only between two reads with no call, await, spawn or conflicting
store in this frame. In that window this task takes no step that could make another task's write
visible. Reading the same value twice is one of the outcomes the original program already
allowed, so the rewrite picks an allowed outcome. It never invents one.

Hoisting a heap read out of a loop is done only for an object no other task can reach. A loop
can spin on a field that another task writes. The original loop ends when the write lands. A
hoisted read never sees it. That is a new outcome, not an allowed one. So a read moves in front
of a loop only when escape analysis says the object is thread-local, and value reuse forgets
every heap read at a loop header. `len(xs)` on a parameter stays in the loop: the caller may
have shared the array.

## Snapshots accepted

All changed snapshots are under `crates/baml_tests/snapshots/`. Each change is one of:
- `goto -> bbN` to a bare `return` block became `return`, and the block went away when nothing
  else reached it. Bytecode: `jump L; L: return` became `return`.
- `call baml.ops.equals_equals(x, const null)` became `is_type_tag(x, NULL)`. Bytecode:
  `load_const null; call baml.ops.equals_equals` became `is_type null`.
- `_t = !c; branch _t` became a branch on `c` with its targets swapped.
- A loop test appears a second time in a new last block, and the latch jumps there.
- A repeated computation became `copy` of the local that holds it.
- Block and local numbers shifted as a consequence of the above.

`assertion_line` was stripped from accepted files, matching the rest of the repo.

One test budget changed. `dead_allocations_and_guarded_patterns_keep_their_old_instruction_budgets`
caps the static size of `gc_map_survives_pressure` at 17 instructions. A rotated loop holds its
test twice and has no jump back, so the function is now 19. The dead allocations the test is
about are still removed.

## Measured

`scripts/speedtest run --rows vm-off --trials 3`, three baseline runs and two runs with the
passes, alternated in one session. Range of the per-run medians.

| Workload | Before | After |
| --- | --- | --- |
| programs/richards-50 | 334 to 349 ms | 191 to 200 ms |
| memory/splay-2k | 125 to 134 ms | 112 to 113 ms |
| compute/if-else-dispatch-1m | 105 to 112 ms | 100 to 101 ms |
| compute/quick-sort-3way-1k | 5.17 to 5.32 ms | 4.94 to 4.99 ms |
| compute/divide-guard-1m | 77.1 to 79.5 ms | 74.4 to 76.3 ms |
| compute/nbody-20k | 169 to 175 ms | 166 to 168 ms |
| compute/merge-sort-20k | 176 to 184 ms | 176 to 178 ms |
| compute/fib-iterative-100k | 203 to 225 ms | 200 to 202 ms |
| compute/collatz-100k | 565 to 575 ms | 560 to 564 ms |
| compute/guard-clauses-1m | 111 to 116 ms | 110 to 114 ms |
| compute/bubble-sort-5k | 1.36 to 1.38 s | 1.35 to 1.38 s |
| compute/nested-loops-500x500 | 8.82 to 9.05 ms | 8.92 to 8.93 ms |
| compute/array-traversal-8k | 2.02 to 2.04 ms | 2.02 to 2.05 ms |
| compute/mixed-ops-5k | 1.99 to 2.08 ms | 2.00 to 2.01 ms |
| programs/json-parser-15 | 207 to 210 ms | 209 to 212 ms |

Richards and splay are the null test. The rest moved by a few percent or stayed inside the noise.
Rotation removes one instruction in fourteen from the nested-loops inner loop and the clock does
not show it: that workload is 250,000 iterations and mostly process start, and the removed
instruction is a `jump`. json-parser is within noise; its bytecode differs only by rotation and
return threading, and neither adds an executed instruction.

## Measured: hoisting and rotation through calls

Bytecode instructions on the hot path of one trip, read off the listings before and after:

| Loop | Before | After | What changed |
| --- | --- | --- | --- |
| bubble-sort inner (no swap / swap) | 21 / 35 | 16 / 30 | hoisting `arr.length() - i - 1` |
| array-traversal `indexed`, inlined into `main` | 13 | 12 | hoisting `xs.length()` |
| array-traversal `iterate` (`for` over an array) | 9 | 8 | rotation through `next` |
| json-parser `read_digits` (`while is_digit(current(p))`) | 28 | 27 | rotation through calls |

Of the fifteen surveyed workloads, hoisting changes the bytecode of three (bubble-sort,
array-traversal, json-parser) and rotation through calls of five (array-traversal, json-parser,
merge-sort, nbody, quick-sort). The other loops test a parameter's length or a counter.

Wall time could not be used. The machine ran at a load average of 30 to 160 on 12 cores for the
whole session, and one alternated pass of all 77 workloads put workloads neither pass touches
anywhere from 2x faster to 2x slower. The numbers below are instructions retired by the whole
process, from `/usr/bin/time -l baml-cli run --file main.baml main`, which load does not move.
Minimum of three runs (six for startup), `fasttest` builds, before and after built from this
tree. A `fasttest` process retires about 21.8 G instructions before the workload starts, mostly
compiling the standard library with debug assertions on, so only a long loop shows.

| Workload | Before | After | Loop only |
| --- | --- | --- | --- |
| compute/bubble-sort-5k | 66.16 G | 61.51 G | 44.40 G to 39.65 G, 10.7% fewer |
| `for` over 500,000 integers, 100 passes (not in the suite) | 70.31 G | 68.40 G | 48.5 G to 46.5 G, 4% fewer |
| process/startup | 21.76 G | 21.86 G | compile cost, see below |

Every other workload moved by the startup difference and nothing more: json-parser, nbody and
array-traversal were each rerun three times per build to check. Their loops that changed run too
few trips to show one `jump` each against 22 G. All 77 workloads print the same result on the
before and after builds.

Compile cost: startup retires about 0.1 G more instructions (0.3 to 0.5%), with a run-to-run
spread of 0.05 G on either build. Bodies with no cycle skip the dominator computation in all
three loop analyses; that did not move the number, so the cost is elsewhere and was not found.

## Left undone, by expected instructions saved

1. Let the emitter keep the left operand on the stack when the right operand can trap
   (`a[i] <= a[j]`, `h * 31 + xs[i]`). Today the left value is stored and kept
   (`store_var_load_var`). One instruction per such expression: bubble, merge, checksum.
2. Fuse `store_var i; load_var i` across a statement boundary. The emitter declines on purpose to
   keep a sequence point for the debugger. One instruction per counted loop after rotation, and
   one per trip of a `for` over an array (`store_var _4; load_var _4` after `array_iter_next`).
3. Hoist from loops over a parameter. `len(xs)` on a parameter stays in the test because the
   caller may have shared the array: merge-sort, quick-sort and `indexed` in array-traversal.
   Needs a caller-side fact (every call site passes a thread-local array) or a guarded copy.
4. Fold `is_type_tag(const null, NULL)` to `true`. Seen once in the snapshots, not in a loop.

Looked at and not done, because the instruction count does not move:

- Store-to-load forwarding for `x.f = v; ... x.f`. A scan of the fifteen surveyed workloads
  found ten store-then-read pairs in one block (richards 4, splay 4, json-parser 2). In every
  one the stored value is a computation (`self.count - 1`, `copy _6.0`), not a local. Keeping
  it in a local costs a store-and-keep and a load, which is what the field reload costs. No
  pair stores a plain local, the only case that pays.
- Reuse on the bubble-sort swap path. Three candidates, estimated from the bytecode listing and
  not built: `j + 1` reused inside the swap block needs a slot for it (`store_var` plus a broken
  load pair, 2 instructions) to save 2, so it breaks even. `j + 1` and `a[j + 1]` held from the
  compare cost about 1 each on the path that does not swap. `tmp = a[j]` could read the
  compare's `a[j]` for about 3 instructions less on the swap path at no cost, but only because
  the emitter already keeps
  that value in a slot (item 1 above). The cost model cannot know that without repeating the
  emitter's classification, and fixing item 1 would turn the free reuse into a cost on the
  other path.
