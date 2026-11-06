# Phase 1 gate: NOT READY — implementation incomplete

This checkpoint does **not** complete the requested runtime-v2 codegen job.
There are no generated v2 packs of the three workloads. Consequently there is
no bytecode/v2 differential result, generated/spike ratio, or perf attribution.
**No-go for advancing on the strength of phase 1; no conclusion about the
architecture's achievable performance.** Missing implementation is not evidence
that generated v2 is slow.

Source baseline: `1f572075e24ef5f4cdbdd894ece8711140847015`, branch `v2-codegen`.
Work and build artifacts are confined to this worktree. Host: Spark, Linux
aarch64. No push, PR, merge, or deployment was performed.

## Delivered in this checkpoint

- A pre-admission MIR inventory API and `baml pack --rust --rust-inventory PATH`.
  This command writes an inventory **instead of an executable**. It includes
  builtin leaves and source bodies even when native admission would reject
  them. It reuses the existing declaration identities and reachability walk.
- Committed user-MIR extracts for the three workloads in `phase1/`. These are
  real compiler output, not translations of the benchmark source to Rust.
- `bex_v2_runtime`, a restricted, unsafe Rust integration seam between the
  existing heap and task loop. It owns a reusable heap, runs a Rust entry as a
  root task, installs scoped TLS, allocates using descriptors and the real loop
  roots, and handles budget/stress safepoints. Pointer stores in its test use
  `write_ptr`. It does not link the VM.
- `RootSource for TaskCx`, delegating to the loop's existing register-spill and
  stack-root implementation. An adapter connects the independent root traits
  without changing either trait's signature.

There is **no** v2 emitter, v2 pack selection flag, typed builtin glue, generated
layout planner, language-error implementation, or observation-word branch yet.
The runtime crate's integer-result and loop-error types are integration-test
boundaries, not substitutes for the language's value/error ABI. Descriptors
with finalizers are rejected by this restricted seam. Child spawning and
heap-valued host results are not exposed. Its 8 MiB stack reservation is
provisional and has not been tested against generated frames.

## MIR feature inventory

The following is confirmed in the user MIR. The **exact monomorphized transitive
stdlib closure remains unfinished**. The existing native reachability walker
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
7. The speedtest BAML driver adds timing and reporting calls. The future v2 host
   must either implement those calls or supply an explicitly documented host
   driver with the same `--jobs`/`--warmup` JSON protocol. Neither is implemented.

## Correctness and gate table

Expected checksums below come from `bench.toml`, **not new bytecode runs**.
The Rust integration tree test is not a generated BAML correctness result.

| Workload | Expected value | Bytecode/v2 normal | Bytecode/v2 GC stress |
|---|---:|---|---|
| Richards-50 | 1161046400 | Not run: no v2 executable | Not run |
| Agent-loop-300 | 592308425 | Not run: no v2 executable | Not run |
| Binary-trees-14 | 3222190 | Not run: no v2 executable | Not run |

| ms/job, telemetry off | Node | Current native | Generated v2 | Hand-written spike | Generated / hand-written |
|---|---:|---:|---:|---:|---:|
| Richards-50 | Not measured | Not measured | Not built | Not rerun | Unavailable |
| Agent-loop-300 | Not measured | Not measured | Not built | Not rerun | Unavailable |
| Binary-trees-14 | Not measured | Not measured | Not built | Not rerun | Unavailable |

No five-round interleaved measurements, spike rebuild, or perf recording was
performed. Historical spike numbers are not substituted into this table.
There is no worst measured ratio and no measured list of gap causes.

The supplied sibling `../v2-harness` directory was absent when checked. The local
`v2-harness` ref was at `b5d0793d6d`; its tracked tree contained the existing
`rust_differential.rs`, but no v2 backend entry point. No claim of a direct
bytecode comparison is made either, because there is no v2 executable to compare.

## Validation

