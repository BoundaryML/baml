# BAML SDK Tests

The BAML programming language allows users to generate an SDK in their
host language of choice with bindings to all their BAML types and functions. These
are the e2e tests for those generated SDKs (which tests both the sdkgen logic and
underlying FFI interfaces).

```bash
# Run every SDK test target across all fixtures.
cargo nextest run -p sdk_test_python_pydantic2 -p sdk_test_typescript_node

# Run SDK tests for a specific generator.
cargo nextest run -p sdk_test_python_pydantic2
cargo nextest run -p sdk_test_typescript_node

# Or run the python/nodejs tests specifically
cargo nextest run -p sdk_test_python_pydantic2 function_calls::pytest
cargo nextest run -p sdk_test_typescript_node function_calls::vitest
```

> SDK tests are designed to be run using `cargo nextest run` and will > fail in
> surprising ways if run using `cargo test`. Specifically, `cargo nextest run`
> is designed to pick up changes in the BAML FFI layers and SDK generators. See
> [DEVELOPMENT.md](./DEVELOPMENT.md) for more details.

## Filtering pytest/vitest

`cargo nextest` will not pass extra arguments through to `pytest` or `vitest`.
To apply test filters to pytest/vitest, first run the `nextest` to set up the
fixture's `generated/` directory, then run the host-language test directly:

```bash
# pytest: run tests matching a keyword expression.
cargo nextest run -p sdk_test_python_pydantic2 function_calls::pytest
(cd sdk_tests/crates/python_pydantic2/function_calls/generated && uv run pytest -v -k optional_args)

# vitest: run tests matching a test-name pattern.
cargo nextest run -p sdk_test_typescript_node function_calls::vitest
(cd sdk_tests/crates/typescript_node/function_calls/generated && pnpm exec vitest run -t optional_args)
```

## SDK implementation

Each SDK is implemented in two parts: an FFI to provide core runtime bindings and an SDK generator to generate typed bindings.

`sdk_test_python_pydantic2` provides coverage for

  - `sdks/python/rust/bridge_python`
  - `sdks/python/src/baml_core`
  - `sdks/python/rust/sdkgen_python_pydantic2`

`sdk_test_typescript_node` provides coverage for

  - `sdks/nodejs/bridge_nodejs`
  - `sdks/nodejs/sdkgen_typescript_node`

## Directory structure

There are two dimensions for SDK tests: generators and fixtures.

There is one Rust crate per SDK generator:

  - `sdk_test_python_pydantic2` for the `python/pydantic2` generator
  - `sdk_test_typescript_node` for the `typescript/node` generator

Each crate fans out over every **fixture** in `sdk_tests/fixtures/`.
Each fixture is a single `baml_src/` tree that contains `.baml` source
for testing different aspects of each SDK.

Host-language test code for each generated SDK/fixture lives in the
corresponding `customizable/` tree.

```text
sdk_tests/
|-- fixtures/                             # generator-agnostic input only -- baml_src/ and nothing else
|   |-- function_calls/baml_src/          # .baml source (input to every generator)
|   |-- llm_functions/baml_src/
|   `-- type_shapes/baml_src/
`-- crates/                               # one crate per generator target; per-fixture content nested inside
    |-- python_pydantic2/
    |   |-- Cargo.toml                    # name = "sdk_test_python_pydantic2"
    |   |-- function_calls/
    |   |   |-- customizable/             # tracked: *.py -- symlinked into generated/
    |   |   `-- generated/                # gitignored: build output
    |   |       |-- baml_sdk/             # codegen output
    |   |       |-- pyproject.toml        # name = "sdk-tests-python-pydantic2-docstrings-etc"
    |   |       |-- .venv/                # uv sync output
    |   |       `-- *.py                  # symlinked from ../customizable/
    |   |-- llm_functions/
    |   |   |-- customizable/
    |   |   `-- generated/                # same shape
    |   `-- type_shapes/
    |       |-- customizable/
    |       `-- generated/
    `-- typescript_node/
        |-- Cargo.toml                    # name = "sdk_test_typescript_node"
        |-- function_calls/
        |   |-- customizable/             # tracked: *.test.ts -- copied into generated/
        |   `-- generated/                # gitignored: build output
        |       |-- baml_sdk/             # empty until sdkgen_typescript_node lands
        |       |-- package.json          # name = "sdk-tests-nodejs-typescript-docstrings-etc"
        |       |-- tsconfig.json
        |       |-- node_modules/         # pnpm install output
        |       `-- *.test.ts             # copied from ../customizable/
        |-- llm_functions/
        |   |-- customizable/
        |   `-- generated/
        `-- type_shapes/
            |-- customizable/
            `-- generated/
```

### Naming

- **Fixture directory** (under `fixtures/` and under each
  `crates/<generator>/`): lowercase snake (`function_calls`,
  `llm_functions`, `type_shapes`). The same name appears in both
  trees -- `fixtures/<F>/baml_src/` is the input;
  `crates/<G>/<F>/` is the output for one generator.
- **Generator directory** (under `crates/`): lowercase snake
  matching the generator key (`python_pydantic2`,
  `typescript_node`).
- **Rust crate name**: `sdk_test_<generator>` -- one per generator.

## Adding a Fixture

1. `mkdir -p sdk_tests/fixtures/<name>/baml_src/` and drop `.baml`
   files in. Nothing else goes under
   `sdk_tests/fixtures/<name>/` -- it's the generator-agnostic
   input only.
2. For each generator target that should run this fixture, drop a
   `<name>/customizable/` directory under the generator's crate
   containing the host-language tests, e.g.
   `sdk_tests/crates/python_pydantic2/<name>/customizable/test_main.py`
   and/or
   `sdk_tests/crates/typescript_node/<name>/customizable/main.test.ts`.
3. `cargo nextest run -p sdk_test_python_pydantic2 <name>::` to run
   (nextest fires `setup.sh` to sync the new fixture's venv).

No code edits needed in `build.rs` or `src/lib.rs` -- the fixture
list is discovered at build time from `sdk_tests/fixtures/` and
emitted into the generated test scaffold.
