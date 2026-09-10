# Size-gate R2 cache audit (2026-09-10)

The nested release builds do use sccache, and their requests reach the same server as the helper build. Previously the reported counters combined helper and release compilation, while the uploaded Cargo timings covered only the helper. This change separates those phases and records timings for the actual release builds, so a high aggregate cache hit rate cannot obscure expensive uncached LTO/linking.

## Build and stats path

1. CI loads `.envrc` via direnv. It sets `RUSTC_WRAPPER`, the R2 bucket/endpoint/prefix, and the server configuration. Windows first builds its native credential-mapping wrapper.
2. Previously `cargo run --timings -p cargo-size-gate -- size-gate check ...` built the debug helper and ran it. `--timings` applied only to that outer build.
3. The helper's `run_cargo_build` uses `Command::new("cargo")` for the CLI/WASM release build; `build_pack` does the same for the CLI and pack host. Neither clears the environment or removes the wrapper/server variables. Those child processes inherit the cache configuration.
4. sccache statistics belong to the server, not to the process asking for them. The final `sccache --show-stats` therefore included both phases. The report-only job has its own runner/server and its helper-only counters must not be confused with measurement-job counters.

The updated workflow builds the helper explicitly, prints its cache counters, resets counters through the credential-mapping wrapper, and invokes the prebuilt helper. End-of-measurement stats now exclude helper/bootstrap compilation. Stats/timing uploads require the reset step to succeed, so a failed helper build cannot masquerade as release-build stats. The wrapper is used for reset so an already-built helper still starts a server with the right credentials.

Both nested release build commands now pass `--timings`. Measurement jobs clear old timing reports before starting. Their timing artifacts therefore describe the nested release invocations rather than helper compilation. Compiler profiles, R2 settings, GitHub cache policy, size baselines, and report enforcement are unchanged.

## Direct reproduction

Built the actual `cargo-size-gate` source, then ran that prebuilt binary against a temporary Cargo workspace with a library and binary, using the production release settings (`lto="fat"`, `codegen-units=1`, `opt-level="s"`, stripped symbols). An isolated sccache 0.10.0 server used a temporary local disk cache; no R2 credentials were supplied. A tracing wrapper recorded the compiler arguments and inherited wrapper/server variables.

| Probe | Requests | Rust hits | Rust misses | Non-cacheable binary calls |
| --- | ---: | ---: | ---: | ---: |
| Cold nested release build | 8 | 0 | 1 | 1 |
| Delete target directory, repeat (cumulative) | 16 | 1 | 1 | 2 |
| Reset counters, delete target, original `cargo run` entry point with helper already built | 8 | 1 | 0 | 1 |

The helper was already compiled and was invoked directly, so none of these requests could come from building it. Both release compiler calls used the inherited wrapper and the same socket. The library invocation had `linker-plugin-lto`; the binary invocation had `lto=fat` and was classified as non-cacheable (`crate-type`). The second build hit for the library but still rebuilt/linked the executable. Repeating through the original `cargo run` entry point also hit the nested library, confirming that outer Cargo does not drop the wrapper/server environment. The modified nested build produced a Cargo timing HTML report.

A separate isolated compiler probe confirmed that identical inputs hit after deleting outputs, while source changes, tracked `env!` changes, and optimization-flag changes each miss. These are local transport tests of the pinned sccache version, not a byte-for-byte cached-versus-uncached BAML release comparison.

## Existing CI evidence (combined counters before this change)

Every job below reports `s3, name: baml-build3, prefix: /baml/ci/` and zero cache read errors, write errors, and timeouts. Hits/misses count cacheable Rust compilations, not all compiler work.

