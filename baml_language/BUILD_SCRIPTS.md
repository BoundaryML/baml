# Build-script integration audit

This audit inventories the 17 BAML-owned Cargo build scripts in the `baml_language` workspace at source revision `8372adbbc28054fd211379bc680005b5ba605ae9`. It describes current behavior and the remaining work for source imports that replace Cargo build scripts with declared build actions. The integration changes recommended below are not yet implemented. See [Vendoring BAML](VENDOR.md) for runtime providers, feature selection, and dependency verification.

## Scope and findings

The combined normal and build dependency graphs of `baml_cli` and `bridge_python` contain 13 of these scripts. The same 13 remain with default features disabled and `no-phone-home,bridge_python/bundle-http` enabled. Individually, the reduced CLI includes 11 and the reduced Python bridge includes 12. These counts include target-conditional paths inspected with `--target all`; they exclude development dependencies. Replacing providers can change the third-party build dependencies and requires a fresh graph review.

Two current behaviors need changes before an integrator can use a supplied protobuf compiler and an immutable source tree without local patches:

1. `bridge_ctypes`, `bex_events`, `btel_bcs`, and `btel_recorder` select `protoc-bin-vendored` unconditionally. Setting `PROTOC` does not override their selection. Disabling `baml-defaults` does not remove this build dependency or its platform binary packages.
2. `bridge_ctypes` writes generated Rust and Python SDK files into sibling source directories during an ordinary build. These writes require a writable checkout and the SDK directory layout, including SDKs other than the artifact being built. Its Rust output under `OUT_DIR` is the only generated output consumed by that Rust crate.

The other production steps generate Rust code, compile an embedded standard-library artifact, compress checked-in sources, or emit build metadata and linker settings. They can be represented as host build actions or explicit compiler/linker configuration. No direct network acquisition was found in the BAML-owned scripts or their BAML code-generation helpers. This is a source audit, not a network sandbox test or a blanket statement about third-party build scripts, Cargo, SDK packaging, or replacement providers.

## Scripts in the Python and CLI graphs

Paths in this table are relative to `baml_language`. Generated files are under the owning crate's `OUT_DIR` unless another location is stated. Build actions run on the build host; target triples and target-specific linker settings describe the artifact being produced.

