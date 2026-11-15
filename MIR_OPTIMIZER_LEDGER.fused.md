# MIR optimizer ledger: fused instructions

Scope: a bytecode pass in `baml_language/crates/baml_compiler2_emit/src/fuse.rs`, the
instructions it emits (`bex_vm_types/src/bytecode.rs`), their handlers in `bex_vm/src/vm.rs`,
and one fix to the VM's push path found while profiling.
Metric: bytecode instructions dispatched per loop iteration on `bex_vm`, confirmed by wall time.

## Profile

`sample` on packed workloads built from the parent's `step3-fast` binaries, 6 to 8 seconds each.
Share of the samples that landed in the interpreter thread's own code:

| Workload | `BexVm::exec` | `drop_glue<VmPanic>` | `EvalStack::try_push` | other |
| --- | --- | --- | --- | --- |
| empty counted loop, 1M | 68% | 22% | 9% | |
| nested-loops-500x500 | 66% | 26% | 8% | |
| fib-iterative-100k | 69% | 25% | 6% | |
| bubble-sort-5k | 56% | 25% | 9% | 9% array lock guards |

Two things followed.

1. A third of the time was not dispatch. Every push called `EvalStack::try_push` out of line, and
   that called the drop glue of a `VmPanic`. `ThreadStackBudget::try_reserve` wrote
   `checked_add(..).ok_or(VmPanic::StackOverflow)?`, and `ok_or` takes its error by value, so the
   error was built and dropped on every push.
2. The rest is `exec` itself, which scales with the instructions dispatched.

## The push-path fix (commit "vm: build the stack-overflow error only when a push fails")

`try_reserve` builds the error only when `checked_add` fails, and `try_push` is `#[inline]`.
No behaviour changes. It applies at every optimization level.

## Fused instructions (commit "emit, vm: fuse operand loads, int operations, ...")

| Run | Fused as |
| --- | --- |
| `[LoadVar \| LoadConst]* ; AddInt..ModInt ; [StoreVar]` | `IntArith { op, lhs, rhs, dst }` |
| `[LoadVar \| LoadConst]* ; CmpIntOp ; PopJumpIf{True,False}` | `IntCmpJump { op, lhs, rhs, offset }` |
| `LoadVar ; LoadField[Local]` | `LoadVarField` |
| `LoadVar ; LoadVar ; LoadArrayElement[Local]` | `LoadVarElement` |
| `LoadVar ; StoreVar` | `MoveVar` |

An operand the instruction does not name comes off the stack, and a result it does not store is
pushed. A constant is named only when it is an `int` that fits `i32`.

### Which levels fuse

`compile_mir_function` runs the pass at `OptLevel::Two` and at no other level. O0 and O1 bytecode
is byte for byte what it was, so the debugger and the instruction-level tests still see one
instruction per operation. `only_o2_fuses` pins this.

The test file says programs are run "unfused, fused at O1 and at O2". That is the test's own
doing, not the emitter's: it compiles at O1, where nothing is fused, and calls the public
`fuse::fuse` on a copy of that bytecode. Fusion is then the only difference between the two
programs, which O1 against O2 cannot give, because O2 also inlines and rotates loops.

### When a run is not fused

A fused instruction has one PC where its run had several, so a run is fused only when nothing
keyed by PC tells its instructions apart. A run stops before an instruction that is:

- the target of a jump or a jump-table entry;
- the start, end or handler of an exception-table entry, or of a handler-context entry;
- the start or end of a shield range (`defer` body);
- the start of an inline-table entry (an inlined call begins or ends);
- the first instruction of a MIR block (`block_pcs`, which the Rust backend reads);
- on another source line, or a debugger sequence point. A line entry that only narrows the span
  on the same line is crossed.

When only the trailing `StoreVar` is past such a point, the rest of the run is fused and pushes.

A `LoadVar2` counts as two loads, so a run may start at its second. If both halves stay, they are
one `LoadVar2` again.

### What the pass rewrites

Jump offsets, jump tables, the line table, the inline table, the exception, handler-context and
shield tables, call layouts and `block_pcs`. A range start or a jump target moves to the fused
instruction that holds it. A call layout or a statement position moves to the instruction that
holds the original's last step.

## Decisions

- **Fuse in the emitter's output, not in MIR.** The runs are an artifact of stack code. MIR has
  no loads or stores to fuse.
- **A pass over finished bytecode, not peephole fusion during emission.** Barriers are only known
  once jumps are patched and the tables are built. The cost is one walk per function at O2.
- **O2 only.** Lower levels are what the debugger steps and what instruction-count tests read.
- **One compact opcode per operand shape (19).** A handler reads its operands without branching
  on where they come from. The operation is one byte the handler switches on. `Instruction` stays
  one variant per fused kind.
- **`PopJumpIfFalse` is fused as the negated comparison.** Exact for `int`. Not done for floats,
  where NaN fails both a comparison and its negation.
- **Only `int` operations.** They are the specialized opcodes whose operand types the emitter has
  already proved. Float arithmetic allocates, so its dispatch share is smaller.
- **A fused arithmetic instruction pops its stack operands before it can raise**, as the unfused
  instruction does, so a handler sees the same stack.
- **A fused branch polls for a yield** exactly as `PopJumpIfTrue` does. A loop made only of fused
  instructions still parks.
- **Format versions bumped** (artifact 32, cache 29). The instruction set is serialized.

## Instructions per iteration

