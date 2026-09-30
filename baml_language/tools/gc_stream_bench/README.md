# Streaming GC benchmark

Measures garbage-collection behavior while Python consumes many streamed LLM responses through a generated `baml_sdk`.

The workload is a structured extraction. A cheap model gets a plain-text rendering of GitHub PR #5041 (`seed/pr-5041.txt`, about 29 KB) and rebuilds the `gh pr view --json` object it came from (`seed/pr-5041.json`) as a `PullRequest` (`baml_src/pull_request.baml`). The prompt renders the schema through `ctx.output_format(...)` as TypeScript declarations instead of the default JSON-like form. Each response is a few thousand tokens, so a single stream yields thousands of partial `PullRequest` objects to Python.

## What it measures

Both collectors are observed at once:

- **BAML engine GC**: `baml_bridge.baml_py._gc_stats()` returns heap occupancy and running totals: cycles by trigger reason, slots reclaimed, promotions, and, in `gc_profiling` builds, stop-the-world pause total and max.
- **CPython GC**: every collection is timed through `gc.callbacks`, per generation.

It also reports:

- time to first partial, and the gaps between partials (p50, p99 and max), where GC pauses show up as consumer-visible latency;
- RSS, and peak BAML runtime objects, sampled every 50 ms;
- leak checks after the run: live BAML handles, and Python objects still alive after `gc.collect()`;
- extraction accuracy: the fraction of the seed's leaf values reproduced exactly.

## Setup

```bash
./setup.sh
```

This generates `baml_sdk/` and installs an optimized (`fasttest` profile) `baml_bridge` wheel, built with `gc_profiling`, into `.venv`. It does not touch the editable extension that `sdk_tests` share. Rerun it after changing Rust code or `baml_src/`.

## Running

By default, runs replay a recorded response from a separate server process, so no key is needed and the server's allocations stay out of both heaps. Replay is deterministic apart from how the consumer's pace coalesces partials.

```bash
.venv/bin/python bench.py                                   # 8 sequential sync streams
.venv/bin/python bench.py --api async --concurrency 4       # 8 streams, 4 at a time
.venv/bin/python bench.py --streams 32 --json-out results/a.json
python3 summarize.py results/*.json                         # side-by-side markdown table
```

Useful knobs:

- `--event-delay-ms` sets the replay pacing per SSE event (default 0.5).
- `--warmup` sets the number of unmeasured streams run first (default 1), so compilation and first-use allocations are excluded.
- `--mode live` calls the real provider. `--mode record` refreshes the recording (it takes one stream).

Provider keys are in the `dev-llm-provider-tests` Infisical environment:

```bash
infisical run --env=dev-llm-provider-tests -- .venv/bin/python bench.py --mode record --streams 1
```

### Providers

| `--provider` | Function | Model | Recording |
|---|---|---|---|
| `openai` (default) | `ExtractPullRequestNano` | gpt-5.4-nano | `recordings/nano-pr-5041.sse` |
| `anthropic` | `ExtractPullRequest` | claude-haiku-4-5 | `recordings/haiku-pr-5041.sse` (not yet recorded) |

The Haiku recording is missing because the Anthropic key in `dev-llm-provider-tests` had no credit balance when this was built. Once the key has credit, record it with `--provider anthropic --mode record --streams 1`, then check the file in.

To refresh the seed:

```bash
python3 seed/make_seed.py --pr 5041
```

The PR keeps changing, so the checked-in pair is a snapshot. Record again after refreshing it.