| Script | Selected artifact | Inputs and tools | Outputs and side effects |
|---|---|---|---|
| [`baml_artifact`](crates/baml_artifact/build.rs) | Python and CLI | `BAML_GIT_SHA`, or Git checkout identity; invokes `git` to inspect HEAD and its reference even when the environment override is supplied | Emits `BAML_ARTIFACT_BUILD_COMMIT`. No generated files. Development and canary compilation requires a commit identity. |
| [`bex_vm_types`](crates/bex_vm_types/build.rs) | Python and CLI | Embedded stdlib sources and `baml_builtins2_codegen` | `sys_op_generated.rs`, `errors_generated.rs`, `panics_generated.rs` |
| [`sys_types`](crates/sys_types/build.rs) | Python and CLI | Embedded stdlib sources and `baml_builtins2_codegen` | `io_generated.rs`, `runtime_io.rs` |
| [`sys_ops`](crates/sys_ops/build.rs) | Python and CLI | Embedded stdlib sources and `baml_builtins2_codegen` | `io_generated.rs`, `io_adapter.rs` |
| [`bex_vm`](crates/bex_vm/build.rs) | Python and CLI | Embedded `baml`, `ai`, and `reflect` package sources and `baml_builtins2_codegen` | `nativefunctions_generated.rs`, `aifunctions_generated.rs`, `reflectfunctions_generated.rs` |
| [`bex_events`](crates/bex_events/build.rs) | Python and CLI | `src/value/proto/bamlvalue.proto`, Prost, vendored `protoc` | `baml.value.v1.rs`; overrides `PROTOC` in the build-script process |
| [`btel_bcs`](crates/btel_bcs/build.rs) | Python and CLI | `proto/cloud.proto`, Prost, vendored `protoc` | `btel.cloud.v1.rs`; explicitly sets Prost's compiler executable |
| [`btel_recorder`](crates/btel_recorder/build.rs) | Python and CLI | `proto/recording.proto`, Prost, vendored `protoc` | `baml.btel.recording.v2.rs`; explicitly sets Prost's compiler executable |
| [`bridge_ctypes`](crates/bridge_ctypes/build.rs) | Python and CLI | Sorted `types/**/*.proto`, Prost, vendored `protoc` with Python and `.pyi` generation | `baml_bridge.cffi.v1.rs`; also overwrites `sdks/rust/bridge_rust/src/wire/baml_bridge.cffi.v1.rs` and Python `*_pb2.py` / `*_pb2.pyi` files under `sdks/python/src/baml_bridge/cffi/v1/`; overrides `PROTOC` |
| [`bex_project`](crates/bex_project/build.rs) | Python and CLI | The BAML compiler built for the host, embedded stdlib, BAML version/channel, and `src/precompiled_stdlib_config.rs` | `stdlib_prefix.borsh`, containing package interfaces and bytecode compiled at optimization level one; no subprocess or network request in the script |
| [`sdkgen_cpp`](sdks/cpp/sdkgen_cpp/build.rs) | CLI | Eight checked-in `.pb.h` / `.pb.cc` files for `baml_handle`, `baml_inbound`, `baml_outbound`, and `baml_type` under `sdks/cpp/bridge_cpp/pb/baml_bridge/cffi/v1/`; `flate2` | Eight files with `.gz` appended to their names. Does not invoke `protoc` or a C++ compiler. Its `protoc-bin-vendored` dependency is test-only. |
| [`bridge_cffi`](crates/bridge_cffi/build.rs) | Python | Cargo `TARGET`; watches `cbindgen.toml`, `src/lib.rs`, and the script | Emits `BAML_CFFI_TARGET`. Does not generate a header or invoke cbindgen. |
| [`bridge_python`](sdks/python/rust/bridge_python/build.rs) | Python | Target, Rust compiler, and Python configuration through `pyo3-build-config` | Emits extension-module and Python library rpath linker arguments. No BAML-generated files. The downstream Python toolchain must supply the matching PyO3 configuration. |

The builtin generators read sources embedded by [`baml_builtins2`](crates/baml_builtins2/src/lib.rs), including the package source and manifest files under `crates/baml_builtins2/baml_std/`. They parse these through BAML's compiler libraries and format generated Rust with `syn` and `prettyplease`; they do not invoke an installed BAML CLI or `rustfmt`.

The stdlib artifact is a compiler output, not a checked-in data file that can be reused across arbitrary compiler changes. A replacement build action must depend on the compiler and serialization sources, embedded stdlib, version metadata, and optimization configuration. Produce its interfaces and bytecode together from the same host compiler build. The consumer checks a version/channel/optimization key, which does not substitute for declaring those dependencies in the build system.

## Other workspace scripts

These scripts are outside both selected production dependency graphs. They matter if the corresponding SDK, executable, or test suite is imported.

| Script | Inputs and tools | Outputs and side effects |
|---|---|---|
| [`baml_pack_host`](crates/baml_pack_host/build.rs) | Target operating system | Adds `-Wl,-headerpad,0x300` on macOS so packed data can be embedded in the executable. No file generation. |
| [`bridge_typescript`](sdks/typescript/bridge_typescript/build.rs) | Target and environment through `napi-build` | Target-specific linker configuration plus macOS dynamic lookup. The pinned N-API helper can write an Android `libgcc.a` linker script under `OUT_DIR` and inspect platform toolchain configuration. This script does not run the separate Node/protobuf SDK generation commands. |
| [`bridge_wasm`](crates/bridge_wasm/build.rs) | Wall clock, Git checkout, `BRIDGE_WASM_FORCE_RERUN` | Emits `BRIDGE_WASM_BUILD_TS` and `BRIDGE_WASM_GIT_SHA`. The timestamp prevents byte-for-byte reproducibility when the script reruns; Git metadata has no explicit override here. |
| [`baml_tests`](crates/baml_tests/build.rs) | Compiler and stdlib, `projects/`, `build_stdlib_prefix_config.rs`, and `tools/speedtest/` within this workspace; invokes `PYTHON3` or `python3` for benchmark export | Generates `stdlib_prefix.borsh`, `speedtest_benches.rs`, and `speedtest_profiling_sources.rs` under `OUT_DIR`, and writes `src/generated_tests.rs` into the source tree. Missing benchmark inputs or Python produce warnings and empty benchmark outputs. |