Read off O2 listings of the speedtest workloads, hot path of one iteration.

| Loop | Before | After |
| --- | --- | --- |
| nested-loops inner | 13 | 5 |
| fib loop | 14 | 5 |
| collatz `while` (even / odd) | 18 / 21 | 7 / 10 |
| array-traversal `indexed` | 13 | 7 |
| array-traversal `rebuild` | 14 | 9 |
| array-traversal `iterate` (for-in) | 9 | 7 |
| bubble-sort inner (no swap / swap) | 21 / 35 | 12 / 23 |

Across the 14 compute workloads surveyed, the listings went from 2,473 lines to 1,754.

## Measured

Packed workloads, driver from `tools/speedtest`, 7 alternating runs of each build in one session
at a load average of 9 to 21. Each cell is the fastest run to the median run, per job.

| Workload | Before | Push-path fix | Fix and fusion |
| --- | --- | --- | --- |
| empty counted loop, 1M | 36.8 to 37.3 ms | 30.3 to 30.5 ms | 15.8 to 15.9 ms |
| nested-loops-500x500 | 11.5 to 11.6 ms | 8.69 to 8.82 ms | 5.67 to 5.72 ms |
| fib-iterative-100k | 258 to 260 ms | 206 to 209 ms | 119 to 121 ms |
| bubble-sort-5k | 1.82 to 1.84 s | 1.35 to 1.36 s | 1.13 to 1.15 s |

The fix alone is 1.2x to 1.35x. Fusion on top of it is 1.9x, 1.5x, 1.7x and 1.2x. Together 2.3x,
2.0x, 2.2x and 1.6x. Bubble sort gains least from fusion: its inner loop is array reads, length
tests and element stores, which are fused least.

"Before" is the parent's `step3-fast` build, which is two commits older than the base of this
work: it lacks the inlining of generic callees and closures, which none of the four programs
use. "Push-path fix" is a build of the first commit alone. "Fix and fusion" is the build the
tests and corpus ran on.

An earlier run at a load average near 40 gave 50.8, 14.3, 507 and 2,380 ms before and 20.2, 7.04,
216 and 1,860 ms after. The ratios agree except for bubble sort (1.3x then, 1.6x now).

`scripts/speedtest` was not run.

## Verified

- `cargo test -p baml_tests --no-fail-fast`: 1,354 passed, 25 ignored, after two fixes to tests
  that pin O2 bytecode (below). `rust_backend` passes: its PCs agree with the fused bytecode.
- `cargo test -p bex_vm -p bex_vm_types -p baml_compiler2_emit -p baml_compiler2_mir`: 464 passed.
- Corpus (`target/mir-opt/corpus.sh`) on the rebuilt `fasttest` binary: 6,081 pass, 16 fail. The
  6,046 passing tests outside `root.trace*` are the same list as `corpus-base.log`. The 16
  failures are all `root.trace*`, and the trace-only run is 51 pass, 0 fail.
- `baml-cli test` compiles at `CLI_OPT_LEVEL` (O2) and runs `BexEngine` in the CLI process, so the
  corpus ran the fused interpreter in `baml-cli` itself. A packed workload runs the interpreter in
  `baml-pack-host`, which `pack` copies from beside the CLI. The host refuses an envelope whose
  toolchain or format differs from its own: packing with the new CLI onto the old host fails with
  "format 32; this runtime: ... format 31". So a packed workload that runs was interpreted by a
  host built from the same source as the CLI that compiled it.

## Snapshots and tests changed

Every snapshot change is a run replaced by its fused form, nothing else:
`compiler2_emit/optional_defaults_emit_snapshot` and the corpus bytecode snapshots of
`ns_catch_throw`, `ns_function_call`, `ns_json_parse_stringify_intrinsics`, `ns_patterns_new`,
`ns_short_circuit_locals` and `ns_spawn_semantics`. `assertion_line` was stripped.

Tests that read O2 instructions:
- `counting_loop_tests_at_the_bottom` counts `IntCmpJump` as a conditional branch.
- `repeated_test_operand_is_computed_once` counts a fused `%` as a remainder.
- `bytecode_takes_relaxed_field_paths_for_thread_local_instances` looks for
  `load_var_field_local c .n`.
- `constant_fold_mixed_not_foldable` expects `add_int x 1` in the optimized listing.
- `optimizer_binary_prefix`: `prefix_nested` is 6 instructions, was 7.

## Left undone

1. `x % 2 == 0` in collatz is still `mod_int x 2; load_const 0; cmp_int_op ==; pop_jump_if_true`.
   The comparison and branch were not fused there although the same shape fuses elsewhere. Not
   investigated; a barrier between them is the likely cause. 3 instructions per iteration.
2. `load_var s; mul_int i j; add_int -> s`: a three-operand form would save one. So would
   reordering the commutative add, which changes which operand is read first and was not done.
3. `load_var2 a b; container_len_local; jump_if_int_lt`: a loop test against a length is 3. A
   fused length compare would make it 1 or 2.
4. `store_array_element` with named operands, and `array_iter_next ; store_var ; load_var ;
   is_type ; pop_jump_if_true` in for-in loops (5 per iteration).
5. Float arithmetic, bigint arithmetic, generic `BinOp` and `CmpOp`.
6. A constant that does not fit `i32` stays a `LoadConst`.
7. The dispatch loop itself was not restructured, and it was not profiled again after these
   commits.
