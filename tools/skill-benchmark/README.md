# BAML skill benchmark

This benchmark measures how well coding agents use the repository's `baml-core`
skill while implementing BAML projects from fixed specifications and test suites.

Pull requests run three Opus attempts for each project at the head revision. The
workflow compares the resulting manifest with the latest compatible successful
canary artifact. It never reruns the pull request's base commit.

The report includes completion rate, average agent turns, tool calls, failed
`baml check` calls, implementation LOC, every `baml describe` call, and concise
issues extracted from failed BAML invocations. Raw Claude events, BAML audit
records, verification output, and generated workspaces are retained as GitHub
Actions artifacts for 90 days.

The benchmark implementation and report renderer are in `baml_src`. The Rust
shim records BAML and Claude Code invocations without changing their exit codes
or terminal behavior. JavaScript under `scripts` only handles GitHub Actions API
data and log links.

## Baselines

A successful canary run uploads `skill-benchmark-baseline`. Pull request runs use
the newest unexpired artifact whose schema, suite revision, model, and attempt
count match. Missing and incompatible baselines are reported explicitly and are
not treated as regressions.

## Projects

- `chess-perft`: legal chess move generation verified with public-domain perft data
- `ere`: POSIX ERE parsing and matching against attributed public test corpora
- `jmespath`: JMESPath compliance behavior using the upstream specification suite
