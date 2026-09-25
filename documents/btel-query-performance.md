# Fast reader performance checks

The short benchmark measures the current reader and SQLite queries against a
fixed corpus of real engine recordings. It does not measure recording speed.
Build and prepare once, then rerun only `run` while changing the reader.

From `baml_language/`, build the release binaries:

```sh
CARGO_INCREMENTAL=0 cargo +1.98.0 build --release --locked \
  -p baml_cli --bin baml-cli \
  -p bex_engine --example btel_record_bench \
  -p baml_query_btel --example query_bench
python3 tools/btel_query_smoke.py prepare
```

The fixture set lives in `target/btel-query-smoke/`. It contains 10,000 tiny
roots, 128 roots with 1,000 leaf calls each, 256 roots that each spawn 32
children, 10,000 calls with repeated captures, and 512 distinct captures with
a 4 KiB string argument. The repeated-capture case exercises a populated
`calls` table without thousands of distinct blobs. The capture
fixture uses the existing benchmark's VM capture setup; no network requests
are involved. Files target 16 KiB to exercise incremental refresh. These are
small iteration workloads, not a replacement for large-history stress tests.

Run the short check:

```sh
python3 tools/btel_query_smoke.py run --output target/reader-before.json
```

After a reader change, rebuild the CLI and query example, then compare on the
**same fixtures**:

```sh
CARGO_INCREMENTAL=0 cargo +1.98.0 build --release --locked \
  -p baml_cli --bin baml-cli \
  -p bex_engine --example btel_record_bench \
  -p baml_query_btel --example query_bench
python3 tools/btel_query_smoke.py run \
  --output target/reader-after.json --compare target/reader-before.json
```

For a focused check, select just the affected workload and query:

```sh
python3 tools/btel_query_smoke.py run --workloads spawn \
  --queries function_stats --repeats 7 \
  --output target/reader-focused.json --compare target/reader-before.json
```

The intended iteration run is under two minutes, excluding compilation and
fixture preparation. The default whole-run budget is 120 seconds, with a
45-second limit per subprocess; an overrun fails rather than running for an
hour. Failed runs save a report marked `failed`, including completed samples
and the reason, and exit nonzero; they cannot be used as a comparison baseline.
Three measured repetitions follow an untimed warmup. The report gives
the median and observed range, refresh time, BTEL bytes read, CAS loads and
CAS bytes read. Each fresh-index and one-file-addition measurement runs once
per workload; repeat a focused run to establish their variability. It records
fixture and executable hashes so comparisons can
be traced to the inputs used. `--compare` also fails if any query's result
rows changed. Results and recordings stay local under the ignored `target/`.

For each fixture the harness:

1. Builds an index from every segment except the last, starting with no
   SQLite database.
2. Adds that last segment and verifies exactly one file is decoded, with no
   reads of previously indexed BTEL bytes.
3. Measures count, recent executions, function statistics, call-path
   statistics and hot call paths. Both capture fixtures also filter `args['n']`;
   the repeated case must load one blob and the unique case one per capture.
4. Runs those queries both through fresh CLI processes and through one reused
   `Index` connection, like the playground. Checks that repeated refreshes
   read no BTEL bytes and write no transactions, and metadata queries load
   no CAS blobs.

Every run creates and removes its own scratch directory beside the fixtures.
Immutable source files are hard-linked (copied when linking is unavailable).
The original fixture files and any existing project databases are untouched.
The manifest hashes are checked before use. An existing fixture directory or
output report is never overwritten.

"First index" means **no SQLite index**, not cold storage: the OS page cache
is not evicted. All repeated measurements use warm OS caches. Do not compare
these directly with the old harness's page-cache-eviction results. Timings on
a shared workstation are descriptive; small differences within the ranges
are noise. The longer `tools/btel_query_bench.py` remains available for
explicit recording-overhead, cold-storage and history-scaling investigations.

## First reader optimization

`call_path_stats` previously summed each context's synchronous child durations
three times per query: for child time, self time and the self-time state.
`function_stats` and `hot_call_paths` inherit that work through the view.

The index now keeps `call_path.direct_child_ticks`. Each indexing transaction
collects children with a new definition or a normal-node aggregate delta,
finds their retained parent definitions, and recomputes each affected parent's
sum once. The cached ticks commit alongside the evidence and watermark. The
public SQL columns and their meanings are unchanged.

This deliberately moves some work into indexing. It does not cache clock
validity or converted nanoseconds: a later clock invalidation still affects
queries immediately. Reentry and spawned work remain excluded from child
time. Late definitions repair the relevant parent; overflow stays NULL;
removing/changing source files follows the existing transactional rebuild.

This optimization advanced the SQLite schema to version 4, so an older derived database was rebuilt
once from its BTEL files. BTEL, CAS and the producer are unchanged. The new
structure test covers late child definitions, subsequent deltas, recursion,
spawn exclusion, invalidation, deletion/rebuild and overflow. Existing query,
CLI and playground integration tests exercise the same public results.

Keep generated before/after reports under `target/`; do not commit recordings
or raw measurements. Check the query gain against fresh-index and incremental
costs before accepting an optimization. The remaining correlated lookups in
`executions`, view evaluation and full-blob CAS reads are separate work items.

The later [population-outcome extension](btel-query-outcomes.md) advances the
cache to version 5 and adds a separate short recording comparator. When
comparing across a public schema extension, use explicit projections: the
default `SELECT *` statistics queries acquire columns and therefore correctly
fail the unchanged-row check against an earlier schema. Existing fixtures can
still test backwards compatibility; their new outcome columns are unknown.
