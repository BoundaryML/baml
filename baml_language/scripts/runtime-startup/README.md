# Execution-ready artifact startup experiment

**Result:** prebuilding executable bytecode removes about 5–6.5 ms (22–27%) from packed-program launch in these fixtures. This is a measured prototype, not a production runtime change.

```baml
function Main() -> void {}
```

The same empty program takes 21.90 ms normally and 16.21 ms with the prototype, with telemetry off. With default local recording, it takes 24.09 ms normally and 18.06 ms with the prototype.

## End-to-end launch measurements

Milliseconds, median / p95. Every cell has 100 fresh process launches after three warmups. Telemetry off applies to the packed program and its fixture child. Local uses the normal recording configuration and no cloud destination.

| Program | Normal, off | Prototype, off | Saved | Normal, local | Prototype, local | Saved |
| --- | --- | --- | --- | --- | --- | --- |
| Empty Main | 21.90 / 25.19 | 16.21 / 19.91 | 5.70 ms (26.0%) | 24.09 / 25.94 | 18.06 / 20.88 | 6.04 ms (25.1%) |
| Print ready | 21.90 / 23.66 | 16.38 / 18.76 | 5.53 ms (25.2%) | 23.86 / 26.04 | 18.38 / 22.06 | 5.48 ms (23.0%) |
| JSON, defaults, callbacks, floats, catch | 22.07 / 24.39 | 16.45 / 18.30 | 5.61 ms (25.4%) | 24.23 / 26.02 | 18.33 / 20.27 | 5.90 ms (24.4%) |
| 1,000 extra functions | 25.31 / 29.87 | 19.06 / 21.28 | 6.25 ms (24.7%) | 27.75 / 30.02 | 21.22 / 22.96 | 6.53 ms (23.5%) |
| Typed arguments and default | 22.05 / 23.62 | 16.31 / 18.92 | 5.74 ms (26.0%) | 23.90 / 27.48 | 18.38 / 19.86 | 5.52 ms (23.1%) |
| Generated --help | 21.17 / 23.14 | 15.38 / 17.84 | 5.78 ms (27.3%) | 23.44 / 24.99 | 17.44 / 21.63 | 6.00 ms (25.6%) |
| Wrapper --version, native CLI fixture | 25.37 / 27.25 | 19.24 / 24.30 | 6.13 ms (24.2%) | 27.34 / 29.42 | 21.21 / 23.84 | 6.13 ms (22.4%) |
| Wrapper forwards to native child | 22.51 / 25.92 | 17.33 / 18.97 | 5.18 ms (23.0%) | 24.70 / 26.44 | 19.34 / 21.02 | 5.36 ms (21.7%) |

The wrapper uses the same C CLI fixture in both variants. These wrapper rows measure forwarding and version handling, not running a second BAML VM or a real LSP. Both wrappers were packed through their respective compiler and host using the same Main source; the Cargo development entry point was not used for the timed comparison.

## First output and CPU

| Program, off | Normal first output | Prototype first output | Normal CPU | Prototype CPU |
| --- | --- | --- | --- | --- |
| Empty Main | No output | No output | 20.10 ms | 14.55 ms |
| Print ready | 18.51 ms | 13.87 ms | 20.10 ms | 14.76 ms |
| 1,000 extra functions | 21.42 ms | 16.32 ms | 23.44 ms | 17.22 ms |
| Wrapper --version | 19.32 ms | 14.25 ms | 23.47 ms | 17.46 ms |

Empty-program instructions fall from 166,233,113 to 121,034,860, about 27.2% fewer. Peak memory and file-size counters are preserved in the data but were not optimization targets.

## Where the saved time comes from

A separate phase probe measures the same empty artifacts. Medians from 40 fresh processes per variant/mode after three warmups. These thin-LTO probes are linked to the same release libraries but have a different entry point; their phases exclude OS startup and are not a partition of the production wall-time result.

| Phase, off | Normal | Prototype | Difference |
| --- | --- | --- | --- |
| Validate envelope and decode program | 5.44 ms | 3.83 ms | +1.61 ms |
| Build engine | 6.27 ms | 3.25 ms | +3.03 ms |
| Destroy engine | 2.51 ms | 1.65 ms | +0.86 ms |
| Execute empty Main | 0.08 ms | 0.08 ms | -0.00 ms |
| Create Tokio runtime | 0.19 ms | 0.19 ms | +0.00 ms |

The main reductions are about 3.0 ms in engine initialization, 1.6 ms in artifact decoding, and 0.9 ms in destruction. The prototype still spends about 7.1 ms decoding and initializing the empty program. It still reconstructs function signatures, declarations, package tables, lookup maps, and rendering schemas. The remaining OS/host launch cost needs more attribution; these probes cannot establish its exact size.

