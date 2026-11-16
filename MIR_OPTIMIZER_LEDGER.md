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
50 ns per iteration. A simple loop runs 13 to 15 bytecode instructions per
iteration (see `MIR_OPTIMIZER_LEDGER.loops.md`), so that is about 3.5 ns per
instruction. MIR passes cannot reduce that much further.

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

- On tight loops the largest remaining factor is how many bytecode
  instructions one iteration takes (13 to 15 for a simple loop, at about
  3.5 ns each), not the cost of any one instruction. An earlier version of
  this ledger said "about 8 ns per instruction"; that assumed fewer
  instructions than the loop survey later counted. The levers are in the
  emitter and VM (fused instructions, a register bytecode, a leaner dispatch
  loop) or native code. The dispatch loop has not been profiled.
- `mir_type_audit.rs` counts how precise MIR types are across the corpus. It
  was moved out of the tree to `baml_language/target/mir-opt/` so the clippy
  hook passes; it is not committed.

## Results

Whole suite on the VM, 2026-11-15, everything together (all MIR passes, fused
instructions, the push-path fix) against the build with the inliner and scalar
replacement off. Both are fasttest builds. Two alternated rounds, three trials
each, on a machine at load 35 to 40.

- 78 workloads, geomean 1.48x faster. 39 faster by more than 7%.
- Largest: wide-nested-class-create 17.0x, default-method 16.5x,
  nested-field-access 11.5x, generic-apply 8.2x, closure-apply 7.2x,
  polymorphic-dispatch 7.1x, method-chain 5.3x, method-call 4.1x.
- Loops: fib-iterative 2.0x, collatz 1.96x, richards 1.97x, nested-loops 1.86x,
  array-pipeline 1.67x, bubble-sort 1.44x.

The harness also showed 19 workloads 7 to 30% slower (strings, JSON, spawn,
enum-equals). That is measurement noise, not a regression. Run directly, the
same packed programs are faster on the new build:

- Instructions retired, 20 jobs: enum-equals 18.5 G to 16.7 G, split-short
  17.0 G to 15.8 G, trim-padded 15.7 G to 14.5 G, k-nucleotide 56.8 G to
  51.1 G, json-typed-orders 3.48 G to 3.22 G.
- Elapsed per run, five runs each, ranges do not overlap: enum-equals 197 ms
  to 183 ms, trim-padded 147 ms to 138 ms, json-typed-orders 32 ms to 30 ms.

Do not read sub-15% differences from the harness on a loaded machine.

Startup costs about 20 M more instructions per process (about 2%), from the
standard library's loop forms.

`array-build-sum-100k` failed once in one of four whole-suite runs on the new
build and passed in the three reruns that followed. Not reproduced, cause
unknown; the harness kept no error text.

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

## Inliner, second round

Rules above that this round changes: "No type arguments" and "Direct call
only" are narrower now. Everything else stands.

### Generic callees

- A callee whose body never reads its frame's type arguments is inlined
  whatever type arguments the call passes. Many generic functions are like
  this (`function id<T>(x: T) -> T { x }`), and so is every function that is
  generic only in what it throws.
- A callee that reads them is inlined when every slot it reads is filled, at
  the call, by a type known where the call is written: the operand is a
  local whose one definition is `LoadType` of a template with no frame
  reference, or the callee is a `GenericFunction` constant. The types are
  substituted into the copy's templates.
- Kept as calls: a type argument that comes from the caller's own frame
  (`function main<U>(x: U) { target(x) }`); a template that is not a whole
  type after substitution (an associated-type projection); a type test
  against a frame type (`x is T`), because lowering states a class or enum
  test by declaration and a substituted template does not; interface default
  bodies, whose `Self` dispatch fills in.
- A copied local can keep a type that names the callee's type parameter
  (`let _8: T`). Nothing after lowering interprets a type variable in a
  local's type (checked in emit, the Rust backend, escape analysis and the
  optimizer), so it reads as an opaque type. Substituting it would be more
  precise and is not done.
- New in `baml_type`: `TyTemplate::replace_type_arg_refs` and
  `TyTemplateInterface::for_each_template_mut`. New in `memory.rs`: mutable
  walkers over every template of a statement or terminator, arm for arm
  with the read-only ones.

### Closures

- A call through a local is inlined when that local's one definition, or the
  one definition of the local it was copied from, is `MakeClosure` of a
  lambda of the same body with no captures and no captured type arguments.
  This is what is left of `apply(double, i)` once `apply` is inlined.
