# BAML Codegen SDK Tests

E2E test crates for BAML code generation. The unit of cargo
packaging is the **generator target** (one Rust crate per target,
e.g. `sdk_test_python_pydantic2`), which fans out internally over
every **fixture** under `sdk_tests/fixtures/`. Each fixture is a
single `baml_src/` tree shared across every target — language-
agnostic, since the same `.baml` source should generate consistent
SDKs no matter the output language.

Codegen runs in each crate's `build.rs` via the full
`baml_project::build_symbol_pool` pipeline (parse → HIR → TIR →
`SymbolPool` → emitter), mirroring the path `baml-cli generate`
takes end-to-end. python_pydantic2's build.rs also runs `uv sync`
once per fixture. nodejs_typescript splits that out: the
per-fixture `pnpm install` and the prereq `pnpm build:debug` of
bridge_nodejs's native `.node` addon live in
[`crates/nodejs_typescript/setup.sh`](./crates/nodejs_typescript/setup.sh) —
run it once after `cargo test --no-run` (or `cargo build`).
Build-script failures (missing tool, codegen panic, install
non-zero exit, codegen file write errors) are recorded to
`$OUT_DIR/build_diagnostics.txt` rather than aborted, and surface
as a `build_diagnostics::no_build_failures` test (see
[Soft-fail build.rs](#soft-fail-buildrs) below). Each build also
emits a `#[test]` scaffold under `OUT_DIR` with one test per
toolchain check per fixture, producing a `cargo test` matrix of
`(fixture × check)` per crate.

The shared infrastructure is split into two crates so the heavy
codegen + project-loading deps only land where they're needed:

- **`sdk_test_harness_setup`** (`[build-dependencies]`) holds the build.rs
  logic — fixture discovery, codegen, install, scaffold emission,
  `BuildDiagnostics`. Depends on `codegen_python`, `codegen_nodejs`,
  `baml_project`, `baml_db`, `baml_workspace`, `baml_codegen_types`.
- **`sdk_test_harness_runner`** (`[dev-dependencies]`) holds every emitted
  test's runtime side — `run_test_cmd` / `run_test_cmd_with_env`,
  the per-generator `<generator>::test_suite!()` macros that
  `include!` each OUT_DIR scaffold, and the shared
  `build_diagnostics!` macro that emits the
  `mod build_diagnostics { #[test] fn no_build_failures }` block.
  Only `std` deps. The scaffold emitted by `sdk_test_harness_setup` is just
  a sequence of macro / function invocations against
  `::sdk_test_harness_runner::*` — every generated `#[test]` body, including
  `no_build_failures`, lives in `sdk_test_harness_runner`.

Targets in tree:
- **`sdk_test_python_pydantic2`** — Python + pydantic2. Three
  checks per fixture: `ruff`, `pyright`, `pytest` (plus the shared
  `build_diagnostics` test).
- **`sdk_test_nodejs_typescript`** — TypeScript on Node.js. Two
  checks per fixture: `tsc` (`node node_modules/typescript/bin/tsc
  --noEmit`) and `jest` (`node node_modules/jest/bin/jest.js`).
  Every test in this crate — `tsc`, `jest`, and
  `build_diagnostics` — is `#[ignore]`d while
  [`codegen_nodejs`](../sdks/nodejs/codegen_nodejs/) is a stub that
  panics: every fixture records a `codegen` diagnostic and
  `baml_sdk/` stays empty, so the toolchain commands can't pass.
  Drop the `#[ignore]`s when the emitter lands (`IGNORE_REASON` in
  `sdk_tests/harness_setup/src/nodejs_typescript.rs`). The native
  `.node` addon build and per-fixture `pnpm install` live in
  [`crates/nodejs_typescript/setup.sh`](./crates/nodejs_typescript/setup.sh)
  rather than build.rs. `cargo nextest run` invokes setup.sh
  automatically (configured as a nextest setup-script binding in
  [`baml_language/.config/nextest.toml`](../.config/nextest.toml)),
  so the common run flow is just:

  ```bash
  cargo nextest run -p sdk_test_nodejs_typescript
  ```

  For plain `cargo test`, run setup.sh manually between
  `cargo test --no-run` and `cargo test`. Re-run after bridge_nodejs
  Rust changes or after adding a new fixture.

```bash
# Run every Python+pydantic2 test across all fixtures
cargo test -p sdk_test_python_pydantic2

# Run just the pyright check for one fixture
cargo test -p sdk_test_python_pydantic2 type_shapes::pyright

# Surface any build-script diagnostics (missing uv, codegen panic, install fail)
cargo test -p sdk_test_python_pydantic2 build_diagnostics

# List discovered tests (without running)
cargo test -p sdk_test_python_pydantic2 -- --list

# Run ignored nodejs_typescript tests too (local debugging of codegen_nodejs)
cargo test -p sdk_test_nodejs_typescript -- --ignored
```

## Directory Structure

`sdk_tests/fixtures/<fixture>/` holds the *generator-agnostic*
input — a `baml_src/` tree and nothing else. Per-generator test
artifacts (host-language overlays + codegen + install output) live
under the owning generator crate at
`sdk_tests/crates/<generator>/<fixture>/`. One `<fixture>/`
subdirectory per fixture per generator crate, each grouping its
`customizable/` and `generated/` together so removing a fixture
under one generator is a single `rm -r`.

```text
sdk_tests/
├── harness_setup/                        # build-script crate (heavy deps: codegen_*, baml_project, …)
│   ├── Cargo.toml                        # name = "sdk_test_harness_setup"
│   └── src/
│       ├── lib.rs                        # generator-agnostic helpers + BuildDiagnostics
│       ├── python_pydantic2.rs           # python+pydantic2 codegen + scaffold emit (run_all)
│       └── nodejs_typescript.rs          # nodejs+typescript codegen + scaffold emit
├── harness_runner/                       # test-side crate (std only)
│   ├── Cargo.toml                        # name = "sdk_test_harness_runner"
│   └── src/
│       └── lib.rs                        # run_test_cmd + build_diagnostics! macro
│                                         #   + per-generator <gen>::test_suite!() macros
├── fixtures/                             # generator-agnostic input only — baml_src/ and nothing else
│   ├── docstrings_etc/baml_src/          # .baml source (input to every generator)
│   ├── llm_functions/baml_src/
│   └── type_shapes/baml_src/
└── crates/                               # one crate per generator target; per-fixture content nested inside
    ├── python_pydantic2/
    │   ├── Cargo.toml                    # name = "sdk_test_python_pydantic2"
    │   │                                 # [build-dependencies] sdk_test_harness_setup
    │   │                                 # [dev-dependencies]   sdk_test_harness_runner
    │   ├── build.rs                      # one-liner → sdk_test_harness_setup::python_pydantic2::run_all()
    │   ├── src/lib.rs                    # invokes sdk_test_harness_runner::python_pydantic2::test_suite!()
    │   ├── docstrings_etc/
    │   │   ├── customizable/             # tracked: *.py — symlinked into generated/
    │   │   └── generated/                # gitignored: build output
    │   │       ├── baml_sdk/             # codegen output
    │   │       ├── pyproject.toml        # name = "sdk-tests-python-pydantic2-docstrings-etc"
    │   │       ├── .venv/                # uv sync output
    │   │       └── *.py                  # symlinked from ../customizable/
    │   ├── llm_functions/
    │   │   ├── customizable/
    │   │   └── generated/                # same shape
    │   └── type_shapes/
    │       ├── customizable/
    │       └── generated/
    └── nodejs_typescript/
        ├── Cargo.toml                    # name = "sdk_test_nodejs_typescript"
        │                                 # [build-dependencies] sdk_test_harness_setup
        │                                 # [dev-dependencies]   sdk_test_harness_runner
        ├── build.rs                      # one-liner → sdk_test_harness_setup::nodejs_typescript::run_all()
        ├── setup.sh                      # pnpm build:debug (bridge_nodejs) + per-fixture pnpm install
        ├── src/lib.rs                    # invokes sdk_test_harness_runner::nodejs_typescript::test_suite!()
        ├── docstrings_etc/
        │   ├── customizable/             # tracked: *.test.ts — copied into generated/
        │   └── generated/                # gitignored: build output
        │       ├── baml_sdk/             # empty until codegen_nodejs lands
        │       ├── package.json          # name = "sdk-tests-nodejs-typescript-docstrings-etc"
        │       ├── tsconfig.json
        │       ├── node_modules/         # pnpm install output
        │       └── *.test.ts             # copied from ../customizable/
        ├── llm_functions/
        │   ├── customizable/
        │   └── generated/
        └── type_shapes/
            ├── customizable/
            └── generated/
```

### Naming

- **Fixture directory** (under `fixtures/` and under each
  `crates/<generator>/`): lowercase snake (`docstrings_etc`,
  `llm_functions`, `type_shapes`). The same name appears in both
  trees — `fixtures/<F>/baml_src/` is the input;
  `crates/<G>/<F>/` is the output for one generator.
- **Generator directory** (under `crates/`): lowercase snake
  matching the generator key (`python_pydantic2`,
  `nodejs_typescript`).
- **Rust crate name**: `sdk_test_<generator>` — one per generator.
- **Generated package name** (written into
  `crates/<G>/<F>/generated/{pyproject.toml,package.json}`):
  `sdk-tests-<generator>-<fixture>` with `_`→`-` substitution, e.g.
  `sdk-tests-python-pydantic2-docstrings-etc`. Mirrors the
  directory hierarchy (generator first, then fixture).

## How It Works

1. **`crates/<generator>/build.rs`** calls
   `sdk_test_harness_setup::<generator>::run_all()`, which:
   - Scans `sdk_tests/fixtures/*/baml_src/` to discover the fixture
     set.
   - For each fixture: loads `.baml` files into a `ProjectDatabase`,
     gates on `Severity::Error` diagnostics, builds the codegen
     `SymbolPool`, calls the target's `to_source_code(...)`, and
     writes the result to
     `crates/<generator>/<fixture>/generated/baml_sdk/`.
   - Symlinks each file in
     `crates/<generator>/<fixture>/customizable/` into
     `crates/<generator>/<fixture>/generated/` (python) — or
     copies (`nodejs_typescript`, because node + ts-jest follow
     symlinks during module resolution and break out of the
     generated dir's `node_modules`).
   - Writes `crates/<generator>/<fixture>/generated/pyproject.toml`
     (or `package.json` + `tsconfig.json` for `nodejs_typescript`)
     with the per-fixture package name.
   - For python_pydantic2: runs `uv sync` inside the generated dir,
     serially per fixture. uv's editable install of `baml_core`
     (declared in `[tool.uv.sources]`) triggers the maturin build
     of `bridge_python` once per fixture.
   - For nodejs_typescript: pnpm side is OUT of build.rs. The
     per-fixture `pnpm install` and the prereq `pnpm build:debug`
     of bridge_nodejs's native `.node` addon live in
     `crates/nodejs_typescript/setup.sh`, run separately after
     `cargo test --no-run`. Populates `node_modules/` from the
     shared `target/pnpm-store/`. Tests then only do read-only
     work against the populated tree.
   - Emits `OUT_DIR/<generator>_tests.rs` — a generated source file
     containing a `::sdk_test_harness_runner::build_diagnostics!(...)` macro
     invocation at the top followed by one `mod <fixture> { … }`
     per fixture, with each `#[test]` body just calling
     `::sdk_test_harness_runner::run_test_cmd(...)`. The emitter writes
     macro / function invocations only — no test logic.
   - Emits `cargo:rerun-if-changed=` for every BAML and
     customizable file.
2. **`crates/<generator>/src/lib.rs`** invokes
   `sdk_test_harness_runner::<generator>::test_suite!()`, a macro that
   expands to `include!(concat!(env!("OUT_DIR"),
   "/<generator>_tests.rs"))` — pulling in the scaffold emitted by
   the build script. The `test_suite!()` macro plus the
   `build_diagnostics!` macro and `run_test_cmd` referenced from
   inside the scaffold all live in `sdk_test_harness_runner` so the
   generator crate's `[dev-dependencies]` slot can pull them in
   without dragging the codegen deps along.
3. The per-fixture `#[test]` fns all call
   `sdk_test_harness_runner::run_test_cmd(fixture, cmd, cache_subdir,
   cache_env_var)`, which `cd`s into
   `<CARGO_MANIFEST_DIR>/<fixture>/generated/` (i.e.
   `sdk_tests/crates/<generator>/<fixture>/generated/`), threads
   the toolchain cache env var (`UV_CACHE_DIR` /
   `npm_config_store_dir`), and spawns `cmd`. The `uv` invocation
   falls back to `mise which uv` if `uv` isn't on PATH.

### Soft-fail build.rs

`uv` / `pnpm` aren't required to *build* the workspace — only to
*test* the SDK targets. python_pydantic2's `build.rs` records
env-dependent failures (missing `uv`, `to_source_code` panic, `uv
sync` non-zero exit, codegen file write errors) to
`$OUT_DIR/build_diagnostics.txt` and exits 0 instead of aborting.
nodejs_typescript's `build.rs` only does codegen + scaffold emit
(no pnpm), so the soft-fail set is just codegen/write errors;
pnpm failures hard-fail in `setup.sh` instead. The `sdk_test_harness_runner::build_diagnostics!` macro expands
to a `mod build_diagnostics { #[test] fn no_build_failures }` that
reads the file and fails with the records. `sdk_test_harness_setup`'s
scaffold emitter stamps one invocation per generator scaffold —
`::sdk_test_harness_runner::build_diagnostics!()` for python and
`::sdk_test_harness_runner::build_diagnostics!(ignore = "…")` for
nodejs_typescript (while `codegen_nodejs` is a stub).

Outcome: `cargo doc` / `cargo check` succeed without `uv` / `pnpm`
installed; `cargo test` surfaces the same failures it would have
hit before, just routed through a test rather than build.rs. The
`nodejs_typescript` crate `#[ignore]`s `build_diagnostics` plus
every per-fixture test until `codegen_nodejs` is real — see
`IGNORE_REASON` in `sdk_tests/harness_setup/src/nodejs_typescript.rs`.

Hard panics are retained for repo/author bugs: missing `fixtures/`
directory, fixtures with zero `.baml` files, `.baml` files with
`Severity::Error` diagnostics, unset `CARGO_MANIFEST_DIR` /
`OUT_DIR`. See `sdk_test_harness_setup::BuildDiagnostics` for the split.

## Adding a Fixture

1. `mkdir -p sdk_tests/fixtures/<name>/baml_src/` and drop `.baml`
   files in. Nothing else goes under
   `sdk_tests/fixtures/<name>/` — it's the generator-agnostic
   input only.
2. For each generator target that should run this fixture, drop a
   `<name>/customizable/` directory under the generator's crate
   containing the host-language tests, e.g.
   `sdk_tests/crates/python_pydantic2/<name>/customizable/test_main.py`
   and/or
   `sdk_tests/crates/nodejs_typescript/<name>/customizable/main.test.ts`.
3. `cargo test -p sdk_test_python_pydantic2 <name>::` to run.

No code edits needed in `build.rs` or `src/lib.rs` — the fixture
list is discovered at build time from `sdk_tests/fixtures/` and
emitted into the generated test scaffold.

## Adding a Generator Target

1. Add `sdk_tests/harness_setup/src/<target>.rs` with `run_all()`
   (codegen + pyproject/package.json template + per-fixture install
   + `OUT_DIR` scaffold emission, threading a `BuildDiagnostics`
   through). The scaffold emitter stamps
   `::sdk_test_harness_runner::build_diagnostics!(...)` at the top and one
   `mod <fixture> { #[test] … ::sdk_test_harness_runner::run_test_cmd(…) }`
   per fixture — no test bodies authored here.
2. Add a `pub mod <target> { ... #[macro_export] macro_rules!
   <target>_test_suite { ... } pub use crate::<target>_test_suite
   as test_suite; }` block to `sdk_tests/harness_runner/src/lib.rs`
   so the generator crate can invoke it as
   `sdk_test_harness_runner::<target>::test_suite!()`. The macro body is
   just `include!(concat!(env!("OUT_DIR"), "/<target>_tests.rs"))`.
3. Add `sdk_tests/crates/<target>/{Cargo.toml,build.rs,src/lib.rs}`
   following `crates/python_pydantic2/`'s shape. `Cargo.toml` wires
   `sdk_test_harness_setup` as `[build-dependencies]` and `sdk_test_harness_runner`
   as `[dev-dependencies]`.
4. For each existing fixture that should run under this target,
   drop a `sdk_tests/crates/<target>/<fixture>/customizable/`
   directory containing the host-language tests.
