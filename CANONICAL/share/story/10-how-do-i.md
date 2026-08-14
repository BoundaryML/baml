# 10: How do I build …?

**Key points**

- Panels (rates, totals, trends) query the complete layer; drill-downs
  read values from the retained layer behind id-narrowed filters.
- Answers have three shapes: complete (every call counted), selected
  (policy-retained calls only), bounded (an exact tape slice).
- Traffic and error totals come from the complete layer only; row counts
  in `retained_calls` are a policy-selected lower bound.
- A result stream with no query outcome is not a successful answer.

Docs 00–09 defined the concepts; this doc gives recipes: the question in
plain English, the SQL, the queried table's grain, why the query is fast,
and the answer's shape. A **complete** answer comes from the complete layer, where
every call is counted; a **selected** answer from the retained layer,
which keeps only interesting calls; a **bounded** answer from a tape dump,
an exact slice around one moment.

All recipes walk the same ladder:

```text
runs                 which run should I look at?
  └─ calling_contexts   which function, under which parent, misbehaved?
       └─ retained_calls   which exact call can I inspect?
            ├─ args / return / error   what data did it carry?
            └─ tape_dumps              what else was happening right then?
```

Panels query the top of the ladder; investigations descend it.

## 1. Which recent runs had problems?

**English:** list recent runs that failed outright, contained errors, or
have incomplete evidence.

```sql
SELECT run_id, started_at, status, entrypoint,
       total_calls, total_errors, structure_state, value_state
FROM runs
WHERE started_at >= :from_time
  AND (
    total_errors > 0
    OR status IN ('failed', 'panicked', 'abandoned')
    OR structure_state IN ('incomplete', 'lost')
    OR value_state IN ('partial', 'lost')
  )
ORDER BY started_at DESC
LIMIT 100;
```

One row is one run; the table grows with runs, not calls, so a run list is
always cheap. Toy results: `run3` failed (Eve's malformed email reached
the root unhandled); `run1` succeeded but shows `total_errors = 1` from
Bo's handled failure; `run2` is still running, its evidence states
pending, which is normal for an open run (doc 06). The evidence arms name
the bad states (`incomplete`, `lost`, `partial`) rather than
everything-but-`complete`, which would flag every open run; drop the
problem filter to see `run2`'s so-far counters.

The answer is **complete**: every run has a row. The filter mixes two
independent axes by design: execution status and evidence state (doc 06).
The one-word evidence summary for dashboards is still open **[open]**;
until it freezes, filter on the typed state columns.

## 2. Which functions fail most, and where did the time go?

**English:** across every finished call in a time range, rank functions by
failures.

```sql
SELECT definition_key,
       SUM(calls_started) AS calls_started,
       SUM(calls_errored) AS failures,
       1.0 * SUM(calls_errored) / NULLIF(
           SUM(calls_succeeded + calls_errored + calls_cancelled + calls_exited),
           0) AS failure_rate
FROM calling_contexts
WHERE run_id IN (
    SELECT run_id FROM runs
    WHERE started_at >= :from_time AND started_at < :to_time)
  AND definition_key IS NOT NULL
GROUP BY definition_key
HAVING SUM(calls_errored) > 0
ORDER BY failures DESC
LIMIT 50;
```

One row of `calling_contexts` is one calling context in one run; a
million identical calls were already folded into it by the runtime
(doc 03).
`definition_key` is null for synthetic internal functions; the filter
excludes them rather than pooling them into one meaningless group. The
denominator sums only the four finished counters, so unfinished calls,
including `run2`'s open ones, never dilute a failure rate.
`ClassifyCustomer` shows Bo's failure from `run1` even though every
`ProcessCustomer` there succeeded: handled errors stay visible in the
complete layer. The answer is **complete**.

Time spent in one run uses the same table:

```sql
SELECT definition_key,
       SUM(calls_started) AS calls,
       SUM(inclusive_ns) AS inclusive_ns,
       SUM(self_ns) AS self_ns,
       SUM(await_ns) AS await_ns
FROM calling_contexts
WHERE run_id = :run_id
GROUP BY definition_key
ORDER BY inclusive_ns DESC;
```

