# 04: Keeping the interesting ones

**Key points**

- Aggregate rows count every call but identify none; exact evidence
  records individual calls and moments, selected by policy, never by
  traffic volume.
- The rolling tape keeps recent structural events in a fixed block of
  memory, recording execution shape only, never data bodies.
- A trigger (an unhandled error at the run root, a slow call, or a manual
  request) copies the tape into a durable, rate-limited dump and can mark
  individual calls for retention.
- A retained call is a single invocation kept with a retention reason;
  counting retained calls measures retention policy, never traffic.

## How evidence is kept

Studio keeps exact evidence in three steps:

1. The runtime appends a small structural event for every call to a
   bounded in-memory tape.
2. A trigger fires: an unhandled error reaching the run root, a call
   crossing the latency threshold, or a manual request.
3. The trigger produces two kinds of durable evidence at once: it copies
   the current tape window into a dump (a preserved moment, one ledger
   row in `tape_dumps`), and qualifying calls get `retained_calls` rows
   (preserved individuals, each with a retention reason).

The sections below give the details.

## The question aggregates cannot answer

Doc 03 produced four aggregate rows for run `run1`. The `context4` row
(ClassifyCustomer) shows three calls started, two succeeded, one errored,
and almost all of its 8.20 seconds spent awaiting. The row is exact,
complete, and by design anonymous: it cannot say which call errored,
which was slow, or what else the run was doing at the time.

The complete layer answers "how often, how slow, how many errors?" for
every call. Questions about a specific call need the second layer: exact
evidence about individual events, kept only for the interesting few.
<!-- "exact evidence" is spine language from doc 00; the term of art "evidence" gets its full treatment in doc 06 -->

Two kinds of exact evidence exist, both selected by policy rather than
traffic volume: a preserved moment (a tape dump, one ledger row in
`tape_dumps`) and a preserved individual (a retained call, one row in
`retained_calls`). "Tape dumps and retained calls" below introduces the
two together, after two prerequisites: the structural event and the
rolling tape.

## Shape, not data

Structural events are not function inputs and outputs. The shape/data
distinction carries through the rest of the document set.

A **structural event** is a small, fixed-size fact about the *shape* of
execution: a call started or ended, a logical thread was spawned, paused
awaiting an LLM response, or resumed. Each event names what it relates
(call, parent call, function, logical thread) plus a timestamp, and never
carries the data flowing through the program. **[built]**
<!-- source: profiler-tape §1 "Structural record", §8 record table -->

Structural events, in reader terms:

| Event | What the record holds |
|---|---|
| call started | logical thread, call id, parent call, function, timestamp, source position of the call *(an optional field; emitting and resolving it end-to-end is committed v1 work)* |
| call ended | call id, timestamp, how it ended (succeeded / errored / cancelled) |
| thread started | new logical thread, parent thread, the call that spawned it |
| thread ended | thread, timestamp, how it ended |
| thread paused / resumed | thread, timestamps, why it paused (awaiting a result, an operation) |
| LLM call metadata | model, token counts, provider/parse/retry flags |

