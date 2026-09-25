# The first query on a big recording

The first `baml query` over a recording builds its SQLite index. On this
branch, the first query over a 228 MB recording took 60 seconds. It now
takes 6.6. Small recordings were already fast and stay fast. Adding a
new file to an indexed recording costs the same as before.

Measured 2026-09-25 with release builds of `baml-cli` on a Ryzen 9 5950X,
recordings on NVMe, the index deleted before each run. Median of three:

| Recording | Size | First query before | Now | Peak memory before → now |
| --- | ---: | ---: | ---: | ---: |
| 2,000 calls | 0.5 MB | 0.10 s | 0.06 s | 41 → 39 MB |
| 500 captured calls, like LLM calls | 0.19 MB | 0.07 s | 0.05 s | 34 → 32 MB |
| `tiny`: 203,480 calls | 52 MB | 6.7 s | 1.2 s | 251 → 256 MB |
| `spawn`: 19,992 roots × 32 threads | 228 MB | 60.4 s | 6.6 s | 304 → 722 MB |

A refresh that finds one new file took 38 ms on `tiny` and 133 ms on
`spawn`, before and after. One that finds nothing new takes about 9 ms.

## How big recordings get

Captured values, such as prompts and responses, are stored in separate blob
files that indexing does not read. So prototyping LLM functions produces
well under a megabyte of recording. The `spawn` recording is a stress case:
660,000 threads, each with its own call contexts, which makes 1.94 million
call paths. You would only reach hundreds of megabytes with something like
hours of a busy service recording, or a benchmark loop.

## Why it was slow

SQLite was not the problem by itself. The index was built one fact at a
time:

- Every fact was reconciled on its own: look up its row, insert or update
  it, check whether it conflicts with an earlier one, and add a placeholder
  row for every ID it mentions.
- Then repair passes walked the tables again for path depths, execution
  roots, child time and source locations.
- That came to about 10 million SQL statements for `spawn`. Each one looked
  a row up in a table of millions of rows, through SQLite's default 2 MB
  page cache, so most of those lookups went to disk.

## What changed

A recording with nothing indexed yet has nothing to reconcile against. That
covers every new recording, and a rebuild. Its files are now merged in
memory under the same rules: the first definition wins, a different one is
a conflict with an issue, and IDs without definitions get placeholder rows.
Depths, roots, child time, raise stacks and source locations are computed
once from the final evidence. Each call site's source location is looked up
once, not once per call path.

The merge keeps its rows in fixed-size chunks with their optional fields
packed into flags, so a row of the big tables takes about 60 bytes. Hash
maps holding the rows directly used more than twice that.

Then every row is written once, in key order, 64 rows per statement, in one
transaction. When a table gets more new rows than it already has, its
secondary indexes are dropped and rebuilt afterwards, which is faster than
updating them row by row.

Later files of the same recording still apply the old way, reconciled
against what is indexed, in small transactions.

Smaller changes:

- A refresh uses a 128 MB page cache. Queries keep 64 MB.
- The call-path table lost two of its four indexes. One existed only to
  recompute a path's child time from all its children; each child now adds
  its own time once. The other now holds only paths whose source location
  is not resolved yet, which is almost none. The index schema is version 8,
  so older index files are rebuilt once.
- Fixed: when a later file defines a function differently from an earlier
  one, the source locations already resolved in that function now change to
  `function_conflicted`. Before, only locations resolved afterwards did.

## How it is tested

A new test indexes the same recordings two ways. One index is built all at
once in memory. The other applies one file per transaction through the old
path. Every row of every table must match.

It runs on two sets of recordings. One comes from the real engine: spawns,
captured values, cyclic values, rethrows, awaits of failed threads,
recursion, defers and a crash, split into many small files. The other is
written by hand, so that evidence arrives before what it refers to, and
every kind of conflict the index reports happens at least once. The second
set found the source-location bug above.

## What is left

- Memory. At its peak, the first index of `spawn` uses about 3 bytes of
  memory per byte of recording. A single refresh merges at most 1 GiB of
  files this way; the rest applies the old way.
- The remaining time is mostly SQLite writing about 4 million rows, a 330 MB
  index: call paths, their thread index, aggregates, threads, and the
  commit. Reading and merging the files takes about 1.5 s.
- Queries still cost about 2 µs per row through the views. On `spawn`,
  counting threads takes 1.3 s after the index is built.

## Reproduce

Record the corpus with the recording benchmark, from `baml_language`:

```sh
BAML_TELEMETRY=medium target/release/examples/btel_record_bench btel spawn 19992 <project> 0
BAML_TELEMETRY=medium target/release/examples/btel_record_bench btel tiny 203480 <project> 0
```

Then time the first query, deleting the index before each run:

```sh
rm -f <project>/.baml/btel/query.sqlite*
time baml-cli query --no-progress --from <project> "SELECT count(*) FROM calls"
```
