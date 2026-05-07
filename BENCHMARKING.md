# Benchmarking

This repo is the **target** of three benchmark suites. The
**infrastructure** that runs them lives separately, with its own release
cadence, in the `benchmarking2` repo (sibling clone alongside this one
in dev; deployed independently in prod).

## Suites at a glance

| Suite | What it measures | Source location | Storage |
|---|---|---|---|
| **A1** | BAML internal VM perf (regression tracking) | `baml_language/crates/baml_tests/benches/runtime_benchmark.rs` | CodSpeed (existing `benchmarks-instrumented` job) |
| **A2** | BAML vs Python vs Go walltime, same workloads as A1 | `baml_language/crates/baml_tests/benches/cross_lang/` | benchmark-results service (in `benchmarking2`) |
| **B**  | Claude Code session metrics per task per language | `baml_language/crates/baml_tests/benches/swe_bench/` | benchmark-results service (in `benchmarking2`) |

A1 measures **execution-only in-process** (compile is amortized).
A2 measures **process startup + compile + execute as a subprocess**
via hyperfine. They are not directly comparable; do not chart together.

B measures developer-experience for an AI coding agent (Claude Code):
how many turns, how many tokens, how much wall time, did the produced
code pass the language grader.

## Where things live in this repo

- `baml_language/crates/baml_tests/benches/runtime_benchmark.rs` — A1
  source. **Untouched** by A2/B work; renaming any benchmark invalidates
  CodSpeed history.
- `baml_language/crates/baml_tests/benches/cross_lang/` — A2 corpus +
  harness + drift check. See its own README.
- `baml_language/crates/baml_tests/benches/swe_bench/` — B task corpus
  (specs, fixtures, graders, references). See its own README.
- `baml_language/crates/baml_tests/Cargo.toml` declares
  `[[bin]] cross_lang_baml` used by A2's hyperfine.
- `.github/workflows/ci.yaml` has three new non-required jobs:
  - `suite-a2-drift-check` (PR-time; ensures `cross_lang/workloads/baml/`
    matches `runtime_benchmark.rs`)
  - `suite-a2-enqueue` (canary push or workflow_dispatch; POSTs to the
    results service)
  - `suite-b-enqueue` (workflow_dispatch or PRs containing
    `RUN_BAML_SWE_BENCH=1` in the body)

## Where the infra lives

`https://github.com/<TBD>/benchmarking2` (or the sibling local clone
during dev). That repo holds:

- `benchmark-results` — HTTP service (axum + sqlite, postgres-shaped).
- `benchmark-worker` — long-lived daemon, one per host, consumes the
  queue, builds BAML at the requested SHA, runs A2 directly. For B,
  reads the per-cell files and POSTs them to claude-proxy.
- `claude-proxy` — owns Anthropic auth and runs Suite B's per-cell
  harness. Exposes `/run-cell`: workers POST a self-contained request
  with all task files in the JSON body; the proxy stages them locally,
  spawns `claude`, runs the language grader (pytest / go test), and
  returns the result row. No shared filesystem with workers.

CI in this repo only **enqueues** runs; the worker on dedicated hardware
produces the numbers (otherwise you can't compare across SHAs because
GitHub-hosted runners are noisy).

## Running locally

A2 (no infra needed):

```sh
python3 baml_language/crates/baml_tests/benches/cross_lang/harness/drift_check.py
bash baml_language/crates/baml_tests/benches/cross_lang/harness/run.sh \
  --warmup 1 --runs 3 --workloads fib_20,arithmetic
```

B (needs the `benchmarking2` stack up + Claude authenticated):

```sh
# In the sibling benchmarking2 clone:
docker compose -f benchmark-results/docker-compose.yaml up -d --build

# Authenticate Claude on the proxy. Pick one:
#   OAuth (recommended for long-lived dev/prod):
docker compose -f benchmark-results/docker-compose.yaml \
    exec -it claude-proxy claude /login
docker compose -f benchmark-results/docker-compose.yaml restart claude-proxy
#   ...or pass ANTHROPIC_API_KEY=sk-ant-... in the env before `up -d`
#   for headless / CI use.

# Enqueue:
curl -X POST -H "Authorization: Bearer devtoken" -H "Content-Type: application/json" \
  -d '{"repo":"local","ref":"canary","sha":"HEAD","suites":["suite_b"]}' \
  http://localhost:8080/benchmark-runs
```

## CI secrets

When deploying the `benchmarking2` service, set two GitHub Action
secrets in this repo:

- `BAML_BENCH_SERVICE_URL` — e.g. `https://bench.example.com`
- `BAML_BENCH_SERVICE_TOKEN` — bearer token expected by the service

When unset, the enqueue jobs degrade to "skipped" with a summary entry
and exit 0.