- A closure that captures anything stays a call. The copy would read the
  captured cell through the caller's local, and a loop can give that local a
  fresh cell after the closure was made. Proving it has not needs a
  reaching-definitions pass over `FreshCell`. Not done.
- A closure passed to a function that is not inlined stays a call.
- `InlineScope::callee` is now a `MirFunctionId`, so a lambda's frame shows
  in a stack trace as `.<lambda(main, 0)>`, as it did when called.

### Result written in place

With no handler around the call, the callee's return slot is the call's
destination itself. A throw leaves the frame, so nothing can see the early
write. Not done when a parameter of the callee stands for that same local,
or when the types differ. This keeps a call result on the operand stack
where the old code had one (`optimizer_binary_prefix`).

### Not done: the default test for a run-time argument

No speedtest workload declares a defaulted parameter, so this cannot move
one. It also needs a proof that a run-time argument is never the omission
sentinel, which is false inside a default expression that forwards a later
parameter. Stopped here.

### Snapshots and budgets, second round

- `ns_spawn_semantics/bytecode.snap`: `baml.spawn.Plan.new` is inlined, so a
  `spawn` builds its plan with `init_instance` instead of a call.
- `ns_json_*`, `ns_llm_quoted_prompt`: results written in place.
- Two `compiler2_mir` snapshots: generic callees that do not read their
  frame are inlined.
- `optimizer_binary_prefix::prefix_template`: 7 to 8 instructions. The
  helper's virtual call replaced the call to the helper.

### Measured, second round

All 77 workloads, `fasttest` builds, VM with telemetry off, 3 trials,
base and new alternated twice with nothing else of ours running (load
average 5 to 11 from the rest of the machine). "Before" is
`target/mir-opt/base-fast`, built by the coordinator with inlining and scalar
replacement off. "After" is commit `0d8727c43b`: all passes, generic callees
and closures included. The new build was linked with the Command Line Tools
because Xcode's licence blocked the system linker mid-session. Rows are
workloads that moved more than 7%.

```
compute/wide-nested-class-create-50k          63.60ms ->     7.85ms   8.10x
compute/generic-apply-inferred-1m            297.68ms ->    44.16ms   6.74x
compute/closure-apply-1m                     272.72ms ->    44.03ms   6.19x
compute/nested-field-access-500k             105.39ms ->    18.63ms   5.66x
classes/method-chain-100k                     18.41ms ->     5.79ms   3.18x
compute/call-chain-100x10k                    31.56ms ->    10.51ms   3.00x
compute/class-instances-100k                  12.51ms ->     5.09ms   2.46x
classes/method-call-100k                      17.94ms ->     8.45ms   2.12x
programs/richards-50                         363.40ms ->   181.22ms   2.01x
compute/pure-call-1m                          65.42ms ->    36.66ms   1.78x
interfaces/generic-from-json-20k              38.50ms ->    25.13ms   1.53x
memory/binary-trees-14                       996.85ms ->   694.89ms   1.43x
memory/heap-calls                              2.40ms ->     1.68ms   1.43x
compute/if-else-dispatch-1m                  125.41ms ->    88.58ms   1.42x
interfaces/to-string-interpolation-10k        21.24ms ->    15.80ms   1.34x
compute/divide-guard-1m                       94.07ms ->    71.44ms   1.32x
interfaces/polymorphic-dispatch-100k          28.96ms ->    22.94ms   1.26x
compute/json-hello                              0.9us ->      0.8us   1.26x (ranges overlap)
compute/guard-clauses-1m                     132.51ms ->   105.86ms   1.25x
interfaces/match-dispatch-100k                28.53ms ->    22.82ms   1.25x
programs/json-parser-15                      226.87ms ->   183.94ms   1.23x
compute/mixed-ops-5k                           2.37ms ->     1.98ms   1.20x (ranges overlap)
memory/live-heap                             242.74ms ->   205.94ms   1.18x (ranges overlap)
interfaces/field-access-100k                  34.63ms ->    29.50ms   1.17x
memory/splay-2k                              147.17ms ->   126.61ms   1.16x
process/startup                                43.5us ->     37.5us   1.16x (ranges overlap)
programs/request-handler-2k                    9.14ms ->     8.02ms   1.14x
concurrency/spawn-await-x10k                 159.82ms ->   141.34ms   1.13x
concurrency/idle-tasks-8k                    393.65ms ->   348.87ms   1.13x (ranges overlap)
compute/json-ndjson-events                     6.28ms ->     5.63ms   1.12x
programs/agent-loop-300                       45.12ms ->    41.06ms   1.10x
interfaces/default-method-100k                61.08ms ->    55.90ms   1.09x
concurrency/spawn-fan-out-100                 777.8us ->    721.4us   1.08x (ranges overlap)

77 workloads; geomean time ratio new/base 0.789 (1.27x faster); faster>7%: 33, slower>7%: 0, within: 44
```

