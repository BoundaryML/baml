# Backend differential gate

From `baml_language`:

```sh
cargo test -p baml_tests --test rust_differential -- --ignored
BAML_RUST_DIFF_PROBES=Scalars,ManualSpan cargo test -p baml_tests --test rust_differential -- --ignored --nocapture
BAML_RUST_DIFF_PROBES='process::startup,programs::richards 50' cargo test -p baml_tests --test rust_differential -- --ignored --nocapture
```

`BAML_RUST_DIFF_TIMEOUT_SECS` bounds each executable (default 120 seconds).
`BAML_RUST_DIFF_BACKENDS=rt,rt-gc-stress` selects just those backends plus the
mandatory bytecode reference. RT-only selections run telemetry off, its current
supported comparison level. Combine with `BAML_RUST_DIFF_PROBES` for scoped checks.
`BAML_RUST_DIFF_DUMP=1` prints process output. Unknown/empty selections fail.
Normalized recordings are saved under Cargo's test scratch directory,
`rust-differential/telemetry/<fixture>-<backend>-telemetry-<level>.jsonl`.
The executable's private temporary `BAML_HOME` is removed after decoding;
recordings never enter the developer's normal BAML home. Delivery is local.

## Adding a backend

Add one `Backend` entry in `tests/rust_differential.rs`: a unique name, artifact
cache key, builder function and run environment. Its builder returns a fixture
name-to-build-result map (executable or explicit rejection), using the same
probe and KPI sources. A distinct artifact
key invokes that builder once; entries sharing a key reuse artifacts. Bytecode
is selected by name as the reference, independently of environment variables.
The rt backend shares only the GC maintenance exception. Existing compiled-body,
GC-stress and VM-frame environment switches remain in the existing entries.

The probe matrix includes default, off, low, explicit medium and high. KPI
fixtures run off/medium/high on each backend; the old backend's GC-stress and
VM-frame variants are covered by the smaller probes. Each KPI invokes its
original `main()` once (one complete job, no warmup or timing driver), discovered
from `tools/speedtest/kpis.toml` and the workload metadata. Packs reuse one Cargo
build directory to amortize runtime compilation.

## Telemetry comparison

`telemetry.rs` validates every sealed BTEL file with `btel_reader`. A test-only
Serde view is generated from the authoritative protobuf schema, so additional
wire fields remain visible. It joins definitions across files, resolves function
and call-path IDs, resolves task/span parents, decodes captured CAS values, and
combines aggregate deltas across flush boundaries. Sorted record multisets retain
kinds, counts, outcomes, PCs, contexts, capture contents, errors and task records.
Canonical subtree signatures retain span-tree shape, including the distribution
of children among repeated sibling spans. Clock readings, durations, file batch
boundaries and cross-task arrival order are excluded. Missing/corrupt files,
unresolved references, missing final seals and renderer overflow fail the test.
Producer capture truncation remains explicit in the comparison.

`known_telemetry_divergences.rs` is the allowlist for existing backend gaps.
Exceptions never apply to a newly registered backend, and never waive process
output. Every observed exception prints its fixture, backend, level and a short
diff, also saved in `rust-differential/known-telemetry-divergences.txt` even
when Cargo captures successful test output. Full normalized evidence remains
on disk.

Currently, successful root `baml.gc` maintenance tasks can differ in count and
heap-occupancy context. `BAML_GC_STRESS` deliberately forces extra collections;
compiled allocation behavior also changes collector scheduling. User tasks,
failed GC completions and all other records remain compared.

Other current compiled-backend exceptions:

- `Cancellation`, `RecursiveCancellation`, `CancelDuringUnwind`, `CancelDeep`,
  `Shielded`: readiness polls at `probes.baml:136`, `probes.baml:167`,
  `records.baml:401`, `probes.baml:581`, and `probes.baml:613` execute
  scheduler-dependent numbers of successful sleeps. Only that call site and its
  descendants are removed, then ancestor tree signatures are rebuilt; captured
  values and unsuccessful completions remain checked.
- `Logs`, `LogsDeep`: compiled logging reports a stale/function-entry PC for
  `Logs`, `LogChain`, its lambda and `LogDefer`. Only these log locations are
  exempt; values, levels, counts and parents remain checked.
  `bex_vm/src/vm/compiled_rt/call.rs::log_event` passes through
  `record_log_at`, which reads `vm.cur_pc` instead of the supplied statement PC.
