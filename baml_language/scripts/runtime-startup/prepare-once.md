# Prepare compact bytecode once

The shared loader now boxes float constants and globals before lowering compact bytecode and specializing calls. Engine and VM constructors consume that prepared program. This removes a complete duplicate preparation pass for every loaded program, including packed applications and SDK execution.

This program keeps the same result and source diagnostics, with less startup work:

```baml
function leaf(value: float) -> float { value }
function Main() -> float { leaf(0.5) }
```

## Complete launch to exit

Default local recording; **median / p95**, 100 fresh processes per cell:

| Program / command | Before, median / p95 | After, median / p95 | Median saved |
| --- | ---: | ---: | ---: |
| Empty Main | 22.86 / 38.95 ms | 21.52 / 24.22 ms | 1.33 ms (5.8%) |
| Print ready | 23.01 / 27.29 ms | 21.57 / 27.04 ms | 1.43 ms (6.2%) |
| JSON / defaults / callbacks / catch | 23.13 / 25.39 ms | 21.68 / 23.43 ms | 1.45 ms (6.3%) |
| 1,000 extra functions | 26.58 / 28.52 ms | 25.16 / 27.37 ms | 1.43 ms (5.4%) |
| Typed arguments and default | 23.12 / 25.37 ms | 21.57 / 23.22 ms | 1.55 ms (6.7%) |
| Generated --help | 22.20 / 26.02 ms | 20.97 / 29.36 ms | 1.23 ms (5.5%) |
| Wrapper --version, real CLI | 30.71 / 53.78 ms | 28.88 / 51.03 ms | 1.83 ms (6.0%) |
| Wrapper run --help, real CLI | 27.95 / 39.23 ms | 26.21 / 33.89 ms | 1.74 ms (6.2%) |

Telemetry off:

| Program / command | Before, median / p95 | After, median / p95 | Median saved |
| --- | ---: | ---: | ---: |
| Empty Main | 20.99 / 22.37 ms | 19.46 / 20.84 ms | 1.53 ms (7.3%) |
| Print ready | 21.04 / 25.67 ms | 19.69 / 20.85 ms | 1.35 ms (6.4%) |
| JSON / defaults / callbacks / catch | 21.09 / 25.89 ms | 19.84 / 22.08 ms | 1.25 ms (5.9%) |
| 1,000 extra functions | 24.35 / 26.19 ms | 23.05 / 24.72 ms | 1.30 ms (5.3%) |
| Typed arguments and default | 20.95 / 22.97 ms | 19.59 / 21.84 ms | 1.36 ms (6.5%) |
| Generated --help | 20.24 / 25.65 ms | 18.96 / 37.12 ms | 1.27 ms (6.3%) |
| Wrapper --version, real CLI | 28.56 / 47.18 ms | 26.87 / 32.99 ms | 1.69 ms (5.9%) |
| Wrapper run --help, real CLI | 25.63 / 27.73 ms | 24.13 / 25.96 ms | 1.50 ms (5.9%) |

Generated help has a higher p95 in the candidate (local: 26.02 → 29.36 ms; telemetry off: 25.65 → 37.12 ms), while several other cells contain large outliers. The result is a median improvement; this run does not establish a uniform tail-latency gain.

## Supporting resource measurements

Medians; full wall time above is the product result.

| Program, default local recording | CPU before → after | Peak RSS before → after |
| --- | ---: | ---: |
| Empty Main | 21.66 → 20.15 ms | 30.82 → 30.42 MiB |
| 1,000 extra functions | 25.39 → 24.03 ms | 36.92 → 36.54 MiB |
| Wrapper --version, real CLI | 29.32 → 27.55 ms | 33.83 → 33.05 MiB |
| Wrapper run --help, real CLI | 25.78 → 24.20 ms | 33.70 → 32.73 MiB |

Packed wrapper size: **33,257,122 → 33,257,090 bytes**. All 32 cells and every metric are in [the retained data](prepare-once-results-20261004.json).

## Native Samply follow-up

A fresh native capture confirms that bytecode lowering now appears only beneath `convert_program`. The baseline had two paths: one beneath `convert_program`, and another directly beneath engine construction. Fat LTO inlines `prepare_compact_code` into `convert_program` in the updated host; its absence as a separate frame does not mean preparation was removed.

Main-thread weighted sample shares, with telemetry off:

| Work | Empty before | Empty after | Wrapper --version before | Wrapper --version after |
| --- | ---: | ---: | ---: | ---: |
| Program deserialization | 26.2% | 26.1% | 19.5% | 21.1% |
| Engine construction | 34.6% | 27.9% | 25.7% | 21.7% |
| Engine / heap destruction | 28.0% | 34.3% | 20.0% | 21.5% |
| Artifact integrity hash | 7.4% | 7.3% | 5.4% | 5.8% |
| Dispatch / finalization, including child waits | 3.5% | 3.8% | 28.8% | 29.1% |
| Bytecode lowering, included in construction above | 9.5% | 5.1% | 7.5% | 4.5% |

