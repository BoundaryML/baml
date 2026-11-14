# MIR optimizer: decision ledger

Decisions made while building the MIR optimizer without the user present
(2026-11-14). Each one says what was chosen, why, and how to undo it. Nothing
here was confirmed by the user unless the entry says so.

## Context

The MIR optimizer before this work was per-function only: dead blocks, copy
propagation, dead stores, constant folding at O2. There was no inlining, no
devirtualization, no removal of allocations, no loop work.

Measured on the VM before starting (loaded machine, treat as ratios):

| Program | ms per 100k iterations |
|---|---|
| Method call on a fresh object (`method-call-100k`) | 30.8 |
| Same, call inlined by hand, object still allocated | 25.4 |
| Same, no object at all | 12.1 |
| Empty counting loop | 5.1 |
| Loop calling a one-line function | 10.9 |

So on this workload the call is about 18% of the time, the allocation and field
reads about 43%, and the loop itself about 17%. A counting loop costs roughly
50 ns per iteration, which is interpreter dispatch, not something MIR can fix.

## Decisions

### D1. Inlined calls leave no trace span of their own

**Chosen:** a call the optimizer inlines produces no telemetry record, timing
or span for the callee. Its work is attributed to the caller.

**Why:** the user said the compiler is free to make tracing decisions and may
skip functions that are too small to be worth capturing. Without this, inlining
is impossible, because the span is produced by the VM's call path.

**Guard rails:** a function is never inlined if it carries trace configuration,
if the call site passes `$trace`, or if it is an LLM function.

**Undo:** set the inliner's size budget to zero, or disable the pass in
`optimize.rs`. No other pass depends on spans being absent.

### D2. Stack traces keep inlined frames

**Chosen:** an error raised inside inlined code still reports the callee's
frame (name, file, line) followed by the caller's. The optimizer records which
blocks came from which callee, the emitter writes that as a side table, and the
VM expands it only when it builds a trace.

**Why:** dropping frames would make errors point at the wrong function. A side
table costs nothing on the fast path, and it is the same mechanism native code
will need.

**Undo:** not needed unless the table format is unwanted; the inliner cannot be
correct without some answer here.

### D3. Inlining stays inside one package

**Chosen:** the inliner only inlines a callee from the caller's own package.
Calls into the standard library or another package stay calls.

**Why:** packages are compiled and cached separately, and escape analysis
already treats other packages as opaque so that an artifact does not depend on
how its dependencies were mounted. Cross-package inlining would break that
cache key.

**Cost:** user code calling small standard-library functions written in BAML
does not benefit yet.

**Undo:** this is a restriction, so lifting it is additive. It needs the
package cache key to include the bodies it inlined.

### D4. No inlining of user functions when hot swapping is on

**Chosen:** with `baml run --hot`, user functions are never inlined, matching
the rule escape analysis already follows there.

**Why:** a swap replaces a function's body, and an inlined copy would never see
it.

### D5. All new passes run at O2 only

**Chosen:** the new passes are gated on `OptLevel::Two`, which is what the CLI
uses for `run`, `test` and `pack`. O0 and O1 are unchanged.

**Why:** O0 and O1 are what the debugger and instruction-level tests rely on.

**Undo:** each pass has one call site in the pipeline.

### D6. Recursion depth and frame counts may change

**Chosen:** inlining removes VM frames, so a deeply recursive program can now
recurse further before hitting the stack limit, and anything that counts frames
sees fewer.

**Why:** unavoidable consequence of inlining.

## Incidents

- **One live model call was made by mistake.** The first baseline run of the
  test corpus (`baml-cli test` in `crates/baml_tests`) included the `::live::`
  group, which found the `claude` CLI on PATH and made one real Claude Code
  call (logged cost $0.17) before the run was stopped. Every later run goes
  through `target/mir-opt/corpus.sh`, which passes `-x "::live::"`.

## Findings that are not decisions

- The interpreter's per-instruction cost (about 8 ns) is the largest single
  factor on tight loops. That is VM work, outside this task.
- `mir_type_audit.rs` (untracked, in `crates/baml_tests/tests/`) counts how
  precise MIR types are across the corpus.

## Results

Filled in as passes land.

