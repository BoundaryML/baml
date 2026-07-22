# BAML compiler and runtime benchmarks

This crate currently defines three Divan benchmark binaries:

| Bench | Source | Measures |
|---|---|---|
| `compiler_benchmark` | `benches/compiler_benchmark.rs` | Cold compilation of an empty project and the full `baml_src/` test project |
| `runtime_benchmark` | `benches/runtime_benchmark.rs` | Pure VM execution workloads generated from `tools/speedtest/workloads/**/*.md` |
| `cache_profile` | `benches/cache_profile.rs` | Hardware instruction/cache/branch counters for fixed VM workloads on Apple Silicon |

Run them from `baml_language/` in the benchmark profile:

```bash
cargo bench -p baml_tests --bench compiler_benchmark
cargo bench -p baml_tests --bench runtime_benchmark
```

Divan accepts a name filter after `--`:

```bash
cargo bench -p baml_tests --bench compiler_benchmark -- compile_empty_project
cargo bench -p baml_tests --bench compiler_benchmark -- compile_baml_tests_project
```

`compiler_benchmark` intentionally creates a fresh `ProjectDatabase` for every sample so it measures uncached compilation. It reads source files before the measured region. The large `compile_baml_tests_project` case is capped at five one-compile samples.

`runtime_benchmark` is generated in part by `crates/baml_tests/build.rs`. Add Markdown workloads under `tools/speedtest/workloads/`; the build script extracts each workload's BAML block and emits benchmark functions into the generated benchmark module.

Use `cargo bench`, not `cargo test`, for timings. The compiler benchmark exits early when built with debug assertions because debug timings are not representative.

`cache_profile` is a profiling executable rather than a Divan suite. On macOS/Apple Silicon, build and run it with the workspace's profiling profile and the privileges required by `darwin-kperf`:

```bash
cargo build -p baml_tests --bench cache_profile --profile profiling
sudo ./target/profiling/cache_profile --output cache-profile.json
```

On Linux, use `perf stat` around an equivalent workload; the executable prints the suggested event list on unsupported hosts.
