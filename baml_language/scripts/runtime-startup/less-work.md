# Doing less startup work

**Result:** successful validation and unused schema construction are real startup work we can remove. In the final prototype, empty/off takes **16.68 → 14.80 ms median**, and wrapper/off takes **19.46 → 17.71 ms**. Empty startup executes **16.7% fewer root-process instructions**. This is an incremental roughly 1.5–2 ms gain on top of the earlier execution-ready-bytecode prototype, with the full compiler still included.

Timing is noisy: other checkouts were compiling at 77 of the 100 round-start snapshots. All samples are retained; the 23 quieter rounds show the same direction. They are not an isolated-machine result. Instruction counts and a separate allocation probe establish that the prototype does less work, rather than merely moving the same work into a different allocator.

## Program behavior

The user writes and invokes the same programs:

```baml
function Main(name: string, n: int = 2) -> string {
    name + baml.json.to_string(n)
}
```

```sh
./app --name test
# test2
```

Scalar argument conversion no longer builds every class/alias wire definition. If the program actually uses a class input or requests a JSON schema, that metadata is constructed at its first lookup:

```baml
enum Color { Red Blue }
class Person { name: string, age: int, favorite: Color }
function Main(person: Person) -> baml.json.json {
    baml.json.schema(reflect.Type.of<Person>())
}
```

This returns the same schema and rejects the same missing required field. Runtime compilation via `reflect.Package.compile` still returns 42 in the existing fixture; reflection, signatures, documentation and source error locations still agree with the control. No new public BAML API is proposed.

## Matched launch measurements

Median / p95 milliseconds, 100 fresh launches per cell. Both columns use **the identical packed executable and release libraries**; environment flags select eager control or both work reductions. Profile emission is disabled for these rows. Telemetry off sets BAML_TELEMETRY=off; local uses default disk recording without a cloud destination.

| Program / telemetry | Eager control | Less work | Median saved |
| --- | ---: | ---: | ---: |
| Empty Main, off | 16.68 / 26.62 | 14.80 / 25.03 | 1.88 ms (11.3%) |
| Empty Main, local | 18.82 / 36.62 | 17.16 / 30.17 | 1.67 ms (8.8%) |
| 1,000 extra functions, off | 19.83 / 30.93 | 17.79 / 30.70 | 2.04 ms (10.3%) |
| 1,000 extra functions, local | 21.91 / 41.62 | 20.09 / 33.95 | 1.82 ms (8.3%) |
| Wrapper --version, native child, off | 19.46 / 37.39 | 17.71 / 31.08 | 1.75 ms (9.0%) |
| Wrapper --version, native child, local | 21.83 / 39.87 | 19.69 / 36.54 | 2.14 ms (9.8%) |
| Typed scalar arguments, off | 16.64 / 25.38 | 15.16 / 24.68 | 1.48 ms (8.9%) |
| Typed scalar arguments, local | 18.59 / 39.48 | 16.72 / 30.14 | 1.87 ms (10.0%) |
| Class input + JSON schema, off | 16.99 / 25.41 | 15.80 / 28.45 | 1.19 ms (7.0%) |
| Class input + JSON schema, local | 18.76 / 35.95 | 17.74 / 36.63 | 1.02 ms (5.4%) |

The wrapper launches the same C CLI fixture. These rows include wrapper version/forwarding work; they do not measure a second BAML VM, real CLI compilation or an LSP session. These absolute numbers come from this run and host build; they should not be subtracted from earlier experiments with different library features/module layouts.

### Separate the two reductions

Telemetry off; median wall milliseconds. Successful validation formats error context only when a check fails. Schema mode defers class/enum/LLM/alias definitions and inbound views until an actual lookup. Both enables both.

| Program | Eager | Deferred diagnostics | Deferred schemas | Both |
| --- | ---: | ---: | ---: | ---: |
| Empty Main | 16.68 | 16.23 | 15.57 | 14.80 |
| 1,000 extra functions | 19.83 | 19.12 | 19.00 | 17.79 |
| Wrapper --version, native child | 19.46 | 19.34 | 18.53 | 17.71 |
| Typed scalar arguments | 16.64 | 16.27 | 15.31 | 15.16 |
| Class input + JSON schema | 16.99 | 16.66 | 16.62 | 15.80 |

Class/schema programs receive a smaller schema-only benefit because they really use the class definitions. The current prototype still builds the entire class map on its first class lookup. It avoids unused work; it does not promise to eliminate necessary work.

### Quieter subset

