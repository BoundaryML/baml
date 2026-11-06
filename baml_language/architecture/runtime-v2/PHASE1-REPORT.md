# Phase 1 gate: M1 and M2 checkpoints; overall NO-GO

Binary-trees-14 now compiles from BAML MIR to typed Rust and runs on the v2 heap
and loop. M2 Richards now also runs end to end. M3 agent-loop remains unsupported. This is not approval
to advance past the three-workload phase-1 gate. M1 now also has fallible allocation and a tested emergency exception reserve.
General process-wide allocator exhaustion is not claimed by the managed-heap test limit.

Work is confined to the `v2-codegen` worktree on Spark/aarch64. No push or PR.
Prior checkpoint commits: `495a15c376` (runtime seam), `deb6f46dbe` (inventory),
`272ca1113b` (initial report). This report accompanies the M1 implementation commit.

## M1 implementation

- Opt in with `baml pack --rust --runtime v2 ... main`. Current backends keep
  their existing path. The v2 executable has no bytecode fallback or linked VM.
- New emission mode reuses the current backend's declaration identities,
  control-flow structuring, call argument layout and MIR source-PC line rules.
  Reachability follows direct calls. Unsupported MIR, types, initializers and
  entry signatures fail with `unsupported in runtime v2`.
- TreeNode is a 24-byte header-plus-two-pointers layout (32 bytes after heap
  alignment); descriptors preserve field order. All class allocations use
  `bex_v2_runtime::alloc`; pointer stores use the heap barrier hook. Scalars
  remain typed Rust locals; arithmetic enforces i63 and shifts reuse `Int63`.
- The main entry runs as a root task. Root access is loop-owned through
  `IntegrationRoots`, not a public `RootSource for TaskCx`. Register capture,
  parked stacks and precise roots remain the loop's responsibility.
- Calls return boxed `Result`s. Overflow/negative-shift errors allocate real
  message-bearing panic instances, use `VmPanic`, and append immutable source
  table frames only on failure. Formatting uses shared `format_traceback`.
  Null field checks use the VM's `VmInternalError::TypeError` constructor:
  today's null type is printed as `any`, not a new null exception class.
- Observation words initialize once at startup to off. Each generated function
  loads and branches on its word, retained under LTO. Non-off telemetry and
  embedded telemetry are rejected. No producer is claimed.
- The loop exposes its task stack bound through the integration handle. Generated
  prologues reserve 64 KiB for error handling on an 8 MiB fiber. Bodies over 1024
  locals are rejected. This bound API was missing in the initial integration seam.
- The host supports a plain integer invocation and native speedtest JSON
  `--jobs N --warmup N`, checking every warmup/timed result. Heap capacity persists
  across jobs; startup is excluded from timing.

## MIR feature inventory

The following is confirmed in the user MIR. The **exact monomorphized transitive
stdlib closure for M2/M3 remains unfinished**; M1 reaches only its three user
bodies and the lowered null-equality intrinsic. The existing native reachability walker
uses method names for virtual edges: Richards' `run` pulls in unrelated clients,
and agent-loop's `iter`/`next` pull in unrelated iterator implementations. Those
candidate edges must not be called actual reached functions. The inventory
command labels this limitation; the extracts omit expanded candidate lists and
retain the actual MIR terminators and types.

| Workload | User bodies | Required representations and operations |
|---|---:|---|
| Richards-50 | 16 | Seven classes: Packet, Tcb, Scheduler, IdleTask, DeviceTask, WorkerTask, HandlerTask; nullable class links; `Task` existential; `int[]`, `(Tcb?)[]`; checked integer arithmetic, bitwise operators and right shift; field/index mutation; branches, loops, null tests; finite `Task.run` dispatch over the four task implementations |
| Agent-loop-300 | 6 | Search, Calc, Finish, AgentState, AgentFailed, Rng; class unions, nullable union, optional int and string; string literals and literal unions; flat strings, string concatenation; `int[]`, `string[]`, `string[][]`, `map<string,int>`; checked arithmetic/remainder; array iteration, indexed access and push; map mutation and values; type tests, switches, null coalescing; typed JSON decoding; caught parse/decode errors, custom throw, typed catch and rethrow |
| Binary-trees-14 | 3 | TreeNode with two nullable TreeNode links; recursive direct calls; recursive allocation with the first child live while constructing the second; retained long-lived tree; checked integer arithmetic and left shift; null tests, branches and loops |

No floating arithmetic, user closures, user generics, spawned tasks, await,
network I/O, cleanup methods, or runtime compilation occur in these workload
sources/user bodies. Their absence does not eliminate compiler-generated type
operands, iterator values, exception contexts or builtin internal behavior.

Direct stdlib/builtin calls in user MIR:

- All three: `baml.ops.equals_equals` (notably nullable comparisons).
- Agent-loop additionally: `baml.Array.join`, `baml.Array.push`, `baml.Map.set`,
  `baml.Map.values`, `baml.String.length`, `baml.String.slice`,
  `baml.String.to_code_points`, `baml._to_string_default`, `baml.json.decode`.
- Richards' virtual call is `run as user.Task`.
- Agent-loop's virtual calls are `Iterable.iter` and `Iterator.next`, with
  item types `int`, `string`, and `string[]`.

Important transitive details visible in the full census/current owners:

1. `baml.json.decode<T>` has an omitted `Naming` argument. Its body supplies
   `Naming.Declared`, calls `_aliased`, loads type argument zero and calls
   `baml.json._decode<T>`. The workload supplies `Search | Calc | Finish`.
   `_aliased` switches on the enum discriminant; `_decode` is a VM builtin.
2. Array iteration goes through the array Iterable implementation,
   `ArrayIterator.new<T>`, and its Iterator implementation. The native compiler
   already recognizes an index-walk lowering. Reuse its proof rather than
   treating every same-name `next` as reachable.
3. Generic type operands in the user bodies are `int`, `string`, `string[]`, and
   `Search | Calc | Finish`. Specialization must follow `LoadType`/copies and
   defaults, not merely function names.