## What the prototype does

- Boxes float constants and finalizes compact bytecode and exact-call selection during packing.
- Serializes final executable streams and reuses them at load, bypassing both runtime bytecode-preparation passes.
- Keeps executable constants, types, signatures, docs, package tables, interfaces, and class initialization programs.
- Stores source-location tables as encoded bytes and decodes a function’s table when a location is requested.
- Removes original instruction/disassembly representations from the experimental artifact. Original operand labels are not preserved in this prototype.
- Uses an experimental format number so ordinary hosts reject these artifacts; the patch’s startup-experiment feature is disabled by default.

The empty baseline has 4,127 objects, 1,662 functions (1,068 bytecode functions), and 40,521 original instructions across 15 packages. Original operand metadata occupies 558,416 bytes, and source-location tables occupy 555,174 bytes. The prototype retains the locations in deferred form. A separate decode-only probe (40 repeated decodes in one process, excluding file reads) measures about 1.3 ms of envelope integrity hashing and 4.8 ms of Borsh object deserialization. These are diagnostic measurements rather than a partition of the fresh-process launch result.

## Correctness and production work

Both variants produce identical output for empty/printing programs, class JSON serialization, numeric constants, generic native callbacks, default/named arguments, catch handling, 1,000 extra function declarations, typed arguments/help, and wrapper child forwarding. Uncaught division-by-zero errors show identical source files and lines in Main and leaf. Top-level let initializers were not tested: the baseline CLI rejects that source syntax.

The experimental compiler/host/wrapper passed cargo check and a normal release build. This does not establish complete runtime equivalence or cross-platform behavior.

**Production work remains:** serialized compact code needs structural/operand/control-flow verification before the VM’s unchecked readers consume it. The prototype trusts compiler-produced fixture images. Original disassembly data needs a separate deferred section so debugger/disassembly behavior remains available. Restoring verification and deferred debug data may change the measured gain. Source-map decoding needs fallible validation rather than the prototype’s expect call. Reuse also needs an explicit prepared-image state rather than inferring it from every function having compact code. SDK loading, dynamic compilation, package initialization, GC, cancellation/defer, and other platform targets need coverage before shipping.

The implementation is saved as [prototype.patch](prototype.patch); the main runtime sources are unchanged. This keeps the measurement experiment out of supported artifact/API behavior.

## Methodology and reproduction

Measured on 2026-10-04 on an Apple M2 Max, 64 GiB RAM, 12 logical CPUs, macOS 15.6.1 (24G90), aarch64. Rust 1.98.0 (88d9e12ae, 2026-08-18). Both variants use the ordinary release profile: opt-level=3, fat LTO, codegen-units=1, stripped symbols, unwind panics. No LTO/profile tuning was introduced.

Baseline code is c43da5cc7c0e6fe5d95b51806e4203995d1000de. Prototype parent is cb83e865e3145974170ca83b973eae60edd79bcc, whose intervening changes are measurement/docs only, plus the saved patch. Each compiler packs the identical project sources with its matching host. The wrapper is packed from crates/baml/baml_src, target Main.

The harness uses the existing C posix_spawn/stdout-pipe/wait4/proc_pid_rusage helper. Wall time runs from before process creation through reaping; first output is the first observed stdout read. CPU is user plus system accounting; instruction/cycle counters are root near-exit counters, not a complete process-tree total.

Variants are interleaved in a fixed-seed shuffled order (20261004). Builds and tests are idle during measurements. 32 variants × 100 measured launches = 3,200 launches, plus 96 warmups. There are 18 additional correctness launches. Phase measurements add 160 launches and 12 warmups. Synthetic HOME/BAML_HOME and a fixture toolchain isolate configuration and credentials; auto_check=false; no production cloud traffic is used.

Build the normal binaries in one checkout/output directory. In an isolated checkout of the prototype parent, apply the patch from the repository root:

```sh
git apply baml_language/scripts/runtime-startup/prototype.patch
cd baml_language
cargo build --release -p baml_cli -p baml_pack_host --features startup-experiment
```

Place baml-cli and baml-pack-host from each build in separate baseline/candidate directories. From baml_language, run:

```sh
python3 -u scripts/runtime-startup/compare.py target/runtime-startup-experiment --baseline /absolute/path/to/baseline --candidate /absolute/path/to/candidate --runs 100
```