## Inliner

Built without the user present (2026-11-14). Code: `crates/baml_compiler2_mir/src/optimize/inline.rs`.
Tests: `crates/baml_tests/tests/inliner.rs`.

### Shape

`lower_function` is now a plain function over two tracked queries.
`lower_function_local` is the old behaviour: one function lowered and cleaned
up from its own source. `lower_function_inlined` copies that result, inlines
into it, and runs the cleanup loop again (`optimize_function`, so
`optimize_body`). It returns nothing when no call was inlined, and the local
MIR is then used as it is, so O2 does not store two copies of every function.
The inliner only ever reads callees through `lower_function_local`. No salsa
cycle is possible, whatever calls whatever. Every existing consumer still
calls `lower_function`.

Consequence: a callee is copied in its un-inlined form and the calls inside
the copy are inlined by the same pass. Nesting is tracked per block, not per
function.

### Eligibility rules

A call is inlined only when all of these hold. Each one is a restriction that
can be lifted later.

- **Direct call only.** The callee operand is a `Constant::Function` of a
  source function. Function values, bound methods, `VirtualCall` and
  `GenericFunction` constants stay calls.
- **No type arguments.** `ntypeargs` is 0 and the callee's generic frame is
  empty (its own generics, its class's, an interface's `Self`). Interface
  default bodies are refused outright. A body that reads a frame type slot is
  refused too, which covers `let type`. Why: the body would read the
  caller's frame.
- **Package (changes D3).** The callee is in the caller's package, or in the
  standard library. The standard library was added after the coordinator
  confirmed the package cache key holds the compiler build, which the
  standard library is part of. Other dependencies stay calls.
- **No inlining in packages loaded at run time** (`SourceRootKind::Dynamic`:
  eval sessions and runtime mounts). A later submission can redefine a
  function an earlier one calls. This is what stands in for D4. **D4 itself
  could not be implemented: there is no hot swap flag in this tree.** There
  is no `baml_env::hotswap_enabled()` and no `--hot`. When hot swapping
  lands, add one check to `eligible_call`.
- **Tracing (D1).** Refused when the call passes `$trace`, or when the callee
  has a declared hook, is `trace_passthrough`, is an LLM function, or is a
  model `invoke`/`_settle`. The same test is applied to the caller: nothing
  is inlined into such a function. That second half was found the hard way.
  A hook is called from its target's entry code, and inlining the hook there
  broke hook fallback (`hooked entry` panic, and a hang in the corpus test
  "panic and exit hooks fall back and restore selection").
- **Recursion.** A callee that calls itself is never copied. A callee already
  on the chain of calls being inlined at that point is not copied again, and
  neither is the function being compiled. Mutual recursion is unrolled once
  at most.
- **No handlers, `defer`, or shielded blocks in the callee.** The exception
  and shield tables are keyed by block. Inlining INTO a protected region is
  fine: the copy takes the call block's `unwind`, `handling` and `shielded`.
- **No closures or captured locals in the callee.** Lambdas are numbered per
  function and captured locals are cells.
- **No intrinsics in the callee** (logging, trace hooks, `BindType`), no
  `CurrentPackage`.
- **No suspension in the callee** (`Await`, `AwaitAny`, `SysOp`, `Spawn`).
  The VM would probably cope, since frames resume by pc. Not allowed because
  a spawn names its task after the running function, cancellation and
  telemetry waits are attributed by frame, and the saving next to a
  suspension is nothing. Not tested, so not allowed.
- ~~No construction of an instance with a finalizer in the callee.~~ Removed.
  The general fix under "Bugs fixed" made it redundant.
- **Arguments must line up.** The call's layout is mapped onto the callee's
  parameters with `CallLayout::map_to`, the same function dispatch uses.
  Named, reordered and omitted arguments are all handled, not refused.

Lambdas: a lambda body gets the pass too, with the same rules. A function
that contains a lambda is never a callee. Top-level `let` initializers are
not processed. They run once.

### Budgets

Size is statements plus terminators of the callee's own MIR.

- `MAX_CALLEE_SIZE = 24`. See the sweep below.
- `MAX_DEPTH = 4` nested copies. Not swept. The deepest chain in the
  workloads is 3.