The common call start/end pair costs 54 and 26 bytes; no record exceeds
292 bytes. **[built]** Because records are small and bounded, the runtime
produces one for *every* call (the same stream that feeds doc 03's fold)
at a cost independent of the program's data.
<!-- source: profiler-tape §2.1, §8: 54 B call entry, 26 B exit, max 292 B -->

Values (argument, return, and error bodies) are arbitrarily large user
data, handled by separate machinery with separate rules (doc 05). The
tape contains shape, never data: a saved slice can show a ProcessCustomer
call failed at 0.19 seconds on thread `thread1`, not the customer's email
address.

## The rolling tape

A busy service produces millions of structural events. Storing all of
them would reintroduce the per-call storage cost that doc 03 rejects, so
the runtime keeps a bounded window of recent events instead.

The **rolling tape** is a bounded, in-memory sequence of the most recent
structural events. New events append at one end; once memory is full, the
oldest are overwritten. Nothing on the tape is durable; the cost is a
fixed block of memory. **[built]**

```text
                             overwritten ──▶ gone forever
        ┌──────────────── rolling tape (fixed memory) ────────────────┐
 ...e408│ e409  e410  e411  e412  e413  e414  e415  e416  e417  e418 │◀── newest
        └─────────────────────────────────────────────────────────────┘
              nothing here is durable until a trigger promotes it
```

In the implementation this is the **flight recorder**, a ring of recent
raw event bytes, plus a small **recent-call ring** of the last few
thousand completed calls per spawn tree (one per run in the example
program). **[built]** The rest of this document set says "the tape."
<!-- source: profiler-tape §1 "Flight recorder", §2.4, §2.5; "partition" = spawn tree -->

Two implementation defaults on the current branch (not product policy)
set the trade-off. The tape is 16 MiB, roughly 200,000 call start/end
pairs: about 11 seconds of history for a typical busy agent workload,
about 21 milliseconds for a pathological hot loop. **[built]** The
asymmetry is intentional: a hot loop that floods the tape is still
counted exactly by the aggregate rows from doc 03. The tape provides
context, not counting.
<!-- source: profiler-tape §2.5.2, measured corpus figures; not SLOs -->

The same bound covers the case of very deep or very busy runs: the tape
never grows, so it cannot be overwhelmed. A burst of events means less
wall-clock coverage.

## Tape dumps and retained calls

A **trigger** is the policy decision that an instant deserves exact
evidence: an unhandled error reaching the run root, a call crossing the
latency threshold, or a manual request ("What fires a trigger", below,
gives the policy). A firing trigger leaves durable records in two tables
at once.
<!-- source: profiler-tape §1 "Trigger" -->

- It copies the current tape contents into a durable **saved tape slice**
  (a **tape dump**) before the ring recycles them, with one ledger row in
  `tape_dumps` recording what fired the dump, which run it belongs to,
  and what span of activity it covers. A dump preserves a *moment*: a
  window of time and every call and thread inside it, as structural
  events only, with no argument, return, or error bodies. A dump answers
  "what else was going on at that moment?"
- It can mark individual calls for retention: one row each in
  `retained_calls`. A **retained call** preserves an *individual*: one
  invocation with a durable, queryable record of identity, timing,
  outcome, and links to whatever else is known about it; captured values
  attach under doc 05's rules and surface as per-role fields on the row
  (doc 09). Every retained call carries a **retention reason**, and a
  trigger is one source of reasons among several, so a row can exist with
  no dump behind it. A retained row answers "show me that call."

The example program's two triggers fill both tables.

### What run1 leaves behind

In run `run1`, Cy's ClassifyCustomer call `call8` completes at 6.2
seconds and crosses the program's latency threshold. The trigger leaves
one record in each table:

- `dump1`, a copy of the tape: ~130 events, the 6.2 seconds ending at the
  trigger.
- a `retained_calls` row for `call8`: ClassifyCustomer, succeeded at
  6.2 s, retention reason "slow call".

The two records link rather than overlap. `call8`'s row lists `dump1`,
the dump that covers it; a dump never retains the calls inside it.
`dump1` holds every call and thread active in its window, but only
`call8` has a row.

A dump reaches backwards from its trigger, never forward: it is the
tape's contents at the moment the trigger fires. It may therefore cover
only *part* of a long run: a dump taken in hour two of a long run holds
the tape's window, not the run's history.

`run1` also leaves a second retained row, with no trigger and no dump
behind it. `call6`, Bo's ClassifyCustomer call, failed with a provider
HTTP 500 that ProcessCustomer handled, so no dump fired; but its error
body is kept under the value rules for LLM functions (doc 05), and that
earns the call a row. Its dump list is empty.

### What run3 leaves behind

In run `run3`, Eve's malformed email makes `validate_email` throw;
nothing handles the error, the run fails, and the error trigger fires at
the root. It leaves one dump and two rows:

- `dump2`: ~40 events. `run3` is short enough to fit inside the tape's
  window, so the dump holds the entire run.
- two `retained_calls` rows: `call1` (ProcessCustomers, the root that
  observed the unhandled error) and `call3` (ProcessCustomer, the frame
  that threw), both retained for the same propagating error.

`call2`, the audit write cancelled at run end, appears inside `dump2` as
an event and has no row of its own. Ada's healthy calls in `run1` have no
rows either; absence from `retained_calls` is the normal case.

```text
              time ──▶
run1   ...earlier activity ─────────────────────────────╖  trigger:
                  ◀═══ dump1 window: 6.2 s ═════════════╜  call8 completes

run3   ╟════════ dump2 window: the whole run ═══════════╖  trigger: error
       0.00 s                                    0.20 s ╜  reaches the root
```

### The stored rows

For the example program, the two tables hold six rows in total: two in
the dump ledger `tape_dumps` (a proposed reader-facing table; internal
name: *exact windows*), and four in `retained_calls`, one per kept call.

| dump | run | trigger | events | covers |
|---|---|---|---|---|
| dump1 | run1 | slow call (`call8`) | ~130 events | the 6.2 s ending at the trigger |
| dump2 | run3 | unhandled error at root | ~40 events | the whole short run |

| call | run | function | context | outcome | reason | dump links |
|---|---|---|---|---|---|---|
| call8 | run1 | ClassifyCustomer | context4 | succeeded, 6.2 s | slow call | dump1 |
| call6 | run1 | ClassifyCustomer | context4 | failed (provider 500) | error body kept under LLM value rules | none |
| call1 | run3 | ProcessCustomers | context1 | failed | unhandled error at root | dump2 |
| call3 | run3 | ProcessCustomer | context3 | failed | unhandled error (same propagating error) | dump2 |

<!-- retained rows and reasons derived from toy-program.md "Retention events"; c6's row is pinned here per that file's instruction -->

The `context` column joins each kept call back to its aggregate row in
the complete layer (doc 03).

The dump machinery runs today, and the `tape_dumps` SQL relation that
exposes the ledger to queries is part of the table-schema freeze.
**[v1]** `retained_calls` is likewise a proposed reader-facing table
**[v1]**; the capture machinery beneath it runs today. **[built]**

Retention reasons include: the call errored; it crossed the latency
threshold; its events lie inside a saved tape slice; staged material
about it was promoted by a trigger; a configured sampling policy selected
it; a policy asked for it explicitly. The exact enumeration is part of
the table-schema freeze. **[open]**

Dumps are rate-limited: currently at least 5 seconds apart and at most 16
per engine (one runtime instance; a process can host several, sharing one
tape); both are implementation defaults. This keeps exact evidence from
becoming traffic-proportional. A dump the limiter suppresses is counted,
not silently skipped; the interval over which the 16-dump budget resets
is a knob the branch docs do not pin. **[built]**
<!-- source: profiler-tape §2.5.5 (per engine; dropped dumps counted); design/03 "Current defaults" states no reset window -->

### What the tape adds over a traceback

Every error object already carries a callstack. A traceback is one stack,
on one thread, at one instant: the chain of frames waiting on the failing
call, nothing else. The tape also shows what the rest of the program was
doing. The slice `dump2` preserves around Eve's failure (event names
simplified; shape only, no bodies):

