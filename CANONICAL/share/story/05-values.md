# 05: Values: inputs, outputs, errors

**Key points**

- A **value** is the user data that flowed through a call, in one of three
  roles: args, return, or error. A failed call has an error and no return.
- Root and LLM calls capture all roles by default, helpers only errors,
  built-ins nothing. This is the implemented default, not product policy.
- Promote-on-error stages helper values in a bounded in-memory buffer and
  makes them durable only when a trigger fires. Helper staging is designed
  but not wired today.
- Identical value bodies are stored once, locally. Query tables hold facts
  about values, never the bodies.

Doc 04 covers the *shape* of a run: structural events on the rolling tape,
counters in `calling_contexts`, rows in `retained_calls`. None of that
contains user data. This doc covers the data itself.

## Value roles: args, return, error

A **value** is the user data that flowed through a call: its arguments,
its result, or the error it ended with. Values are the expensive half of
observability: a structural event is a few dozen fixed bytes, while a
value is arbitrarily large (a customer record, a rendered prompt, a
64-turn conversation transcript). Values therefore get their own capture
machinery, policy, and storage.
<!-- source: factpacks/profiler-tape.md §1 "Structural record", "Values plane" -->

Every captured value has a **value role**: the call's **args**,
**return**, or **error**. Roles are not interchangeable. A failed call
has an error and no return: not an empty or null return; the role does
not exist for that call. In run `run1`, `ClassifyCustomer(Bo)` (`call6`)
failed with a provider HTTP 500: args (Bo's `Customer` record), an error,
no return. Its sibling `ClassifyCustomer(Ada)` succeeded: args, a return
of `Approve`, no error.
<!-- source: factpacks/profiler-tape.md §2.6 (capture draft roles); toy-program.md r1 -->

(A fourth role exists for log bodies; the default host turns log capture
off, so this doc sets logs aside. **[built]**)

## The capture policy matrix

Capturing every value for every call would reintroduce the
one-row-per-call cost problem doc 03 avoids, so a **capture policy**
decides, per function kind and per role, whether a value is kept. The
policy that ships today (everything below is on the current branch
**[built]** except the two cells marked open):

| Function kind | args | return | error | promote-on-error |
|---|---|---|---|---|
| Entry (root) call | captured | captured | captured | root failure already promotes |
| LLM functions | captured | captured | captured | flag on; staging not wired **[open]** |
| Plain helpers | not captured | not captured | captured | designed, not wired **[open]**; see next section |
| Built-in / runtime functions | never | never | never | never |

<!-- source: factpacks/profiler-tape.md §2.6 (CLI enables values by default; helper inputs/outputs disabled, errors+promote Auto; LLM all-Auto; built-ins off; root capture direct, mask-independent) -->

Notes on the table:

- These defaults apply when the host (today, the CLI) enables value
  capture; it does by default. **[built]**
- Per-call capture flags in the language
  (`capture(inputs=…, output=…, error=…)`) override the policy. **[built]**
- This is the current implemented default, not product policy. Which
  roles are captured in which environments is an open policy decision (X1
  in the register). **[open]**
<!-- source: factpacks/profiler-tape.md §2.6; factpacks/decisions-plan.md §X1 -->

In run `run1`, the root `call1` captures its args (the customer list
`[Ada, Bo, Cy]`, `batch_label = "aug-batch"`) and its return,
`[Approve, Review, Approve]`. Each `ClassifyCustomer` captures args plus
a return or, for Bo, the provider error. The three `ProcessCustomer`
helpers and the spawned `WriteAuditLog` capture nothing: they succeeded,
and helper inputs and outputs are off by policy. Eight calls ran; values
exist for four, the root and the three LLM calls. The asymmetry is
deliberate and recorded: doc 06 shows how a query distinguishes "off by
policy" from "went wrong."

In run `run3`, `ProcessCustomer(Eve)` (`call3`) failed. Helper *error*
capture is on, so the ValidationError is kept at `call3` and, by the
root's own error capture, at `call1`. Eve's malformed `Customer` argument
was **not** captured at the helper: helper args are off. The bad email is
visible in this run only because Eve's record sits inside the root's
captured args, which is a property of this particular call tree, not a
guarantee. Promote-on-error is designed to make it a guarantee.

## Promote-on-error

> **Experimental.** Promote-on-error is adjacent to the core system, and
> this section is not asking for schema-level review. Before the v1 gate
> closes, the plan is to benchmark what promoted values recover in real
> incidents against the staging cost, and drop the feature if the
> customer value does not cover the cost. Read what follows as a
> candidate design, not a commitment.

Studio does not retroactively capture every input and output up the call
stack when something fails; that promise would hide
traffic-proportional cost in the error path. **Promote-on-error** bounds
the cost instead.

While a helper runs, its values *may* be staged: copied into a fixed-size
in-memory buffer and tagged speculative, with no encoding, hashing, or
disk writes. If the call finishes normally, the staged copy is dropped at
no further cost. If a trigger fires (doc 04's triggers: an error the
run's root observes, a slow call, a manual request), staged drafts in the
failing subtree are promoted to durable storage, each tagged with the
justifying trigger. Promoted is provenance (why the value was kept), not
a fourth role: a promoted draft is still the args, return, or error it
always was. Full capture cost is paid only for failures that mattered.
The buffer is byte-bounded (32 MiB native, 8 MiB wasm; implementation
defaults **[built]**) and evictions are counted (`staging_evicted`), so
an undersized buffer is a visible, tunable fact, not a silent hole.
**[built]**
<!-- source: factpacks/profiler-tape.md §2.7, §3 -->

Every staged draft ends in exactly one of three states:

```text
              call succeeds
  staged ───────────────────────> dropped    (free, never encoded)
  (in the      │
   bounded     │ trigger fires
   buffer)     ├────────────────> promoted   (durable, tagged with the trigger)
               │
               │ buffer fills
               └────────────────> evicted    (counted: staging_evicted)
```

The staging buffer, promotion machinery, and eviction counter exist on
the branch, and a failure at the run's root calls promotion. **[built]**
No production code path stages helper drafts, so "a failing helper's
arguments are retroactively promoted" is not shipped behavior today; the
delivery plan requires wiring helper staging or removing the promise.
**[open]** Also unfrozen: which ancestors and descendants of a failing
call are eligible for promotion, and how a *partial* promotion (some
drafts promoted, some already evicted) is reported. **[open]**
<!-- source: factpacks/profiler-tape.md §2.7 item 4, §6.3; factpacks/decisions-plan.md C1 -->

Until that closes, the guaranteed places an error's data lives are those
in the matrix: the error value itself (helpers included), everything the
root captured, and everything LLM functions captured.

## What a million failures store

Suppose a provider outage makes a million `ClassifyCustomer` calls fail.

**Aggregates: everything, cheaply.** All million failures increment
counters on the same few `calling_contexts` rows. Counting is never
sampled or turned off; a million failures cost the same rows as three.
**[built]**

**Tape dumps: a bounded few.** Error triggers are rate-limited (at least
five seconds between dumps, at most sixteen per engine, and a process
normally hosts one engine; implementation defaults **[built]**), and
dumps suppressed by the limit are counted.

**Values: by policy, deduplicated.** Each failing LLM call's error is
captured per the matrix, and values are stored by content: a million
copies of the same provider error are one stored body (see "Where values
live"). Staged helper values, once wired, are bounded by the staging
buffer, with evictions counted.

### A rethrown error is stored once

In run `run3`, Eve's ValidationError was captured at two frames, `call3`
and `call1`: two small rows sharing *one* stored body. Capturing a
propagating error at N frames costs N rows and one body; the cost is
bounded by how many frames *capture* the error, never by how many rethrow
it. **[built]** The stricter rule (that the rows are also linkable as the
*same* error, one identity across rethrows) is the contract doc 04 marked
open. **[open]**

Capture cost under an incident is therefore not bimodal: there is no mode
where a bad day silently multiplies storage by traffic. Every bound is
explicit, every bound gets a counter by design rule, and a hit limit is
queryable: doc 06 introduces the table where Studio reports its own
shortfalls (*evidence issues*). Two caveats. First, one bound has no
counter yet: under extreme sustained record rates, the structural plane's
1 GiB memory cap currently *aborts the process*; the graceful degradation
ladder meant to replace the abort is not wired. **[open]** Second, the
full promise ("every value the policy selected is either captured or has
an explicit loss marker, and every shortfall is queryable") is the
committed target; today a few CLI failure paths are best-effort, and some
degradations are not yet persisted into queryable diagnostics. **[v1]**
<!-- source: factpacks/profiler-tape.md §4 "No silent truncation", §1 "CaptureLoss", §2.8 (current abort, ladder not wired), §6.16, §3 dump-rate defaults -->

## Distinctions that must never collapse

A value can be absent for four different reasons. Studio's rule is that
the reasons must stay distinguishable.

**Policy-omitted vs promised-but-lost.** "Helper args are off by policy"
is a healthy system following its configuration. "The policy selected
this value but it could not be kept" is a capture failure and must leave
an explicit loss record, never a quiet absence. The first is
*not_captured*; the second is *lost*. Collapsing them would make no
absence trustworthy. (The loss-marker contract is the [v1] caveat above.)

**Omitted optional argument vs explicit null.** Run `run2` called
`ProcessCustomers([Dee], null)`: the caller *passed* null for
`batch_label`, so the captured args show a real null, ordinary data.
Omitting the optional argument entirely would be a different fact, and
the two must stay distinguishable in captured args and in queries. The
exact representation (args-object shape, spelling of omission) is
table-schema freeze work, not settled yet. **[open]**
<!-- source: factpacks/query-system.md Q1 freeze items; factpacks/decisions-plan.md Q1 -->

**Redacted vs missing.** A redaction policy deliberately removes content
(a password field, a customer email) while recording that it did so. A
redacted value is not missing: the call happened, the value existed, the
content was withheld on purpose. Which fields are redacted, and the
allow/deny-list mechanism, are open policy work (X1) **[open]**; that the
*state* "redacted" exists and is distinct is a committed schema
requirement. **[v1]**
<!-- source: factpacks/decisions-plan.md §X1 invariant -->

### Privacy of default-on capture

The recorded reasoning for default-on capture of prompts and outputs:
captured data never leaves the machine by default; uploading is a
separate, explicit opt-in (doc 08); and seeing the exact prompt and
output is the point of the product during debugging. The opt-outs are
cheap and layered: turn history off, per-class capture flags, retention
**[built]**, and redaction once the X1 policy work lands. **[open]**
<!-- source: factpacks/profiler-tape.md §5 "Why default-on capture" -->

These distinctions give each value role a vocabulary of **availability
states** (preview: *available*, *not_captured*, *omitted*, *redacted*,
*lost*, *truncated*, *corrupt*). Every `retained_calls` row will report
one state per role, so a query never guesses why a value is absent. The
full state list and exact column spelling are part of the table-schema
freeze **[open]**; docs 06 and 09 give the full treatment.
<!-- source: factpacks/query-system.md retained_calls_v1 availability list (also pending/unsupported) -->

## Where values live

```text
  the call                     run end                     this machine
     |                            |                             |
  value --bounded deep copy--> drafts in memory --encode--> value store
         (no disk, no hash      (role + call                (one copy per
          on the hot path)       identity)                   distinct content)
```

At capture time the value is deep-copied out of the program's memory into
a profiler-owned buffer: a bounded copy and nothing else, so the running
program never waits on encoding or disk. **[built]** At run end the
drafts are encoded into canonical form and written to a local **value
store** holding one copy per distinct content: identical values (the same
args twice, the same error a thousand times) occupy storage once. Doc 08
names the mechanism and how data leaves the laptop. The dedup is
measured: a 64-turn agent conversation, where each turn's transcript
contains all previous turns, saw a ≥20× reduction from storing shared
structure once. **[built]** (Some current artifacts still carry an inline
legacy copy next to the canonical reference; "bodies live only in the
value store" is the v1 target. **[v1]**)
<!-- source: factpacks/profiler-tape.md §2.6, §3 measured evidence, §6.22 -->

"At run end" is literal today. The shipping CLI encodes and writes drafts
once, synchronously, when the run's root call finishes; a promoted draft
joins the same durable queue and lands on that write. **[built]** A run
that crashes mid-flight loses the drafts still in memory: the structural
tape survives with its intact prefix (doc 08's crash behavior); the
values plane has no equivalent yet. An off-thread drain service that can
encode values continuously *while* a run is open (needed by a days-long
agent loop) exists, but the CLI does not use it, and when incremental
value flush becomes the behavior is an open question. **[open]**
<!-- source: factpacks/profiler-tape.md §2.6 item 6 (CLI drains once at boundary finish; ValueDrainService implemented but unused), §2.7 item 2 (promotion → durable queue) -->

Two placement rules follow, and both affect queries:

**Local first.** Values live on the machine that ran the program; doc 08
shows exactly where. Nothing uploads by default; upload is explicit
configuration, also in doc 08. **[built]**

**Never rows in the query tables.** `retained_calls` and the other tables
hold small facts *about* values (which roles exist, their availability
state), never the bodies. A query that touches `args`, `return`, or
`error` fetches the body on demand from the value store (mechanism in doc
09). Value size never bloats the tables a query scans, and a table scan
never accidentally reads customer data. **[v1]** (settled semantics; the
query tier itself is target work, doc 09)
<!-- source: factpacks/query-system.md virtual query fields; factpacks/storage.md value-content boundary -->

## What an LLM call records

The bar for LLM calls: capture everything an LLM-evals product records,
plus what only the runtime can know, the calling context the call came
from.
<!-- fresh analysis: review; "everything Braintrust records, plus the calling context" is the founders' framing (Aaron, 2026-08-11 call), not a feature-by-feature audit -->

Each LLM call produces two kinds of record. Structurally (doc 04's tape
and counters), the call carries a compact LLM marker: model, token
counts, and provider/parse/retry flags (small fixed facts that fold into
aggregates). **[built]** As values, LLM functions capture all three roles
by default: input, output, and on failure the error. **[built]** Whether
the exact rendered provider request and raw response are captured in
addition to the BAML-level args and return is not specified anywhere in
the design today; it falls inside the LLM-instrumentation rework noted
below. **[open]**

The aggregate side has its own table, `llm_usage`: one row per run, per
calling context, per provider and model. Run `run1`'s row:

| run | context | provider | model | llm_calls | input_tok | output_tok | provider_errors | parse_errors | token_state |
|---|---|---|---|---|---|---|---|---|---|
| run1 | context4 | openai | gpt-5 | 3 | 1,602 | 20 | 1 | 0 | partial |

The row shows two things. The error columns are *typed*: Bo's HTTP 500 is
a `provider_error`, distinct from a `parse_error` (the model answered,
but the answer did not fit the declared return type); a "is the provider
down or is my prompt bad?" query depends on the distinction. And
`token_state = partial`: Bo's failed call reported no usage, and absent
tokens are recorded as absent, not as zero, the same
distinct-states rule as the rest of this doc.

The LLM instrumentation is being reworked; `llm_usage` is explicitly
provisional pending that work. **[open]**
<!-- source: factpacks/profiler-tape.md §1 LlmCallMeta; factpacks/decisions-plan.md (llm relation provisional); toy-program.md llm_usage -->

## Terms defined here

- **Value**: the user data that flowed through a call, as opposed to the
  structural events that record its shape.
- **Value role**: the part a value plays for its call: **args**, **return**,
  or **error**. A failed call has an error and no return.
- **Capture policy**: the per-function-kind, per-role decision of whether a
  value is kept. Root and LLM calls capture by default; helpers do not.
- **Promote-on-error**: stage helper values cheaply in a bounded buffer;
  make them durable only when a trigger shows they mattered.
- **Availability state**: the per-role answer to "why is this value not
  here?" (not_captured, omitted, redacted, lost, truncated, corrupt), never
  a bare gap.
- **Redaction**: deliberately withholding content while recording that it
  was withheld.
