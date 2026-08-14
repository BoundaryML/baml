# 06: When the evidence itself is incomplete

**Key points**

- Missing evidence is a first-class fact, recorded, counted, and queryable
  in `evidence_issues`.
- Run status and evidence completeness are independent axes; neither
  implies the other.
- Every query ends with a typed outcome grading its own completeness; no
  outcome means no answer.
- Completeness is measured against capture policy's promise, not against
  everything the program did.

## The failure mode observability tools hide

Every observability tool drops data sometimes: a buffer fills under load,
or a process dies mid-write. Most tools drop silently. A dashboard shows
90,000 calls instead of 100,000 with no indication the other 10,000
existed; "it never happened" and "the record was lost" look identical.
For an LLM application the ambiguity appears during incidents, when
failure counts matter most.
<!-- framing per doc brief item 1; fresh phrasing: review -->

Studio's rule is the opposite: **missing evidence is a first-class
fact**. Everything Studio records about a program (aggregate rows,
retained calls, tape dumps, captured values) is its **evidence**. When
promised evidence is missing, the absence is itself recorded, counted,
and queryable, like the data it stands in for.

## Two different kinds of failure

An *application error* is the program failing while Studio watched. In
run `run1`, Bo's `ClassifyCustomer` call errored with a provider HTTP
500. The error is counted in the `calling_contexts` aggregate
(`errored = 1` on the ClassifyCustomer context), the error value was
captured under LLM capture policy, and the run's evidence is complete.

An **evidence issue** is Studio failing to watch: somewhere between the
call happening and the query, the recording machinery hit a limit, lost
data, or produced something it cannot vouch for, even if the program ran
correctly. These failures go in `evidence_issues`, not an `errors`
table: "error" is the program's word, and the separate table keeps the
two apart.

Runs `run1`–`run3` are healthy (zero issue rows), so this example row
comes from a hypothetical run `runX` under heavy load:

```text
run=runX  source=profiler  kind=structure  reason=records_dropped
count=10,000  first_seen=12:00:01  last_seen=12:00:04
```

The runtime's recorder (internally, the profiler) dropped 10,000
structural-event records over three seconds: one row, one typed reason,
one count, not 10,000 rows. The aggregates for `runX` may therefore be
missing calls; anything computed over the run is a lower bound, and this
row says so.
<!-- example row: toy-program.md / reader brief §7 -->

## What counts as an issue, and what does not

Four things belong in `evidence_issues`:

- **Dropped records.** Structural events discarded under pressure, as in
  the `runX` row.
- **Lost values.** A promised value capture (for example, an LLM call's
  prompt) that could not be kept. Losing a promise is an issue.
- **Corrupt records.** Stored evidence that fails integrity checks on
  read. Studio reports rather than repairing or guessing.
- **Conflicting evidence.** Two records that cannot both be true. Rare;
  surfaced, not silently resolved.

Two kinds of apparent loss are deliberately *not* issues:

- **Normal tape rotation.** The rolling tape overwriting its oldest
  events is the design working as intended (doc 04); no issue row.
- **Policy non-capture.** A helper's arguments not captured because
  capture policy says not to (doc 05) is a decision, not a loss; it
  appears as the per-value state `not_captured`, never as an issue.

A healthy run has zero rows in `evidence_issues`, so any row that appears
signals a real problem.

The runtime already counts most of these events (every bounded buffer
that discards keeps a counter, and loss markers are written on several
paths [built]), but not every loss path is consistently persisted into
per-run, queryable diagnostics yet. Closing that gap, so every material
loss becomes a queryable issue row, is a v1 correctness gate [v1].
<!-- profiler-tape §4 "no silent truncation" + §6.10 -->

## Status and evidence are independent axes

A run's **status** answers how the program did. Its evidence answers how
much of what Studio promised is present. Neither implies the other:

|  | Evidence complete | Evidence incomplete |
|---|---|---|
| **Run succeeded** | `run1`: trust every number | `runX`: succeeded, but 10,000 events are missing |
| **Run failed** | `run3`: the failure is fully evidenced | worst case, and both facts are still labeled |

