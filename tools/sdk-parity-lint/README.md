# SDK parity lint

This BAML pipeline inventories test declarations under `baml_language/sdk_tests/crates/*/*/customizable`, normalizes them to exact `<category>/<name>` IDs, applies TypeScript runtime gates and `SDK_PARITY_LINT(skip)` annotations, and compares a generated Markdown coverage matrix with the checked-in baseline.

The tool measures whether a test declaration is checked in for each SDK environment. It does not run tests or report pass/fail status. The report includes parity percentages using the test IDs declared in `python_pydantic2` as the baseline; SDK-only test IDs do not affect those percentages. C# currently exposes its native SDK integration tests through Rust `#[test]` wrappers, which are reported under the `integration` category.

## Run

Build the current local BAML CLI and install the repository-pinned ast-grep:

```sh
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli --bin baml-cli
mise install ast-grep
mise exec ast-grep -- ast-grep --version
tools/sdk-parity-lint/run --repo-root .
```

The checked-in baseline is `baml_language/sdk_tests/crates/parity_analysis.md`. Each run writes `baml_language/sdk_tests/crates/parity_analysis.new.md`.

The command exits with status `0` when parity is unchanged or better, `1` when parity is worse, and `2` for discovery, configuration, or output errors. Parity is worse when the required-gap count increases, the checked-in declaration count decreases, or an existing `SDK_PARITY_LINT(skip)` annotation would waive an environment that previously contained the test. Existing baseline gaps do not fail the command. The generated report lists newly missing and resolved pairs, and it is written on parity regression but not after a discovery error.

Use `--baseline` or `--output` to override either path:

```sh
tools/sdk-parity-lint/run --repo-root . --baseline path/to/baseline.md --output path/to/report.md
```

Set `BAML_BIN` to override the default `baml_language/target/debug/baml-cli`.

## Language-specific cases

Put an annotation immediately before a discovered declaration:

```python
# SDK_PARITY_LINT(skip): exercises Python's UNSET sentinel
def test_optional_args_python_unset_and_none_differ_in_one_call():
    ...
```

Use `//` in C++, C#, Go, Java, Rust, Swift, and TypeScript. The reason after the colon is required. The annotation applies to its canonical test ID across the matrix: environments where the declaration is currently absent are waived, while present declarations remain required by the baseline ratchet.

## Test

```sh
baml_language/target/debug/baml-cli check --from tools/sdk-parity-lint
baml_language/target/debug/baml-cli test --from tools/sdk-parity-lint
```

The fixtures cover declaration rules, runtime gates, annotations, duplicate IDs, baseline parsing, exact report rendering, parity improvements, declaration deletion, and weakened required-environment sets.
