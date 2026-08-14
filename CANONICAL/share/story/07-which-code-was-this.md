# 07: Which code was this? Relating runs to code

**Key points**

- A **revision** is a content hash of the whole compiled program; any
  behavior-affecting edit makes a new one. Every run records its revision.
- A **function_id** is meaningful only inside one revision. Cross-revision
  grouping uses the **definition_key**, which a rename changes on purpose.
- An unchanged **local definition hash** never proves unchanged behavior;
  the hash is deliberately not dependency-aware.
- Studio's code identity names the code a run came from. It is not version
  control: no diffs, no deploy tracking.

## The question

In August, run `run1` classified Ada, Bo, and Cy; Cy's classification
took 6.2 seconds. In September, run `run4` processed the same three
customers and every classification finished in about a second. The cause
could be the prompt edit, provider variance, or different code entirely.

Deciding requires knowing which program produced each run, and "the code"
is ambiguous: the branch moved twice that week, a teammate deployed
Tuesday, one server restarted with a stale build. Doc 02 defined the
revision as "the exact compiled program". This doc describes how Studio
names programs, functions, and source locations so that runs point at
code without guesswork.

## Revision: the exact program

A **revision** is a content hash over the whole compiled program: every
source file, class, and prompt, plus the configuration and compiler
inputs that affect behavior. The identity commits to *every*
behavior-affecting input, so **any edit produces a new revision**: a
rename, one changed prompt word, a new field on a class that some
function consumes. **[built]**
<!-- facts: vocabulary-lifecycle §1 (program snapshot, GLOS L53; PROD L42-54); query-system §4 ("revision identity must commit to every behavior-affecting compiler input") -->

Every run records at start the revision that produced it. In the example
program, one server process on `rev1` hosted `run1`, `run2`, and `run3`.
Editing `ClassifyCustomer`'s prompt text, and nothing else, produced
`rev2`, which hosted `run4`. The `revisions` table holds one row per
compiled program Studio has observed:

| revision_id | hosted (teaching annotation) |
|---|---|
| `rev1` | runs `run1`, `run2`, `run3` (August) |
| `rev2` | run `run4` (September): only `ClassifyCustomer`'s prompt text differs |

(These are teaching rows; doc 09 gives the real column list.)

A git commit, release tag, build number, or service name is a label
attached to a revision, not the identity. Two deploys of the same bytes
are one revision; one commit built with two compiler versions is two.
**[built]**
<!-- facts: vocabulary-lifecycle §4 invariant 28 (PROD L54) -->

### Why the strict rule is affordable

A revision costs one hash and one small identity dictionary per compile,
recorded once, not per call. The strictness is deliberate: a class field
can silently change what a prompt renders, so the whole program is the
only safe unit of "exactly this behavior". The identities below make the
strictness practical.

## The identity dictionary

With each revision the compiler emits, once, a small **identity
dictionary**: every function's name, source location, capture flags, and
the three identity fields defined below. Per-call records hold only small
integers (doc 03: recording must cost nanoseconds); the dictionary turns
integers back into names at read time. If the dictionary is missing,
Studio shows `fn#18` rather than a fabricated name. **[built]**
<!-- facts: vocabulary-lifecycle §1 (revision dictionary, GLOS L64); profiler-tape §4 (missing dictionaries degrade to numeric identities) -->

## function_id: cheap at runtime, meaningless alone

A **function_id** is a dense integer the compiler assigns per function
within one revision. Real functions start at 16; 0 is the "unknown"
sentinel that degraded evidence folds into. **[built]** The runtime
records this integer; integers are cheap enough to write millions of
times.
<!-- facts: profiler-tape §1 (function ID); vocabulary-lifecycle §1 (GLOS L40) -->

The cost: **a function_id means nothing without its revision_id.** The
`functions` rows for the example program (one row per function per
revision):

<!-- derived: the specific integer assignments and dk_/lh_ values below are
illustrative teaching placeholders; dense IDs are assigned per revision in an
order the contract does not specify; that unreliability is the lesson -->

| revision | function_id | name | definition_key | local_definition_hash |
|---|---|---|---|---|
| `rev1` | 16 | ProcessCustomers | `dk1` | `lh_a1` |
| `rev1` | 17 | ProcessCustomer | `dk2` | `lh_b7` |
| `rev1` | 18 | ClassifyCustomer | `dk4` | `lh_9a` |
| `rev1` | 19 | WriteAuditLog | `dk3` | `lh_c2` |
| `rev1` | 20 | validate_email | `dk5` | `lh_d8` |
| `rev2` | 16 | ClassifyCustomer | `dk4` | `lh_f3` |
| `rev2` | 17 | ProcessCustomers | `dk1` | `lh_a1` |
| … | … | … | … | … |