`closure-apply-1m` and `generic-apply-inferred-1m` are the two this round was
for: both were near 1.1x before it. Nothing is slower. An earlier whole-suite
run (three passes, before this round) was taken while test builds were
running and is not recorded here.

## Inliner, third round

### Closures with captures

- A call through a local that holds a capturing closure is now inlined. The
  copy reads and writes each captured variable through the cell the
  enclosing body holds for it (`*capture[i]` becomes `*_local`, or the
  enclosing closure's own capture when the body is itself a lambda).
- The condition: the cell a captured local holds at the call is the cell the
  closure was made with. A local gets a new cell only at a `FreshCell`
  (where its binding is created, once per loop iteration for a binding in a
  loop). So no `FreshCell` of a captured local may run between the
  `MakeClosure` and the call.
- The check is plain reachability, not a dataflow fixpoint. For each link of
  the chain (made, copied, copied, called) it looks at the rest of the first
  block, the start of the second, and every block that is both reachable
  from the first and able to reach the second without running the first
  again. A path that runs the first again has a newer closure or copy to
  start from. Handler edges count as edges.
- Refused: any `FreshCell` of a captured local in that region, a closure
  whose body reads captured type arguments, a lambda with lambdas of its
  own, and (as before) a parameter or a local with more than one definition.
- From source the refusal is hard to reach: a binding's `FreshCell` sits
  where the binding is declared, before any closure over it. It is covered
  by unit tests on hand-built MIR in `inline.rs`.
- Snapshots: `ns_lambda_basic` (mir, bytecode) and `ns_lambda_owners`
  (bytecode) show closure calls replaced by their bodies.

### Captured variables become plain locals again

Once the calls through a closure are inlined and the `MakeClosure` is dead,
no closure captures its variables. A captured local that no `MakeClosure`
names any more loses its cell: `*_x` becomes `_x`, its `FreshCell`s go, and
`is_captured` is cleared. The cleanup then runs once more. This is what lets
scalar replacement and constant folding see through code that used a lambda
(`ns_lambda_basic` now folds to a constant).

- It runs only in `lower_function_inlined`, after the first cleanup.
- Dropping a `FreshCell` relies on every read being dominated by a write of
  the binding, which the debug verifier checks.
- `optimizer_scalar_replace`'s `captured` fixture now hands its closure to a
  function that keeps its call, since a closure called where it is made no
  longer captures anything by the time scalar replacement runs.

### Interface calls and field reads on a known class

Code: `optimize/devirtualize.rs`, driven from the inliner's block scan.

- **When the class is known.** The receiver is followed back from the call:
  last definition in the block, then the one predecessor's, through copies,
  until a class literal. A call result, a parameter, a join or a handler
  entry ends the search with "unknown".
- **What it resolves to.** `resolve_impl`, the checker's own selection, for
  that class and the call's interface. If the implementation provides the
  method, that function. If not, the interface's default body. Only an
  explicit implementation written in source counts: a structural default
  (`Equals` on a class with no impl) and an implementation served from a
  package interface stay virtual calls.
- **Why this is allowed in an open world.** It never assumes an interface
  has one implementor. It assumes a class that has an implementation of an
  interface keeps that one. The checker enforces one implementation per
  class and interface, and the runtime checks implementations minted at run
  time against existing ones and rejects an overlap
  (`bex_vm/src/package_reflect/type_kinds.rs`, `OverlappingImplements`).
  Lowering's own comment about not devirtualizing concrete classes is about
  the structural default, which a later explicit implementation overrides.
  That case is excluded here.
- **The call is only rewritten when it is then inlined.** Otherwise the
  virtual call is put back. A direct call that stays a call would save only
  the VM's per-site dispatch cache lookup.
- **Join splitting.** A function that returns one class from one `return`
  and another from the next leaves two definitions meeting before the call.
  When following the receiver back stops at such a join and every way in
  ends at a class literal, the blocks from the join to the call are copied
  once per extra way. Limits: 16 statements and terminators per copy, 4
  ways, never the entry block, a handler block, or a chain a way loops back
  into. Copies count against the body's growth budget. A copied block keeps
  its inline scope, handler and shield.
- **A local per way.** When each way builds the instance in its own last
  statement and nothing outside the ways and the chain mentions the local,
  each way gets its own local. One local built as two classes cannot be
  scalar-replaced; two locals can. With this, the `polymorphic-dispatch`
  shape allocates nothing.
- **Field reads.** `VirtualFieldAccess` on a known class without type
  arguments becomes a read of the class slot the interface field is linked
  to (`name as model`, or the same name), through a temporary typed as the
  class. Field stores are not done.
- **Interface default bodies** may now be inlined, like any function, when
  the types they read from their frame are known or not read at all.
- **Not done: the guarded form** (test the class tag, direct path on a hit,
  virtual call otherwise). It was to be added only if it measurably beat the
  VM's dispatch cache, and no workload was left with a monomorphic site of
  unknown class to measure it on.
- Refused and left virtual: unknown receiver, generic class literal with a
  type argument from the frame, interface template that is not a whole type,
  method-level type arguments, `$trace` on the call, an implementation that
  reads its frame's type arguments, and everything the inliner refuses.

### Function and bound-method values

A call through a local whose one definition is a function constant, or a
generic function constant, is inlined as a direct call. A call through a
local whose one definition binds a method to a receiver is inlined with that
receiver as the first argument, when the receiver's local is not written
again (a parameter never written, or a local written once) and the method
does not read its frame's type arguments.

## Lines of fused instructions (user decision, 2026-11-15)

The user said: "it's ok if under some fusing we can't give exact line numbers
for failures".

**What changed.** The fuse pass (`baml_compiler2_emit/src/fuse.rs`, O2 only)
no longer stops a run at a line change or at a statement's sequence point. A
fused instruction that covers two lines reports the line of the first
instruction of its run.

**Exact scope.** The line number of a failure raised by a fused instruction
may be a neighbouring line of the same function. Nothing else about an error
changed:

- its kind and message;
- the frames, their order and their function names, inlined frames included
  (an inlined call's range edge is still a barrier);
- which handler catches it (handler, handler-body and shield range edges are
  still barriers, and so are jump targets);
- the line of every frame but the innermost. An outer frame's line is its
  call's, a call is never fused, and each unfused instruction keeps the line
  entry that covered it.

**Sequence points.** Every MIR statement starts with one, so they were the
barrier that mattered: a run could not cross a statement even on one line.
Nothing in the VM reads `sequence_point`; only the emitter and the fuse pass
do. The fuse pass runs at O2 only. `bex_project` compiles at O2 as well as
the CLI, so a tool that steps that bytecode by sequence point sees fewer of
them than before.

The user was asked whether those coarser single-step stops at O2 matter and
said (2026-11-15): "it doesn't at O2 we can break that boundary." So dropping
the sequence-point barrier is approved too, not only the line barrier. The
consequence: stepping stops at O2 are coarser, one per fused run instead of
one per statement. O0 and O1 are unchanged.

**Kept.** The barrier at the first instruction of a MIR block. The Rust
backend reads those positions and it was not shown that they only carry
lines.

**What it bought.** Read off O2 listings, combined build:

| Loop | Before | After |
|---|---|---|
| collatz `while` (even / odd) | 7 / 10 | 5 / 8 |
| quick-sort, two `==` tests | 3 each | 1 each |
| nested-loops inner | 5 | 5 |
| fib loop | 5 | 5 |
| bubble-sort inner (no swap / swap) | 8 / 19 | 8 / 19 |
| array-traversal, merge-sort | unchanged | unchanged |

The collatz case the fused ledger flagged (`x % 2 == 0` left as three
instructions) was this barrier: the `%` and the `== 0` test are two
statements. It is now `mod_int x 2; jump_if_int_eq 0`.

Instructions retired by the whole process (`/usr/bin/time -l`, fasttest
build, best of 3, two rounds). A run costs about 22 G before the workload
starts and that part moves by about 0.1 G between runs, so only collatz is
outside the noise:

| Workload | Before | After |
|---|---|---|
| collatz-100k | 30.23 G, 30.09 G | 29.65 G, 29.62 G |
| bubble-sort-5k | 45.94 G | 45.74 G |
| fib-iterative-100k | 24.98 G, 24.90 G | 24.94 G, 24.88 G |
| merge-sort-20k | 24.94 G, 24.83 G | 24.82 G, 24.82 G |
| quick-sort-3way-1k | 22.19 G, 22.03 G | 22.11 G, 22.05 G |
| nested-loops-500x500 | 22.04 G, 22.03 G | 22.12 G, 22.03 G |
| array-traversal-8k | 22.06 G | 22.02 G |

Collatz retires about 0.5 G fewer, some 6% of what the workload itself costs.

**Tests.** No test pinned a line that moved. One emit test looked for a plain
load of a local after a `NarrowBind` and now also accepts the fused
load-and-read-field. One bytecode snapshot changed. Corpus: 6081 pass and the
same 16 `root.trace*` failures in the full run, the non-trace pass list
identical to the baseline, trace-only 51 / 0. No corpus test pins a line that
fusion moved.

**Undo.** Restore the line-table loop at the end of `barriers` in `fuse.rs`.

## Array helpers that take a closure

`map`, `filter`, `reduce`, `find`, `find_index`, `some` and `every` are Rust
functions. They stay Rust functions. A call that passes a closure the inliner
can see and inline is replaced, at O2, by a loop written in BAML with the
closure's body inside it.

### Why not move the helpers to BAML

Measured first, on the fasttest build, 800,000 elements, neither form
inlined: `map` + `filter` + `reduce` cost 3.17 G instructions in Rust and
7.16 G as BAML loops called out of line. A call through a closure value
costs about 1,300 instructions and the Rust helper already pays about 1,240
per element, all in. So a BAML body is only worth having where the closure
is inlined into it. The Rust functions are what O0, O1 and every call with
an unknown closure still run.

### The loop forms

`containers.baml` gives each converted helper a sibling method named
`_inlined_` and its name, with the same parameters and type parameters.
`inline::loop_form` finds it by that name. `callee_of` uses it in place of
the Rust function only when one of the call's arguments is a closure made in
the same body that `closure_callee` accepts and that is within
`MAX_CALLEE_SIZE`. The loop form itself is exempt from `MAX_CALLEE_SIZE`,
not from `MAX_GROWTH`.

Three small Rust helpers back them:

- `_copy()`: a new array with the receiver's elements and its run-time
  element type. One call takes the snapshot the Rust helpers take with
  `to_vec()`.
- `_results<U>()`: the array `map` returns, as long as the receiver, element
  type `U`, slots unset. The loop writes every slot before the array is
  visible to anything.
- `_truncate(length)`: `filter` compacts the kept elements to the front of
  its copy and cuts the rest off.

Two things decided the shape of the loops:

- No `push`. A call of `push` costs about 2,000 instructions: the call
  passes the array's type argument and the VM's native-call path allocates
  for it. A `map` loop that pushes measured 1.89 G against 1.01 G for the
  Rust `map`. Index stores into a buffer of the right length cost about 500
  per element, read included.
- No `for ... in`. In generic standard library code it lowers to
  `Iterable.iter` and one `Iterator.next` interface call per element. The
  loop forms index with a counter.

### Exactness

`crates/baml_tests/tests/array_helpers.rs` was written before the switch
and pins, for each helper: the order of the closure calls, that the helper
iterates the elements it was called with when the closure changes the array,
what a throwing closure leaves behind, the run-time element type of the
result (empty arrays and union element types included), that an empty array
never calls the closure, a closure that suspends, and the stack trace of an
error that escapes. Every program runs at O1 and O2 and the two must agree.

A stack trace taken in a closure called by a Rust helper shows the helper's
frame with no file and line 0 (`File "", line 0, in baml.Array.map`). The
copy keeps that: its `InlineScope` names the Rust function and has
`native: true`. The emitter then writes an empty file name for that frame,
line 0 for the loop's instructions, and call line 0 for a closure inlined
inside it. No format change.

A closure that suspends is not inlinable, so the helper it is passed to
stays the Rust function.

### Left as Rust functions

- `sort_by`. A loop form was written (the same bottom-up merge, same
  comparison order, one-step write-back) and passed the tests, but it was
  slower: 200 sorts of 400 items cost 1.64 G against 1.30 G. The merge loop
  is about 35 bytecode instructions per comparison, more than the Rust merge
  plus a call. Removed.
- `flat_map`. Its result has no known length, so it needs a `push` or a
  `concat` per element.
- `find_last`, `find_last_index`. Not converted: each loop form adds to what
  every program compiles, and nothing measured uses them. The tests pin
  their behaviour anyway.
- `for_each` and `filter_map` were already BAML. They use `for ... in` and
  `push`, so they have the costs above. Not changed.

### Measured

Instructions retired, fasttest build, `/usr/bin/time -l`, best of 2, two
rounds alternating. Each program runs one helper 200 times over 4,000
elements with a capturing closure. The figure is the program minus the same
program with zero rounds.

| Helper | Before | After |
|---|---|---|
| `map` (ints) | 1.15 G, 1.17 G | 0.60 G, 0.60 G |
| `map` (class instances) | 1.43 G | 0.97 G |
| `filter` | 1.24 G, 1.31 G | 0.92 G, 0.92 G |
| `reduce` | 1.13 G, 1.14 G | 0.43 G, 0.42 G |
| `find` (no match) | 1.23 G, 1.17 G | 0.57 G, 0.58 G |
| `some` (no match) | 1.16 G, 1.14 G | 0.54 G, 0.51 G |
| `for_each` (not changed) | 0.58 G | 0.48 G |
| `sort_by`, 400 items (not changed) | 1.37 G | 1.28 G |
| the `array-pipeline` loop, 200 rounds | 7.31 G, 7.33 G | 5.40 G, 5.41 G |

The two unchanged rows show the noise of the subtraction, about 0.1 G.

Wall time per job from `scripts/speedtest` (3 trials, two rounds, a busy
machine, so only the best trial is worth reading): `array-pipeline-4k`
12.6 ms before, 8.0 ms after. `closure-apply-1m` and `generate-sort-8k` did
not move.

### Cost

Every loop form is standard library code compiled by every uncached run.
The startup workload retires 22.06 to 22.14 G instructions with them and
21.86 to 21.87 G without, about 1% more. Compiling a `main` with seven
helper calls inlined adds about 0.1 G. So one cold process running
`array-pipeline-4k` as committed (5 rounds, about 0.27 G of work) retires
more instructions in total than before (22.30 G against 22.08 G), while the
work itself costs about a quarter less.

## for-in loops

### What was there

Lowering always writes `for (let x in xs)` as `Iterable.iter` once and
`Iterator.next` per element. At O1 and O2 the emitter then recognises the
pair on an array (`escape::array_walks`) and emits one `array_iter_next`
instruction per element with a hidden index: no iterator object, no call.
The test is the declared type of the local `iter` is called on, and that the
iterator is seen by nothing but its `next`.

So the note above, that `for ... in` in generic standard library code pays an
interface call per element, was wrong. Checked in the bytecode: a user loop
over `int[]`, over `Point[]`, over a type alias of an array, over a field,
inside a lambda, a user generic `f<T>(xs: T[])`, and the standard library's
own `for_each` and `filter_map` all use `array_iter_next`. A map is not
iterable at all (`for` over a `map` is a type error).

Two cases did pay per element:

- An optional array parameter narrowed by a null test (`xs: int[]?`,
  `if (xs != null) { for (let x in xs) ... }`). Copy propagation replaced the
  narrowed temporary with the parameter, whose type is not an array, and the
  emitter no longer recognised the walk: three interface calls.
- A real iterator object. `Iterator.next` through the interface costs about
  3,300 instructions per element on a small user class and about 4,400 on
  the generic `ArrayIterator`. The inliner already resolves an interface call
  on a receiver whose class literal it can follow back, but it followed
  definitions block by block and gave up at a loop head, which is exactly
  where a `for` calls `next`.

### What changed

- `optimize::propagate_copies` leaves a copy that narrows a parameter to an
  array in its own local.
- `devirtualize::only_class`: a local written in one place only, with a class
  literal or a copy of a local of which the same holds, has that class
  wherever it is read. Parameters and captured locals do not qualify; a
  store into a field of the instance does not count as a write of the local.
  `class_of` asks it when following definitions fails. With it, `next` on an
  iterator the function builds is inlined into the loop, and scalar
  replacement then removes the instance when nothing else sees it.

`crates/baml_tests/tests/for_in.rs` pins what a loop does, at O1 and O2:
which elements it sees when the body appends, clears, overwrites or rebinds
the array, that the iterated expression runs once, the calls an iterator
object receives, a throwing and a suspending body, stack traces, and an
iterator local that holds two classes or a caller's value.

### Measured

Instructions per element, fasttest build, `baml-cli run --file` of a program
that loops 1,000 times over 4,000 elements (200 times for the chains), minus
the same program with zero rounds, best of 3. The "after" binary also has
the other VM work of the same day in it, which moved the unchanged rows by
about 8%.

| Loop | Before | After |
|---|---|---|
| `for` over `int[]` | 627 | 679 |
| the same as an index loop | 567 | 620 |
| `for` over `Point[]`, reads a field | 774 | 819 |
| user generic `f<T>(xs: T[])` | 491 | 529 |
| `xs.for_each(closure)` | 639 | 684 |
| `xs.filter_map(closure)` | 2,375 | 1,993 |
| `for` over a user iterator class built in place | 3,337 | 1,263 |
| `for` over `xs.iter()` held in a variable | 4,466 | 4,471 |
| `xs.iter().count()` | 4,398 | 4,287 |
| `xs.iter().reduce(f, 0)` | 5,821 | 5,826 |
| `xs.iter().map(f).reduce(g, 0)` | 14,582 | 14,264 |
| `xs.iter().filter(p).map(f).reduce(g, 0)` | 19,476 | 19,047 |

The narrowed optional array was checked in the bytecode only (three
`virtual_call` before, `array_iter_next` after), not timed.

**Tests.** `cargo test --profile fasttest -p baml_compiler2_mir -p baml_tests
-p baml_compiler2_rust`: 1445 passed, 0 failed, 26 ignored, no snapshot
changed. Corpus: 6081 pass and the same 16 `root.trace*` failures in the full
run, the non-trace pass list identical to the baseline, trace-only 51 / 0.

### Left: the adapter chain

Each stage of a chain costs one `next` through the interface (about 4,400)
and one closure call (about 1,400); a `Map` or `Filter` stage costs more
than that sum, about 8,700 and 4,900. Three things stand between the chain
and a plain loop, and none is small:

- `xs.iter()` has an array receiver. `devirtualize` only knows classes, so
  the call that would produce the `ArrayIterator` literal is not resolved.
- `Iterator.map` and `filter` build `Map<Self.Item, ...>` in a default
  method. Inlined, the literal's type arguments are projections of `Self`,
  which `only_class` and `trace` refuse (not fully concrete).
- `Map.next` calls `self.source.next()`. The class of what a field holds is
  only known once scalar replacement has turned the field into a local, and
  the inliner runs once, before it.

### To undo

Remove the `only_class` call in `class_of` to get the old devirtualization
back. Remove the `RuntimeTy::List` condition in `propagate_copies` to let
the narrowed copy be propagated again; the last test in `for_in.rs` then
fails.

## Native calls with type arguments

### What the cost was

The "about 2,000 instructions per `push`" above was not special to generic
standard library code. Plain user code (`let xs: int[] = []; xs.push(i)`) paid
it too, because every call of an array method passes the element type as a
type argument. Measured on packed programs (fasttest build, telemetry off,
instructions retired, 10 million pushes, minus the same loop without the
push):

| Case | Before | After |
|---|---|---|
| user code, `int[]` | 2,549 | 1,014 |
| user code, class instances | 2,646 | 1,065 |
| `f<T>(xs: T[], v: T)` kept a call, per push | 3,971 | 1,264 |
| `filter_map`, per kept element | 3,841 | 2,318 |
| a generic bytecode call on its own | 3,571 | 1,845 |
| loop with no calls (count, index sum, for-in sum) | 412 / 586 / 654 | 413 / 587 / 655 |

The `filter_map` row also contains the `let y: U` type test, which only runs
for a kept element, so it overstates the push.

No single allocation dominated. A profile of the old path showed the cost
spread over the general call path: two vectors of type arguments allocated
and freed per call (a quarter of the samples in a packed program, which uses
the system allocator), a drain and collect of the arguments, the call layout
lookup and compare, the pending type lane saved and restored, and a return to
the outer loop after every native call. In a generic frame, `LoadType` of `T`
also cloned the template and the frame's type arguments, substituted, and
allocated a new type object every time.

### What changed (all in `crates/bex_vm/src/vm.rs`)

- Type-argument buffers are reused. A call takes an emptied buffer, the
  pending lane owns it for the call, a bytecode frame that carries nothing
  takes it whole, and a returning generic frame gives it back.
- A native function called by name (`Call` on a plain native `Function`, no
  `$trace`, argument layout already the callee's) runs from the `Call` handler
  through `call_named_native`, and the loop goes on to the next instruction.
  Type operands are read where they sit instead of being drained. Every other
  callee takes the general path as before. The native body of the general
  path and of this one is the same function, `call_native`.
- `LoadType` of a constant that is one frame slot, in a static function whose
  frame binds no runtime declaration, keeps the type object it built with the
  slot type it was built from (`slot_load_type_cache`), and reuses it while
  the slot holds the same type naming the same declaration objects. A hit on
  the existing cache for realized constants no longer walks the constant.

The new code is out of line (`#[inline(never)]`). Inlined into the dispatch
loop it made loops that make no calls about 4 to 8% heavier; out of line they
measure the same as before (last row of the table).

### What remains

About 1,000 instructions per push is still far from a few hundred. From a
profile of the new path, per push, roughly: 250 for the type lane of a native
that never reads it (read and clone the type, save, restore, clear), 100 to
150 for the call layout lookup and compare on every call, 75 for the
`LoadType` cache lookup, 100 for copying arguments and settling, 150 in the
push itself and array growth. Two changes would remove most of that and were
not made: mark natives that never read their type arguments so the lane is
skipped, and pick a native call opcode once at link time, as
`call_specialize.rs` does for exact bytecode calls, so the layout is not
looked up per call. A generic bytecode call still goes through the general
path and costs about 1,850.

### To revert

Each part stands alone. Removing the early `call_named_native` branch in the
`Call` handler restores the general path for natives. Removing the
`built_load_type` and `keep_slot_load_type` calls in `LoadType` restores the
old lookup. The buffer reuse has no behaviour of its own.

Test: `crates/baml_tests/tests/generic_type_args.rs` pins that one site in a
generic function that stays a call reads the type of the frame it runs in,
across frames bound to different types in turn.

## Merge with canary (2026-11-16)

Canary gained a set of VM changes that solve the same problems as some of the
work above. Where both sides had an answer, canary's was kept and ours was
removed: canary's is merged upstream, runs inside its register-resident
dispatch loop (`run_fast_path`), and was measured on release builds. The
removed work stays in history under the commits named here.

**Dropped.**

- Fused instructions (`030fcd1c71`, `3adde41c44`, `15c620ef5f`): the
  instructions `IntArith`, `IntCmpJump`, `LoadVarField`, `LoadVarElement`,
  `MoveVar` and their compact opcodes, the pass in
  `baml_compiler2_emit/src/fuse.rs`, the VM handlers, and
  `baml_tests/tests/fused_instructions.rs`. Canary fuses at emit time
  (`fuse_tail` in `emit.rs`) into `AddLocalSmall`, `LoadVarField`,
  `LoadVarFieldLocal`, `JumpIfLocalsCmp` and `JumpIfLocalConstCmp`. With our
  pass gone, so is the relaxation in "Lines of fused instructions" above:
  canary's fusion never drops a sequence point or moves one to another span,
  so a failure reports its own line again. `MIR_OPTIMIZER_LEDGER.fused.md`
  describes the removed pass.
- The push-path fix (`56e4ec71c9`): canary's `try_reserve` builds its error
  only on failure and its `push_stack` is a single compare.
- Native calls with type arguments (`8f30ffe1bc`, `2c231cfd66`): the recycled
  type-argument buffers, `call_named_native` and `slot_load_type_cache`.
  Canary's `call_native_direct` and its per-site `LoadType` cache cover them.
  The test `baml_tests/tests/generic_type_args.rs` stays: it pins behaviour.
- The null tag test (`b2e99ab27c`): canary lowers `x == null` as a primitive
  comparison at every level and emits `IsNull` / `IsNotNull`. A comparison
  with a `null` operand no longer reaches the driver path our rule lived in:
  it fired zero times over the test corpus at O2, so the rule and its test
  were removed.

**Kept.** Everything else: the inliner and its frames in stack traces
(`Bytecode::inline_frames`, `inline_table`), scalar replacement, the
redundancy and loop passes, devirtualization, the array loop forms and their
private natives, the finalizer fix, the speedtest Go row and workloads.

**One change for the two to agree.** Canary's `fuse_tail` replaces
instructions without going through `emit`, which is where the inline table is
written. It now drops the replaced instructions' inline table entries and
records the superinstruction under the inlined call of the step whose span it
takes, with line 0 when that call is a function written in Rust. A test in
`baml_tests/tests/inliner.rs` raises an overflow from an `AddLocalSmall`
inside an inlined callee and checks both frames.

**Format versions.** Artifact 33, cache 30. 32 and 29 were only ever written
by builds of this branch with the removed instruction set.