Deserialization and destruction account for about 60% of the updated empty program's captured samples. Prominent stacks construct `Function`, `Bytecode`, strings and vectors, then individually free their allocations. This suggests testing an execution-ready image with shared ownership to reduce both reconstruction and per-object destruction; the capture does not establish how much that would save. The wrapper additionally spends 23.9% of its captured samples in a blocked wait; its CLI child ran successfully but contributed no native samples.

These are sample shares from separate, noisy executions, not phase timings. A larger destruction share does not establish a destruction regression. Use the complete launch-to-exit measurements above for the latency result.

Recorded 2026-10-04 on the same Apple M2 Max / macOS 15.6.1 host with Samply 0.13.1: 40 launches per program, requested 4,000 Hz, repeated non-overlapping threads merged, telemetry off, isolated BAML_HOME and the identical real release CLI child used by the benchmark. Updated source: 843837715c. Diagnostic and ordinary production binaries match stdout, stderr and exit status.

The diagnostic host links the existing production release dependencies with opt-level=3, fat LTO, one codegen unit, unwind panics and retained native symbols; the compiler remains linked. Only the ignored diagnostic entry point uses `dladdr` plus header-aware `getsectiondata` to locate the embedded artifact under Samply's injected helper. Production section lookup is unchanged. Runtime dependencies lack full DWARF information, so inline/source attribution is incomplete, and the preload handshake can miss early native loader work.

Empty weighted sample totals are 3,302 before and 3,446 after; wrapper totals are 4,656 before and 4,426 after. Weighted totals differ from raw counts because some scheduling gaps represent multiple samples. Captures, symbolized binaries, build/library hashes, output checks, diagnostic build/capture scripts and symbolication analyses remain local build outputs in `target/firefox-startup-profiles-20261004` and `target/firefox-prepare-once-profiles-20261004`.

## Correctness

Regression tests inspect boxed constants/globals, deduplication, signed zero and distinct NaN bit patterns; execute a float global through the VM; and retain compact source/exception tables. Existing tests cover engine float execution and exact calls, defaults/generics/native callbacks, VM resumption, throwing source spans and malformed-program refusal. VM/engine/BAML unit tests and workspace Clippy pass. The benchmark independently verifies all eight commands and matching uncaught-error files/lines.

## Methodology and reproduction

Measured on 2026-10-04: Apple M2 Max, 64 GiB, 12 logical CPUs, native aarch64, macOS 15.6.1 (24G90), Rust 1.98.0 (88d9e12ae). Ordinary Cargo release: opt-level=3, fat LTO, codegen-units=1, stripped symbols, unwind panics. Compiler linked and available.

The baseline is the retained validation release (source 5149a8c8c0); the candidate is parent 8733c5f4b5 plus the exact two-file runtime patch stored in the data. Matching CLIs pack identical sources with their corresponding hosts. All seven format-12 payloads match after normalizing only authenticated build identity/hash fields. Every original integrity hash is checked. The real wrapper CLI child is identical in both variants, selected through an explicit local toolchain path.

The native timer starts before posix_spawn and stops after wait4 reaps the program. It includes native loading, artifact decode, engine setup, target/child execution, recording shutdown, destruction and exit. First stdout, CPU, peak RSS/physical footprint, instructions/cycles, disk I/O, faults and context switches are also retained. CPU is OS user+system accounting; near-exit detailed counters describe the root process rather than a summed process tree.

32 variants × 100 fresh launches = 3,200 timed launches; three warmups per variant (96) and 18 output/error checks. Fixed-seed shuffled interleaving (20261004), warmed executable/file caches. Isolated HOME/BAML_HOME, update.auto_check=false, no configured cloud destination; telemetry off or default local recording. All builds/tests/Clippy finish before timing; the harness waits for other build workers before each round and retains all 100 clear round-start snapshots. Ordinary desktop activity and work starting inside a round remain uncontrolled. p95 is the nearest-rank 95th sample; this is one ARM Mac measurement, and tail latency remains noisy.

Rebuild each source variant with `cargo build --release -p baml_cli -p baml_pack_host` and copy its CLI/host into separate baseline/candidate directories. Extract sources.compare.py from the retained JSON; adjust its recorded checkout/helper paths, copy the identical real release CLI child to OUTPUT/cli-child, then run:

```sh
python3 -u compare.py OUTPUT --baseline BASELINE --candidate CANDIDATE --runs 100
```

The data include the exact production patch, source/binary/artifact hashes, all scalar samples in lossless columns, verified summaries, fixture checks, full test/build logs and the executable measurement/check/export sources.