- `FrameLimit`, `NoLeaks`, `CallbackDepth`, `TaskOverflow`: native fiber frames
  and VM frames consume different stack space. Absolute recursion populations,
  span depths and overflow/unwind counts differ. These are explicitly
  **fixture-wide telemetry exceptions**, at non-off levels, rather than
  normalization rules. The fixtures' relative-depth process assertions and
  recording integrity/non-vacuity still run.
- `InitializerTests`, `InitializerTestErrors`: cancellation of the losing
  deadline task can precede the host wait. Only the sleep's sysop count at
  `<builtin>/testing/registry.baml:609` is exempt; function counts, task tree,
  cancellation outcomes and other sysops remain compared.

These other exceptions are restricted to the existing compiled backend variants.
The runtime uses only the GC maintenance exception.

## Cost and isolation

The exact regular-probe matrix is off/medium/high for bytecode, compiled,
GC-stress and VM-frames; low for bytecode/compiled; combined VM-frames/GC-stress
at medium. `Scalars`, `Hooked` and `ManualSpan` additionally exercise the unset
(default) level. Every KPI uses off/medium/high for bytecode/compiled. There are
32 unique scorecard workloads, with no source reductions or warmup repetitions.

The BTEL reader runs as an optimized helper built from the same `telemetry.rs`
source and protobuf schema used by the unit tests. It indexes identities in one
pass, then folds equivalent wire events before rendering in a second pass.
This keeps high-volume recursive traces practical under a debug Cargo test.
The reader has three times the executable timeout. Large high-level recordings
can still require gigabytes of scratch space and several minutes to normalize.
Captures include a canonical content digest plus readable values, with all CAS
child blobs checked. PCs resolve through source maps; unmapped PCs stay explicit.

Set `BAML_RUST_DIFF_BUILD_DIR=/absolute/scratch/path` to isolate artifacts and
recordings when running subsets concurrently. Within one build directory runs
must be serial. The probe selector also limits exported functions, reducing
subset compile time. Initializer variants also export only their required
roots, so unrelated probe bodies are not repeatedly compiled. Use the examples
above for quick iteration before the full
gate; do not treat a subset as full backend acceptance.

## BAML runtime coverage

`rt` and `rt-gc-stress` pack each selected fixture separately with
`--rust --rt`, using the same complete source project as the oracle.
Single-entry executables receive no subcommand argument. Both modes run every
admitted probe and all admitted KPI workloads, including GC stress on KPIs.
Process output must match exactly. Telemetry allows only the successful root
GC maintenance exception described above.

Only a failed pack containing `unsupported in baml_rt:` is unsupported;
its first diagnostic construct is recorded. Every other pack/run error,
crash, or mismatch fails the gate. Stale output executables are removed before
packing. RT's closed-world compiler rejects unsupported reachable code rather
than emitting a partial bytecode fallback. The shared probe project currently
contains top-level initializers: their rejection is a project-level blocker,
not evidence that each probe's individual body is unsupported. Synthetic
initializer/registry fixtures also require an rt host adapter once admitted;
the harness fails explicitly rather than running an unmodified variant.

`rust-differential/rt-coverage.txt` records one row per selected probe/workload,
with pass (both modes), unsupported (first construct), or fail, plus totals and
construct counts sorted by the number of fixtures blocked. The report is also
printed at the end, before the final failure assertion. Legacy-only failures
still fail the gate but do not change rt coverage. Subset runs report only
the selected fixtures; run the full gate to regenerate the complete coverage report.

RT telemetry off is required now. Low/medium/high/default are explicitly
reported as **not yet supported**. `TELEMETRY_SUPPORTED` in `rt.rs` controls
whether all existing applicable matrix cells become
required, including recording integrity, non-empty telemetry, and exact
normalized equality. All other telemetry records remain strict.

The initializer registration fixture runs collected test bodies without a deadline
task (`testing._run_test(..., 0)`). The harness process timeout still bounds it.
This avoids scheduler-dependent records from cancelling a losing timer before
its body starts; initializer/capture assertions and exact telemetry checks remain.

