# Runtime v2 phase 1 results

## What phase 1 built

A single-owner non-moving heap, cooperative stackful task loop, integrated native
runtime and closed-world typed Rust emitter. The admitted subset covers the
three scorecard programs: binary-trees-14, richards-50 and agent-loop-300.
It includes classes and unions, finite interface dispatch, strings, growing
lists, maps, checked integers, streaming typed JSON decoding, catches/rethrows,
fallible allocation and source-side error frames. Unsupported code fails at pack
time; there is no interpreter fallback.

Telemetry uses the existing processor and published records: medium exact
function aggregates, high spans, policy captures, promotion and error evidence.
Medium remains the default, with full-precision ordered timing. Diagnostic
producer variants and code-generation hooks are not shipped.

## Correctness evidence

Phase 1 recorded matching stdout, stderr and status against bytecode, both
normally and with `BAML_GC_STRESS=1`:

| Program | Integer result |
|---|---:|
| binary-trees-14 | 3222190 |
| richards-50 | 1161046400 |
| agent-loop-300 | 592308425 |

Tests cover allocation and interior roots, eight parked fibers surviving ten
collections (80 checks), scheduler RootSource behavior, cancellation/shutdown,
managed containers, JSON errors, catch/rethrow, allocation failure and captures.
The final phase 1 telemetry run passed 24/24 workload/level/stress cases across
off/low/medium/high. Only successful C5 collector-maintenance records were
exempt; all six off cases matched without that filter. This historical parity
run is distinct from the checked-in differential gate, which currently requires
v2 off and explicitly skips v2 telemetry levels above off.

## Performance gates

Historical measurements on Spark Linux aarch64, pinned to CPU 12. Values are
median milliseconds per job; host CPUs were not isolated. These are recorded
phase 1 results, not new benchmarks from the slimming change.

Telemetry off, after streaming JSON integration:

| Workload | Node | Native | Generated v2 | Handwritten | v2 / handwritten |
|---|---:|---:|---:|---:|---:|
| binary-trees-14 | 38.358 | 418.961 | 36.389 | 37.965 | 0.958× |
| richards-50 | 3.938 | 83.121 | 3.035 | 2.660 | 1.141× |
| agent-loop-300 | 6.011 | 56.135 | 3.450 | 1.241 | 2.780× |

Streaming JSON brought agent-loop below Node; its handwritten gap remains.
The final policy-capture gate refresh used five rotated/interleaved rounds:

| Workload | v2 off | v2 medium | v2 high | Native medium | Node | Medium/off |
|---|---:|---:|---:|---:|---:|---:|
| binary-trees-14 | 42.170 | 320.709 | 1429.673 | 1031.116 | 39.971 | 7.605× |
| richards-50 | 3.737 | 85.852 | 413.782 | 291.230 | 5.642 | 22.976× |
| agent-loop-300 | 3.602 | 5.173 | 9.436 | 57.729 | 7.816 | 1.436× |

The Richards medium ≤1.5× off gate **fails**. Two full-precision clock reads per
call alone measured about 69–73 ms/job versus roughly 3.7 ms off. Exact diagnostic
variants did not approach the gate; the best combined experiment was 21.24× off.
Count-only and sampling changed duration semantics, lost accuracy and still
missed Richards' gate. No default, precision or policy relaxation was shipped.

## Reproduction

From `baml_language`, keep `CARGO_TARGET_DIR` inside the current worktree:

```sh
cargo test -p bex_v2_heap -p bex_v2_loop -p bex_v2_runtime -p baml_compiler2_rust
cargo clippy -p bex_v2_heap -p bex_v2_loop -p bex_v2_runtime -p baml_compiler2_rust --all-targets -- -D warnings
BAML_RUST_DIFF_PROBES='programs::richards 50,memory::binary trees 14,programs::agent loop 300' cargo test -p baml_tests --test rust_differential -- --ignored --nocapture
```

The differential harness discovers original workloads through
`tools/speedtest/kpis.toml`, packs bytecode/compiled/v2 variants and runs normal
and GC-stressed v2. See `crates/baml_tests/rust_diff/README.md` for matrix limits.
For timing, pack each original workload with `baml pack --rust --runtime v2`,
set `BAML_TELEMETRY`, and use the generated `--jobs N --warmup N` driver. Compare
against native and Node with CPU pinning and interleaved rounds; profile outside
timed rounds. Reproducing historical diagnostic variants requires the external
phase 1 artifact archive; they are not runtime features.

## Open items

- Reconcile the Richards medium gate with exact-duration clock cost without
  silently changing the published contract or default.
- Integrate the completed telemetry subset into the strict differential matrix;
  retain explicit collector-maintenance scope and recording-integrity checks.
- Broaden closed-world admission beyond the three programs, including general
  initializers, generic/virtual shapes and asynchronous generated task bindings.
- Extend host ownership, opaque payload accounting and retained OOM guarantees.
- Reduce agent-loop's remaining gap to handwritten code.

`INTERFACES.md` and `CONTRACTS.md` define the retained design and ownership seams.
