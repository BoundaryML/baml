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
