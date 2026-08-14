# 11: The agent skill

**Key points**

- The terminology's acceptance test: an agent must be able to query Studio
  from one file far smaller than the design corpus behind it.
- The draft skill below is that file: ten terms, the table schemas in one table,
  seven query rules, the outcome contract, three worked queries, and the
  claims an agent must never make.
- The skill adds no new claims; every line traces to one earlier doc.

<!-- Sources: compressed from docs 09 and 10 of this set; the seven rules
adapt the "Query rules" list in share/query-examples.md; outcome and
typed-unknown semantics from the query-system fact pack (D12/D13); statuses
from the decisions-plan fact pack; skill-as-test framing from
founder-concerns (Vaibhav #17). -->

Everything between the rulers below is a **draft skill**: self-contained
instructions an agent loads before querying Studio. The skill doubles as
an acceptance test for the vocabulary: if it is usable on its own, the
terminology is sufficient; if it requires the other ten docs, the names
need revision before the table-schema freeze.

The skill targets the v1 table schemas. The SQL semantics are settled, but the
`baml query` command that executes them is not on the branch today
**[v1]**; what exists now is the capture machinery beneath the tables
**[built]**. Table names are the reader-facing proposals from doc 09; the
shipped table schemas add a version suffix (`runs_v1`), and the rename set
is a table-schema-freeze decision **[open]**.

---

## SKILL (draft): querying BAML Studio

**Purpose.** You are answering questions about a BAML program's behavior
through Studio's SQL surface. This file is everything you need. Discover
exact column lists with `baml query --schema`; never guess a column name.

**The ten terms.**

1. **Run**: one top-level BAML execution. A server process hosts many runs;
   runs never nest.
2. **Call**: one function invocation inside a run. Calls form a tree under
   the run's entry call.
3. **Calling context**: one path of parent functions down to one function.
   Every invocation along the same path folds into one aggregate row, so a
   million identical calls cost one row.
4. **Complete layer**: the tables where every call is counted: totals,
   never samples. Use it for rates, traffic, and time.
5. **Retained call**: an individual call kept by policy (error, latency,
   membership in a dump, promotion, explicit capture). The retained layer
   holds selected evidence, never totals.
6. **Tape dump**: one preserved slice of the rolling tape of structural
   events (call and thread starts and ends, suspensions), kept around an
   incident or on manual request; the row's `trigger` column says why.
   Events only, no value bodies; may cover only part of a run.
7. **Virtual value fields**: `args`, `return`, `error` on `retained_calls`.
   Not stored columns: values load on demand when a statement needs them.
   Ordinary SQL applies: `WHERE args['c']['plan'] = 'pro'`; the argument
   root shape and subscript spelling freeze at table-schema v1 **[open]**.
8. **Typed unknown**: what a predicate over an unavailable value (redacted,
   lost, never captured) evaluates to. Never a silent `NULL` or quiet
   non-match; the query outcome reconciles it. A captured null is data.
9. **Evidence issue**: a grouped record of data Studio itself failed to
   keep. Zero rows means healthy. Independent of whether the program failed.
10. **definition_key vs revision**: a `revision_id` names one exact
    compiled program; a `definition_key` names the same logical function
    across revisions.

**The table schemas.** A relation's grain is what one row stands for.

| Relation | One row is | Layer | Key joins |
|---|---|---|---|
| `runs` | one run | complete | `run_id` reaches every run-scoped table; `revision_id` reaches the code-identity tables |
| `calling_contexts` | one calling context in one run | complete | `run_id`+`node_id` → `retained_calls`, `llm_usage`; `parent_node_id` rebuilds the tree |
| `llm_usage` | run × context × provider × model | complete **[open]** | `run_id`+`node_id` → `calling_contexts` |
| `thread_edges` | run × spawning context × spawned function | complete **[open]** | `edge_id` → `retained_threads` |
| `retained_calls` | one kept call | selected | `node_id` → its aggregate row; `tape_dump_ids` → `tape_dumps`; `call_site_id` → `call_sites` |
| `tape_dumps` | one preserved tape slice | selected | `trigger_call_id` → `retained_calls` |
| `retained_threads` | one kept spawned thread | selected **[open]** | `edge_id` → `thread_edges` |
| `evidence_issues` | one grouped loss report (source × kind × reason, with a count) | health | `run_id` |
| `functions`, `call_sites`, `revisions` | one function / call expression / compiled program per revision | code identity | `revision_id`+`function_id` from the complete layer; the retained layer joins through `node_id` or `run_id` |

**The seven rules.**

1. Use `runs` to find a run.
2. Use `calling_contexts` for complete all-call totals.
3. Use `retained_calls` only for selected exact evidence.
4. Filter on small resident columns before touching `args`, `return`, or
   `error`: values hydrate on demand and are the expensive part.
5. Group across revisions by `definition_key`, never by `function_id`: the
   dense per-revision id is meaningless outside its revision.
6. Treat `local_definition_hash` as a local-change signal only; equal hashes
   do not prove equal behavior, because a callee may have changed.
7. Check per-role value states, `evidence_issues`, and the query outcome
   before claiming completeness.

**The outcome contract.** Every SQL stream ends with exactly one typed
out-of-band **query outcome**: whether the result is complete, the fixed
snapshot it ran against (later arrivals are invisible to it), and how many
value reads were attempted, available, and unavailable, by reason. Rows
streamed before a late failure are incomplete. A stream with no outcome may
never be reported as a successful answer. Field names freeze with
table-schema v1 **[open]**; the rule does not.

**Three worked examples.** English first, always.

Which functions fail most, across every call ever made? (complete)

```sql
SELECT definition_key, SUM(calls_errored) AS failures
FROM calling_contexts
GROUP BY definition_key
HAVING SUM(calls_errored) > 0
ORDER BY failures DESC
LIMIT 20;
```

Handled errors stay visible here: a call can fail inside a run that
succeeded.

What exactly went wrong in this failed run? (selected)

```sql
SELECT call_id, definition_key, retention_reasons, error
FROM retained_calls
WHERE run_id = :run_id AND status IN ('failed', 'panicked')
ORDER BY started_at;
```

`error` is a virtual value field, hydrated for these few rows only, after
the resident filters ran. One propagating error is retained where it was
observed, not once per rethrow; the exact retention point for a
propagating error is not yet frozen **[open]**.

Can I trust the data for this run? (health)

```sql
SELECT kind, reason, SUM(count) AS affected_records
FROM evidence_issues
WHERE run_id = :run_id
GROUP BY kind, reason;
```

Zero rows means Studio kept everything capture policy promised. Then read
the query outcome before reporting anything.

**Never claim:**

- completeness without the query outcome: no outcome, no answer;
- traffic or failure totals from `retained_calls`, `retained_threads`, or
  `tape_dumps`: retained counts are policy-selected lower bounds;
- that a typed unknown is `NULL`, zero, or a non-match: report its reason
  and mark the answer incomplete;
- that missing token counts are zero tokens: respect `token_state`;
- that equal `local_definition_hash` values mean equal behavior;
- that a tape dump covers a whole run: it is a bounded slice; check
  `event_count` and its evidence state first;
- program health from `evidence_issues`, or evidence health from run
  `status`: the two are independent axes.

---

The skill is about a hundred lines. Run against this set's example
program:
the first example returns three rows, `ClassifyCustomer` with Bo's handled
failure from `run1` plus the two frames of Eve's one unhandled error from
`run3`. The second, pointed at `run3`, returns `call3` and the root
`call1`, both retained for reason `error`, and hydrates Eve's
`ValidationError` once. The third returns zero rows for all three runs;
all three are healthy.

Every line traces to one doc: the terms to 02 through 07, the table
schemas to 09, the rules and examples to 10, the outcome contract to 06.
The skill adds nothing new, by design. The test is whether a newcomer,
human or agent, can query Studio from this file alone; on this draft,
they can.

**Terms defined here:** none, deliberately. This doc compresses what the
set already taught.
