# BAML codegen SDK test development

The SDK test system separates build-time code generation from test-time toolchain execution. This keeps normal workspace builds from installing npm, Python, Gradle, Go, or C++ dependencies while still making nextest exercise each generated SDK end to end.

## Lifecycle

1. `sdk_tests/crates/<target>/build.rs` calls `sdk_test_harness_setup::<target>::run_all()`.
2. The setup module loads shared BAML fixtures through `baml_project`, checks diagnostics, builds the codegen `SymbolPool` and bytecode, runs the target generator, stages `generated/`, overlays `customizable/`, and emits an `OUT_DIR/<target>_tests.rs` scaffold.
3. `sdk_tests/crates/<target>/src/lib.rs` invokes the matching macro from `sdk_test_harness_runner`, which includes that scaffold.
4. `cargo nextest run` executes the target's platform-specific `setup.sh` or `setup.ps1` binding from `.config/nextest.toml` before tests. The setup script builds the required bridge and prepares package-manager/build caches.
5. Scaffold tests call `sdk_test_harness_runner` helpers to run host-language gates from each fixture's `generated/` directory.

`sdk_test_harness_setup` is a build dependency and owns codegen/staging. `sdk_test_harness_runner` is a lightweight dev dependency and owns process execution, setup guards, generated ESM assertions, and the per-target `test_suite!()` macros.

## Target behavior

| Target | Staging and generated gates | Nextest setup responsibility |
|---|---|---|
| `python_pydantic2` | Symlinks customizable Python files; emits Ruff, Pyright, and pytest gates | Sync fixture environments and rebuild/install `baml_bridge` |
| `typescript` | Copies canonical tests into the Node tree; emits ESM, `tsc`, Vitest, and bridge `attw` gates | Build the native bridge and install fixture packages |
| `typescript_web` | Reads canonical tests from the sibling TypeScript target, copies/re-writes them into Web and Workers trees; emits ESM, `tsc`, and two Vitest gates | Build the Web/Wasm bridge, install packages, and install Chromium |
| `rust` | Generates a complete Cargo crate, symlinks ports into `generated/customizable/`, and writes a gated `tests/main.rs`; emits rustfmt, Clippy, and Cargo-test gates | Build the engine library and pre-warm the shared Cargo target directory |
| `java` | Copies Java tests into the Gradle test source tree; emits enabled `javac` and JUnit gates | Prepare the shared Gradle home and bridge inputs; Java tests are serialized by nextest |
| `go` | Generates supported shared fixtures plus synthetic `package_edges`; emits one `go_test` gate each | Build/stage the Go bridge prerequisites and cache |
| `cpp` | Symlinks customizable C++ sources and writes a per-fixture `test.sh`; emits independent compile and run gates | Build the `bridge_cffi` dynamic library |

The exact setup bindings and cache/test-group policies are authoritative in `.config/nextest.toml`. Unix and Windows setup scripts must remain behaviorally equivalent.

## Generated artifacts and diagnostics

Generated output is disposable and gitignored, except for lockfiles a target deliberately preserves for reproducibility. Do not hand-edit files under `generated/`; update the generator, harness template, or `customizable/` source instead.

Fixture discovery failures, empty BAML fixture trees, and compiler diagnostics are repository-author errors and fail the build. Several generators catch codegen/staging panics and record them in `$OUT_DIR/build_diagnostics.txt`; the generated `build_diagnostics::no_build_failures` test surfaces those records. Targets whose generator is expected to be complete may fail directly instead. Inspect the target's `harness_setup/src/<target>.rs` rather than assuming every target has identical soft-fail behavior.

Every target scaffold also includes a setup guard. The setup script appends its target-specific `SDK_TEST_<TARGET>_SETUP=1` breadcrumb to the file named by `$NEXTEST_ENV`; nextest injects it into that run's test processes. This proves the matching setup script ran during the current invocation instead of accepting a stale persistent marker. Plain `cargo test` does not provide the same clean setup guarantee.

## Port gating

Rust places hand-ported test files under `generated/customizable/`, not `generated/tests/`, so Cargo cannot auto-discover ports that are not ready. `TEST_MODS` in `harness_setup/src/rust.rs` writes the single `generated/tests/main.rs` module gate and records deferred rows as `LATER(...)` comments.

Java uses `GREEN_JAVAC_FIXTURES` and `GREEN_JUNIT_FIXTURES` in `harness_setup/src/java.rs`; non-enabled generated gates are emitted as ignored tests with the rollout reason.

Go deliberately uses an explicit source-fixture list and adds a synthetic `package_edges` fixture. Update those constants when expanding Go coverage.

## Adding a generator target

1. Add `sdk_tests/harness_setup/src/<target>.rs` with `run_all()` for fixture loading, codegen, staging, watch paths, diagnostics, setup guard, and scaffold emission; export it from `harness_setup/src/lib.rs`.
2. Add target command helpers or a `test_suite!()` macro to `sdk_tests/harness_runner/src/lib.rs`.
3. Add `sdk_tests/crates/<target>/{Cargo.toml,build.rs,src/lib.rs,setup.sh,setup.ps1}`. The generator crate uses `sdk_test_harness_setup` as a build dependency and `sdk_test_harness_runner` as a dev dependency.
4. Add Unix and Windows setup bindings filtered to `package(=sdk_test_<target>)` in `.config/nextest.toml`, including any serialization group required by a shared external cache.
5. Add customizable host tests for supported fixtures and document any explicit rollout gates.

Run the focused fixture gates while iterating, then the complete `sdk_test_<target>` crate through nextest before merging.