For `run1`, `ClassifyCustomer` shows 8.20 s inclusive, 0.02 s self, 8.18 s
await: almost all model wait, not compute, the dominant pattern for LLM
programs. Both queries group by resident columns only: no tree recursion,
no value reads, no per-call rows (doc 09 explains why tree questions stay
cheap).

Do not count rows in `retained_calls` for the failure panel: that counts
*retained* failures, a policy-selected subset and a lower bound. Traffic
and error totals always come from the complete layer. This is the most
common mistake when using these tables.

## 3. Drill into one failure

**English:** find the retained calls that explain a failed run, then read
their captured values.

```sql
SELECT call_id, definition_key, status, duration_ns, retention_reasons
FROM retained_calls
WHERE run_id = :run_id
  AND status IN ('failed', 'panicked')
ORDER BY started_at;
```

For `run3` this returns `call3` (`ProcessCustomer`) and the root `call1`,
both with retention reason `error`: a propagating error is retained where
observed, not once per rethrow (the exact retention point is not yet
frozen **[open]**, doc 04). Load values on demand:

```sql
SELECT args, error
FROM retained_calls
WHERE run_id = :run_id AND call_id = :call_id;
```

`args` and `error` are virtual fields (doc 09): the first query touched
only resident columns; this id-narrowed one reads value bodies from the
content-addressed store. For `call3`, `error` holds the `ValidationError`
but `args` does not hold Eve's `Customer`: `call3` is a plain helper, and
helper args are off by policy (doc 05), so `args` evaluates to a typed
unknown with reason `not_captured` (never a silent SQL `NULL`), reconciled
by the query outcome (doc 06). Eve's record is readable in the root
`call1`'s captured args. Neither call here has a return role; a failed call
has none (doc 05).

The answer is **selected**: the calls policy kept, for an unhandled
failure the ones that explain it. Nested value predicates like
`WHERE args['c']['plan'] = 'pro'` use the same surface; the argument
object is keyed by declared parameter names (doc 09). Root shape and
subscript spelling freeze at table-schema v1 **[open]**.

## 4. What exact evidence exists around this incident?

**English:** list the preserved tape slices for a run, then the retained
calls inside one of them.

```sql
SELECT dump_id, trigger, started_at, ended_at, event_count, evidence_state
FROM tape_dumps
WHERE run_id = :run_id
ORDER BY started_at;
```

For `run3`: one row, `dump2`, about 40 structural events preserved because
the root observed an unhandled error. For `run1`: `dump1`, preserved
around the slow `call8`, about 130 events over 6.2 s of surrounding
activity. One row is one preserved slice, not one call and not one time
bucket; the events stay in the dump's sealed artifact (doc 08). To list
the inspectable calls inside a dump:

```sql
SELECT call_id, definition_key, status, started_at
FROM retained_calls
WHERE run_id = :run_id
  AND list_contains(tape_dump_ids, :dump_id)
ORDER BY started_at;
```

The list-membership spelling is freeze work **[open]**. `dump2` also holds
the audit thread's start and cancellation adjacent to Eve's failure:
concurrent context a traceback cannot show (doc 04). The answer is
**bounded**: a dump is an exact record of a slice of the run;
`evidence_state` and `event_count` say how much of one before you open it.

## 5. Did the new revision make it worse?

**English:** compare the same logical function's behavior across
revisions.

```sql
SELECT revision_id, definition_key,
       SUM(calls_started) AS calls,
       SUM(calls_errored) AS failures,
       SUM(inclusive_ns) / NULLIF(SUM(calls_started), 0) AS mean_inclusive_ns
FROM calling_contexts
WHERE definition_key = :definition_key
GROUP BY revision_id, definition_key
ORDER BY revision_id;
```

Group across revisions by `definition_key`, never `function_id`: the dense
per-revision id means nothing outside its revision (doc 07). Keep
`revision_id` in the result; collapsing revisions averages two different
programs into one meaningless number. For `ClassifyCustomer`, `rev1` shows
three started calls and Bo's failure; `rev2` (run `run4`, after the prompt
edit) shows three successes and a smaller mean. Counters are started, not
finished: open runs contribute so-far numbers, so once `run2` reaches
Dee's classification it joins the `rev1` row and dilutes the mean with
so-far time (doc 07); at this snapshot it has not. To check whether the
function itself changed or only its surroundings:

```sql
SELECT revision_id, definition_key, local_definition_hash
FROM functions
WHERE definition_key = :definition_key
ORDER BY revision_id;
```

Between `rev1` and `rev2` only `ClassifyCustomer`'s local definition hash
differs: the prompt edit. This is a local-change signal only: equal hashes
do not prove equal behavior; a callee or shared type may have changed
(doc 07). Both queries are **complete** and cheap: identity columns are
duplicated into the hot aggregate table, so no joins. The comparison is an
investigation signal, not statistical proof of a regression.

## 6. Which model spend is growing, and what are the expensive inputs?

**English:** total token usage and provider errors by model, over a time
range.

```sql
SELECT provider, model,
       SUM(llm_calls) AS calls,
       SUM(input_tokens) AS input_tokens,
       SUM(output_tokens) AS output_tokens,
       SUM(provider_errors) AS provider_errors
FROM llm_usage
WHERE token_state = 'available'
  AND run_id IN (SELECT run_id FROM runs WHERE started_at >= :from_time)
GROUP BY provider, model
ORDER BY input_tokens + output_tokens DESC;
```

One row of `llm_usage` is one run × calling context × provider × model:
aggregate arithmetic, no prompt bodies touched. Cost is your token price
times these sums. The `token_state` filter excludes the example `run1` row,
whose state is `partial` because Bo's failed call reported no usage:
absence of tokens is not zero tokens. A cost dashboard therefore needs a
coverage panel:

```sql
SELECT token_state, SUM(llm_calls) AS calls
FROM llm_usage
WHERE run_id IN (SELECT run_id FROM runs WHERE started_at >= :from_time)
GROUP BY token_state;
```

Spend is **complete over what was measured**; the coverage query says how
much was measured. For the exact expensive inputs, find the worst
(run, context) keys (the by-model rows deliberately carry neither),
dropping the `token_state` filter because this hunt wants the partial rows
too:

```sql
SELECT run_id, node_id,
       SUM(input_tokens + output_tokens) AS tokens
FROM llm_usage
WHERE run_id IN (SELECT run_id FROM runs WHERE started_at >= :from_time)
GROUP BY run_id, node_id
ORDER BY tokens DESC
LIMIT 20;
```

The top (and only) example row is the one the spend panel excluded: `run1`'s
`ClassifyCustomer` context (`context4`), ranked by the tokens it did
report. Take its (run, context) key and descend:

```sql
SELECT call_id, duration_ns, args
FROM retained_calls
WHERE run_id = :run_id AND node_id = :node_id
ORDER BY duration_ns DESC
LIMIT 20;
```

This surfaces `call8`, the slow classification retained by the latency
trigger; its `args` holds Cy's `Customer`. The drill-down is **selected**:
retained LLM calls are examples, not a census. Two open edges: `llm_usage`
is provisional pending the in-flight LLM instrumentation changes
**[open]**, and per-call token counts are not resident columns, so "sort
exact calls by tokens" is not yet expressible **[open]**.

## 7. Is my spawned work healthy?

**English:** which spawned functions produced failed or cancelled work?

```sql
SELECT f.definition_key,
       f.fqn AS child_function,
       SUM(te.spawned) AS spawned,
       SUM(te.errored) AS failed,
       SUM(te.cancelled) AS cancelled
FROM thread_edges AS te
JOIN runs AS r ON r.run_id = te.run_id
JOIN functions AS f
  ON f.revision_id = r.revision_id AND f.function_id = te.child_function_id
WHERE r.started_at >= :from_time
GROUP BY f.definition_key, f.fqn
HAVING SUM(te.errored + te.cancelled) > 0
ORDER BY failed DESC, cancelled DESC;
```