- `MAX_GROWTH = 400` per caller body. Not swept. It is a guard against
  blow-up, and no workload reaches it.

**The size sweep was cut short.** Variants at 8, 48 and 96 were built but
never timed, on the coordinator's instruction to stop spending time on
builds. 24 is therefore a first guess that measured well, not a tuned value.
What was measured, at 24, release build, VM with telemetry off, median of 3
trials, two alternating rounds on a loaded machine (load average 5 to 11):

| Workload | Before (ms) | After (ms) | Change |
|---|---|---|---|
| classes::method call 100k | 15.4 / 15.5 | 13.3 / 12.8 | -16% |
| classes::method chain 100k | 16.3 / 16.0 | 9.74 / 9.81 | -39% |
| compute::call chain 100x10k | 29.4 / 29.2 | 8.63 / 8.76 | -70% |
| compute::class instances 100k | 10.9 / 10.1 | 10.1 / 9.94 | about -4%, within noise |
| compute::divide guard 1m | 81.2 / 78.8 | 61.5 / 60.2 | -24% |
| compute::fib32 recursive | 315.5 / 309.7 | 307.8 / 308.7 | none (recursive, never inlined) |
| compute::guard clauses 1m | 115.0 / 112.4 | 91.5 / 91.4 | -20% |
| compute::if else dispatch 1m | 107.8 / 106.7 | 85.2 / 84.5 | -21% |
| compute::nested field access 500k | 88.7 / 88.1 | 90.5 / 88.7 | none |
| compute::pure call 1m | 57.9 / 56.6 | 30.5 / 29.0 | -48% |
| interfaces::polymorphic dispatch 100k | 26.4 / 26.4 | 21.4 / 21.4 | -19% |
| programs::json parser 15 | 217.2 / 220.3 | 173.6 / 170.3 | -22% |
| programs::request handler 2k | 8.47 / 8.33 | 7.93 / 7.92 | -6% |
| programs::richards 50 | 336.9 / 328.7 | 290.3 / 288.5 | -13% |

"Before" is this same tree with the inliner call switched off, built the same
way. "After" includes the standard library step but was built before the
rule that keeps calls inside traced functions, which no workload uses.
No workload got slower.

### What the copy does

- A required parameter the callee never writes, filled from a caller local of
  the same type, becomes that local. No copy is made. The inliner also looks
  through the temporary an argument is usually copied into. Other parameters
  get a fresh local and an assignment in the call block.
- An omitted optional is the omission sentinel, as in a real call. When the
  argument is a constant or is omitted, the callee's entry test for the
  default is folded at compile time. When it is a run-time value the test
  stays.
- `Return` becomes an assignment to the call's destination and a jump.
- Copied locals keep their names but lose their debugger scope.

### Stack traces (D2)

Each copied block carries an `InlineScope`: the callee, the call's span, and
the scope of the block the call was in. Cleanup passes never merge non-empty
blocks, so the scope survives. The emitter writes two things per function:
`Bytecode::inline_frames` (callee identity, callee file, line of the call,
parent frame) and `Bytecode::inline_table` (run-length, pc to frame, like the
line table). `CompactCode` gets the table in byte offsets. A function with
nothing inlined has both empty.

`BexVm::capture_stack_trace` expands a frame's pc into the function's own
frame plus one frame per inlined call. Nothing reads the table on the normal
path. The Rust backend reports bytecode pcs, so it gets the same frames; its
"did the position change" mark now includes the inline frame.

The emitter also had to learn that one function can hold code from several
files: line numbers were computed with the function's own file. They are now
computed with the file the span is in.

Standard library frames inlined into user code keep their `<builtin>/` path,
so the user-facing traceback filters them exactly as before.

**Known small difference.** The caller's line for an inlined call is the line
the call starts on. For a real call whose expression spans exactly two lines
the old emitter sometimes reported the second line. Single-line calls and
longer ones are unchanged.

### Format changes

`Bytecode` gained two serialized fields. `baml_artifact::FORMAT_VERSION` went
30 to 31 and `bex_cache::FORMAT_VERSION` 27 to 28.

### Snapshots and tests changed

