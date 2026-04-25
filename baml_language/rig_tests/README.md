# BAML Codegen Test Rig

E2E test crates for BAML code generation. Each crate drives codegen
from real `.baml` source under `baml_src/` through the full
`baml_project::build_symbol_pool` pipeline — i.e. parse → HIR → TIR →
`SymbolPool` → emitter, mirroring the path `baml-cli generate` takes
end-to-end.

```bash
# Run tests for a specific crate
cargo test -p rig_python_empty

# Run all rig tests
cargo test -p 'rig_python_*'
```

## Directory Structure

```text
rig_tests/
└── crates/
    └── python_<fixture>/
        ├── Cargo.toml
        ├── build.rs
        ├── baml_src/        # real BAML source the crate exercises
        ├── customizable/    # pytest assertions over the generated SDK
        ├── src/lib.rs       # `cargo test` harness — shells out to
        │                    # generated/test.sh
        └── generated/       # build output (gitignored)
```

Crates are hand-maintained. The reference template is
`crates/python_example_09a/`; new crates are added by copying its
shape (build.rs, Cargo.toml, src/lib.rs, customizable/) and authoring
a new `baml_src/`.

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
   - Writes `conftest.py`, `pyproject.toml`, and `test.sh` /
     `test.ps1` shims into `generated/`.
2. `src/lib.rs` runs `generated/test.sh` (or `test.ps1` on Windows).
3. `test.sh` runs `py_compile`, `ruff`, and `pytest` over the
   generated SDK plus the `customizable/` assertions.

## Adding a New Fixture

1. `cp -r crates/python_example_09a crates/python_<name>` (or copy
   from any existing rig crate).
2. Adjust the `[package] name` in `Cargo.toml` and the
   `pyproject.toml` `name = ...` in `build.rs` to match.
3. Replace `baml_src/` contents with the BAML source for the new
   fixture.
4. Replace `customizable/test_main.py` with assertions over the
   generated SDK.
5. `cargo test -p rig_python_<name>`.
