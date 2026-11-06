# Backend differential gate

From `baml_language`:

```sh
cargo test -p baml_tests --test rust_differential -- --ignored
BAML_RUST_DIFF_PROBES=Scalars,ManualSpan cargo test -p baml_tests --test rust_differential -- --ignored --nocapture
BAML_RUST_DIFF_PROBES='process::startup,programs::richards 50' cargo test -p baml_tests --test rust_differential -- --ignored --nocapture
```

`BAML_RUST_DIFF_TIMEOUT_SECS` bounds each executable (default 120 seconds).
`BAML_RUST_DIFF_DUMP=1` prints process output. Unknown/empty selections fail.
Normalized recordings are saved under Cargo's test scratch directory,
`rust-differential/telemetry/<fixture>-<backend>-telemetry-<level>.jsonl`.
The executable's private temporary `BAML_HOME` is removed after decoding;
recordings never enter the developer's normal BAML home. Delivery is local.

## Adding a backend

Add one `Backend` entry in `tests/rust_differential.rs`: a unique name, artifact
cache key, builder function and run environment. Its builder returns a fixture
name-to-executable map, using the same probe and KPI sources. A distinct artifact
key invokes that builder once; entries sharing a key reuse artifacts. Bytecode
is selected by name as the reference, independently of environment variables.
No comparator or telemetry code needs a v2 branch. Existing compiled-body,
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

- `Cancellation`, `RecursiveCancellation`, `CancelDuringUnwind`: readiness
  polls at `probes.baml:136`, `probes.baml:167`, and `records.baml:401` execute
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

All exceptions are restricted to the existing compiled backend variants.
Runtime v2 starts with no exceptions.

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