`run3` failed (Eve's malformed email threw an unhandled error), but its
evidence is complete: tape dump `dump2`, the retained calls, and the
captured error value are present, so the failure can be debugged.
`runX` succeeded, yet its numbers are suspect. Program success does not
imply data trustworthiness.

The still-running `run2` is a third case: its evidence is *pending*, not
incomplete. Counters for a running run are explicitly "so far"; running
data is never presented as final [v1].
<!-- query-system: pending, D15 -->

## One simple surface, many precise reasons

Evidence health has several dimensions: call-structure completeness,
presence of each promised value, integrity of stored records, processing
completion, and intactness of everything policy retained. The table
schemas carry each as its own run-level state column (doc 09 calls these
the run's **evidence axes**); each axis is a rollup of finer-grained
per-call or per-value states. Doc 05 listed the per-value states
(`available`, `not_captured`, `omitted`, `redacted`, `lost`, `truncated`,
`corrupt`, `unsupported` (the machinery could not represent the value),
plus `pending` while a run is open), defined so that no two different
situations collapse into one label [v1].

A dashboard needs one label, not nine. The run's **evidence state**
summarizes whether the promised evidence is ready and trustworthy: a
small enum, on the order of *ready / partial / pending / unavailable /
untrusted*, with the typed reasons one drill-down away. The exact enum,
and the mapping from the fine-grained states onto it, is an open design
decision [open]. The settled direction is a simple summary on top, typed
reasons preserved underneath.
<!-- reader brief §8; enum wording illustrative, not frozen -->

## The query outcome: every answer grades itself

Run-level evidence state covers one run. Most questions span many runs,
so each answer must grade itself. Every query ends with exactly one typed
**query outcome**, delivered alongside the rows rather than as a fake
final row: whether the query completed, whether the answer was complete,
and, when values were involved, how many were attempted, available, and
unavailable, by typed reason [v1, settled decision D13].

Example: of the retained `ClassifyCustomer` calls that succeeded this
morning, how many returned `Approve`?

```sql
SELECT count(*) AS approvals
FROM retained_calls
WHERE definition_key = 'ClassifyCustomer'
  AND status = 'succeeded'
  AND "return" = 'Approve';
-- syntax illustrative; column spellings freeze with the v1 table schemas [open]
```

The `status` filter is required: `call6` is also a retained
`ClassifyCustomer` call, but it failed, and a failed call has an error
value and no return (doc 05), so a question about return values must
scope itself to calls that have one. Over the healthy runs the query
touches one call, `call8`, its return is available, and the outcome
confirms completeness. Widening the window to include `runX` adds two
more succeeded retained `ClassifyCustomer` calls, so the query touches
three, and the overload that dropped 10,000 records also lost one of
their promised return values.

The lost value does not become SQL `NULL`, which would conflate "the
value was null" with "the evidence is missing" and return a count one
short with no warning. It evaluates to a **typed unknown**: a marked
non-answer carrying its reason, never `NULL`, never a silent non-match
[v1, settled decision D12]. A captured BAML `null` is ordinary data; the
two are never confused. The outcome reconciles the totals:

```json
{
  "resultState": "incomplete",
  "valueEvaluations": {
    "attempted": 3,
    "available": 2,
    "unavailable": 1,
    "byReason": { "lost": 1 }
  }
}
```

<!-- adapted from query-system §8 illustrative outcome; field names and wire shape are not frozen [open] -->

The count is a labeled lower bound instead of a silent undercount.

**No outcome means no answer.** If a result stream ends without its
outcome (a dropped connection, a truncated response), the caller may not
claim the query succeeded. Even a failure of the reporting channel is
detectable, because its absence is the signal [v1].
<!-- query-system M5 -->

Doc 05 showed the same principle: `llm_usage` marks `run1`'s token totals
`partial` because Bo's failed call reported no usage, and absent tokens
are not zero tokens. The query outcome applies that rule to every answer.

## What "all promised evidence" means

A run's evidence being complete means everything capture policy promised
for that run is present, not that every value of every call was recorded.
Helpers' values are mostly not captured, by design (doc 05). A run whose
helper arguments were never captured is complete: nothing promised is
missing. A run whose promised LLM prompt was lost is incomplete, however
well the program ran. Completeness is measured against capture policy's
promise.

### Error storms do not flood the table

Application failures are not evidence issues: a million provider errors
are counted in the aggregates like any other calls. When recording
pressure does cause loss, `evidence_issues` groups by scope and typed
reason with a `count`: one row for 10,000 dropped records, never 10,000
rows [v1]. <!-- query-system: evidence_issues_v1 grouping -->

### Why self-reported failures are credible

Losses are counted at the bound that causes them: a buffer that
overwrites knows how many records it overwrote at the moment it does so.
The design rule is that bounded never means silent: every limit has a
counter, a marker, or an explicit error [v1]. Most limits meet the rule
today [built], but a few internal counters can still saturate silently at
extreme scale; marking those, like making every loss queryable, is part
of the same v1 gate above. And the mandatory query outcome means a
failure of the reporting channel surfaces as a missing outcome, not a
plausible answer.
<!-- profiler-tape §4 -->

## Terms defined here

- **Evidence**: everything Studio records about a program: aggregates,
  retained calls, tape dumps, values.
- **Evidence issue**: a recorded failure of Studio's own observation
  machinery, grouped by scope and typed reason in `evidence_issues`.
- **Evidence state**: the run-level summary of whether promised evidence
  is ready and trustworthy; the exact enum is an open decision.
- **Query outcome**: the mandatory typed record ending every query,
  reconciling attempted, available, and unavailable values. No outcome, no
  answer.
- **Typed unknown**: an unavailable value as a query sees it: a marked
  non-answer with a reason, never a silent `NULL`.