The same 23 round indices are selected for **every** variant using only the absence of cargo/rustc in the round-start snapshot, not favorable timings. Builds may still start within a round and other desktop activity remains uncontrolled. Median milliseconds:

| Program | Eager, off | Both, off | Eager, local | Both, local |
| --- | ---: | ---: | ---: | ---: |
| Empty Main | 16.16 | 14.21 | 18.19 | 16.34 |
| 1,000 extra functions | 19.16 | 17.07 | 21.61 | 19.61 |
| Wrapper --version, native child | 19.04 | 17.52 | 21.09 | 19.51 |
| Typed scalar arguments | 16.14 | 14.53 | 18.14 | 16.41 |
| Class input + JSON schema | 16.69 | 15.43 | 18.14 | 17.44 |

## Evidence that work disappeared

A separate allocator-counting validation probe resets its counter immediately around validation of the same valid empty program. It counts alloc/alloc_zeroed/realloc requests, not bytes or retained memory. It has no concurrent telemetry threads and is not used for launch timing.

| Valid program validation | Allocation requests |
| --- | ---: |
| Original eager error context | 11,583 |
| Format context only on error | 31 |

All structural checks remain. **115 differential checks** cover the valid graph and malformed object/global/package indices, type-head tags, initialization order, member kinds and generic cells. Each agrees with the original success/failure and exact error text. This is not exhaustive malformed-artifact verification; the earlier compact-bytecode prototype still needs its own verifier.

Uninstrumented controls, telemetry off; medians. Memory is measured rather than an optimization requirement. Root instructions/cycles are sampled near exit; they are not a complete child-process instruction total.

| Program | CPU ms, eager → both | Root instructions M, eager → both | Peak RSS MiB, eager → both | Peak footprint MiB, eager → both |
| --- | ---: | ---: | ---: | ---: |
| Empty Main | 14.89 → 12.95 | 120.86 → 100.71 | 23.92 → 20.39 | 16.49 → 13.18 |
| 1,000 extra functions | 17.91 → 15.64 | 152.37 → 129.61 | 29.50 → 25.33 | 21.27 → 17.30 |
| Wrapper --version, native child | 17.53 → 15.88 | 126.35 → 106.54 | 25.08 → 23.08 | 16.10 → 14.26 |
| Typed scalar arguments | 14.79 → 13.24 | 121.18 → 101.63 | 24.19 → 21.72 | 15.97 → 13.67 |
| Class input + JSON schema | 15.10 → 14.06 | 122.36 → 109.42 | 24.88 → 24.13 | 16.02 → 15.24 |

### Loading passes

Mean milliseconds from all 100 instrumented empty/off launches per mode, including contention. The 32 internal marks identify pass boundaries; separate host marks cover envelope integrity verification, deserialization, engine construction and destruction. These are diagnostic phase means, not a partition of the uninstrumented median table.

| Phase | Eager | Both |
| --- | ---: | ---: |
| SHA-256 / envelope validation | 1.236 | 1.238 |
| Borsh object deserialization | 3.355 | 3.322 |
| Program validation | 1.085 | 0.420 |
| Class definition projection | 0.493 | 0.000 |
| Inbound wire views | 0.343 | 0.000 |
| Total engine construction | 3.844 | 2.409 |
| Engine / host-state destruction | 1.788 | 1.500 |

The remaining substantial target is deserialization/materialization of the executable graph, followed by native loading. Callable rendering, builtin attachment and type-head binding still run. These measurements do not establish a cache-miss bottleneck or a benefit from adding an arena. Prebuilding or borrowing specific derived tables should be evaluated against the actual pass counts, with a verified representation.

## What the prototype changes

- Retains the original Program::validate and adds a diagnostic equivalent whose role/error strings are closures evaluated only on failure. This duplication is for the differential experiment, not the proposed production API.
- Uses shared one-time initialization for LLM/class/enum/alias projections. Eager control forces them at their original construction points.
- Shares existing name maps through Arc instead of copying them into closures. Pending closures hold the static heap backing until they initialize or are dropped.
- Makes inbound coercion accept a definition lookup provider. Scalars avoid any lookup. Recursive aliases have a separate alias-only cache so an alias lookup does not force all classes.
- Keeps program checks, envelope kind/build fingerprint/SHA-256 checks, package initialization, normal engine shutdown/destructors, compiler linkage and ordinary telemetry configuration.

An earlier version deferred the tables but forced all inbound views at the first argument conversion. It helped an empty Main but lost the schema saving for typed/wrapper/large programs. That version has 6,400 retained samples and its own patch/build metadata in the data file; it is excluded from the final tables. The final lookup-provider version fixes where the work is demanded. The 120-launch pass-identification pilot is also retained and excluded.

