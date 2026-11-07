# Global local-store query performance

`baml query --local` opens the derived SQLite index, discovers completed BTEL files, reconciles changed evidence, creates the public views, translates SQL, and executes it in a read snapshot. An unchanged store is not decoded again. A store shared by many projects makes query work over unrelated spans particularly expensive.

## Causes and changes

The fixed planner statistics previously assigned every table one million rows. `event_context` actually contains two slots for each function, future, and network span, while each of those three tables independently assumes one million rows. Underestimating the combined context table made SQLite drive all three span branches from that table, probing function/future/network IDs for unrelated contexts, including empty branches. The corrected relative estimate keeps ordinary scans on their own span tables and preserves indexed context-identity lookups. A separate statistics version upgrades existing indexes under the writer lock without rebuilding the schema, rereading recordings, or changing indexed facts.

Final BAML-value projections were rendered before the `ORDER BY ... LIMIT` sorter discarded rows. For a metadata top-N, the translator now keeps handles through an ordered, limited CTE and renders the selected rows outside it. Both levels retain the requested metadata ordering, including positional ordering and offsets. The CTE uses `NOT MATERIALIZED`: forcing materialization of wide span rows was measurably slower. This follows SQLite's [documented co-routine technique for deferring expensive projection work](https://sqlite.org/optoverview.html#using_co_routines_to_defer_work_until_after_the_sorting).

The rewrite is deliberately conservative: it applies to a simple SELECT with a positive literal LIMIT and ordering by unambiguous, projected SQL columns. Value ordering, DISTINCT, grouping, HAVING, non-output/complex ordering, parameterized or unbounded limits, and predicates that might read a rendered output alias retain the ordinary translation. Value predicates still run before the limit. Availability diagnostics continue to describe the values actually evaluated; an unavailable value in a deferred BAML-value projection no longer causes a blob read or diagnostic solely because it was a discarded sort candidate.

## Reproduction

Build optimized examples from `baml_language/`:

```sh
cargo build --release -p baml_query_btel --examples
target/release/examples/global_store_fixture /tmp/btel-global-example 5000 200 1024
target/release/examples/query_bench /tmp/btel-global-example 5 'SELECT count(*) FROM spans' "SELECT * FROM spans WHERE span_type = 'future' AND parent_span_id IS NULL ORDER BY start_time DESC LIMIT 20" 'SELECT * FROM spans ORDER BY start_time DESC LIMIT 20' "SELECT * FROM spans WHERE span_type = 'network_span' ORDER BY start_time DESC LIMIT 20"
```

The fixture command refuses an existing destination directory. It creates 20 project identities in a single store, one process and recording per run, one BTEL file per recording, 200 completed function spans plus one root future and one network span per run, and one distinct captured output per run shared by its function calls. The optional fourth argument controls the output string size; omit it or use zero for metadata-only data. The optional fifth argument is `shuffled` (the default) or `ordered`, controlling whether recording IDs are permuted independently of timestamps or increase with them. No model or network is used. All files and the derived index are below the supplied temporary directory; the user's real store is never opened.

The benchmark reports first-refresh work, translated SQL, query plans, first query timings, and repeated refresh-and-query timings. Every unchanged repeat asserts zero files decoded, zero bytes read by reconciliation, zero reconciliation transactions, and identical result rows. Query value metrics independently report CAS reads and value evaluations. The command's wall time excludes printing its final JSON report, as opposed to CLI timing, which also includes startup, view creation, and serializing recording extents.

For before/after comparisons, build `query_bench` at the baseline and changed revisions. Generate the fixture once, index and measure it with the baseline, and copy the closed SQLite database into a second fixture tree before measuring the changed binary. Recording and CAS files may be hard-linked into that tree; copy `query.sqlite` itself rather than hard-linking it. This keeps file identity stamps unchanged while giving each binary independent planner statistics. The changed binary's first refresh should upgrade statistics with `schema_rebuilt = false`, no decoded/read recording files, and unchanged indexed extents.

## Measured example

Measured on an Apple M3 Max (14 cores), macOS, Rust 1.98.0, release builds, with warm filesystem caches and other compilation work running on the machine. Baseline: `c3ff351043`. This is a constructed reproduction of the scaling problem, not a measurement of the user's private store or its reported 46-second query.