`ClassifyCustomer` is id 18 in `rev1` and 16 in `rev2`. Nothing is wrong;
dense ids are assigned fresh at each compile. Joining across revisions on
`function_id` silently compares different functions: **cross-revision
grouping uses definition_key, never function_id.**
<!-- facts: vocabulary-lifecycle §4 invariant 27 (ROOT L83); GLOS L40 "not a cross-revision key" -->

`validate_email` is compiled into the revision and has identity rows, but
in the example accounting its invocations fold into the caller's work
rather than being recorded as separate calls; that is why `run1` counts 8
calls, not 11. The dictionary says what code *exists*; call counts say
what the runtime *records*.
<!-- derived: toy accounting (merged into toy-program.md) -->

## definition_key: the same function across many revisions

Most questions span revisions: a month of `ClassifyCustomer` behavior can
cover fourteen revisions, one per prompt tweak. The **definition_key** is
the stable identity of the *logical function*: `ClassifyCustomer` keeps
`dk4` through `rev1` and `rev2` though its id and hash both changed.
**[built]**

One deliberate exception: **a rename changes the definition_key on
purpose.** Renaming `ClassifyCustomer` to `ScoreCustomer` makes a new
logical function; Studio does not guess intent. A hash match across a
rename is a hint, not identity.
<!-- facts: vocabulary-lifecycle §1 (GLOS L30 "a rename intentionally changes it"); profiler-tape §7 (RB §11) -->

`dk4` is a teaching placeholder. The real key is a readable string
derived from the function's kind and fully qualified name, closer to
`function:user.ClassifyCustomer`, which is why a rename changes it.
**[built]** Doc 09's example keys look like function names because that
is the key's real form, not shorthand. Its exact spelling, like every
physical identifier form, is not frozen for v1 **[open]**.
<!-- facts: bamldict identity dictionary (definition_key strings of the form
"function:user.Main"); GLOS L30 (rename changes it) -->

The three identities in one picture:

```text
              rev1 (August)                rev2 (September)
              ────────────────             ────────────────
name          ClassifyCustomer             ClassifyCustomer
function_id   18  ─────── renumbered ───►  16      (revision-scoped)
local hash    lh_9a ───── changed ──────►  lh_f3   (its prompt is its own code)
definition_key  dk4 ═════ the same ══════  dk4     (the cross-revision thread)
```

## local definition hash: whether one function changed

Between two revisions most functions did not change. The **local
definition hash** identifies the ones that did: a hash (BLAKE3-256 today
**[built]**) over one function's *own* compiled signature and bytecode.
In the rows above, the prompt edit changed only `ClassifyCustomer`'s
hash (`lh_9a` → `lh_f3`); the rest carried identical hashes into `rev2`.
That is the precise reading of "a new revision shipped, but only one
function changed".
<!-- facts: vocabulary-lifecycle §1 (GLOS L49) -->

The hash covers the function's own definition; referenced definitions
contribute **by name only, not their contents**. It is deliberately *not*
dependency-aware. Example: a hypothetical third revision adds a field to
`class Customer`. That is a new revision under the strict rule, but
`ClassifyCustomer`'s local hash may be unchanged: its own text still says
only "Customer". Its *behavior* may still differ, because the prompt
renders customer data, which now has another field. **An unchanged local
hash never proves unchanged behavior.** The revision remains the only
exact statement of "this program"; the local hash is a lean change
signal. V1 ships tests pinning exactly this: sensitivity to direct edits,
and documented non-transitive behavior never presented as transitive
proof. **[v1]**
<!-- facts: vocabulary-lifecycle §1/§5 (PROD L50-52); decisions-plan §5 (three-part identity); toy-program (worked caveat) -->

### Why the hash is not dependency-aware

A dependency-aware hash, where an equal hash means equal
behavior, requires hashing each function's full dependency closure: every
referenced type, template, and callee, cycle-safe, per definition. That
is a real future design, explicitly not in v1 **[open]**. Until then
Studio keeps the weak signal and does not overclaim.
<!-- facts: decisions-plan §5 ("full dependency-aware versioning ... is a deferred future design") -->

## Which identifiers travel, and which don't

These docs use friendly placeholders (`run1`, `call8`, `context4`,
`dk4`); real identifiers are opaque strings whose exact physical form is
a decision still to be frozen for v1 **[open]**.
<!-- facts: toy-program (IDs are teaching placeholders); reader-brief §11 -->