4. A MIR local can carry a function-shaped type while holding an extracted
   builtin receiver (for example `_20` in `run_conversation` before Array.push).
   Mapping every local's declared type directly to a Rust storage type is not
   sufficient; preserve the current backend's value/call-shape analysis.
5. `_decode` first attempts its typed text path and then uses parse/decode
   fallback. Existing constructors allocate ParseError and DecodeError (message
   plus path for DecodeError). See `bex_vm/src/package_baml/json/mod.rs` and
   `text.rs`. A serde visitor with approximate messages would not satisfy the
   requested error contract, even if this benchmark catches those errors.
6. Catch `_` explicitly tests the PANIC membership tag and rethrows panics with
   the saved context. It is not a catch-all Rust `Err => null` conversion.
7. The speedtest BAML driver adds timing and reporting calls. The v2 pack
   supplies a host driver with the same `--jobs`/`--warmup` JSON protocol for
   its admitted integer entry, rather than compiling the timing/report BAML.

## Correctness and milestone status

| Milestone | Normal | GC stress | Status |
|---|---|---|---|
| M1 binary-trees-14 | Bytecode = v2: stdout `3222190\n`, empty stderr, exit 0 | Same exact output/status | Verified, including forced managed-heap allocation failure |
| M2 richards-50 | Bytecode = v2: stdout `1161046400\n`, empty stderr, exit 0 | Same exact output/status | Verified; see M2 checkpoint below |
| M3 agent-loop-300 | Not run | Not run | Pack rejects nested string lists; unions/JSON/catches not implemented |

The oracle is a locally built current Rust pack with `BAML_COMPILED_BODIES=0`.
Both executables were also run with `BAML_GC_STRESS=1`; that setting forces the
v2 collector, while the existing bytecode backend retains its own GC policy
(the existing stress switch is implemented in compiled_rt, not the bytecode
heap). There is no claim that setting the environment variable makes bytecode
collect at every allocation. Exact workload stdout/stderr/status evidence is
[`phase1/trees-correctness.json`](phase1/trees-correctness.json).

Overflow and negative-shift programs also match the bytecode oracle's error
class, message, source file/line frames, empty stdout and exit 1 in both modes.
For these small probes the oracle used `baml run`; its one source-format
advisory was removed before comparing runtime diagnostics. Raw normalized
results: [`phase1/errors-correctness.json`](phase1/errors-correctness.json).
The null-error constructor is checked against the existing shared VM type
mapping in the runtime unit test; no accepted source-level null-dereference
fixture was found in this narrowly typed subset.

The sibling harness now has a generic `Backend` builder seam, but no v2 builder
or supported-entry selector has landed. Its full fixture corpus requires M2/M3
and later features. This milestone therefore compares against bytecode directly;
it does not claim that the full differential harness passed.

## Gate measurements (initial M1, before fallible allocation)

Spark/aarch64, CPU 12 via `taskset`, telemetry off, one untimed warmup, five
interleaved rounds (Node, native, v2, spike). Final trials use **20 timed jobs**
per backend, normalized to ms/job. Every return is checked, including warmups;
all processes exited 0 with empty stderr. Node uses the speedtest Node driver
with a result-checking wrapper around the unchanged workload. Native uses the
same BAML driver with its ignored timed/warmup returns changed to comparisons.

Rust packs and the rebuilt spike use Rust 1.98.0, release/fat LTO/one codegen
unit/unwind, without CPU-native flags or PGO. Node is v24.15.0. The spike is a
local copy of the supplied source, rebuilt in this worktree, default **arena**
variant (small error Result ABI). It is not the spike's GC or boxed-error
ablation: this ratio includes allocator/runtime differences, not just emitter
quality. The generated pack uses the real v2 collector and boxed errors.

| Median ms/job, telemetry off | Node | Current native | Generated v2 | Handwritten spike | Generated / handwritten |
|---|---:|---:|---:|---:|---:|
| binary-trees-14 | 38.466 | 420.184 | 37.572 | 38.401 | **0.978×** |
| richards-50 | — | — | Unsupported | — | — |
| agent-loop-300 | — | — | Unsupported | — | — |

Final ranges (min–max ms/job): Node 38.283–39.254, native 418.533–424.854,
v2 37.471–37.962, spike 37.992–39.186. This supports the M1 performance gate;
it does not establish the other workloads or full runtime contract.

Two earlier complete five-round sets used bench.toml's one timed job. Their
medians were respectively `(52.553, 940.416, 79.633, 77.111)` and
`(47.220, 1015.614, 116.103, 80.776)` in table-column order, yielding **1.033×**
and **1.437×**. Host load was high (observed load average about 44 on 20 CPUs),
with large scheduling variance; these are not discarded individual outliers.
After local validation finished, the full 20-job set above was run to average
short-trial scheduling effects. Host load also fell during that run. All three
sets remain checked in: `phase1/trees-benchmark-{first,confirmation,batched}.jsonl`.
No samples were selected or mixed between sets. An idle-host one-job rerun
would still be useful before a final architecture-wide go decision.

## Profile and remaining costs

Although the measured ratio never exceeded the requested 1.5 profiling trigger,
a `perf` user-cycle profile was recorded for 100 checked v2 jobs on CPU 12.
It contains 3,961 samples with no lost samples; the main CPU event accounts for
nearly all samples. [`phase1/trees-v2-perf-report.txt`](phase1/trees-v2-perf-report.txt)
contains the flat profile. Dominant self-time:

- **42.18% allocation wrapper including inlined heap bump work**. This symbol
  includes metadata/header stores and allocation checks; it is not 42% TLS cost.
- **21.33% tree walk; 16.33% recursive construction**. These include direct-call
  Result handling, i63/null checks, observation loads and stack-bound checks.
- **5.68% zeroing**, plus **11.06% locate/drain/mark** and **1.36% allocation slow
  path**. The arena spike does not pay this collector's metadata/tracing costs.

There is no positive generated/handwritten gap in the stable set to explain.
These are measured cost centers and plausible future optimization targets,
not an ablation-based decomposition of a slowdown. No error/GC/observation
checks were removed to obtain the headline. `phase1/trees-observation.asm`
shows the actual `ldrb`/`cbnz` observation branch and stack-bound prologue.

## Validation