Both measured fixtures contain 5,000 runs across 20 projects, 1,010,000 completed spans, 51,932,500 BTEL bytes, 5,000 CAS blobs totaling 5,373,893 bytes, and approximately 232 MB of SQLite index data. The first table uses shuffled recording IDs, resembling real random IDs, and three unchanged-store repetitions per binary on a reused connection; it excludes refresh and final report serialization. Every corresponding result row and result order was identical.

| Query | Baseline SQL | Changed SQL | Baseline → changed value evaluations | Baseline → changed CAS loads |
| --- | ---: | ---: | ---: | ---: |
| Count all spans | 2,432 ms | 976 ms | 0 → 0 | 0 → 0 |
| Latest 20 root futures, all columns | 561 ms | 28 ms | 792 → 120 | 0 → 0 |
| Latest 20 spans, all columns | 7,608 ms | 5,894 ms | 11,200 → 160 | 7 → 1 |
| Latest 20 network spans, all columns | 577 ms | 23 ms | 1,056 → 160 | 0 → 0 |

An ordered-ID stress case has the same file/row/value shape, but increasing IDs align physical row order with increasing timestamps. Every next function becomes a top-N candidate, making discarded-row rendering particularly expensive. Reproduce it with a new fixture directory and a final `ordered` argument. Medians below use three baseline and five changed repetitions:

| Query | Baseline SQL | Changed SQL | Baseline → changed value evaluations | Baseline → changed CAS loads |
| --- | ---: | ---: | ---: | ---: |
| Count all spans | 2,427 ms | 1,192 ms | 0 → 0 | 0 → 0 |
| Latest 20 root futures, all columns | 569 ms | 36 ms | 30,000 → 120 | 0 → 0 |
| Latest 20 spans, all columns | 10,038 ms | 6,734 ms | 8,000,000 → 160 | 5,000 → 1 |
| Latest 20 network spans, all columns | 582 ms | 28 ms | 40,000 → 160 | 0 → 0 |

In the ordered-ID case, baseline first indexing took 6.69 seconds and decoded all 5,000 files. The changed binary upgraded a copy of that existing index in 236 ms, with no schema rebuild, no files decoded, and no recording bytes read. Repeated refresh costs remained proportional to discovering/checking the 5,000 files (roughly 0.1–0.2 seconds on this machine); total refresh-and-query medians were 1,391 ms, 191 ms, 6,884 ms, and 152 ms respectively. A separate interleaved check (three fresh processes per binary/query, independent indexes, identical result-row hashes) measured harness wall-time medians of 2,732 → 1,340 ms for counts, 812 → 229 ms for roots, 11,181 → 7,808 ms for latest spans, and 790 → 197 ms for network spans. This includes opening, view creation, and refresh, but not the CLI wrapper or full extent serialization. Captured outputs here are only about 1 KiB each: the eliminated blob reads and renders can matter much more for large values, while the remaining global metadata scan is still visible in the latest-span timing.

## Regression coverage

- Planner gates verify that unfiltered span and announcement branches do not scan all contexts and that selective context identities still use both context indexes without scanning span tables.
- An upgrade test preserves an indexed recording and its watermark while replacing old planner statistics in place.
- Real engine recordings verify typed value results, metadata/positional ordering, offsets, reversed unordered SQLite scans, CAS loads bounded by the SQL top-N, value predicates, and value ordering.
- Translation gates retain the ordinary path for DISTINCT, grouping, value ordering, output aliases used in predicates/joins, and other unsupported rewrite shapes.

## Remaining scaling limits

Refresh still discovers and stats every recording file and checks its ledger entry under the writer lock. It scales with file count; `--verify` additionally hashes indexed contents. First indexing and changed-file reconciliation still scale with the evidence, and process-profiler construction has existing process-to-recording lookups that can scan recording metadata. Query outcomes still include every recording's extent.

Unfiltered counts and metadata sorts still scan matching span rows. In particular, formatted start times are computed before a global time sort: LIMIT bounds final rendering, not that scan. Filters and sorts that depend on captured values necessarily decode candidates before choosing rows. SQL scalar expressions that already render values retain their existing evaluation work, even when other value columns are deferred. Process-ID filters also retain their existing SQL/index behavior. This change removes avoidable context scans and discarded-row value rendering; it does not make every global-store query independent of history size.
