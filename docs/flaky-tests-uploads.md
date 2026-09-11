# Trunk Flaky Tests Uploads

CI uploads JUnit reports to [Trunk Flaky Tests](https://docs.trunk.io/flaky-tests)
so intermittent failures are tracked per test rather than per red job.

## Setup

Uploads are gated on the `TRUNK_ORG_TOKEN` repository secret; every upload
step skips itself when it is unset, which is what keeps fork PRs out. The org
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

The test step takes `continue-on-error` and an `id`; the upload step gates the
job in its place. That ordering is what lets quarantining work — with
quarantining off the upload just reports the test outcome, and with it on Trunk
can clear failures it owns.

```yaml
- name: "Run tests"
  id: my-tests
  continue-on-error: true
  run: cargo nextest run --profile ci -p my_crate
  working-directory: baml_language

- name: "Upload test results to Trunk"
  if: ${{ !cancelled() }}
  uses: ./.github/actions/upload-test-results
  with:
    junit-path: baml_language/target/nextest/ci/junit.xml
    # https://app.trunk.io/boundaryml/flaky-tests/collections/jgs6yJXj
    collection: jgs6yJXj
    normalize: nextest
    cargo-manifest-path: baml_language/Cargo.toml
    test-outcome: ${{ steps.my-tests.outcome }}
```

Fork PRs get no token, so the upload is skipped and the action fails the job
itself when the tests failed.

Two gotchas. Upload before anything that clears `target/` — the Windows leg's
`cargo clean` is why its upload sits above it. And give a second nextest run in
the same job its own profile, or it overwrites the first one's report.

## File paths

Trunk uses a test's file for CODEOWNERS and for flaky-test fix investigations,
so reports are enriched where their runner can't do it:

- **vitest** emits `file` with `addFileAttribute: true`, and each config sets
  `root` to the repo so the paths come out repo-relative rather than
  package-relative.
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
`pack_e2e` and `exit_code_e2e` (left on `cargo test`), `wasm-pack test`, and
`tree-sitter test`.

Each `sdk_test_*` fixture gate is one Rust test that shells out to a whole
foreign suite (`pytest`, `gradle test`, `go test`, `dotnet run`, …), so
`sdk-conformance` tracks the gates, not the assertions behind them.

`editor-frontend` covers only what CI runs today: `pkg-playground`,
`pkg-editor`, `pkg-lsp`, `pkg-proto`, `app-vscode-ext` and `app-website` have
`test` scripts that no workflow invokes.