- Compiler units: 9 passed. Existing native backend regression tests: 9 passed;
  the existing full-corpus admission test remains explicitly ignored. New v2
  tree-layout/unsupported-admission test: passed after correcting its fixture
  to keep the unsupported value live through MIR optimization.
- Loop: 2 units, 28 integration tests, 2 ownership doctests passed after the
  root-ownership change. Runtime: 3 tests passed in debug and optimized release,
  including recursive tree retention under GC stress and actual panic object
  fields/messages under collecting allocations.
- CLI library: **459 passed**, using `nix shell nixpkgs#go` for the existing
  formatter dependency. No CLI test failures remain.
- Strict all-target clippy (`--no-deps -- -D warnings`) for compiler, CLI, loop
  and runtime passed. Formatting and `git diff --check` passed.
- Full bytecode corpus and the ignored native executable differential suite
  were not rerun. M2/M3 reject explicitly; diagnostics are recorded in
  `phase1/unsupported-workloads.json`.

## Reproduction

Run from `/home/rathi/Documents/Git/boundary/baml/.worktrees/v2-codegen`.
All generated projects, executables, raw profiles and build logs stay under
`target/phase1/`; the preparation script only reads the original spike/workloads.

```sh
export CARGO_TARGET_DIR="$PWD/target"
export BAML_CLI_ALLOW_DIRECT=1
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli
uv run --no-project python baml_language/architecture/runtime-v2/phase1/prepare_trees.py

target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --runtime v2 --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/trees-v2-build" \
  --file baml_language/tools/speedtest/workloads/memory/binary-trees-14/main.baml \
  main -o target/phase1/trees-v2

target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/trees-native-build" \
  --from target/phase1/trees-native-project speedtest_driver \
  -o target/phase1/trees-native

# Separate plain oracle; --rust uses the local runtime matching the artifact.
# BAML_COMPILED_BODIES=0 below selects its bytecode execution.
target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/trees-native-build" \
  --file baml_language/tools/speedtest/workloads/memory/binary-trees-14/main.baml \
  main -o target/phase1/trees-oracle

CARGO_TARGET_DIR="$PWD/target/phase1/spike-target" cargo +1.98.0 build \
  --release --manifest-path target/phase1/spike/Cargo.toml

BAML_TELEMETRY=off BAML_COMPILED_BODIES=0 taskset -c 12 target/phase1/trees-oracle
BAML_TELEMETRY=off taskset -c 12 target/phase1/trees-v2
BAML_TELEMETRY=off BAML_COMPILED_BODIES=0 BAML_GC_STRESS=1 \
  taskset -c 12 target/phase1/trees-oracle
BAML_TELEMETRY=off BAML_GC_STRESS=1 taskset -c 12 target/phase1/trees-v2

PHASE1_JOBS=20 uv run --no-project python \
  baml_language/architecture/runtime-v2/phase1/bench_trees.py

# perf 7.1 from nixpkgs; keep this separate from timed gate rounds.
perf record -e cycles:u -F 997 -g --call-graph dwarf \
  -o target/phase1/trees-v2.perf.data -- \
  taskset -c 12 target/phase1/trees-v2 --jobs 100 --warmup 1
perf report -i target/phase1/trees-v2.perf.data --stdio --no-children \
  --call-graph none --sort symbol --percent-limit 1

cargo test --manifest-path baml_language/Cargo.toml -p baml_compiler2_rust \
  -p bex_v2_loop -p bex_v2_runtime
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime --release
cargo test --manifest-path baml_language/Cargo.toml -p baml_tests --test rust_backend
nix shell nixpkgs#go --command cargo test --manifest-path baml_language/Cargo.toml \
  -p baml_cli --lib
cargo clippy --manifest-path baml_language/Cargo.toml \
  -p baml_compiler2_rust -p baml_cli -p bex_v2_loop -p bex_v2_runtime \
  --all-targets --no-deps -- -D warnings
```

The plain prebuilt bytecode pack template on this host had a different toolchain
fingerprint and rejected this branch's artifact, so the oracle uses the matching
local Rust pack with compiled bodies disabled. `--agent-skill-check off` only
bypasses the CLI installation preflight; it does not affect language execution.
The generated Rust snapshot is `phase1/trees.generated.rs`. To reproduce the
error probes, repeat the v2 pack command with `--file
baml_language/architecture/runtime-v2/phase1/overflow.baml` (then
`negative-shift.baml`), and compare the resulting executable with
`baml-cli run --file <same-file> main`, normally and with `BAML_GC_STRESS=1`.
Only the CLI source-format advisory is excluded from those diagnostic comparisons.

## Remaining interface gaps and questions

1. **§2.2 managed allocation failure is implemented.** `try_alloc` performs a
   full collection/retry before returning refusal. The old infallible API remains
   available. Each runtime instance reserves one separate, traced 32 KiB block;
   ordinary allocation cannot spend it, and error construction does not request
   more heap backing. The admitted uncaught-OOM path needs one small string and
   class object. Arbitrarily retained caught OOM objects can exhaust a finite
   reserve; that general case is not admitted in phase 1. Host `Box`, trace-vector,
   collector-metadata and process-wide allocator failure are not simulated by
   the managed-capacity limit.
   Ordinary interpreter class allocation had no recoverable OOM message to
   preserve. A shared `VmPanic::allocation_failure(bytes)` now supplies
   `Allocation of {bytes} bytes failed`; existing operation-specific messages
   are unchanged. The interpreter's real `panic_to_exception_value` conversion
   and the forced-limit v2 path are paired tests in separate executables.
2. The message-bearing exception layout matches the VM, and `VmPanic`, null
   `VmInternalError`, `Int63` and traceback formatting are shared. The actual
   heap-object allocation adapter is v2-specific. Factoring general builtin
   error mapping into a heap-neutral owner is still required before M3 catches;
   this checkpoint does not claim the full §3.3 builtin ABI.
3. Catches need rooted exception storage: current errors leave the root task
   without further managed allocation, so their host boxes are not scanned.
   General caught/rethrown values, host heap results, finalizers and child-task
   runtime bindings remain unimplemented and are outside admission.

