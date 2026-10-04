# Compiler linkage and native loading experiment

**Removing the runtime compiler is a small latency win, not the next substantial startup optimization.** Across the matched fixtures, median savings range from 0.21 to 0.68 ms (1–3.3%). The compiler-free host loses 127 startup initializers and about 9.1 MiB of native code/data, but still spends about 6 ms before Rust main.

```baml
function Main() -> void {}
```

On the execution-ready prototype, this program takes 16.15 ms with the compiler and 15.86 ms without it, with telemetry off. Normal local recording takes 18.19 → 17.59 ms. The program payloads are byte-identical within each artifact format.

**Recommendation:** keep runtime compilation available and prioritize artifact decoding, engine construction, and executable-graph ownership. Their measured cost remains substantial. This experiment does not justify removing `reflect.Package.compile` or building a separate compiler loader for latency alone.

The compiler-free builder and comparison harness have been removed. The measured results and methodology remain as a record of the rejected optimization; the shipping host keeps its compiler.

## End-to-end comparison

Milliseconds, median / p95. Each cell contains 100 fresh process launches. Full and compiler-free hosts use matching release dependencies and the same program bytes. “Normal” is the supported artifact representation; “execution-ready” is the earlier experimental representation, with its existing verification/debugging limitations.

| Artifact / program | Telemetry | Full compiler | Compiler-free | Median saved |
| --- | --- | --- | --- | --- |
| Normal / empty | off | 21.87 / 23.73 | 21.61 / 23.12 | 0.26 ms |
| Normal / empty | local | 23.98 / 26.52 | 23.47 / 26.51 | 0.52 ms |
| Normal / print ready | off | 22.17 / 25.73 | 21.49 / 22.95 | 0.68 ms |
| Normal / print ready | local | 24.02 / 26.78 | 23.51 / 29.30 | 0.51 ms |
| Normal / 1,000 extra functions | off | 25.47 / 27.33 | 25.22 / 27.54 | 0.25 ms |
| Normal / 1,000 extra functions | local | 27.92 / 31.45 | 27.40 / 29.36 | 0.52 ms |
| Normal / wrapper version | off | 25.43 / 27.99 | 25.06 / 27.09 | 0.37 ms |
| Normal / wrapper version | local | 27.59 / 29.49 | 27.22 / 29.23 | 0.37 ms |
| Execution-ready / empty | off | 16.15 / 17.89 | 15.86 / 17.76 | 0.28 ms |
| Execution-ready / empty | local | 18.19 / 22.14 | 17.59 / 20.05 | 0.60 ms |
| Execution-ready / print ready | off | 16.33 / 18.21 | 16.12 / 17.98 | 0.21 ms |
| Execution-ready / print ready | local | 18.24 / 20.20 | 17.76 / 20.11 | 0.48 ms |
| Execution-ready / 1,000 extra functions | off | 19.23 / 21.00 | 18.80 / 21.71 | 0.43 ms |
| Execution-ready / 1,000 extra functions | local | 21.31 / 23.89 | 20.73 / 23.39 | 0.58 ms |
| Execution-ready / wrapper version | off | 19.15 / 20.64 | 18.90 / 22.31 | 0.25 ms |
| Execution-ready / wrapper version | local | 21.15 / 22.72 | 20.76 / 23.27 | 0.39 ms |

The small median gain is fairly consistent; p95 does not consistently improve. CPU medians change by about 0–0.44 ms. The data retains first-output timing, CPU, peak RSS/footprint, instructions, cycles, faults, switches and disk counters. Memory was not the optimization target. Wrapper rows use the same native CLI fixture and explicit-local-toolchain path.

## Phase attribution and physical linkage

Mean milliseconds, 100 instrumented empty-program launches per cell, telemetry off. These are instrumented timelines, not exact partitions of the uninstrumented medians above.

| Phase | Normal full | Normal compiler-free | Execution-ready full | Execution-ready compiler-free |
| --- | ---: | ---: | ---: | ---: |
| Launch, loader/initializers, Rust setup | 6.431 | 6.183 | 6.717 | 6.344 |
| Decode artifact | 5.726 | 5.754 | 4.122 | 4.096 |
| Build engine | 6.288 | 6.239 | 3.609 | 3.429 |
| Destroy engine / host state | 2.440 | 2.509 | 1.697 | 1.705 |
| All other phases | 1.161 | 1.176 | 1.124 | 1.159 |
| **Total mean wall time** | **22.046** | **21.861** | **17.269** | **16.733** |

The large initializer count was a plausible suspect, but its removal does not eliminate the before-main cost. The remaining execution-ready decode/construction costs are about 7.5 ms, plus about 1.7 ms destruction. Those remain targets, not promised savings.

Bare control hosts, excluding embedded BAML program bytes:

| Host | Native size | Initializers | Loader fixups | Imports |
| --- | ---: | ---: | ---: | ---: |
| Normal full | 28.65 MiB | 133 | 36,706 | 265 |
| Normal compiler-free | 19.52 MiB | 6 | 28,599 | 264 |
| Execution-ready full | 28.66 MiB | 133 | 36,719 | 265 |
| Execution-ready compiler-free | 19.54 MiB | 6 | 28,623 | 264 |