```text
dump2: run3, ~40 events
 0.00s  thread1  call1 starts    ProcessCustomers
 0.01s  thread2  thread starts   (spawned by call1)
 0.01s  thread2  call2 starts    WriteAuditLog
 0.02s  thread1  call3 starts    ProcessCustomer
  ...   thread1  pause / resume  call3's work, including validate_email
 0.19s  thread1  call3 ends      errored    ← the failure
 0.20s  thread1  call1 ends      errored    ← root observes it; trigger fires
 0.20s  thread2  call2 ends      cancelled  ← teardown, adjacent to the failure
```

About ten of the ~40 events are the call and thread records shown; the
rest are the pause/resume records (elided above) that an awaiting thread
writes each time it parks and wakes. Those records are also why `dump1`,
with 6.2 seconds of awaiting inside it, runs to ~130 events.

The traceback for this error says `validate_email ← ProcessCustomer ←
map ← ProcessCustomers` and nothing more: not that a spawned audit thread
existed, was still running when the error hit, or was cancelled by run
teardown. If the underlying bug is "audit records go missing whenever a
batch fails," the traceback never shows it; the tape shows it directly.
`dump1` in run `run1` works the same way: it holds slow call `call8` from
first event to trigger, including the await gaps and the LLM metadata a
stack cannot carry, and its 6.2 seconds of coverage reach back almost to
Bo's provider failure, just over six seconds before the trigger. Sibling
threads, just-finished calls, and suspensions are what the tape records
and a traceback cannot.
<!-- source: profiler-tape §5 "Why the tape beats the error's own callstack"; worked example derived from toy-program.md r3/w2 as the brief requires -->