Root ownership is resolved as requested; no decision is pending on TaskCx.
No representation relaxation is requested. M2/M3 and the full three-row gate
remain work to do, not implicit fallbacks or performance conclusions.

## M1 allocation-failure checkpoint

The user independently confirmed both binary-trees executables return `3222190`.
Their reported five-round run with 10 jobs/trial and host load about 27 measured
Node 40.5, native 470, v2 38.3 and spike 45.1 ms/job (ratio 0.849×). Those are
user-reported confirmation numbers, not substituted for the measurements below.

After adding fallible allocation, a fresh five-round set on CPU 12 with 10
jobs/trial, one warmup, telemetry off and every return checked measured:

| Workload | Node ms/job | Native ms/job | V2 ms/job | Spike ms/job | V2 / spike |
|---|---:|---:|---:|---:|---:|
| binary-trees-14 | 39.145 | 419.181 | 36.277 | 37.922 | **0.957×** |

`phase1/trees-benchmark-allocation.jsonl` preserves each measurement and the
1/5/15-minute load averages immediately before and after each backend.
The observed one-minute load range was **15.60–24.80**.
The previous profile above belongs to the initial M1 checkpoint; this update
has no ratio above 1.5 and makes no new causal performance attribution.

`BAML_V2_HEAP_LIMIT=0` deterministically refuses ordinary allocation after full
collection and raises `baml.panics.AllocFailure` with message
`Allocation of 24 bytes failed`, exit 1, empty stdout and the correct tree
allocation/caller lines. Both stress modes pass. Unlimited runs still produce
exactly `3222190`, empty stderr and exit 0. Evidence: `phase1/oom-correctness.json`.
The limit measures usable ordinary block capacity plus large-object backing,
not slab reservation, metadata or RSS; small-object capacity has 32 KiB granularity.

Additional reproduction commands:

```sh
BAML_V2_HEAP_LIMIT=0 BAML_TELEMETRY=off target/phase1/trees-v2
BAML_V2_HEAP_LIMIT=0 BAML_GC_STRESS=1 BAML_TELEMETRY=off target/phase1/trees-v2
PHASE1_JOBS=10 uv run --no-project python baml_language/architecture/runtime-v2/phase1/bench_trees.py
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_heap
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_heap --release
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime --release
cargo test --manifest-path baml_language/Cargo.toml -p baml_tests --test rust_backend
```

Heap tests cover full-collection refusal with a retained live graph, emergency
object tracing, recovery after releasing roots, large-object refusal, and size
overflow. The runtime test forces a zero limit, checks the actual error object
and string through a GC, and successfully allocates after lifting the limit.
The paired interpreter test checks the same class and message through its real
exception constructor. Linking both runtimes into one test was rejected by the
linker because the two corosensei builds export the same assembler symbols;
the tests correctly remain separate executables, as do the packed backends.

Checkpoint validation: heap 4 unit + 1 fiber + 15 integration + 2 doctests pass
in debug and release; runtime 4 tests pass in debug/release; compiler 9 unit
tests and VM types 128 tests pass; native regression/exception conversion
11 pass with the existing full-corpus test ignored. Strict clippy passes for
heap, runtime, compiler, VM types and the rust_backend test target.


## M2 Richards checkpoint

M2 adds seven typed class layouts, `int[]`, nullable-class lists, negative
subscripts, boolean short circuit, bitwise operators/right shift, and 16-byte
interface slots. `Task.run` resolves **only the four implementations in the
linked Task implementation rules**, rather than every method named `run`.
There are 16 emitted user bodies, including those four methods; the only stdlib
edge is the existing lowered null-equality intrinsic. Lists have a header,
eight-byte length and inline typed slots. These fixed-capacity lists suffice
for Richards; growth and the M3 container builtins remain unsupported.

Pointer fields precede interface tagged fields, then scalar fields. Descriptors
retain declaration-order field metadata. Pointer lists and tagged class slots
are traced by the real heap; pointer stores use the barrier hook. A collecting
runtime test covers both kinds together. Results, observation branches, i63
checks, source-line side tables, root tasks and loop-owned roots remain in use.

`phase1/richards-correctness.json` records exact stdout, stderr and exit-status
comparisons against **direct CLI bytecode execution**, normally and under v2 GC
stress. Both return `1161046400`. The signed-index error probe also matches the
interpreter's actual `IndexOutOfBounds` object fields and traceback. Negative
indices within range count from the end, as in bytecode. No v2 backend builder
has appeared in the sibling differential harness; direct comparison remains
necessary. As for M1, its stress environment does not force the bytecode heap's
collection frequency.

### M2 gate and profile

Spark aarch64, CPU 12, telemetry off, five interleaved Node/native/v2/spike
rounds, **10 jobs/trial**, one warmup, all results checked. One-minute host load
was **11.34–12.07**, with five-minute load around 18. The spike is the same rebuilt
arena executable used for M1, not its GC ablation. Commands and all observations
are in `phase1/richards-benchmark.jsonl`; generated code is
`phase1/richards.generated.rs`.

| Workload | Node ms/job | Native ms/job | V2 ms/job | Spike ms/job | V2 / spike |
|---|---:|---:|---:|---:|---:|
| binary-trees-14 (allocation checkpoint) | 39.145 | 419.181 | 36.277 | 37.922 | **0.957×** |
| richards-50 | 5.632 | 82.922 | 3.936 | 2.670 | **1.474×** |
| agent-loop-300 | — | — | Unsupported | — | — |

Richards min–max ms/job: Node 5.521–6.699, native 82.610–83.845,
v2 3.922–3.940, spike 2.664–2.682. An earlier complete run, before fixing
negative indexing, measured 4.132/86.415/5.222/5.432 (0.961×); it is retained as
`phase1/richards-benchmark-first.jsonl`, not mixed into the final set.

Richards is the worst current ratio. Although below the 1.5 trigger, it was
profiled for 1,000 checked jobs using `perf record -e cycles:u -F 997 -g
--call-graph dwarf`. No samples were lost. The final flat profile is
`phase1/richards-v2-perf-report.txt`:

