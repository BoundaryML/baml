# BAML SDK tests

These end-to-end tests generate SDKs from the shared fixtures in `sdk_tests/fixtures/`, overlay host-language tests from `sdk_tests/crates/<target>/<fixture>/customizable/`, prepare the target toolchain, and run compile/type/lint/runtime gates through Rust test scaffolds.

## Running the suites

Run SDK test crates with nextest from `baml_language/`. The nextest setup scripts are load-bearing: they build native bridges and prepare package-manager state before the generated Rust tests launch host tools.

```bash
cargo nextest run \
  -p sdk_test_python_pydantic2 \
  -p sdk_test_typescript \
  -p sdk_test_typescript_web \
  -p sdk_test_rust \
  -p sdk_test_java \
  -p sdk_test_go \
  -p sdk_test_cpp
```

Do not substitute `cargo test` for a clean end-to-end run. Plain Cargo does not execute the setup-script bindings in `.config/nextest.toml`, so local generated artifacts may be missing or stale.

Each generated test name is `<fixture>::<gate>`. Useful focused examples:

```bash
cargo nextest run -p sdk_test_python_pydantic2 function_calls::pytest
cargo nextest run -p sdk_test_typescript function_calls::vitest_node
cargo nextest run -p sdk_test_typescript_web function_calls::vitest_web
cargo nextest run -p sdk_test_typescript_web function_calls::vitest_workers
cargo nextest run -p sdk_test_rust function_calls::cargo_test
cargo nextest run -p sdk_test_java function_calls::junit
cargo nextest run -p sdk_test_go function_calls::go_test
cargo nextest run -p sdk_test_cpp function_calls::run
```

To pass a filter directly to pytest or Vitest, first run the nextest gate so `generated/` is current, then invoke the host tool in that generated directory:

```bash
(cd sdk_tests/crates/python_pydantic2/function_calls/generated && uv run pytest -v -k optional_args)
(cd sdk_tests/crates/typescript/function_calls/generated && pnpm exec vitest run --config vitest.node.config.ts -t optional_args)
(cd sdk_tests/crates/typescript_web/function_calls/generated && pnpm exec vitest run --config vitest.web.config.ts -t optional_args)
```

## Target matrix

| Crate | Generated SDK / bridge family | Principal gates |
|---|---|---|
| `sdk_test_python_pydantic2` | Python Pydantic generator and Python bridge | Ruff, Pyright, pytest |
| `sdk_test_typescript` | TypeScript generator and native Node bridge | generated ESM, `tsc`, Node Vitest, bridge package `attw` |
| `sdk_test_typescript_web` | TypeScript generator and Web/Wasm bridge | generated ESM, `tsc`, Chromium Vitest, Workers Vitest |
| `sdk_test_rust` | Rust generator and `baml_bridge` | rustfmt, Clippy, Cargo tests with the engine library |
| `sdk_test_java` | Java generator, Java runtime jar, and JNI bridge | Gradle `compileTestJava`, JUnit for currently enabled fixtures |
| `sdk_test_go` | Go generator and Go bridge | `go test` for the supported source fixtures and synthetic package-edge fixture |
| `sdk_test_cpp` | C++ generator and C FFI bridge | compile and run scripts per fixture |

Some targets intentionally gate incomplete ports. Rust's source of truth is `TEST_MODS` in `sdk_tests/harness_setup/src/rust.rs`; Java's enabled fixture sets are in `sdk_tests/harness_setup/src/java.rs`.

## Layout

```text
sdk_tests/
|-- fixtures/<fixture>/baml_src/          # generator-independent BAML input
|-- crates/<target>/<fixture>/
|   |-- customizable/                     # checked-in host-language tests or overlays
|   `-- generated/                        # generated SDK, manifests, copied/symlinked tests; gitignored
|-- harness_setup/                        # build-time fixture discovery, codegen, staging, scaffold emission
|-- harness_runner/                       # test-time command runners and scaffold macros
`-- harness/llm_recordings/               # offline streaming-response recording crate
```

The ordinary shared fixtures currently include `docstrings_etc`, `function_calls`, `llm_functions`, `type_shapes`, and `unsupported_only`. Individual targets may run a subset or add a synthetic fixture, as Go does with `package_edges`.

TypeScript keeps the canonical checked-in test corpus under `crates/typescript/<fixture>/customizable/`. The Web target copies that corpus into separate Web and Workers generated trees and rewrites bridge imports. Use `BAML_TEST_RUNTIME` through the generated `test_runtime.js` helper to gate only genuinely platform-specific behavior; portable bridge assertions should run in Node, Chromium, and workerd.

## Adding a fixture

1. Add `sdk_tests/fixtures/<name>/baml_src/*.baml`.
2. Add host tests under each participating target's `sdk_tests/crates/<target>/<name>/customizable/` directory. For Web TypeScript, add canonical tests under the TypeScript target; the Web harness reads them from there.
3. Run each participating `sdk_test_<target>` crate with the `<name>::` filter, then run each complete target crate before merging.

Most targets discover shared fixture directories automatically. Targets with an explicit supported-fixture list or port gate (currently Go, Rust, and Java) must also be updated in their `harness_setup/src/<target>.rs` module.

See [DEVELOPMENT.md](DEVELOPMENT.md) for the build/setup/scaffold lifecycle.