One row of `thread_edges` is one parent-context-to-spawned-function
relationship per run: the complete layer's view of fan-out. The time range
spans revisions, so grouping follows recipe 5's rule: `definition_key`
through the `functions` join, never the per-revision `child_function_id`,
which would split one logical function across revisions. On the example data,
`WriteAuditLog` shows one cancellation: `run3`'s audit thread, cancelled
when the run ended early. Cancellation is deliberately not a *default*
tape trigger (doc 04; the trigger matrix is still freeze work **[open]**),
so nothing exact was preserved because of it; the complete layer still
counted it, which is all this panel needs. `retained_threads` is the
retained-layer counterpart for inspecting individual spawned work: the
same two-layer pattern as calls. The answer is **complete**. Both thread
tables are conditional on concurrency diagnosis being a first-shipped
priority **[open]**.

## 8. Can I trust this dashboard?

**English:** before believing any panel, check what the observability
system itself failed to capture.

```sql
SELECT kind, reason, SUM(count) AS affected_records
FROM evidence_issues
WHERE run_id = :run_id
GROUP BY kind, reason
ORDER BY affected_records DESC;
```

A healthy run returns zero rows; `run1`, `run2`, and `run3` all do. The
hypothetical overloaded run from doc 06 returns one grouped row,
`structure / records_dropped, affected_records = 10,000`: upstream counts
may be undercounts, which changes how to read every panel for that run.

Trust is also per-query. Every query on this page ends with one query
outcome: a typed record stating whether the answer is complete, which
fixed view of the data it ran against (later arrivals are invisible), and
how many value reads were attempted, available, and unavailable, by
reason. Illustratively:

```text
resultState: complete
snapshot:    tableSchemaVersion v1, projectedThrough …
valueEvaluations: attempted 2, available 2, unavailable 0
```

Field names and wire shape freeze with table-schema v1 **[open]**; the rule
does not: a result stream with no outcome may not be treated as a
successful answer, even for a simple count. A budget can expire
mid-stream, a value read can fail, evidence can still be pending; rows
alone cannot report this.

## 9. The dashboard checklist

A dashboard built from these table schemas follows four rules:

- Panels (rates, totals, trends) query the complete layer (`runs`,
  `calling_contexts`, `llm_usage`, `thread_edges`): cheap, never sampled.
- Drill-downs descend to the retained layer (`retained_calls`,
  `tape_dumps`) and touch values only after id-narrowing filters.
- Every page carries an evidence panel: the `evidence_issues` summary for
  whatever is in view.
- Every number surfaces its query outcome; a truncated answer is never
  presented as complete. Check `resultState` on every panel,
  `valueEvaluations` when a drill-down touched virtual value fields, and
  `projectedThrough` on any time-ranged panel (illustrative names; they
  freeze with table-schema v1 **[open]**).

Table coverage: recipes 1–8 exercise `runs`,
`calling_contexts`, `retained_calls`, `tape_dumps`, `functions`,
`llm_usage`, `thread_edges`, and `evidence_issues`. Three tables appear in
no ranking recipe and serve navigation instead: `revisions` resolves a
`revision_id` to one compiled program; `call_sites` will jump a retained
call to the source expression that made it (its producer is not built; the
dictionary is empty today **[open]**); `retained_threads` is recipe 7's
drill-down counterpart, conditional with the other thread tables
**[open]**.

## The agent loop

The recipes are written for people, but the surface is designed to be
used by agents as well **[v1]**. An agent's loop: discover the table schemas with
`baml query --schema` (relations, grains, column semantics); query the
complete layer first; descend to retained evidence only for examples;
check the query outcome before asserting anything; cite claims by `run_id`
and `call_id` so a human can load the same values and verify. The
vocabulary an agent needs is the ten-or-so terms this series defines; doc
11 fits the entire skill into one short file.

## Terms defined here

- **complete** answer: from the complete layer; every call is counted.
- **selected** answer: from the retained layer; only policy-kept calls.
- **bounded** answer: from a tape dump; an exact slice around one moment.

No other new vocabulary. Also introduced: the recipe pattern (English →
SQL → grain → answer shape → outcome) and the investigation ladder (runs →
calling contexts → retained calls → values), with tape dumps alongside for
concurrent context.