Third-party crates and in-tree third-party forks are outside this BAML-owned script inventory. Inspect their normal and build dependency graphs with the final provider and feature selection. In particular, making BAML's own protobuf compiler optional would not by itself remove every third-party build script.

## Recommended integration changes

### Supplied protobuf compiler

Use one selection policy across the four protobuf scripts: honor an explicit `PROTOC` first; retain the bundled compiler as a developer convenience under `baml-defaults`; when defaults are disabled, use the supplied compiler or `protoc` on the host tool path. An invalid explicit path must fail with a useful error rather than silently selecting another compiler. Pass the selected executable to Prost and any explicit protobuf command without mutating process-wide environment variables.

Make `protoc-bin-vendored` an optional build dependency and propagate the existing umbrella feature through every dependency path that uses it. An environment override alone is insufficient for vendoring because the binary packages would remain in the build graph. Track compiler selection changes and schema inputs for rebuilds. No network acquisition should be introduced as a fallback.

### Declared generation outputs

Move the committed Rust and Python SDK regeneration out of `bridge_ctypes/build.rs` into an explicit maintenance command. Normal compilation should write only the crate's required generated Rust file under `OUT_DIR`. Update the existing [protobuf sync job](../.github/workflows/ci.yaml) and [regeneration instructions](crates/bridge_ctypes/README.md) to invoke the maintenance command, retaining the committed SDK files and drift checks.

For Blaze or another build system that does not run Cargo scripts, declare protobuf, builtin Rust generation, stdlib compilation, and C++ source compression as host actions with the inputs and outputs listed above. Compile the generated Rust with the same Prost configuration and compatible runtime dependency as the consuming crate. Supply each crate's output directory for its `include!` / `include_bytes!` expressions, and reproduce the emitted environment values and target-specific linker settings. There is currently no unified BAML command that exports all these outputs for an external build system.

### Build identity and reproducibility

Retain `BAML_GIT_SHA` as the source-import identity input and avoid invoking Git when it is explicitly supplied. A build system replacing the script can directly provide the equivalent `BAML_ARTIFACT_BUILD_COMMIT`; it must preserve the validation and development/canary fingerprint requirement. Keep producer and consumer source identities consistent and retain downstream patches alongside the upstream revision.

The WebAssembly timestamp and test-suite source writes are separate follow-ups if those artifacts enter the import scope. They do not block the requested Python/CLI integration. No blanket removal of build scripts is necessary: their generation and configuration contracts need to be explicit and reproducible in the downstream build system.

## Verification and remaining evidence

The audit inspected every workspace `custom-build` target reported by Cargo metadata, the source of its script and relevant BAML generation helpers, and the selected normal/build dependency graphs. Reproduce the inventory from `baml_language/` with:

```sh
cargo metadata --locked --no-deps --format-version 1
cargo tree --locked -p baml_cli -p bridge_python --target all -e normal,build
cargo tree --locked -p baml_cli -p bridge_python --target all -e normal,build --no-default-features --features no-phone-home,bridge_python/bundle-http
```

The counts and findings above are source and dependency analysis, not a completed Blaze build or an immutable-source build test. Acceptance of the recommended changes requires an actual Python/CLI build using a supplied `protoc`, no `protoc-bin-vendored*` packages in the selected reduced normal/build graph, no source-tree writes during ordinary compilation, and successful Python import and representative CLI operations. Separately verify that the explicit SDK generation command preserves the existing generated files and that default developer builds still work. External build rules must be validated in the integrator's environment.