- Runtime seam: debug and optimized release recursive-tree stress tests pass.
  The running fiber retains child pointers and a completed tree across further
  allocation. Three jobs require at least 1,146 collections and each returns 382.
  An error-return test verifies TLS restoration and subsequent instance reuse.
- Existing loop tests: 2 unit tests, 28 integration tests, 2 ownership doctests
  passed in the debug run.
- Native backend regression suite: 9 passed, 1 explicitly ignored full-corpus
  admission check. Includes bytecode identity, unsupported-function fallback,
  exact source positions and the new inventory test.
- CLI library suite: 458 passed; one failed because `gofmt` was absent. That
  exact test passed when rerun under `nix shell nixpkgs#go` (no source fix).
- Compiler unit tests: 9 passed. Post-edit runtime tests: 2 passed in debug and
  2 passed in optimized release.
- Strict clippy for compiler, CLI, loop and runtime, all targets with no
  dependency linting, passed. Two existing signal-test duration expressions
  were changed from `from_secs(60)` to equivalent `from_mins(1)` to satisfy
  the pinned toolchain's lint. Signal e2e execution was not rerun for that
  expression-only change.
- The final inventory test rerun and strict clippy for the `rust_backend`
  integration-test target passed. Formatting and `git diff --check` passed.

No full bytecode corpus, ignored Rust differential executable suite, or generated
v2 differential suite has been run by this checkpoint.

## Reproduce

All commands run from `/home/rathi/Documents/Git/boundary/baml/.worktrees/v2-codegen`.
Build artifacts stay in `target/` in this worktree.

```sh
export CARGO_TARGET_DIR="$PWD/target"
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli
mkdir -p target/phase1

# Repeat with programs/richards-50 and programs/agent-loop-300.
target/debug/baml-cli --agent-skill-check off --no-progress --color never \
  pack --rust --rust-inventory target/phase1/trees-full.mir \
  --file baml_language/tools/speedtest/workloads/memory/binary-trees-14/main.baml main
uv run --no-project python baml_language/architecture/runtime-v2/phase1/inventory.py \
  target/phase1/trees-full.mir baml_language/architecture/runtime-v2/phase1/trees.mir

cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime -p bex_v2_loop
cargo test --manifest-path baml_language/Cargo.toml -p bex_v2_runtime --release
cargo test --manifest-path baml_language/Cargo.toml -p baml_compiler2_rust
cargo test --manifest-path baml_language/Cargo.toml -p baml_tests --test rust_backend
nix shell nixpkgs#go --command cargo test --manifest-path baml_language/Cargo.toml -p baml_cli --lib
cargo clippy --manifest-path baml_language/Cargo.toml \
  -p baml_compiler2_rust -p baml_cli -p bex_v2_loop -p bex_v2_runtime \
  --all-targets --no-deps -- -D warnings
git diff --check
```

`--agent-skill-check off` bypasses the CLI's agent-installation preflight for
this local compiler investigation. It does not select a runtime or change BAML
semantics. `--rust-inventory` is an audit-only mode, not a v2 pack mode.

## Outstanding work and interface review

The requested finish line remains unmet: concrete type/instance planning,
closed-world dispatch, typed emitter using the existing stackifier, descriptor
emission, typed containers/strings/builtins, exact errors with line side tables,
observation-word branches, v2 pack/driver integration, normal/stress bytecode
comparisons, and the full pinned measurement/profile sequence.

No relaxation of INTERFACES is proposed or counted as accepted. The root-trait
adapter is an integration extension, not a representation or GC change. Review
question for Hari: **is `RootSource for TaskCx` the desired public allocation
seam, or should this same synchronous access remain behind a loop-owned context?**
The implementation is isolated and does not require changing the root signatures.
Missing language errors, telemetry and finalizer support are incomplete work,
not silent deviations to approve.

Checkpoint implementation commits: `495a15c376` (heap/loop integration) and
`deb6f46dbe` (MIR census). This report and its MIR extracts are committed separately.