Use separate Cargo target directories across different checkouts/revisions. compare.py generates the wrapper and fixture executables, verifies output and error locations, then runs the timings. It is macOS-only because the C measurement helper uses libproc.

[results-20261004.json](results-20261004.json) contains artifact hashes, all summary counters, every timed launch as lossless column-oriented samples, and raw phase/decode probes. Full local artifacts, pack logs, and recordings remain under target/runtime-startup-experiment-20261004.

## Full packed-host launch profile

**The earlier missing time is mostly before Rust main.** In the execution-ready prototype, process launch, native loader/initializers, and Rust setup take about 6.5 ms mean. Decoding plus engine construction still take about 7.5 ms, and engine/host-state destruction takes about 1.7 ms. These are two substantial startup targets plus a smaller teardown target.

The diagnostic now uses the real `baml_pack_host` main, `run_single`, and `finalize_dispatch` code paths, with boundary timestamps. A constructor and the C parent share macOS CLOCK_MONOTONIC, covering before-main work and termination/reaping. This replaces the earlier partial thin-LTO probe for attribution.

### Complete timeline

Mean milliseconds from 100 fresh processes per column. Groups sum to the corresponding instrumented mean wall time; individual medians/p95 do not sum. The final trace/exit group includes diagnostic emission overhead and parent observation.

| Phase | Normal empty, off | Prototype empty, off | Prototype empty, local | Prototype wrapper version, off |
| --- | --- | --- | --- | --- |
| Process launch, loader/initializers, Rust setup | 6.376 | 6.514 | 6.427 | 6.218 |
| Find embedded section | 0.004 | 0.004 | 0.004 | 0.004 |
| Decode artifact | 5.833 | 4.052 | 4.107 | 4.182 |
| Build argv, native ops, compiler handle, recording config | 0.015 | 0.014 | 0.014 | 0.015 |
| Build engine | 6.932 | 3.401 | 5.007 | 3.426 |
| Lookup function, parse argv and JSON | 0.018 | 0.019 | 0.020 | 0.048 |
| Create/destroy Tokio runtime | 0.408 | 0.397 | 0.385 | 0.398 |
| Execute target | 0.085 | 0.088 | 0.104 | 2.963 |
| Record exit, shutdown, drain tasks | 0.208 | 0.172 | 0.595 | 0.188 |
| Destroy engine and host state | 2.685 | 1.682 | 1.674 | 1.762 |
| Trace emission, remaining cleanup, EOF observation | 0.507 | 0.440 | 0.454 | 0.429 |
| Parent counter query and reaping | 0.020 | 0.011 | 0.013 | 0.011 |
| **Total mean wall time** | 23.092 | 16.793 | 18.804 | 19.643 |

The parent counter query takes about 0.002 ms mean and reaping about 0.009 ms. The 6.5 ms before main is not measurement-counter overhead. The embedded-section lookup takes about 0.004 ms on this ARM Mac; there is no sizeable file-read stage hidden there. Empty-target execution is about 0.09 ms, and Tokio creation plus destruction about 0.40 ms.

Default local recording adds about 1.6 ms to engine construction and 0.42 ms to shutdown in these mean phase comparisons. This run does not measure cloud delivery.

### Controls and end-to-end timings

Median / p95 milliseconds, 100 fresh launches per cell. Original is the previously measured Cargo-built host. Control rebuilds the unchanged host sources in the same module layout used by the instrumented host. Profile adds the constructor, boundary timestamps, and one stderr trace after host cleanup.

| Program / telemetry | Original | Rebuilt control | Instrumented |
| --- | --- | --- | --- |
| Normal empty, off | 21.34 / 24.51 | 21.93 / 27.94 | 22.04 / 24.90 |
| Normal empty, local | 23.41 / 26.04 | 23.89 / 28.87 | 23.95 / 26.53 |
| Normal wrapper --version, off | 25.25 / 27.96 | 25.50 / 29.42 | 25.62 / 32.38 |
| Normal wrapper --version, local | 27.25 / 30.89 | 27.26 / 32.33 | 27.79 / 31.45 |
| Prototype empty, off | 15.89 / 18.03 | 16.07 / 18.69 | 16.21 / 18.61 |
| Prototype empty, local | 17.62 / 19.40 | 18.14 / 21.55 | 18.18 / 19.75 |
| Prototype wrapper --version, off | 18.77 / 21.12 | 19.36 / 21.45 | 19.06 / 21.23 |
| Prototype wrapper --version, local | 20.71 / 22.89 | 20.94 / 23.70 | 21.27 / 25.88 |

