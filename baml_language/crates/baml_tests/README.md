# BAML Compiler Tests

This crate contains all tests for the BAML compiler.

## Test corpora

There are two corpora, split by whether the BAML code is expected to compile:

- `baml_src/` — one BAML project holding **everything that compiles cleanly**:
  - `ns_<name>/` namespaces: runtime tests executed by `baml test`
    (driven by `../baml_cli/tests/baml_corpus.rs`, offline profile from
    `baml_src/baml.toml`).
  - `ns_fixtures/ns_<name>/` namespaces: compile-only compiler-phase fixtures.
    They are excluded from runtime execution, but all are checked and emitted
    by the single-compile pass in `src/corpus.rs`. Only the representative
    examples in `src/corpus_snapshot_policy.rs` get textual phase snapshots.
    Every corpus file is still formatted and checked for idempotency.
- `projects/{broken_syntax,diagnostic_errors}/` — projects that must **fail**
  to compile (parse errors / semantic errors). These cannot join a shared
  compile, so `build.rs` still generates one isolated test module per project
  with tier-invariant assertions (see `src/generated_tests.rs`).

`projects/empty/` is a comment-only project used by benches and
`emit_determinism` as a constant-overhead baseline.

## Adding tests

Code that compiles and should be *executed*: add a `test`/`testset` block in an
existing (or new) `baml_src/ns_<name>/` namespace.

Compiler-only fixtures go under `baml_src/ns_fixtures/ns_<name>/`. New fixtures
produce **no IR or formatter snapshots by default**. Prefer an assertion for
the specific property being tested. When a textual golden is the appropriate
regression test, add a small example to `src/corpus_snapshot_policy.rs`, with
the phase and its rationale (and exact function names for MIR/bytecode), then run

```bash
cargo insta test --test-runner=nextest --dnd --accept -p baml_tests -- corpus_
```

Code that must fail to compile: add a project folder under
`projects/broken_syntax/` (parse errors) or `projects/diagnostic_errors/`
(semantic errors only) — tests are generated automatically.

Type-system fixtures in `src/type_spec/fixtures/` always check their caret
type/error annotations, including the clean-by-default error channel. Only
the five named `SNAPSHOT_EXAMPLES` in `src/type_spec/fixtures.rs` render full
node dumps. Do not add a dump just because another fixture was added.

The conforming type fixtures run in a private four-worker Rayon pool, with a
fresh compiler database per fixture. Results and snapshot assertions are
processed in fixture order on the test thread. Nextest reserves four slots
for this test; keep its `threads-required` setting aligned with
`FIXTURE_WORKERS` in `src/type_spec/fixtures.rs`.

## Running tests

```bash
# Run compiler tests (the runtime corpus is owned by baml_cli)
cargo nextest run -p baml_tests

# Just the corpus snapshot pass
cargo nextest run -p baml_tests --lib -E 'test(/corpus_/)'

# Run one failing-tier project's tests
cargo nextest run -p baml_tests --lib -E 'test(/my_project/)'

# Update snapshots
cargo insta test --test-runner=nextest --accept -p baml_tests

# Build the matching CLI and execute the runtime corpus the way CI does
cargo nextest run -p baml_cli --test baml_corpus
```

## Snapshot layout

The golden tree still mirrors source paths, but it is deliberately sparse:

- 6 PPIR examples: selected source files.
- 8 MIR examples: selected functions, ordered by source position.
- 9 bytecode examples: exact emitted function names, not growing namespaces.
- 12 formatter goldens: representative syntax; all other files retain
  format-success and idempotency checks.
- Nonempty diagnostics groups remain exhaustive (currently 13 snapshots).

MIR/bytecode use `mir.snap` / `bytecode.snap` beneath the selected source's
namespace directory; formatter goldens use `<file stem>.fmt.snap`. Whole-stdlib
phase dumps are intentionally absent. Runtime tests, prefix byte-equivalence,
link-oracle and emit-determinism checks remain unchanged.

Missing selectors, duplicate destinations, and missing/orphaned goldens fail
policy/inventory tests. Run them with the corpus using
`cargo nextest run -p baml_tests --lib -E 'test(/^corpus::/)'`.

Type-spec fixture dumps are reduced separately to five examples; snapshots
owned by its other tests and the diagnostic-error/CLI suites are unchanged.

A namespace that emits no diagnostics has no `diagnostics.snap`, so fixing the
last warning in a namespace leaves that file behind — clear it with
`cargo insta test --test-runner=nextest --accept --unreferenced=delete`. The
corpus-wide "zero errors" rule is an assertion, not a snapshot: errors fail the
run outright with the full rendered list.

## Benchmarks

This crate also includes comprehensive performance benchmarks. See
[BENCHMARKS.md](BENCHMARKS.md) for details.

```bash
# Run benchmarks
cargo bench --bench compiler_benchmark

# Run specific benchmark
cargo bench --bench compiler_benchmark bench_incremental_add_user_field
```