### How not to read `retained_calls`

`retained_calls` invites three misreadings.

**Retained is not sampled.** The set is not a uniform sample of traffic;
even when a sampling policy contributes rows, it is one selection rule
among several. The set is policy-selected and deliberately biased toward
failures and outliers. Counting retained calls measures how many calls
matched retention policy, never how many happened; retained counts are
lower bounds on traffic. Population questions go to `calling_contexts`,
which counts everything.
<!-- source: decisions-plan D1: "retained counts are lower bounds on traffic" -->

**Retention does not spread.** Retaining `call6` does not retain its
parent `call5` or anything else in its ancestry or subtree; each call
gets its own row or none. A retained call links to its calling context in
the complete layer, so "how common is this path?" stays answerable, but
the linkage is a join, not a bulk-retention rule.

**`retained_calls` is not a health table.** A retained call is not
evidence that the application, or Studio, is unhealthy. `call8`
succeeded; it was slow. Whether Studio's own capture pipeline had
problems is a different question with its own machinery, in doc 06.

## What fires a trigger

The trigger is the selector both tables share: it seals the tape into a
dump, and it can mark individual calls for retention, directly or by
promotion (below). There are three kinds.

**Errors: at the root, once.** In run `run3`, Eve's malformed email makes
`validate_email` throw. ProcessCustomer, the map, and the root each
observe the error; nothing handles it, the run fails, and the runtime
saves **one** dump, `dump2`, not three. The design constraint this
document set adopts as the target: the error trigger fires where the
*runtime root* observes an unhandled failure, not on every propagation
hop, and one propagating error produces at most one dump no matter how
many frames rethrow it. On the current branch, root error handling is
what fires the dump **[built]**; the precise written contract (the exact
propagation point, and the dedup rule giving one error one identity
across rethrows) is not yet frozen. **[open]**
<!-- Corpus policy is unsettled per profiler-tape §6.19–6.20 (rethrow dedup missing from both source files); presented here as the implemented root behavior plus the founders' constraints, per the drafting brief -->

Bo's case in run `run1` is the contrast. His ClassifyCustomer call
(`call6`) failed with a provider HTTP 500, but ProcessCustomer *handled*
it and returned the Review fallback. The failure never reached the root,
so no error dump fired. That is correct behavior: a handled error is
normal control flow, not an incident. The failure is still recorded in
the aggregate row (`context4`, errored = 1), and its error body is kept
under the value rules for LLM functions (doc 05).

**Slow calls.** A call that outlives a configured latency threshold fires
a dump when it completes. The threshold is a knob, not policy: the only
recorded default is an archive-historical 30 seconds, unconfirmed on the
current branch, so this document set leaves the number unfrozen. The
example program sets the knob near 5 seconds, so in run `run1` Cy's
ClassifyCustomer call (`call8`) at 6.2 seconds crosses it and produces
dump `dump1`. **[built]** <!-- source: profiler-tape §3 latency-trigger row (30 s archive-historical, absent from design/03); toy threshold is teaching fiction -->