The rebuilt/profiling variants are somewhat slower than the originals. For prototype empty/off, original → control adds 0.19 ms median and control → profile adds 0.14 ms. Other control/profile differences range up to roughly 0.7 ms median. Attribution uses the instrumented host’s timeline, not an exact partition of the original executable. These perturbations are materially smaller than the 3–6 ms groups.

Minimal native controls launch and exit in 1.77 ms median (Rust) and 1.59 ms (C). The Rust wrapper’s explicit-local-path version command is 17.94 ms here, matching the prior 17.77 ms version_local control. The earlier ~5.30 ms Rust number used an installed version. Those cases include different work; they are not interchangeable startup floors.

### What to investigate next

1. **Decode and reconstruct the executable graph.** About 7.5 ms remains in artifact decoding and engine initialization, plus about 1.7 ms destroying the resulting graph. A verified image with borrowed metadata/arena ownership could remove whole-table allocation, relocation, rendering-schema reconstruction, and per-object destruction. This is a design direction, not a measured implementation or a promised saving. Debugging, reflection, telemetry metadata, package boundaries, and malformed-artifact rejection must keep working.

2. **Native loading, with the compiler hypothesis now measured.** The [compiler-linkage follow-up](compiler-linkage.md) removes the compiler through both the packed host and the native bridge dependency. Initializers fall from 133 to six and bare host size from about 28.7 to 19.5 MiB, but median launch improves only 0.21–0.68 ms across fixtures, with about 6 ms still before main. This does not justify removing runtime compilation for latency. A minimal Rust system-library control shows a larger native-platform cost: linking the host's Security/CoreFoundation/libiconv/libSystem set changes empty launch from 1.69 to 4.40 ms median. It does not fully partition the packed-host launch cost or establish a safe production change. The executable-graph work above remains the main cross-platform target.

These costs affect ordinary packed BAML applications. The wrapper adds about 2.6 ms of target execution here, including the native child/version handling; most of its elapsed time is shared runtime cost.

### Reproduction and retained evidence

All runtime source files remain unchanged. The standalone diagnostic reads the actual host sources and generates temporary instrumented/control sources in the ignored output directory. Existing release libraries are selected by the host’s Cargo dependency fingerprints, rather than guessing library filename hashes. Both use opt-level 3, fat LTO, one codegen unit, stripped symbols, and unwind panics.

For the recorded builds, the normal host library is baml_pack_host-c2085387d94da70b and the prototype is baml_pack_host-654baeda5263d170. Build both library sets first using the matching normal/prototype source as described above, then run:

```sh
python3 scripts/runtime-startup/profile_host.py build target/host-startup-profile --release-dir target/release
python3 scripts/runtime-startup/profile_host.py measure target/host-startup-profile --baseline /absolute/path/to/baseline --candidate /absolute/path/to/candidate --fixtures /absolute/path/to/comparison-fixtures --rust-wrapper /absolute/path/to/rust-wrapper --runs 100
```

Use --baseline-fingerprint and --candidate-fingerprint on build to select explicit existing host library fingerprints. The directories passed to measure contain baml-cli plus the previously packed empty-packed and baml-packed originals. --fixtures points at compare.py’s output containing projects/empty and the native child. Finish builds and tests before measuring.

Measured 2,700 fresh launches across 27 variants, plus 81 warmups and 12 separate stdout correctness checks. Eight instrumented variants supply 800 complete timelines. Every timeline’s monotonic timestamps are ordered and its phase intervals sum to that sample’s parent wall time within 0.001 ms. Fixed shuffle seed 2026100402; synthetic HOME/BAML_HOME and disabled update checks; fixture child; no inherited credentials/cloud destination. No Linux/Windows/iOS/Wasm or cold-cache performance claim.

[profile_host.py](profile_host.py) reproduces the builds/measurement. [host-profile-results-20261004.json](host-profile-results-20261004.json) retains every counter, absolute parent timestamp, child trace, artifact/source hash, matched library selection, and summary. Local generated sources, executables, logs and loader inspection output remain under target/host-startup-profile-20261004.

## Compiler-linkage follow-up

The compiler-free experiment does **not** deliver a substantial latency reduction. Matched execution-ready empty programs take 16.15 → 15.86 ms with telemetry off, and 18.19 → 17.59 ms with local recording. Wrapper version commands take 19.15 → 18.90 ms off and 21.15 → 20.76 ms local. These are separate fresh interleaved measurements, not the earlier profiling run.

[Compiler linkage and native loading](compiler-linkage.md) contains the full tables, phase attribution, runtime-compilation capability check and reproduction. It retains 4,800 compiler-linkage samples and 800 minimal native-library controls. The shipping runtime remains unchanged.
