# BAML SDK Tests

The BAML programming language allows users to generate an SDK in their
host language of choice with bindings to all their BAML types and functions. These
are the e2e tests for those generated SDKs (which tests both the sdkgen logic and
underlying FFI interfaces).

```bash
# Run every SDK test target across all fixtures.
cargo nextest run -p sdk_test_python_pydantic2 -p sdk_test_typescript -p sdk_test_typescript_web -p sdk_test_rust

# Run SDK tests for a specific generator.
cargo nextest run -p sdk_test_python_pydantic2
cargo nextest run -p sdk_test_typescript
cargo nextest run -p sdk_test_typescript_web
cargo nextest run -p sdk_test_rust

# Or run one host-language runner specifically.
cargo nextest run -p sdk_test_python_pydantic2 function_calls::pytest
cargo nextest run -p sdk_test_typescript function_calls::vitest_node
cargo nextest run -p sdk_test_typescript_web function_calls::vitest_web
cargo nextest run -p sdk_test_typescript_web function_calls::vitest_workers
cargo nextest run -p sdk_test_rust function_calls::cargo_test
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

# vitest: run tests matching a test-name pattern in one runtime.
cargo nextest run -p sdk_test_typescript function_calls::vitest_node
(cd sdk_tests/crates/typescript/function_calls/generated && pnpm exec vitest run --config vitest.node.config.ts -t optional_args)
(cd sdk_tests/crates/typescript_web/function_calls/generated && pnpm exec vitest run --config vitest.web.config.ts -t optional_args)
(cd sdk_tests/crates/typescript_web/function_calls/generated && pnpm exec vitest run --config vitest.workers.config.ts -t optional_args)

# rust: run tests matching a name filter (set CARGO_TARGET_DIR to reuse the
# shared build cache the nextest-driven runs populate).
cargo nextest run -p sdk_test_rust function_calls::cargo_test
(cd sdk_tests/crates/rust/function_calls/generated && CARGO_TARGET_DIR=../../../../../target/sdk-rust-target cargo test optional_args)
```

### Rust port gating

A Rust test file that references a symbol the generator does not emit yet
fails to *compile* (unlike pytest/vitest, where it just fails), so ported
Rust tests are compiled only when the capability they exercise has landed:
`generated/tests/main.rs` declares the enabled files as `#[path]` modules
and lists the rest as `// LATER(<reason>)` comments. The single source of
truth is the `TEST_MODS` table in `sdk_tests/harness_setup/src/rust.rs` —
enabling a port is a one-line flip there.

## SDK implementation

Each SDK is implemented in two parts: an FFI to provide core runtime bindings and an SDK generator to generate typed bindings.

`sdk_test_python_pydantic2` provides coverage for

  - `sdks/python/rust/bridge_python`
  - `sdks/python/src/baml_bridge`
  - `sdks/python/rust/sdkgen_python_pydantic2`

`sdk_test_typescript` provides coverage for

  - `sdks/typescript/bridge_typescript`
  - `sdks/typescript/sdkgen_typescript_shared`

`sdk_test_typescript_web` provides coverage for

  - `sdks/typescript/bridge_typescript_web`
  - `sdks/typescript/sdkgen_typescript_shared`

`sdk_test_rust` provides coverage for

  - `sdks/rust/bridge_rust`
  - `sdks/rust/sdkgen_rust`

## Directory structure

There are two dimensions for SDK tests: generators and fixtures.