`Async` passes settled inputs to `future.any`, keeping its result and task
outcomes deterministic. `AnyWithFailure` still exercises pending inputs and
error handling. No task cancellation outcome is normalized away.

## Production corpus gate

From `baml_language`, with `CARGO_BUILD_JOBS=6` and `CARGO_TARGET_DIR` inside
this worktree:

```sh
cargo test -p baml_tests --test rt_corpus -- --ignored --nocapture
# Equivalent direct entry; avoids compiling the other Rust test dependencies:
uv run --no-project crates/baml_tests/rust_diff/rt_corpus.py
```

The reference is the existing `baml-cli test --from crates/baml_tests/baml_src`
registry, including the manifest's offline profile and testset runners.
Profile exclusions are printed by canonical test/subtree name. Live subtrees
remain unexpanded because constructing them can require provider credentials;
they are listed separately from the 6,054 selected leaves and category counts.
`pack --tests --rust --rt` discovers the same canonical leaves at pack time,
admits each body's reachable closure separately, and packs native entries
without an interpreter. Captured test environments and custom runners are
explicitly unsupported until the native test host can reproduce them. An
unsupported sibling or unrelated initializer does not reject admitted leaves.

Each admitted leaf runs in a separate process on VM, RT and `BAML_GC_STRESS=1`.
Outcomes, failure messages, stdout and stderr are compared; durations are not.
The gate prints category totals, grouped admission/failure signatures and every
correctness bug. Raw process evidence and the complete per-test JSON report
remain under `$CARGO_TARGET_DIR/rt-corpus/`. Infrastructure failures never count
as unsupported. `BAML_RT_CORPUS_TIMEOUT_SECS` bounds each leaf (default 120).

The admission burn-down prints the top 30 normalized signatures, sorted by test
count with example names. It strips pointer addresses, type/object IDs and caller
coordinates for grouping; exact diagnostics remain in the per-test evidence.
The `only` column counts leaves whose observed blockers belong only to that
group. Admission currently records the first error, so this is an upper-bound
estimate: fixing it may expose another blocker, and does not guarantee a pass.
Use `--summarize-report PATH/report.json` to reprint this table from saved evidence
without compiling or running the corpus again.

`rt-corpus-baseline.json` records category counts **and each canonical test's
category**, so unchanged aggregate counts cannot hide a match regressing. A
missing test, new mismatch/crash, lost match or newly excluded test fails the
gate. After reviewing a complete run, use the direct entry's
`--update-baseline` to record the accepted state. `--select TEXT` is diagnostic
only and cannot update the full baseline. `--workspace PATH --report-only`
audits a source snapshot without altering or accepting its baseline.
The built CLI is retained in the evidence directory; `--cli PATH` can reuse it
to resume an interrupted audit without rebuilding that branch.

The runner uses the workspace's pinned Rust toolchain, six Cargo jobs and no dev
debug information. A source hash lets audits reuse an existing VM observation
only when its frontend, interpreter, stdlib, corpus, dependencies and toolchain
are identical. The RT emitter/runtime and RT-only builtin generators are excluded
from that hash because the bytecode oracle never executes them. Native artifacts
are built once per source snapshot and shared by the two RT modes.

The initial CI-profile baseline has 6,054 selected leaves: **2 match, 6,052
unsupported, 0 mismatch, 0 crash/timeout** (both native modes). The VM reference
runs at CI's `medium` telemetry setting: 6,052 pass and two runner-tolerated
failures. The corpus gate compares outcomes/messages/output at that same setting;
it does not replace the differential suite's recording comparison. Admission
is currently the limiting factor, including unknown/type arguments, closures,
container representations and builtin exception classes.

Each admission also has the per-leaf process deadline. If the compiler stalls or aborts,
the gate records that leaf as a crash/timeout and restarts the pack, reusing
completed admission results. The private cache is keyed by the CLI executable
hash and packed-program digest. A stuck specialization cannot hide the other
leaves. Ordinary pack/compiler/build errors still fail the gate.
Native compiler diagnostics inside generated test modules are attributed to
those leaves as crashes, then the remaining modules are rebuilt. Shared host or
library build errors remain fatal. Failed modules are retried on the next gate
invocation so a runtime-only fix cannot leave a stale compilation failure.