- **47.31%** in the inlined scheduler/task-dispatch body (`entry`). The generated
  interface is a tag plus a pointer to a separately allocated implementation;
  the handwritten spike embeds its task enum and fields in Tcb. This introduces
  an additional pointer load in generated dispatch. Null checks, per-function
  observation loads and stack-bound checks are also present in the inlined body.
- **36.74%** across enqueue (14.97), suspend (6.46), queue walk (6.31), release
  (5.79) and hold (3.21). These direct-call bodies retain Result and language
  checks; the profile does not separate each check's individual cost.
- **15.57%** in runtime safepoint (6.51), loop maintenance (4.63) and task
  checkpoint (4.43). The spike poll only reads its cancellation flag; generated
  code also performs the real scheduler/collector checkpoint contract.

These are cost centers plus source-supported differences, **not an additive
ablation of the 47.4% gap**. No required checks were disabled for the gate.

Validation: compiler 9 units, runtime 5 tests in debug/release, and existing
native regression target 11 passing tests (one existing ignored full-corpus
test). Strict all-target clippy for compiler/runtime passes. The M1 CLI suite
was not repeated because M2 does not change CLI code. Unsupported nested lists,
optional scalars, strings, catches and generic interfaces still fail admission.

Reproduce from this worktree, with the M1 environment exports:

```sh
uv run --no-project python baml_language/architecture/runtime-v2/phase1/prepare_richards.py
target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/trees-native-build" \
  --from target/phase1/richards-native-project speedtest_driver \
  -o target/phase1/richards-native
target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --runtime v2 --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/richards-v2-build" \
  --file baml_language/tools/speedtest/workloads/programs/richards-50/main.baml \
  main -o target/phase1/richards-v2
BAML_TELEMETRY=off target/phase1/richards-v2
BAML_TELEMETRY=off BAML_GC_STRESS=1 target/phase1/richards-v2
BAML_TELEMETRY=off target/debug/baml-cli --agent-skill-check off \
  --no-progress --color never run \
  --file baml_language/tools/speedtest/workloads/programs/richards-50/main.baml main
PHASE1_JOBS=10 uv run --no-project python \
  baml_language/architecture/runtime-v2/phase1/bench_richards.py
# Index-error: repeat pack/run using phase1/index-error.baml.
```

No new interface deviation is requested in M2. M3's rooted caught errors,
container growth, string/JSON builtins and specialized decoder remain pending;
the overall phase-1 gate is still NO-GO.

## M3 preparation checkpoint: caught-error roots (M3 incomplete)

M2 is committed as `2f25d5b908`; allocation failure as `4c3723d094`.
This checkpoint adds the root lifetime needed before admitting catches. It does
**not** admit agent-loop or claim its correctness/performance row.

`IntegrationRoots::root` registers a precise slot with the task loop and returns
an owning lease. Dropping a lease releases the root without borrowing the loop,
which matters during fiber unwinding. Expired registrations are pruned when a
new lease is made. The loop remains the sole RootSource; TaskCx does not
implement it. `bex_v2_runtime::CaughtError` keeps an error's language value rooted
while Rust owns the boxed error, and transfers the original value/trace on
rethrow. No catch code is emitted yet.

The new loop test verifies precise enumeration, release during a borrowed loop,
and repeated lease reuse. The runtime test retains an actual IntegerOverflow
value through 100 forced collecting safepoints, checks its message object, and
checks that rethrow preserves value identity and the source trace. Debug and
release pass: loop 2 unit + 29 integration + 2 ownership doctests; runtime 6
unit tests. Strict all-target clippy passes for both crates.

```sh
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_loop -p bex_v2_runtime
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_loop -p bex_v2_runtime --release
cargo clippy --manifest-path baml_language/Cargo.toml -p bex_v2_loop -p bex_v2_runtime \
  --all-targets --no-deps -- -D warnings
```

Still required for M3: string and growing-container representations and builtins,
union/coalesce/type-test emission, proven array index-walk reuse, unwind edges
and typed catches, and the typed JSON decoder with exact interpreter failure
ordering/messages. The interpreter decoder's fast text path falls back to
parse-plus-structural decode: syntax errors precede schema errors, duplicate
keys use the last value, field failures follow declaration order, and union
candidates are ranked by matching keys before aggregated error reporting.
A simple serde derive would not satisfy this contract. There is no VM fallback
and no new requested deviation from INTERFACES. The three-workload gate remains
NO-GO until these pieces and the agent-loop row are verified.

After the root-lease change, freshly rebuilt trees and Richards packs again
return `3222190` and `1161046400`, respectively, both normally and with
`BAML_GC_STRESS=1`. Agent-loop still fails pack with
`unsupported in runtime v2: list element`. Benchmark rows above belong to their
named completed checkpoints; this preparation checkpoint adds no agent-loop
measurement and does not substitute correctness timings for benchmark trials.

## Richards gap checkpoint: shared poll word and bounded inlining

The user independently confirmed the M2 result and measured Node 3.91, native
83.0, v2 3.93 and spike 2.66 ms/job at load about 21 (50 jobs/trial, ratio 1.48).
The measurements below are new local runs, not substituted user numbers.

Changes:

- The loop and heap share an owner-thread checkpoint word. Bit zero requests
  scheduler/cancellation work; bit one requests collection. A quiet generated
  poll reads this word and skips task cancellation walks, RefCell borrows,
  maintenance and heap-accounting reads. The actual task/heap checks are cold,
  out-of-line code. The release path retains a separate null-binding guard for
  the public safe language wrapper; assembly shows `cbz` for that guard followed
  by `ldrb`/`cbnz` for the work word, not an assertion-free unchecked API.
- Local cancellation sets the scheduler bit immediately. Ready tasks, external
  cancellation sources, deadlines, I/O and host submissions keep the scheduler
  bit active. A slow poll refreshes it; cancellation remains sticky. Existing
  scheduling and cancellation delivery tests run unchanged.
- Budget-crossing allocation sets the heap bit, and collection clears it unless
  stress is enabled. Stress keeps it set, so every generated safepoint still
  collects. Small allocations retain an inline bump path: the layout's budget
  comparison threshold is computed on the cold path, avoiding an extra add or
  branch in every allocation. Collection policy still charges the same objects.