| Identifier | Example | Meaningful within | What it names |
|---|---|---|---|
| run id | `run1` | everywhere | one run |
| call id | `call8` | one run | one call inside that run |
| context id | `context4` | one run's summaries | one aggregate calling-context row |
| function_id | `18` | one revision | one function, cheaply, at runtime |
| revision_id | `rev1` | everywhere | one exact compiled program |
| definition_key | `dk4` | everywhere | one logical function across revisions |
| local definition hash | `lh_9a` | content comparison | one function's own compiled shape |

Not every id is a UUID, by design: global names are hashes or opaque
ids; identifiers written millions of times per second are small scoped
integers. Today's run identifiers are random and do **not** sort by
time; chronology comes from the recorded start timestamp. **[built]**
<!-- facts: decisions-plan §3 (BoundaryId UUIDv4; "chronology comes from created_ms"); profiler-tape §7 (RB §11) -->

## Call sites: where the call was made from

A **call site** is the static source expression that makes a call: in
`ProcessCustomer`, `ClassifyCustomer(c)` and `validate_email(c.email)`
are two call sites; in `ProcessCustomers`, `spawn
WriteAuditLog(batch_label)` is a third. Call sites are static facts about
the source, so `validate_email(c.email)` qualifies even though its
invocations fold into the caller's accounting. Recording them enables
navigation from a retained call in Studio to the source line that made
it.

Status: the per-call record reserves a slot for a call-site reference and
the identity dictionary has a call-site section, but the compiler writes
no entries, so the `call_sites` table has no rows today. Populating it is
committed target work, tied to retained-call source navigation; until the
producer lands, nothing may rely on it. **[v1]** (producer not built
today)
<!-- facts: vocabulary-lifecycle §1 (GLOS L64 "call-site section exists but is currently empty"); query-system §6 (call_sites_v1 "current dictionary emits no rows"); profiler-tape §6.21 -->

## Comparing revisions

The opening question in query terms: per revision, how often did
`ClassifyCustomer` run, how often did it fail, and how long did it wait?
Group the complete layer by revision, holding the logical function fixed
with its definition_key:

```sql
-- Illustrative target SQL [v1]; exact column spellings are not frozen yet.
SELECT
  revision_id,
  sum(calls_started) AS calls,
  sum(calls_errored) AS failures,
  sum(await_ns)      AS time_waiting
FROM calling_contexts
WHERE definition_key = 'dk4'      -- ClassifyCustomer, in every revision
GROUP BY revision_id;
```

| revision_id | calls | failures | time_waiting |
|---|---|---|---|
| `rev1` | 4 | 1 | ~11.2s |
| `rev2` | 3 | 0 | ~3.3s |

<!-- derived: r4 detail (three ClassifyCustomer calls of roughly 1.1s each,
no failures) consistent with the shared toy program's "all fast and successful" -->

The `rev1` count is 4, not 3, because `run2` is still running on `rev1`
and running work is not excluded. Dee's classification is in flight at
this snapshot, about three seconds into its wait, so the `rev1` row
includes its so-far counters: a fourth started call and its waiting time
so far, on top of `run1`'s three sealed calls and 8.18s. Whether a total
includes still-open work is read from run states at the query snapshot,
not a per-row mark (docs 03 and 06). `run3` never ran `ClassifyCustomer`,
so it adds nothing.
<!-- facts: decisions-plan D15 (durable running state visible to ordinary SQL);
vocabulary-lifecycle GLOS L66 (running/so-far fact), PROD L69 (completeness is
relative to the bound query snapshot) -->

These totals are complete: every call in every run contributed, as doc 03
described. They do not by themselves prove the prompt edit caused the
improvement; different runs saw different traffic on different days. Doc
10 builds the cross-revision comparison recipe on this query shape.
<!-- facts: vocabulary-lifecycle §8b caption ("population-true totals by revision; does not, by itself, prove a latency regression") -->

## What this is not

Studio's code identity is an identity dictionary for observations, not a
version-control system. It states which exact program produced a run,
which logical function is which across revisions, and whether a
function's own definition changed. It does not show diffs (git does),
track deploys or releases (labels attached as dimensions), or claim two
revisions behave the same. It answers one class of question: which code
was this.

## Terms defined here

- **revision**: content hash of the whole compiled program; any edit makes
  a new one
- **definition_key**: the same logical function across revisions; a rename
  changes it on purpose
- **function_id**: dense per-revision integer; meaningless without its
  revision
- **local definition hash**: whether this function's own definition
  changed; not dependency-aware, never proof of equal behavior
- **call site**: the static source expression that made a call
