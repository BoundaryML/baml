# BAML Codegen SDK Tests

E2E test crates for BAML code generation. Each crate drives codegen
from real `.baml` source under `baml_src/` through the full
`baml_project::build_symbol_pool` pipeline — i.e. parse → HIR → TIR →
`SymbolPool` → emitter, mirroring the path `baml-cli generate` takes
end-to-end.

```bash
# Run tests for a specific crate
cargo test -p sdk_test_llm_functions

# Run all sdk tests
cargo test -p 'sdk_test_*'
```

## Directory Structure

```text
sdk_tests/
└── crates/
    └── <fixture>/
        ├── Cargo.toml
        ├── build.rs           # hand-written codegen driver
        ├── baml_src/          # real BAML source the crate exercises
        ├── customizable/      # pytest assertions over the generated SDK
        ├── src/lib.rs         # empty (doc comment only)
        ├── tests/sdk_test.rs  # one-liner → sdk_test_build::sdk_test_suite!()
        └── generated/         # build output (gitignored)
```

Crates are hand-maintained. The reference template is
`crates/llm_functions/`; new crates are added by copying its shape
(build.rs, Cargo.toml, src/lib.rs, tests/sdk_test.rs, customizable/)
and authoring a new `baml_src/`.

## How It Works

1. `build.rs`
   - Discovers `.baml` files under `baml_src/` via
     `baml_workspace::discover_baml_files`.
   - Loads them into a `ProjectDatabase`.
   - Bails on any `Severity::Error` diagnostic.
   - Calls `baml_project::build_symbol_pool(&db)` and
     `baml_codegen_python::to_source_code(&pool, &user_baml_files)`.
   - Writes the generated tree to `generated/baml_sdk/`.
   - Symlinks `customizable/*.py` into `generated/`.
   - Writes `pyproject.toml` into `generated/`.
2. `tests/sdk_test.rs` invokes `sdk_test_build::sdk_test_suite!()`,
   which expands to four `#[test]` functions — `sync_only`, `ruff`,
   `pyright`, `pytest`. The latter three each run `uv sync` then
   `uv run <check>` inside `generated/` via
   `sdk_test_build::run_test_cmd` (which splits the string and spawns
   `Command::new(prog).args(rest)`). `sync_only` runs `uv sync` on its
   own so CI can pre-warm the shared `target/uv-cache` (editable build
   of `baml_core` + wheel extractions) by running it serially across
   all sdk_test crates before fanning the rest of the suite out in
   parallel — see the sdk-tests job in
   `.github/workflows/cargo-tests.reusable.yaml`.

## Adding a New Fixture

1. `cp -r crates/llm_functions crates/<name>` (or copy from any
   existing sdk-test crate).
2. Adjust the `[package] name` in `Cargo.toml`. The generated
   `pyproject.toml`'s `name` is auto-derived inside
   `sdk_test_build::run` (strip `sdk_test_`, swap `_` → `-`).
3. Replace `baml_src/` contents with the BAML source for the new
   fixture.
4. Replace `customizable/test_main.py` with assertions over the
   generated SDK.
5. `cargo test -p sdk_test_<name>`.