- Polls were already every 16 iterations using local counters at MIR loop
  headers. That frequency is unchanged. Observation loads and stack checks
  remain per function and are separate from the loop checkpoint: merging them
  would conflate different frequencies and lifetimes. Inlining may eliminate
  redundant checks when the machine frame and pointer facts are unchanged.
- Small nonrecursive generated functions are forced inline. The linked call
  graph includes virtual dispatch targets; recursive SCCs keep real frames.
  Forced inlining is limited to programs with at most 1,024 total MIR locals.
  Immutable error side tables and each call's error-frame append remain present,
  including when LLVM inlines the body.
- Task representation is unchanged: the 16-byte interface slot holds a concrete
  type tag and an implementation pointer. The spike instead embeds its task enum
  in Tcb. Removing that pointer without proving unique ownership could change
  aliasing semantics and would violate the specified class layout.

### Final measured gate

Spark aarch64, CPU 12, telemetry off, five interleaved rounds, one warmup,
**50 jobs/trial for Richards and 20 for trees**. Every result, exit status and
stderr was checked. Trees includes the saved pre-change v2 executable as a
fifth backend in every round, to check regression under the same host load.

| Workload | Node ms/job | Native ms/job | V2 ms/job | Spike ms/job | V2 / spike |
|---|---:|---:|---:|---:|---:|
| richards-50 | 3.938 | 83.121 | 3.035 | 2.660 | **1.141×** |
| binary-trees-14 | 38.358 | 418.961 | 36.389 | 37.965 | **0.958×** |
| agent-loop-300 | — | — | Unsupported | — | — |

One-minute load ranges: **8.71–10.22** for Richards; **4.68–8.81** for trees.
The previous trees executable measured **36.411 ms/job** in the same rounds;
new/old is **0.9994×**, effectively unchanged. Richards meets the requested
≤1.2 ratio. This is a two-workload checkpoint, not the three-workload GO gate.
Final observations: `phase1/checkpoint-guard-{richards,trees}.jsonl`.

Intermediate complete trials are retained, not pooled into the final medians:
shared word alone 3.422/2.661 ms/job (1.286×); first inlining trial 2.772/2.659
(1.042×); subsequent loaded run 3.187/2.748 (1.160×). The first paired trees run
found 37.854 versus 37.002 ms/job (2.3% slower), prompting the cold-path budget
threshold fix. Files: `poll-richards-bench`, `inline-richards-bench`,
`poll-trees-bench`, and `checkpoint-final-{richards,trees}`, all `.jsonl` under
`phase1/`. The final threshold fix then produced the table above.

### Before/after profile

Before: `phase1/richards-v2-perf-report.txt` from M2, 1,000 checked jobs,
15.57% across safepoint (6.51), maintenance (4.63) and task checkpoint (4.43),
47.31% in entry and 36.74% in the named scheduler helpers.

After: `phase1/richards-checkpoint-after-perf.txt`, 1,000 checked jobs,
3,110 samples and zero lost samples. **99.32%** is in the fully inlined entry;
allocation slow path is **0.35%**. The old scheduler/checkpoint symbols no longer
appear at the 0.1% reporting threshold. This does not mean polling costs zero:
the counter and word load/branches now live inside entry. The annotation confirms
those instructions and the observation-word branches remain. Inlining also
makes old/new per-symbol percentages incomparable as an exact causal split.
Remaining costs are the scheduler algorithm, typed loads/stores, dispatch,
observation/stack/null/i63 checks, and the inline poll branch. No required check
was disabled. Perf wall times are excluded from gate medians.

### Validation and commands

Heap 5 unit + 1 fiber + 15 integration + 2 ownership doctests pass; loop 2 unit
+ 30 integration + 2 ownership doctests pass; runtime 7 tests pass, all in debug
and release. New tests cover the shared word at small/large allocation budget
crossings, persistent GC stress, immediate/repeated cancellation after quiet
polls, and actual recursive stack overflow followed by instance reuse.
Compiler units (including recursive-SCC inlining exclusion), the existing native
regression target (11 passed, one existing ignored test), and strict all-target
clippy for touched crates pass. CLI code is unchanged.

```sh
# Save the prior executable before rebuilding (same worktree, no checkout change).
cp target/phase1/trees-v2 target/phase1/trees-before-checkpoint
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli
# Rebuild the v2 trees/Richards packs using the M1/M2 pack commands above.
PHASE1_JOBS=50 uv run --no-project python \
  baml_language/architecture/runtime-v2/phase1/bench_richards.py
PHASE1_JOBS=20 PHASE1_TREE_BEFORE=target/phase1/trees-before-checkpoint \
  uv run --no-project python baml_language/architecture/runtime-v2/phase1/bench_trees.py
perf record -e cycles:u -F 997 -g --call-graph dwarf \
  -o target/phase1/richards-checkpoint-after.perf.data -- \
  taskset -c 12 target/phase1/richards-v2 --jobs 1000 --warmup 1
perf report -i target/phase1/richards-checkpoint-after.perf.data --stdio \
  --no-children --call-graph none --sort symbol --percent-limit 0.1
cargo test --manifest-path baml_language/Cargo.toml \
  -p bex_v2_heap -p bex_v2_loop -p bex_v2_runtime -p baml_compiler2_rust
cargo test --manifest-path baml_language/Cargo.toml \
  -p bex_v2_heap -p bex_v2_loop -p bex_v2_runtime --release
cargo test --manifest-path baml_language/Cargo.toml -p baml_tests --test rust_backend
cargo clippy --manifest-path baml_language/Cargo.toml \
  -p bex_v2_heap -p bex_v2_loop -p bex_v2_runtime -p baml_compiler2_rust \
  --all-targets --no-deps -- -D warnings
```

No representation deviation or user decision is required. The new shared-word
integration API preserves root ownership. M3 remains pending at this checkpoint.

Final correctness was rerun after the budget-guard change: trees and Richards
match bytecode's stdout, empty stderr and zero exit status in both modes. The
newly inlined index-error fixture matches the complete bytecode diagnostic and
exit status in both modes, demonstrating retention of the caller/callee source
frames. Evidence: `phase1/checkpoint-trees-correctness.json` and the refreshed
`phase1/richards-correctness.json`. The final guard-specific release tests and
clippy also passed.