There is one Rust crate per SDK generator:

  - `sdk_test_python_pydantic2` for the `python/pydantic2` generator
  - `sdk_test_typescript` for the Node generator and native bridge
  - `sdk_test_typescript_web` for the Web generator and browser/Workers runtimes
  - `sdk_test_rust` for the `rust` generator

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
    |-- typescript/
    |   |-- Cargo.toml                    # name = "sdk_test_typescript"
    |   |-- function_calls/
    |   |   |-- customizable/             # tracked: shared *.test.ts with inline runtime filters
    |   |   `-- generated/                # gitignored: build output
    |   |       |-- node/baml_sdk/        # Node generator + copied canonical tests
    |   |       |-- package.json          # native bridge + Node runner tools
    |   |       |-- tsconfig.node.json
    |   |       |-- vitest.node.config.ts
    |   |       `-- node_modules/         # pnpm install output
    |-- rust/
    |   |-- Cargo.toml                    # name = "sdk_test_rust"
    |   |-- function_calls/
    |   |   |-- customizable/             # tracked: *.rs -- symlinked into generated/customizable/
    |   |   `-- generated/                # gitignored: the generated SDK is itself a Cargo crate
    |   |       |-- Cargo.toml            # package = "sdk-tests-rust-function-calls", lib = "baml_sdk"
    |   |       |-- src/                  # codegen output
    |   |       |-- customizable/         # symlinked from ../customizable/ (NOT under tests/ --
    |   |       |                         #   cargo would auto-discover gated-off ports)
    |   |       `-- tests/main.rs         # gate file: only modules declared here compile;
    |   |                                 #   rows come from TEST_MODS in harness_setup/src/rust.rs
    |   |-- llm_functions/
    |   |   |-- customizable/
    |   |   `-- generated/
    |   `-- type_shapes/
    |       |-- customizable/
    |       `-- generated/
    `-- typescript_web/
        |-- Cargo.toml                    # name = "sdk_test_typescript_web"
        |-- function_calls/generated/     # no checked-in customizable tree
        |   |-- web/baml_sdk/             # Web generator + copied canonical tests
        |   |-- workers/baml_sdk/         # Web generator + copied canonical tests
        |   |-- package.json              # Web bridge + browser/Workers runner tools
        |   |-- tsconfig.{web,workers}.json
        |   |-- vitest.{web,workers}.config.ts
        |   `-- wrangler.jsonc
        |-- llm_functions/generated/
        `-- type_shapes/generated/
```

### Naming

- **Fixture directory** (under `fixtures/` and under each
  `crates/<generator>/`): lowercase snake (`function_calls`,
  `llm_functions`, `type_shapes`). The same name appears in both
  trees -- `fixtures/<F>/baml_src/` is the input;
  `crates/<G>/<F>/` is the output for one generator.
- **Generator directory** (under `crates/`): lowercase snake
  (`python_pydantic2`, `typescript`, `typescript_web`, `rust`). `typescript`
  owns the canonical checked-in test corpus and Node suite; `typescript_web`
  owns generated browser and Workers output only.
- **Rust crate name**: `sdk_test_<generator>` -- one per generator.

### TypeScript runtime selection

Every checked-in `*.test.ts` and helper `*.ts` file lives under `crates/typescript/<fixture>/customizable`. The Node build copies the complete corpus into its Node tree, while the Web build reads that sibling source and copies it into its own Chromium and workerd trees. Each Vitest config sets `BAML_TEST_RUNTIME`; import `isTestRuntime` from the generated `test_runtime.js` helper and use `describe.runIf(isTestRuntime("node"))`, `describe.runIf(isTestRuntime("web"))`, or `describe.runIf(isTestRuntime("workers"))` to interleave runtime-specific coverage in one test file.

Portable bridge semantics must run without a runtime gate so the same assertion executes in Node, Chromium, and workerd. Use the smallest capability-specific `describe.runIf` block for platform behavior: local listeners and mutable host files are Node-only, mocked `globalThis.fetch` runs in Chromium and workerd, and synchronous bundle reads are workerd-only. Every remaining runtime gate needs a nearby comment naming the concrete unavailable capability.

The full Web parity gate includes generated ESM imports, TypeScript declarations, Chromium Vitest, and workerd Vitest for every fixture:

```bash
cd baml_language
cargo nextest run -p sdk_test_typescript_web
```

Use fixture and runner filters while iterating, but rerun the complete crate before merging:

```bash
cargo nextest run -p sdk_test_typescript_web function_calls::
cargo nextest run -p sdk_test_typescript_web type_shapes::vitest_web
cargo nextest run -p sdk_test_typescript_web type_shapes::vitest_workers
```

Raw Web bridge boundary tests cover contracts hidden by generated SDKs, including WASM exports, exact package-root exports, declaration synchronization, handle ownership, callable registry cleanup, call contexts, setup errors, and sync deadlock rejection:

```bash
cd baml_language/sdks/typescript/bridge_typescript_web
pnpm build:debug
pnpm test:web
pnpm test:workers
```

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
   `sdk_tests/crates/typescript/<name>/customizable/main.test.ts`.
3. Run `cargo nextest run -p sdk_test_python_pydantic2 <name>::` for Python, `cargo nextest run -p sdk_test_typescript <name>::` for Node TypeScript, and `cargo nextest run -p sdk_test_typescript_web <name>::` for browser and Workers.

No code edits needed in `build.rs` or `src/lib.rs` -- the fixture
list is discovered at build time from `sdk_tests/fixtures/` and
emitted into the generated test scaffold.
