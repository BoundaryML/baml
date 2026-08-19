# BAML Compiler Tests

This crate contains all tests for the BAML compiler.

## Test corpora

There are two corpora, split by whether the BAML code is expected to compile:

- `baml_src/` — one BAML project holding **everything that compiles cleanly**:
  - `ns_<name>/` namespaces: runtime tests executed by `baml test`
    (driven by `tests/baml_src.rs`, offline profile from `baml_src/baml.toml`).
  - `ns_fixtures/ns_<name>/` namespaces: compile-only compiler-phase fixtures.
    They are excluded from execution by the offline profile and instead get
    per-namespace PPIR / MIR / bytecode / formatter snapshots from the
    single-compile pass in `src/corpus.rs` (`corpus_snapshots`,
    `corpus_formatter`). The whole corpus is compiled **once** into one Salsa
    database and every phase snapshot is read out of that shared compile.
- `projects/{broken_syntax,diagnostic_errors}/` — projects that must **fail**
  to compile (parse errors / semantic errors). These cannot join a shared
  compile, so `build.rs` still generates one isolated test module per project
  with tier-invariant assertions (see `src/generated_tests.rs`).

`projects/empty/` is a comment-only project used by benches and
`emit_determinism` as a constant-overhead baseline.

## Adding tests

Code that compiles and should be *executed*: add a `test`/`testset` block in an
existing (or new) `baml_src/ns_<name>/` namespace.

Code that compiles and should be *snapshot* through the compiler phases: add a
file under `baml_src/ns_fixtures/ns_<name>/`, then run

```bash
cargo insta test --test-runner=nextest --accept -p baml_tests -- -E 'test(/corpus_/)'
```

Code that must fail to compile: add a project folder under
`projects/broken_syntax/` (parse errors) or `projects/diagnostic_errors/`
(semantic errors only) — tests are generated automatically.

## Running tests

```bash
# Run all tests
cargo nextest run -p baml_tests

# Just the corpus snapshot pass
cargo nextest run -p baml_tests --lib -E 'test(/corpus_/)'

# Run one failing-tier project's tests
cargo nextest run -p baml_tests --lib -E 'test(/my_project/)'

# Update snapshots
cargo insta test --test-runner=nextest --accept -p baml_tests

# Execute the runtime corpus the way CI does
target/debug/baml-cli test --from crates/baml_tests/baml_src
```

## Snapshot layout

The snapshot tree mirrors the corpus source tree — a namespace's snapshots sit
exactly where its sources sit under `baml_src/`, named for their phase:

```
snapshots/
├── baml_src/
│   ├── bytecode.snap                 # root namespace + synthesized functions
│   ├── ns_arrays/
│   │   └── bytecode.snap             # runtime namespaces: bytecode only
│   ├── ns_floats/
│   │   ├── bytecode.snap
│   │   └── diagnostics.snap          # only namespaces that emit diagnostics
│   ├── ns_fixtures/
│   │   └── ns_function_call/         # fixtures also get phase snapshots
│   │       ├── ppir.snap
│   │       ├── mir.snap
│   │       ├── bytecode.snap
│   │       ├── function_call.fmt.snap   # formatter output is per-file
│   │       └── builtin_call.fmt.snap
│   └── stdlib/<pkg>/{ppir,mir,bytecode}.snap
├── broken_syntax/<project>/
└── diagnostic_errors/<project>/
```

`ppir`/`mir`/`bytecode`/`diagnostics` aggregate a whole namespace; only
formatter output is per-file, since each file formats independently. Nested
namespaces nest as directories, matching their sources.

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