## M3 checkpoint: flat strings and typed builtin glue

M3 is **not complete**. This checkpoint adds managed flat strings, class string
fields, concatenation, and `String.length`/`char_count`/`slice` emission. The
stdlib's BAML `length` body is compiled, including its Hidden observation branch.
`baml_builtins2_codegen::generate_v2` now emits the implemented typed adapters,
validates their extracted stdlib signatures, and rejects unsupported requests.
Interpreter glue is unchanged. The current adapter set is deliberately limited
to `char_count` and `slice`; this is not a claim that all of §3 is finished.

Slicing reuses `BexStr::char_slice` and the exact signed/clamped slice-bound
helper, moved unchanged from `bex_vm` into `baml_type::sequence`. Runtime tests
cover Unicode scalars, extreme positive/negative bounds, concatenation and
integer formatting across 81 stressed collections. Generated calls use the
existing argument-layout mapping, including reordered named arguments.

MIR's extracted builtin receivers can occupy function-typed temporaries. V2
now infers only unnamed, copy-assigned receiver storage whose definitions all
agree; actual function values, call-produced function values, and ambiguous or
cyclic storage remain unsupported. Builtin companion `baml.String` is resolved
by its language-root identity to the primitive representation, not by a
user-spoofable short name.

`phase1/strings.baml` packs to a standalone v2 executable and returns **340**,
matching bytecode stdout, stderr and exit status normally and with v2 GC stress.
Only the CLI formatting warning is normalized. Evidence and reproduction:
`phase1/strings-correctness.json` and `phase1/check_strings.py`. The bytecode
stress environment does not imply every-allocation interpreter collection.

Validation: codegen 37, compiler 10, baml_type 253 unit + 2 integration,
runtime 8, and VM 179 unit tests pass. Native backend regression target:
12 passed, one existing ignored test. This includes new fail-closed tests for
function values and an unsupported string builtin, and unchanged bytecode
fingerprints. Strict all-target clippy passes for codegen, compiler, runtime,
baml_type and VM.

```sh
export CARGO_TARGET_DIR="$PWD/target" BAML_CLI_ALLOW_DIRECT=1
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli
target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --runtime v2 --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/strings-v2-build" \
  --file baml_language/architecture/runtime-v2/phase1/strings.baml main \
  -o target/phase1/strings-v2
uv run --no-project python baml_language/architecture/runtime-v2/phase1/check_strings.py
cargo test --manifest-path baml_language/Cargo.toml \
  -p baml_builtins2_codegen -p baml_compiler2_rust -p baml_type -p bex_v2_runtime
cargo test --manifest-path baml_language/Cargo.toml -p bex_vm --lib
cargo test --manifest-path baml_language/Cargo.toml -p baml_tests --test rust_backend
cargo clippy --manifest-path baml_language/Cargo.toml \
  -p baml_builtins2_codegen -p baml_compiler2_rust -p baml_type \
  -p bex_v2_runtime -p bex_vm --all-targets --no-deps -- -D warnings
```

No new gate timing is claimed for this component fixture. The Richards/trees
gate and before/after profile above remain the latest complete measurements.
Still required for agent-loop: growing typed lists, ordered string/int maps,
tagged unions and optional scalars, proven index-walk iteration, remaining
builtin specializations, exact typed JSON decoding, typed catch/rethrow emission,
and the complete normal/stress differential and interleaved benchmark row.
No new INTERFACES decision is requested at this checkpoint.

## M3 checkpoint: managed growing-list runtime

Added `List<T>` runtime components for `i64`, flat strings and nested lists.
A stable 32-byte list object holds a pointer to a separately traced typed
buffer, length and capacity. Growth replaces the buffer pointer through the
barrier hook, preserving list identity and aliases; pointer elements also use
the hook. Descriptor constructors define the list/buffer pair; callers must supply
matching descriptor IDs through an unsafe integration API. The existing fixed-size Richards list layout is unchanged.
`lists::join` for `List<Str>` produces a flat string and polls every 16 elements through the
same cancellation/GC path as generated loops.

Runtime debug and release suites now pass **10 tests**, including nested-list
aliasing across growth and collection, Unicode joining, negative indexing, and
forced allocation failure with a 32 KiB heap limit. A failed growth preserves
the original length and every element. Strict runtime all-target clippy passes.
Commands: the runtime commands above, plus `cargo test --manifest-path
baml_language/Cargo.toml -p bex_v2_runtime --release`.

These growing lists are **not yet wired into the emitter or builtin adapters**.
M3 remains incomplete, with no agent-loop correctness or timing claim. No
additional INTERFACES decision is required by this runtime component.

### Additional Richards hotspot evidence

The final recording (not the earlier intermediate annotation) was inspected
with `perf annotate -i target/phase1/richards-checkpoint-after.perf.data --stdio
--symbol baml_rust_program::entry`. `phase1/richards-checkpoint-hotspots.txt`
retains the hottest annotated instructions. The largest sampled instructions
are the Tcb state load at `0x13208` (**8.72%**) and its state-test branch at
`0x1320c` (**4.28%**), followed by the comparison (**2.27%**). Remaining hot
instructions include pointer traffic, poll binding checks, observation loads
and stack-limit checks. The safe binding check alone samples at **1.47%**;
its presence was already disclosed in the optimization checkpoint. These are
local-period sample percentages, not proven additive causes of the residual
1.14× gap: sampling skid and inlining prevent that attribution. Allocation's
out-of-line slow path remains only 0.35% of the whole-program profile. Indirect
task storage remains unchanged; no measured claim isolates its cost against
the spike's enum layout.

The added admission regression test also passes strict clippy:
`cargo clippy --manifest-path baml_language/Cargo.toml -p baml_tests
--test rust_backend --no-deps -- -D warnings`.

## M3 sub-step: growing-list emitter integration

