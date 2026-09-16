# BAML Codegen SDK Test Development

Each generator crate compiles the shared fixture corpus through the full
compiler pipeline (parse -> HIR -> TIR -> `SymbolPool` -> emitter), mirroring
the path `baml generate` takes end-to-end, and installs the result as a real
SDK the host language's toolchain then builds and tests.

That codegen runs from the crate's `setup.sh` (Unix) / `setup.ps1` (Windows),
alongside the toolchain install. **No generator has a `build.rs`.** Nothing
under `sdk_tests/` is generated during an ordinary `cargo build`, `cargo check`
or `cargo clippy`.

`cargo nextest run` fires the matching setup script automatically via
platform-filtered (`cfg(unix)` / `cfg(windows)`) setup-script bindings in
[`baml_language/.config/nextest.toml`](../.config/nextest.toml). Plain
`cargo test` does not, and the `setup_guard::ran` test fails when it hasn't —
see [setup.sh guard](#setupsh-guard-setup_guardran).

## Why codegen is not a build script

A build script's dependencies are *built*, never merely checked, so a
`[build-dependencies]` edge on the codegen crate pulled its whole compiler and
sdkgen closure into every `cargo check` of the workspace — 26 crates compiled
to object code, plus nine build-script executions, on every edit to any
compiler crate. Running codegen from `setup.sh` instead removes both, and
removes the invalidation channel entirely: a compiler edit can no longer
trigger SDK regeneration, because nothing regenerates during a build.

The cost is that the set of `#[test]`s can no longer be generated. Setup
scripts run *after* nextest has compiled the test binaries, so a scaffold
written at setup time would be too late to compile. Each generator crate
therefore declares its fixtures in source — see below.

## The two owners

Everything written into a fixture's `generated/` tree has exactly one owner, so
nothing has to be wiped to stay correct:

- **`write_codegen_output`** owns the emitted SDK subtree, through the same
  filesystem transaction `baml generate` uses. It removes files it previously
  owned and no longer emits, and skips the install outright when the tree
  already matches byte-for-byte.
- **`Overlay`** owns everything else the generator writes: the `customizable/`
  overlay of ported tests, and the per-fixture scaffolding (`package.json`,
  `go.mod`, `Package.swift`, …). It records what it staged in
  `<fixture>/.baml-overlay` so the next run removes exactly what the last one
  left, and materializes only entries whose bytes differ.

The practical consequence: **an unchanged regeneration touches zero files.**
Downstream toolchains — cargo, uv, pnpm, gradle, SwiftPM, MSBuild — all key
their own caches on mtime, so a wipe-and-restage would invalidate every one of
them on every run.

Rust is the single exception, documented in `codegen/src/rust.rs`: its overlay
lands *inside* the output writer's own tree, and the writer refuses to run over
symlinks it does not own, so those links are cleared and re-staged each run.
That costs nothing — re-creating a symlink leaves the file it points at, and
therefore what cargo fingerprints, untouched.

## Directory Structure

```text
sdk_tests/
|-- codegen/                              # codegen crate (heavy deps: sdkgen_*, baml_db, baml_ide, ...)
|   |-- Cargo.toml                        # name = "sdk_test_codegen"; lib + bin
|   `-- src/
|       |-- lib.rs                        # CodegenCtx, load_fixture, write_codegen_output, Overlay
|       |-- main.rs                       # the `sdk_test_codegen` CLI; GENERATORS table
|       |-- <generator>.rs                # one per target: run_all(&CodegenCtx)
|       `-- templates/                    # per-fixture scaffolding, as real files
|-- harness_runner/                       # test-side crate (std only)
|   |-- Cargo.toml                        # name = "sdk_test_harness_runner"
|   `-- src/
|       |-- lib.rs                        # run_test_cmd + per-generator test_suite!() macros
|       `-- fixtures.rs                   # SHARED corpus table + the manifest oracle
|-- fixtures/<fixture>/baml_src/          # generator-agnostic input only -- .baml and nothing else
`-- crates/                               # one crate per generator target
    `-- <generator>/
        |-- Cargo.toml                    # [dev-dependencies] sdk_test_harness_runner -- no build-deps
        |-- setup.sh                      # runs `sdk_test_codegen <generator>`, then the toolchain
        |-- setup.ps1                     # parallel script for Windows; nextest picks one by host cfg
        |-- src/lib.rs                    # declares the suite: test_suite! { fixture <name>; ... }
        `-- <fixture>/
            |-- customizable/             # ported tests, tracked
            |-- .baml-overlay             # what the last run staged, gitignored
            `-- generated/                # codegen output, gitignored
```

C# is the exception to the corpus layout: its fixtures are whole BAML projects
living in-crate (`crates/csharp/<fixture>/baml.toml` + `baml_src/` + a
hand-written `Program.cs` consumer), so it reads nothing from `fixtures/`.

## How It Works

1. **`crates/<generator>/setup.sh`** runs
   `cargo run -p sdk_test_codegen -- <generator>` first, then the toolchain
   steps. Codegen must come first: every later step builds against the
   generated tree, and each script's per-fixture loop silently skips a fixture
   whose `generated/` is missing.

2. **`sdk_test_codegen::<generator>::run_all(&CodegenCtx)`** then, per fixture:
   - loads the `.baml` files into a `ProjectDatabase` and gates on
     `Severity::Error` diagnostics;
   - builds the codegen `SymbolPool` and calls the target's
     `to_source_code_with_bytecode(...)`;
   - installs that output via `write_codegen_output`; and
   - stages the `customizable/` overlay plus the per-fixture scaffolding
     through an `Overlay`.

   `CodegenCtx` carries the two roots (`fixtures_root`, `crate_dir`), so
   generators never consult `CARGO_MANIFEST_DIR` themselves — inside the driver
   that would name the driver's own directory.

   Overlays are symlinked where the language tolerates it (C++, Python, Rust),
   so editing a ported test is picked up without re-staging, and copied where it
   does not: Node follows symlinks during module resolution and would resolve
   `node_modules` from `customizable/`, which has none; Gradle and SwiftPM
   resolve target membership by path.

3. **`crates/<generator>/src/lib.rs`** declares the suite:

   ```rust
   #[cfg(test)]
   sdk_test_harness_runner::cpp::test_suite! {
       fixture docstrings_etc;
       fixture function_calls;
       fixture llm_functions;
       fixture type_shapes;
       fixture unsupported_only;
   }
   ```

   That expands to one `mod <fixture>` per row holding the generator's
   toolchain checks, plus `setup_guard::ran` and
   `fixture_manifest::matches_corpus`. Some generators take richer rows — Java
   marks each gate `on` or `later`, Swift takes an optional
   `later "<reason>"`, Go distinguishes `fixture` from `synthetic` — see each
   macro's docs.

4. **The fixture rows are pinned.** `fixture_manifest!` emits a test asserting
   that a crate's declared rows, the `fixtures::SHARED` table, and the corpus on
   disk all agree. Adding a fixture is `mkdir fixtures/<name>/baml_src/` plus a
   row in `fixtures::SHARED` and in each `crates/*/src/lib.rs`; miss one and
   that test fails, naming the file to edit. `sdk_test_codegen` asserts the same
   thing at startup, so drift aborts the setup script rather than waiting for a
   test.

5. **The per-fixture tests** call `run_test_cmd(fixture, cmd, cache_subdir,
   cache_env_var)`, which `cd`s into
   `sdk_tests/crates/<generator>/<fixture>/generated/`, threads the toolchain
   cache env var (`UV_CACHE_DIR`, `npm_config_store_dir`, `GRADLE_USER_HOME`,
   `CARGO_TARGET_DIR`), and spawns `cmd`. The `uv` invocation falls back to
   `mise which uv` if `uv` isn't on PATH.

### Failures are loud

There is no soft-fail path. A codegen panic, a failed install, or a fixture
with `Severity::Error` diagnostics aborts the driver, which fails `setup.sh`
under `set -e`, which nextest reports as a `SETUP FAIL` with the panic message
attached.

This used to be routed through a `$OUT_DIR/build_diagnostics.txt` record and a
`build_diagnostics::no_build_failures` test, so that `cargo check` stayed green
on a machine without the SDK toolchains. Codegen no longer runs during `cargo
check`, so that reason is gone.

### setup.sh guard (`setup_guard::ran`)

Since `setup.sh` is what generates each fixture's SDK *and* installs the
toolchain, a run where it did not fire would test a stale tree or none at all.
Each suite emits a `mod setup_guard { #[test] fn ran }` that fails unless the
script ran **this** run.

**Breadcrumb format.** At the end of each run, the generator's setup script
appends a single line to the file named by nextest's `$NEXTEST_ENV`:

```text
SDK_TEST_<GEN>_SETUP=1
```

`<GEN>` is the upper-cased generator key — `SDK_TEST_PYTHON_PYDANTIC2_SETUP=1`,
`SDK_TEST_TYPESCRIPT_SETUP=1`. The canonical name is the literal in that
generator's `test_suite!` macro in `harness_runner/src/lib.rs`; the setup
scripts must agree with it. nextest reads that file after the setup script and
injects the var into the matched tests' processes for that run only, so
presence of the var proves the script ran *this* invocation.

It is deliberately an env var via `$NEXTEST_ENV` rather than a file marker: a
file would persist across runs and false-pass after the `.so` / `node_modules`
went stale, and checking `NEXTEST=1` alone would only prove "under nextest",
not "this script ran". Plain `cargo test` sets neither, so the guard fails
there — that is the intended answer, not a gap.

Swift is `#[cfg(target_os = "macos")]`-gated: its nextest binding is host-gated,
so off-macOS no setup script runs and the fixture tests are `#[ignore]`d to
match. Its `fixture_manifest` test stays live on every host, since it only reads
the corpus.

## Adding a Generator Target

1. Add `sdk_tests/codegen/src/<target>.rs` with
   `pub fn run_all(ctx: &CodegenCtx)`: iterate `fixtures::SHARED`, install
   codegen output through `write_codegen_output`, and stage the overlay and
   scaffolding through an `Overlay`. Assert the corpus matches `SHARED` first.

2. Register it in the `GENERATORS` table in `sdk_tests/codegen/src/main.rs`.

3. Add a `pub mod <target>` to `sdk_tests/harness_runner/src/lib.rs` holding a
   `#[macro_export] macro_rules! <target>_test_suite` that takes
   `fixture <name>;` rows, re-exported as `test_suite`. It should emit
   `setup_guard!`, `fixture_manifest!`, and one `mod <fixture>` per row. Test
   bodies live here, not in the codegen crate.

4. Add `sdk_tests/crates/<target>/{Cargo.toml,src/lib.rs,setup.sh,setup.ps1}`,
   following `crates/cpp/`'s shape. `Cargo.toml` wires
   `sdk_test_harness_runner` as a `[dev-dependencies]` and has **no**
   `[build-dependencies]` and no `build.rs`. `setup.sh` runs
   `cargo run -p sdk_test_codegen -- <target>` before anything else, and
   appends its `SDK_TEST_<GEN>_SETUP=1` breadcrumb to `$NEXTEST_ENV` at the end.

5. Add a platform-filtered setup-script binding for the crate in
   `.config/nextest.toml`.

6. For each fixture that should run under this target, drop a
   `sdk_tests/crates/<target>/<fixture>/customizable/` directory containing the
   host-language tests.