| Run | Job | R2 hits | R2 misses | Non-cacheable calls |
| --- | --- | ---: | ---: | ---: |
| [Canary 34499800289](https://github.com/BoundaryML/baml/actions/runs/34499800289) | [Linux measurement](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645574) | 708 | 1 | 143 |
| Same | [macOS measurement](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645723) | 713 | 1 | 144 |
| Same | [Windows measurement](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645605) | 640 | 77 | 133 |
| Same | [WASM measurement](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102947645501) | 449 | 0 | 104 |
| Same | [Report helper only](https://github.com/BoundaryML/baml/actions/runs/34499800289/job/102958675002) | 41 | 0 | 16 |
| [PR 34512477133](https://github.com/BoundaryML/baml/actions/runs/34512477133) | [Linux measurement](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973154) | 689 | 20 | 143 |
| Same | [macOS measurement](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973209) | 689 | 25 | 144 |
| Same | [Windows measurement](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973217) | 618 | 99 | 133 |
| Same | [WASM measurement](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/102989973274) | 433 | 16 | 104 |
| Same | [Report helper only](https://github.com/BoundaryML/baml/actions/runs/34512477133/job/103001956988) | 41 | 0 | 16 |

The canary Linux log says the debug helper finished in 4.31s, the nested CLI release build in 7m43s, and the nested CLI/pack-host release build in 7m10s. The latter still compiles/links the two shipping executables. Fat LTO executes inside the uncached binary compilation, so the high hit rate does not imply that most CPU work is cached. New nested timing reports are needed to quantify that work; the previous helper-only timing artifact cannot do so. Likewise Cargo's `Compiling` log messages alone do not establish sccache misses.

The first canary push after the repository-secret change, [run 33797930291](https://github.com/BoundaryML/baml/actions/runs/33797930291), already showed Linux 703/1 hits/misses, macOS 708/1, Windows 635/77, and WASM 443/0, all using `baml-build3` with zero read/write errors. Native policy violations in the September 10 canary run are reported in freshly produced JSON and are separate from cache transport. The daily baseline-refresh run failed while enqueueing its PR, after successfully adopting reports; this change does not modify that workflow or its baselines.

## R2 configuration, keys, and branch behavior

[PR #4685](https://github.com/BoundaryML/baml/pull/4685) moved the cache from `baml-build1` in the old account to `baml-build3` at `https://c0bf62c013f607c849c6a50cdd627b7b.r2.cloudflarestorage.com`, region `auto`. This selects a new remote namespace. HTTPS and `auto` match [sccache's R2 requirements](https://github.com/mozilla/sccache/blob/v0.10.0/docs/S3.md). [PR #4738](https://github.com/BoundaryML/baml/pull/4738) moved credentials from the GitHub deployment environment to optional reusable-workflow repository secrets. Current callers pass both credential names, and the sampled logs demonstrate actual access to the new bucket.

`.envrc` enables R2 only when both `BAML_SCCACHE_R2_*` credentials are present. POSIX `tools/baml-sccache` and Windows `tools_sccache` map them to `AWS_*` before starting sccache. CI reads and writes objects under `baml/ci/`; local developer builds use `baml/local/`. R2 has no GitHub branch scope: credentialed PR, canary, main and merge-group jobs can share compiler objects. Fork PRs without secrets fall back to local disk; cross-run compiler reuse is not expected there.

The [pinned sccache key construction](https://github.com/mozilla/sccache/blob/v0.10.0/src/compiler/rust.rs) hashes compiler identity, compiler arguments, source and dependency contents, tracked environment, and working directory. Sharing an R2 namespace therefore does not mean sharing outputs across different source/profile/target/features. Linked executables and cdylibs are not cached by this version. `.envrc` exports `SCCACHE_BASEDIRS`, but version 0.10.0 does not implement it: different checkout paths can cause extra misses. Hosted paths are stable in the samples; cross-path reuse must not be assumed.

Swatinem/rust-cache is secondary and caches Cargo downloads, not `target/`. All sampled measurement jobs miss it because its environment keys differ from cargo-test writers (and the macOS writer is disabled). This is missed dependency-download caching, not evidence that the nested release builds bypass R2. No Swatinem writes or key changes are introduced here. The daily GitHub cache purge does not delete R2 compiler objects. mise's tool cache and uploaded size-report artifacts are also separate from R2.

Validation passed: all 11 `cargo-size-gate` tests, package clippy with warnings denied, rustfmt, actionlint, and parsed-workflow assertions for build/reset/measurement order, reset-gated diagnostics, and unchanged Swatinem inputs.

## Limits

No evidence of stale compiled output reuse was found. This does not prove byte-for-byte output equivalence for a full BAML uncached rebuild, audit every procedural macro's undeclared inputs, or inspect R2 credential permissions/lifecycle settings. [Upstream documents caveats for procedural macros reading untracked files](https://github.com/mozilla/sccache/blob/v0.10.0/docs/Rust.md). Existing CI counters combine phases and cannot precisely allocate hits between individual nested builds; the new phase reset and timings make that distinction observable. A fresh fork runner path has not been exercised. The ix preview was also inspected at the original audit revision; it inherited the same wrapper/server settings and retained seeded build output, but was removed from canary in [#4819](https://github.com/BoundaryML/baml/pull/4819). No ix workflow changes remain in this PR. No bucket/cache purge, baseline update, or merge was performed.