The emitter now plans and emits `string[]` and `string[][]`, including class
fields, stable list objects and separately traced buffers. Initializers, indexing,
`Array.push`, `Array.join`, and length work through typed runtime calls. The
builtin generator validates the actual stdlib declarations before emitting the
adapters. Required primitive type arguments are proven from `LoadType`
definitions and erased at pack time; dynamic type values and unrelated generic
calls remain unsupported. Indexed mutation of growing lists is still rejected
explicitly because this workload uses `push` instead.

`phase1/lists.baml` grows a string list through an alias, retains it in a class
and nested list, indexes from the end and joins Unicode strings. Its v2 pack
matches bytecode stdout (**6836**), empty stderr and exit status 0, normally and
with GC stress. Evidence: `phase1/lists-correctness.json`. No interpreter fallback
is linked into that pack. The fixture exercises indexed access, not the pending
index-walk iteration lowering.

Codegen's 37 tests and compiler's 10 tests pass. Existing native backend target:
13 passed, one existing ignored test, including a new growing-list admission
and unsupported-mutation regression. Strict codegen/compiler all-target clippy
passes. There is no new workload timing claim; the latest complete Richards and
trees gate remains above, and agent-loop is still pending.

```sh
export CARGO_TARGET_DIR="$PWD/target" BAML_CLI_ALLOW_DIRECT=1
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli
target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --runtime v2 --rust-workspace "$PWD/baml_language" \
  --rust-build-dir "$PWD/target/phase1/lists-v2-build" \
  --file baml_language/architecture/runtime-v2/phase1/lists.baml main \
  -o target/phase1/lists-v2
uv run --no-project python \
  baml_language/architecture/runtime-v2/phase1/check_fixture.py lists 6836
cargo test --manifest-path baml_language/Cargo.toml \
  -p baml_builtins2_codegen -p baml_compiler2_rust
cargo test --manifest-path baml_language/Cargo.toml -p baml_tests --test rust_backend
cargo clippy --manifest-path baml_language/Cargo.toml \
  -p baml_builtins2_codegen -p baml_compiler2_rust --all-targets --no-deps -- -D warnings
```

## M3 design question: headerless map-entry descriptors

List emission is committed as `78ff4e9d4b`. Map integration exposed a heap API
gap, so no map backend has been admitted and no padded layout has been silently
substituted.

A map buffer of `{ key: string-pointer, value: i64 }` entries needs a 16-byte
stride and a pointer at entry offset 0. `ElementLayout::Descriptor` is documented
for inline map entries, and tracing already applies the referenced descriptor
at each entry's base. However, `Heap::new` validates **every** descriptor as a
heap object: `validate_ranges` asserts `offset >= 8`. This rejects the entry's
pointer range `(0, 1)` before the program runs. The entry is not an independently
allocated heap object and has no object header.

Reproduction is committed as the explicitly expected-rejection test
`headerless_map_entry_descriptor_is_currently_rejected` in
`crates/bex_v2_heap/tests/heap.rs`:

```sh
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_heap --test heap \
  headerless_map_entry_descriptor_is_currently_rejected
```

It reproduces `assertion failed: offset >= 8 && offset % 8 == 0` in
`bex_v2_heap/src/lib.rs:375`. Passing this regression means the gap is reproduced,
**not** that map tracing works. The exploratory map runtime is not exposed or
compiled; its local draft and failed integration log remain in
`target/phase1/maps-runtime-draft.rs` and `target/phase1/m3-map-tests.log`.

**Decision requested:** distinguish headerless inline entry layouts from heap
object descriptors, allowing pointer ranges to start at 0 only for the former?
Recommended: an explicit inline-record element layout (or equivalent separate
inline descriptor type), retaining the existing header checks for all allocated
objects. The alternative is a required header-sized prefix in every inline
entry, adding eight bytes per string/int pair. This needs an explicit heap
integration decision; simply relaxing validation for every descriptor would
weaken the verified heap-object contract.

M3 is paused at this design question. Remaining work: resolve the entry layout,
wire map operations, tagged unions/optionals, index-walk iteration, typed JSON
and catches/rethrows, then run the full bytecode differential and the agent-loop
benchmark/profile. Agent-loop has no v2 result or measured ratio yet. The current
three-workload gate is therefore **NO-GO / incomplete**, despite M1 and M2 passing.

At this checkpoint the full heap tests pass (5 unit, 1 fiber, 16 integration,
2 doctests), including the explicit gap reproduction. The codegen/compiler
checks and native regression results are as recorded in the list sub-step.
Strict all-target clippy passes for heap, codegen and compiler, and strict clippy
for the touched `baml_tests --test rust_backend` target also passes.

## M3 sub-step: approved inline elements and map runtime

The headerless-entry question is resolved with the approved `Kind::InlineElement`.
It is valid only when referenced by a list/map buffer's element descriptor,
has element-relative pointer/tagged offsets (including zero), and cannot contain
another variable element layout or a finalizer. Allocatable kinds retain header
checks. Ordinary allocation rejects inline descriptors on the cold path before
caching a layout; emergency allocation also rejects them. Thus the existing
allocation fast path gains no extra check, and inline entries never get an
object-start bit. INTERFACES §1.3 now records the contract.

The old expected-rejection test was replaced by positive list tracing under GC
stress, checks that interior element pointers resolve to the owning list rather
than a new object, ordinary/emergency allocation rejection, and preservation of
header validation for allocatable descriptors. The managed string/int map
runtime now uses 16-byte inline entries with the string pointer at offset zero.
Its stress test grows through several capacities, replaces an equal-content key
without moving its insertion position, and verifies that `values()` snapshots
survive later changes and collections. The phase-one map uses a linear key
search over its insertion-ordered buffer; this is a performance choice, not a
claim of constant-time lookup.

Debug and release checks pass: heap 5 unit + 1 fiber + 18 integration + 2 doctests;
runtime 11 tests. Strict all-target heap/runtime clippy passes. Commands:

```sh
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_heap -p bex_v2_runtime
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_heap -p bex_v2_runtime --release
cargo clippy --manifest-path baml_language/Cargo.toml -p bex_v2_heap -p bex_v2_runtime \
  --all-targets --no-deps -- -D warnings
```

Map emission and the rest of M3 continue next; this checkpoint adds no workload
benchmark result and does not mark agent-loop complete.
