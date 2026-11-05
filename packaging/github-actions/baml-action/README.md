# baml-action

`BoundaryML/baml-action` checks, format-checks, tests, and measures a BAML project in one step. It installs BAML with [`BoundaryML/setup-baml`](https://github.com/BoundaryML/setup-baml), then runs:

| Step | Command | Fails the job when |
| --- | --- | --- |
| check | `baml check` | the project has compiler errors |
| fmt | `baml fmt --check` | any file is not formatted |
| test | `baml test` | any test fails |
| codequality | `baml codequality` | never; it only reports |

`codequality` needs a toolchain from 2026-10-28 or later (a nightly, until the next release) and is skipped with a warning on older ones; `fmt` needs one with `baml fmt --check` and fails without it.

Every enabled step runs even if an earlier one fails, so one run shows every problem. The job summary lists each step's result and the code quality report.

## Usage

```yaml
name: BAML

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read
  pull-requests: write # for the code quality comment

jobs:
  baml:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5

      - uses: BoundaryML/baml-action@v1
        env:
          # Only needed if your tests call an LLM.
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
```

## Inputs

| Input | Default | Description |
| --- | --- | --- |
| `toolchain` | from `baml.toml` | Toolchain selector passed to `setup-baml`: an exact version, `canary`, or `nightly`. |
| `project` | `.` | Project directory, passed as `--project`. |
| `check` | `true` | Run `baml check`. |
| `fmt` | `true` | Run `baml fmt --check`. |
| `test` | `true` | Run `baml test`. A project with no tests passes with a notice. |
| `test-profile` | | Test profile from `baml.toml`, passed as `baml test --profile`. |
| `codequality` | `true` | Run `baml codequality`. |
| `comment` | `true` | On pull requests, post a code health summary. |
| `github-token` | `github.token` | Token for the comment. Needs `pull-requests: write`. |

## Outputs

| Output | Description |
| --- | --- |
| `version` | Resolved concrete BAML toolchain version |

## Code health comment

On pull requests, the action comments with a code health summary comparing the pull request to its base. The comment updates in place on each push.

Both commits are measured with the same toolchain, so a toolchain upgrade does not show up as a code change.

Pull requests from forks get a read-only token, so the action skips the comment there and leaves the comparison in the job summary. To post as a GitHub App instead of `github-actions[bot]`, pass that app's installation token as `github-token`.

## Several projects

Run the action once per project. Each project gets its own summary section and its own comment.

```yaml
strategy:
  matrix:
    project: [services/agent, services/extractor]
steps:
  - uses: actions/checkout@v5
  - uses: BoundaryML/baml-action@v1
    with:
      project: ${{ matrix.project }}
```

## Contributing

This repository is a read-only mirror. Open changes against [`packaging/github-actions/baml-action`](https://github.com/BoundaryML/baml/tree/canary/packaging/github-actions/baml-action) in `BoundaryML/baml`.