Accepted, each read: `ns_catch_throw` (bytecode, mir), `ns_function_call`
(bytecode), `ns_json_parse_stringify_intrinsics` (bytecode),
`ns_json_to_from_string_concrete` (mir), `ns_llm_quoted_prompt` (mir),
`optional_defaults_emit_snapshot`, and three `compiler2_mir` optional-argument
snapshots. All show a call replaced by its body.

Existing tests edited because their fixtures were inlined away:

- `compiler2_emit::chained_catches…`: looked at the first `throw` in `main`,
  which is now the inlined callee's. It now accepts any throw.
- `compiler2_emit::optional_param…`: checks the call site sentinel at O1.
- `compiler2_mir::escape` `needs()` helper: analyses at O1, where calls exist.
- `compiler2_mir::source_param_interface_dispatch…`: expects the inlined
  marker instead of a call.
- `optimizer_binary_prefix`: two fixture helpers made self-calling so their
  calls survive. One count went 3 to 2.
- `optimizer_stack`: `array_mixed` 8 to 6 instructions.
- `optimizer_effects`: one budget raised (17 to 43), an inlined map builder.
- `rust_backend`: fixture helpers made self-calling, and the expected set of
  reachable functions updated. The Rust backend prunes functions nothing
  calls, and a function inlined everywhere is one of those.

### Found on the way

- The optimizer dropped a dead allocation of a class with a finalizer. Now
  fixed, see "Bugs fixed".
- **The 16 `root.trace*` corpus failures are not stable.** They come and go
  with machine load on every compiler, including the unmodified one (it
  failed them once in a trace-only run here).

### Left undone

- Suspending callees, generic callees, callees with handlers or closures.
- The default test for an optional parameter filled by a run-time value.
- `rust_differential` is `#[ignore]`d in the repo and was not run.
- Depth and growth budgets were not swept.

## Other pass ledgers

- Scalar replacement: `MIR_OPTIMIZER_LEDGER.scalar_replace.md`.
- Value reuse, negated branches, return threading, loop rotation and the
  null tag test: `MIR_OPTIMIZER_LEDGER.loops.md`.

## The three passes together

Found when the inliner met the other two passes:

- **Loop rotation dropped the inlined-call marker.** The copy of a loop test
  was a fresh block with no `inline` scope, so an error raised in a rotated
  test inside inlined code would have lost the callee's frame. The copy now
  takes the header's scope, and a test that spans blocks of different scopes
  is not rotated.
- **Callees now arrive with several returns** (return threading runs before
  the inliner copies them). The inliner handled that already. It now also
  computes a value straight into the call's destination when the value is
  computed right before the return, instead of into a temporary that is then
  copied. Not done when the callee reads its return slot.
- **The cleanup is run a second time on inlined bodies.** A unit test
  (`loops.rs`, `cleaning_up_a_cleaned_up_body_changes_nothing`) checks that
  two runs equal one. Rotation does not repeat because a rotated loop has no
  unconditional back edge.
- Test budgets: `gc_map_survives_pressure` is 46 instructions (inlined map
  builder plus a rotated loop). `manhattan` in the Rust backend prune test is
  now inlined, so it is unreachable there. Checked in the MIR, it is not a
  reachability bug.

## Bugs fixed

- **A dead allocation of a class with a `cleanup` finalizer was dropped.**
  `let x = Handle { … }` with `x` never read was removed by dead-code
  elimination, at every level including O0, so `cleanup` never ran. Cause:
  `Rvalue::can_discard` treated every aggregate as free to drop, and it has
  no database to ask about the class. Fix: `AggregateKind::Class` carries
  `finalizable`, set at lowering from `escape::may_finalize`, and
  `can_discard` refuses a finalizable aggregate. Every pass that drops
  unused evaluations goes through `can_discard`, so none can drop one.
  Test: `finalizable_instances.rs::an_instance_nothing_reads_is_still_finalized`
  (O0, O1, O2; it fails with the fix reverted). The inliner's own rule
  against such callees was removed as redundant. Scalar replacement keeps
  its own check: it replaces rather than discards.
  Cost: `may_finalize` answers "maybe" for any class outside the package and
  the standard library, so an unused instance of a dependency's class is
  kept even when that class has no finalizer.
