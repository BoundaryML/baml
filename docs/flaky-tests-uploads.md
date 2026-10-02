# Trunk Flaky Tests Uploads

CI uploads JUnit reports to [Trunk Flaky Tests](https://docs.trunk.io/flaky-tests)
so intermittent failures are tracked per test rather than per red job.

## Setup

Uploads are gated on the `TRUNK_ORG_TOKEN` repository secret, passed to each
test step as the `run-and-upload-tests` action's `token` input — never as an
env, so the test command that runs PR-authored code never sees it. With no
token the action skips the upload, which is what keeps fork PRs out. The org
slug (`boundaryml`) and the collection short IDs are hard-coded in the
workflows, each next to a link to that collection in the web app.

## Collections

A collection groups tests that share monitors, quarantining, and ticketing
settings. There are four:

| Collection                                                                             | Contents                                                         | Jobs                                           |
| -------------------------------------------------------------------------------------- | ---------------------------------------------------------------- | ---------------------------------------------- |
| [`baml-language`](https://app.trunk.io/boundaryml/flaky-tests/collections/jgs6yJXj)     | The `baml_language` cargo workspace, including snapshots and CLI | `cargo-test-*` (3 platforms), `snapshot-tests` |
| [`sdk-conformance`](https://app.trunk.io/boundaryml/flaky-tests/collections/9AZXu27s)   | The `sdk_test_*` legs plus `baml_bridge`                         | `sdk-tests`                                    |
| [`editor-frontend`](https://app.trunk.io/boundaryml/flaky-tests/collections/xl5LO9gK)   | `app-vscode-webview`, `pkg-grammar`, its highlight.js port       | `Webview Tests`, `Grammar Tests`               |
| [`developer-docs`](https://app.trunk.io/boundaryml/flaky-tests/collections/aDqMrNXq)    | `app-developer-docs`                                             | `Developer Docs`                               |

One test run uploads to exactly one collection, so the boundaries follow the
jobs. A test's identity hashes its collection, so moving tests between
collections restarts their flake history.

## How a job wires up

Each test step is the `run-and-upload-tests` action, which runs the tests,
enriches the report, and uploads it in one step. That step fails when the tests
do, and a failure skips the rest of the job as any failing step would. With
quarantining off the step's outcome is the tests' own; with it on, Trunk clears
failures it owns and the step passes when every failure is quarantined.

The tests run inside the action rather than in a step of their own because a
failed step cannot be overridden by a later one: a separate test step would
fail the job before Trunk could clear a quarantined failure.

```yaml
- name: "Run tests"
  uses: ./.github/actions/run-and-upload-tests
  with:
    run: cargo nextest run --profile ci -p my_crate
    working-directory: baml_language
    junit-path: baml_language/target/nextest/ci/junit.xml
    # https://app.trunk.io/boundaryml/flaky-tests/collections/jgs6yJXj
    collection: jgs6yJXj
    normalize: nextest
    cargo-manifest-path: baml_language/Cargo.toml
    token: ${{ secrets.TRUNK_ORG_TOKEN }}
```

`run` executes in bash. `working-directory` is an input because a `uses:` step
cannot take the key itself; `env` and `if` stay on the step.

Fork PRs get no token, so the upload is skipped and the action fails the step
itself when the tests failed.

Two gotchas. Run tests before anything that clears `target/` — the Windows
leg's `cargo clean` is why its first test step sits above it. And give a second
nextest run in the same job its own profile, or it overwrites the first one's
report: that is why `ci-bridge` and `ci-cli-e2e` exist alongside `ci`.

## File paths

Trunk uses a test's file for CODEOWNERS and for flaky-test fix investigations,
so reports are enriched where their runner can't do it:

- **vitest** emits `file` with `addFileAttribute: true`, but relative to the
  package, so `junit-normalize.py prefix` re-roots it.
- **nextest** cannot emit a file at all — libtest never tells it which source a
  test came from, so there is no config option and
  `cargo nextest list --message-format json` carries no path either.
  `.github/scripts/junit-normalize.py nextest` fills it in from the testsuite
  name, which is the nextest binary ID, resolved against `cargo metadata`:

  ```
  baml_tests::interfaces  -> crates/baml_tests/tests/interfaces.rs
  baml_path               -> crates/baml_path/src/lib.rs
  ```

  An integration suite is one binary and one file, so that path is exact; a lib
  suite resolves to the crate's `lib.rs` rather than the test's own module.
- **node:test** emits `<testcase>` with no `<testsuite>` wrapper, which Trunk
  parses as zero tests without erroring. `junit-normalize.py node-test` groups
  them by file.

## Not covered

These run in CI but upload nothing, because their runners emit no JUnit:
`wasm-pack test` and `tree-sitter test`.

Each `sdk_test_*` fixture gate is one Rust test that shells out to a whole
foreign suite (`pytest`, `gradle test`, `go test`, `dotnet run`, …), so
`sdk-conformance` tracks the gates, not the assertions behind them.

`editor-frontend` covers only what CI runs today: `pkg-playground`,
`pkg-editor`, `pkg-lsp`, `pkg-proto`, `app-vscode-ext` and `app-website` have
`test` scripts that no workflow invokes.