Removing only `Some(bex_project::runtime_compiler())` does not remove the compiler dependency. There is also a runtime type/trait dependency through `sys_native → bridge_ctypes → bex_project`. The diagnostic replaces that second route with a runtime-only facade assembled from the actual `bex_project` runtime exports, `BexArgs`, `RuntimeError`, and unchanged Bex trait/implementation. It rebuilds the unchanged `bridge_ctypes` and `sys_native` sources against that facade. Other runtime dependencies retain their original fingerprint-matched release libraries and features.

The generated facade is measurement scaffolding. A production separation would put runtime bridge vocabulary in an appropriate shared crate and require SDK/callback coverage; it would not copy the project facade into generated application code.

## Native system-library control

An additional minimal Rust executable has no BAML/compiler/runtime code. Its main is empty. Both versions use the same release flags; one links only libSystem, and the other additionally links Security, CoreFoundation, and libiconv—the same four direct system libraries as the packed host. `otool -L` verifies the linked libraries. Builds finish before measurement.

200 fresh processes per cell, median / p95 milliseconds:

| Direct system libraries | Empty executable | Instrumented executable | Before main, instrumented mean |
| --- | --- | --- | ---: |
| libSystem | 1.69 / 2.03 | 1.72 / 2.06 | 1.637 |
| libSystem + Security + CoreFoundation + libiconv | 4.40 / 5.56 | 4.37 / 5.36 | 4.281 |

Linking the host's system-library set adds about 2.7 ms median in this minimal control, independently of BAML and Salsa. It is a meaningful native-platform cost; this does not attribute the increase to an individual framework, fully partition the packed host's 6 ms, or establish a safe way to omit required platform functionality. Native library loading can be investigated separately while the main cross-platform work targets the executable graph.

## Correctness and scope

Full and compiler-free controls match exit status, stdout and stderr for empty/printing programs, 1,000 declarations, JSON/default arguments/native callbacks/floats/catch, typed arguments, uncaught source errors, and wrapper version forwarding. Control/profile variants share identical program payloads and stdout. The native callbacks fixture exercises VM/native collection callbacks, not an externally registered CFFI host callback.

The runtime-compilation fixture deliberately distinguishes the two hosts:

```baml
function Main() -> int {
  let package = reflect.Package.compile({
    "leaf.baml": "function answer() -> int { 42 }"
  });
  let answer = package.get_function<() -> int>("root.answer")
    ?? throw "missing answer";
  answer()
}
```

It returns `42` in both full-host artifact formats. Both compiler-free diagnostics report `runtime compiler was not installed by the host` and fail. This is expected for the diagnostic and would be an unacceptable silent capability loss in a production host.

No supported runtime source, package behavior, telemetry API, or artifact format changes in this follow-up. Production shipping still has the earlier execution-ready prototype's verification/debugging requirements, and a runtime bridge split would need full SDK/callback/GC/cancellation coverage.

## Methodology and reproduction

Apple M2 Max, 64 GiB RAM, macOS 15.6.1, native ARM, Rust 1.98.0. Unchanged release settings: opt-level 3, fat LTO, one codegen unit, stripped symbols, unwind panics. Reuse the normal and execution-ready compiler/host libraries from the [main experiment](README.md); normal host fingerprint `baml_pack_host-c2085387d94da70b`, execution-ready `baml_pack_host-654baeda5263d170`. The facade/bridge/provider library rebuilds embed bitcode for final fat LTO and preserve the original library feature selections. Exact commands, dependencies, generated schema hash and runtime source hashes are retained with the samples.

The compiler-free experiment's historical builder and comparison harness are available at [commit 782c8c6d89](https://github.com/BoundaryML/baml/commit/782c8c6d893d0be02782155d2be00f06c79a0535). They are not part of the current implementation. Exact build commands remain in the recorded data. The separate native-library control remains runnable:

```sh
python3 scripts/runtime-startup/native_loader.py target/native-loader-measurements --profile-hosts target/host-startup-profile-20261004 --runs 200
```

The original full hosts are uninstrumented/instrumented controls rebuilt in the same module layout used by the diagnostic. Host source differs only at the two compiler-provider arguments; the native bridge/provider source stays unchanged. The original compiler is retained for packing both variants, and every paired embedded envelope is checked byte-for-byte, rather than assuming deterministic compilation. The measurement script also checks control/profile program equality.

4,800 fresh timed launches across 48 variants, 144 warmups, and 40 observable-behavior checks. The native control adds 800 timed launches and 12 warmups. In total, 2,000 instrumented timelines preserve parent/child timestamps; every ordered timeline sums to its parent wall time within 0.001 ms. Seeds 2026100403/2026100404; shuffled/interleaved variants, warmed executable/file caches, builds/tests idle, synthetic HOME/BAML_HOME, disabled update checks, fixed native child, no inherited credentials/cloud destination. Local uses ordinary disk recording; off sets BAML_TELEMETRY=off.

The existing C helper measures from posix_spawn through EOF/counter sampling/wait4. CPU and instruction/cycle counters are root-process observations, not full process-tree accounting. Mean phase groups sum; independent medians/p95 do not. No physical Linux/Windows/iOS/Wasm or cold-cache performance claim.

- [Compiler-linkage samples, hashes, source/library selections and behavior checks](compiler-linkage-results-20261004.json)
- [Native-library samples, commands, library lists and hashes](native-loader-results-20261004.json)
- [Minimal native-library control harness](native_loader.py)

Compiler-free generated source, libraries and binaries have been removed. Retained local measurement logs/data and native-library controls remain under target/compiler-linkage-measurements-20261004 and target/native-loader-experiment-20261004.