**Manual.** The trigger machinery accepts an explicit "save the tape now"
request, exercised today from engine-internal code. **[built]** How a
user asks for one (a language builtin, an SDK call, or a CLI command) is
not yet designed. **[open]**
<!-- source: bex_events prof/consumer.rs flight_dump ("manual" trigger,
same rate limits); callers on branch: consumer error/latency paths + one
test (prof_gate.rs). No BAML builtin, CLI verb, or bridge surface exists,
and design/ specifies none -->


A trigger can also perform **promotion**. Some material is *staged*
cheaply in memory in case it becomes interesting; a trigger upgrades
(promotes) the staged material connected to the failure into durable
storage. Staging and promotion machinery exists and root-error promotion
is wired **[built]**; automatically staging helper data so it is there to
promote is committed but unfinished, and the v1 gate is to wire it or
explicitly drop the promise **[v1]**. What gets staged and promoted is a
value-capture question, covered in doc 05.
<!-- source: profiler-tape §2.7: staging ring, promotion API and accounting exist; no production caller stages helper drafts yet -->

### Why cancellation is not a trigger

In this codebase cancellation is routine: racing two futures cancels the
loser every time, by design. If cancellation fired dumps, `future.race`
would saturate the rate limiter with recordings of healthy behavior. In
run `run3` the audit thread was cancelled at run end; that cancellation
preserved nothing. The *error* fired `dump2`; the cancellation appears
inside it as an event. The design position is that cancellation must not
be a default trigger. An older design draft had cancellation inside the
error-trigger clause, so the final trigger matrix still needs to be
written down and frozen. **[open]**
<!-- source: profiler-tape §6.19; founder-concerns Vaibhav 6 -->

## Where capture stops

Structural events exist for BAML function calls, logical threads, and the
runtime's own operations: suspensions, resumptions, LLM request metadata.
The tape does not record arbitrary host-language stdlib frames or the
internals of external packages: the runtime records the execution it
orchestrates, by construction, since the nine record types can only
describe BAML VM lifecycle events and function ids are assigned by the
compiler.
<!-- architectural description, not a frozen boundary contract; source: design/03 "Structural records", "Function IDs" -->
Built-in internal functions additionally have all value capture disabled
by default, so the machinery never pulls library internals into captured
data. **[built]** Depth is bounded the same way as volume: a million-call
stack meets a fixed-size tape, which covers less time, while the complete
layer counts all million calls exactly. The bounding is deliberate design,
not an accepted limitation.

## The raw firehose

One escape hatch exists for the rare case where a bounded window is not
enough: an opt-in raw event stream that writes every structural event to
disk, verbatim, with no window at all. It is off by default, and its cost
is traffic-proportional by design. Its current job is to serve as the
correctness oracle Studio's own developers use to validate the tape and
the aggregates. **[built]** Whether a bounded "full trace" mode becomes a
supported product feature, and how it would be packaged, is an open
decision. **[open]**
<!-- source: profiler-tape §1 "Raw firehose" (implemented, opt-in), "Full trace" (not implemented; decide-or-keep-absent is open v1 work) -->

## Terms defined here

**Structural event**: a small fixed-size fact about execution shape (call
and thread starts/ends, pauses, LLM metadata), never carrying data bodies.
**Rolling tape**: the bounded in-memory window of recent structural events,
continuously overwritten. **Saved tape slice / tape dump**: a durable copy
of the tape taken at an interesting moment, with one ledger row in
`tape_dumps`. **Trigger**: the policy moment that makes exact evidence
durable: unhandled error at the root, a slow call, or a manual request;
and, by design, not cancellation (the final trigger matrix is still to be
frozen). **Promotion**: a trigger upgrading cheaply staged material into
durable retention. **Retained call**: an individual invocation kept with a
**retention reason**; a biased, policy-selected set, never a traffic count.