## Correctness and production work

**112 candidate output/exit/stderr comparisons**, **18 comparisons with the previous prepared host**, and **six previous-host schema comparisons** pass. Cases include JSON, defaults, native callbacks, floats/catch, 1,000 functions, typed help/arguments, source errors, reflection/docs, runtime compilation, wrapper forwarding/version, class schemas/invalid input and recursive alias schemas. All four modes are checked with telemetry off and local. The focused coercion suite passes **52 tests**, including recursive aliases, ambiguous unions, sparse annotations and media wrappers.

The runtime source tree is restored after the experiment. [less-work.patch](less-work.patch) contains the full temporary prepared-bytecode + instrumentation + work-reduction prototype. It applies to parent 47eabccbf4096fa43b57a8eba33a9777ba865939, from the repository root. It is not a shipping API or default feature.

Production should first refactor the existing validator in place to create diagnostics lazily. Schema deferral needs a deliberate ownership/cache boundary, a coherent context-construction API and concurrency/GC/drop/SDK coverage. The experiment’s Deferred helper uses unique Arc assumptions and environment switches; those are not production interfaces. First class/schema use can pay the deferred cost. The prepared-image verifier/debug-data/fallible-source-map work documented in the earlier report remains required before shipping that artifact format.

## Methodology and replay

2026-10-04; Apple M2 Max, 12 logical CPUs, 64 GiB, aarch64 macOS 15.6.1 (24G90), Rust 1.98.0 (88d9e12ae). Unchanged release settings: opt-level 3, fat LTO, one codegen unit, stripped symbols, unwind panics. Full runtime compiler is linked. The matched host library is baml_pack_host-7154125a2adf2a27; each transitive extern is chosen by the recorded Cargo dependency fingerprint. The artifact build identity is pinned to cb83e865e3145974170ca83b973eae60edd79bcc for the existing prepared compiler; the experiment source parent is 47eabccbf4096fa43b57a8eba33a9777ba865939 plus the patch. Their intervening commits contain reports rather than runtime changes.

**64 variants × 100 fresh launches = 6,400 final timings**, plus 192 warmups. Forty variants are uninstrumented controls (five programs × two telemetry settings × four modes); 24 are profiles (empty/large/wrapper × two settings × four modes), supplying **2,400 ordered timelines**. Fixed shuffle seed 2026100406; sample index is its shared round index. Each full host timeline sums to parent wall time within 0.001 ms. The retained 23-round subset is 1,472 of these launches, not an additional experiment.

The native C helper uses posix_spawn, a stdout pipe, proc_pid_rusage and wait4. Wall time spans before creation through reaping; first output is first stdout observed; CPU is user+system. The validation allocation probe uses thin LTO and is intentionally separate from the fat-LTO timing host. Synthetic HOME/BAML_HOME, disabled update checks, a fixed native toolchain fixture, allowlisted environment and no inherited credentials/cloud traffic isolate configuration. Executable/file caches are warmed; no Linux/Windows/iOS/Wasm or cold-start platform conclusion.

[less-work-results-20261004.json](less-work-results-20261004.json) contains all final scalar samples, timestamp trees, pass phases and summaries; the exact quieter round selection; per-round foreign-build snapshots; all validation/correctness checks; fixture sources; artifact/source hashes; fingerprint-matched build commands; and frozen Rust/Python/C diagnostic sources. Column encoding is lossless: zip columns with a row, then merge each recursively decoded child at the same row index. Reconstructed samples and recomputed final summaries match the original output exactly.

To replay in an isolated checkout, apply less-work.patch from the repository root. Build matching release libraries/CLI with the startup-experiment feature and the pinned BAML_GIT_SHA. Restore sources.host.rs and the frozen harness sources from the JSON into an ignored output directory. Compile the host with build.command after adjusting checkout/output/dependency paths, or use build_host.py with the recorded host fingerprint; this generates the real host entry-point probe through profile_host.render_sources. Compile the stored measure.c and fixture_child with clang -O2. Recover fixture sources and pack them through the matching prepared compiler/host. The frozen measure_candidates.py expects the documented ignored target directories; adjust its root/output/helper/fixture paths if necessary. Finish all builds/tests, arrange an idle machine, then run 100 rounds. Use the recorded fixtures directly when reproducing timing; the frozen historical correctness harness additionally expects the earlier prepared-host control outputs.

Local binaries, pack/build/test logs, recordings and the unpacked raw JSON remain under target/loading-passes-experiment-20261004.
