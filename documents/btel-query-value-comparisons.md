# Comparing captured lists, maps and objects

`baml query` now supports `=` and `!=` between captured structured values,
including paths into different captures. It also accepts a constant plain JSON
value through `baml_value_json(...)`:

```sh
baml query "SELECT call_id, fqn FROM calls
  WHERE output['tags'] = baml_value_json('[\"support\",\"speed\"]')"

baml query "SELECT call_id FROM calls
  WHERE args['customer'] = output['customer']"
```

The literal can appear on either side of `=` or `!=`. It is not a general SQL
constructor: projection, ordering, `IN`, dynamic string arguments and aggregate
modifiers are rejected. Malformed JSON is rejected even if no recordings exist.
Objects in the JSON literal are maps; display markers such as `$class` and
`$ref` do not construct classes or graph references. Compare class values with
other captured class values instead.

To compare outputs of different calls, filter the inputs before the join:

```sql
WITH samples AS MATERIALIZED (
  SELECT call_id, output FROM calls
  WHERE fqn = 'user.Classify' AND status = 'ok'
)
SELECT a.call_id, b.call_id
FROM samples a JOIN samples b ON a.output = b.output
WHERE a.call_id < b.call_id;
```

`MATERIALIZED` matters when unrelated captures contain unsupported or missing
values. SQLite may otherwise evaluate comparisons before other filters. The
query outcome conservatively reports unavailable evidence encountered during
evaluation, even if a later filter removes that row. It never suppresses those
diagnostics to make the answer look complete.

## Equality rules

- Lists compare element by element, with equal lengths and order.
- Maps compare keys and values independently of insertion order.
- Classes compare their recorded declaration names and fields. Classes and
  maps are different kinds; omitted, null and absent fields stay distinct.
- Enums compare their recorded declaration names and variant names. The existing
  scalar SQL string shorthand still matches a variant name, but a JSON string
  is a string, not an enum.
- Bytes compare their full captured contents, regardless of display limits.
- Numbers compare exactly across captured integer/bigint/float kinds. NaNs
  compare equal to NaNs; float `+0.0` and `-0.0` differ, as in the old query
  semantics. Integer zero compares equal to either float zero.
- JSON integers through `u64::MAX` retain integer precision. Larger/exponential
  JSON numbers use the JSON parser's floating-point representation. This avoids
  the old JSON helper's conversion of unsigned integers above `i64::MAX` to
  floats; captured bigints retain their full precision.

CAS IDs, runtime-local type tags, object IDs and sharing topology do not stand
in for equality. Two equal values can be stored in different graphs. Decoding
still verifies each blob's content ID before any comparison.

## Unknown answers and limits

Missing/corrupt blobs and relevant capture truncation produce an incomplete
outcome. Cyclic graphs report `comparison_cycle`; cell-only cycles report
`value_cycle`. Opaque host values, descriptive objects and type values report
`comparison_unsupported`. Comparing a value with itself still requires its
evidence: identical handles cannot prove equality of an unavailable capture.

The comparison validates both reachable values before deciding equality or
inequality. Unsupported children therefore keep the answer incomplete even
when another field differs. Missing paths remain SQL NULL-like non-matches;
captured nulls compare equal to captured nulls or a JSON null literal.

Each comparison has independent limits: depth 64, 100,000 visited nodes and
16 MiB of examined text/keys/bytes across both operands. Shared subtrees count
each time they are expanded. Exceeding a limit reports `comparison_limit`.
JSON literals are limited to 1 MiB and the JSON parser's depth limit. The query
deadline is checked around comparisons; display/render limits do not decide
whether captured values are equal.

The implementation builds bounded borrowed trees over verified snapshots;
it does not convert rendered JSON back into values. CAS hydration retains its
per-query cache. Metadata-only queries still read no CAS blobs. No writer,
VM, capture format or SQLite schema changes are required.

Structured ordering, value-based `DISTINCT`/grouping/`UNION`, cyclic graph
equality and the old CID-literal constructor remain separate work.

## Validation

Reader tests cover reordered maps, changed list contents, sharing, class/enum
identity, field presence, bytes/cells, precision boundaries, NaN/signed zero,
truncation, opaque/cyclic values, argument names and traversal limits. SQL tests
cover literal validation and lowering. Real-engine fixtures compare nested
classes and lists across captures, check lazy CAS reuse and missing-blob
recovery, and separate comparison limits from rendering limits.

The [CLI demo](btel-query-demo/run_demo.sh) records actual `baml run` calls
against its local mock LLM and asserts four matching tag lists and seven equal
output pairs through fresh `baml query` processes. Generated recordings and
the validation transcript remain local. No performance benchmark is part of
this reader-only change.

Validation passed on pinned Rust 1.98.0: all 50 query tests, 15 reader tests,
both CLI query tests and the playground reader integration test. The demo
exited successfully with both new counts asserted. Clippy with warnings denied
passes for both changed crates and all their targets; formatting is clean.
The producer audit reports 120 frozen files, zero changed. TypeScript is
unchanged; no full-workspace build or interactive playground check was run.
