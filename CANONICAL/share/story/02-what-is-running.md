# 02: What is actually running

**Key points**

- A **run** is one top-level BAML execution; the runtime records every
  call and thread it causes as one causal graph it owns.
- A run's outcome is independent of its calls' outcomes; a run fails only
  when an error escapes to its root.
- `total_calls` counts the entrypoint call plus every descendant on every
  thread; a batch of any size is one run.
- The runtime produces the call tree itself rather than stitching emitted
  spans, so Studio says *run*, not *trace*.

Doc 01 described cheap summaries of every call and exact records of the
interesting few. This doc defines the units involved: process, run, call,
logical thread, and revision.

## The example program

Every document in this set uses the same program. It processes a batch of
customers (validate each email, classify with an LLM function, fall back
if classification fails) and spawns one background audit task.

<!-- syntax verified with `baml check` (toolchain 0.15.1-nightly.20260731); corrected block at scratchpad/toycheck/corrected.baml -->
```baml
class Customer {
  name string
  email string
  plan string
}

enum Verdict {
  Approve
  Review
  Reject
}

// An LLM function.
function ClassifyCustomer(c: Customer) -> Verdict {
  client "openai/gpt-5"
  prompt #"
    Decide whether to approve {{ c.name }} on the {{ c.plan }} plan.
  "#
}

// A plain helper. Classification errors are handled with a fallback.
// (validate_email and WriteAuditLog are ordinary BAML functions,
// defined elsewhere in the program.)
function ProcessCustomer(c: Customer) -> Verdict {
  validate_email(c.email)            // throws on malformed email
  ClassifyCustomer(c) catch (e) {
    _ => Verdict.Review,             // degrade gracefully
  }
}

// The entrypoint. Spawns one background audit thread, then maps.
function ProcessCustomers(customers: Customer[], batch_label: string?) -> Verdict[] {
  spawn { WriteAuditLog(batch_label) }
  customers.map(ProcessCustomer)
}
```

The program has the features that make LLM applications hard to observe:
batch fan-out, a helper that can throw, an LLM call that can fail or
stall, concurrency, and an optional argument.

## One process, two runs

Deployed as a server, one operating-system process has handled one batch
and is in the middle of another. The document set uses readable
placeholder IDs (`run1`, `call3`) throughout:

```text
Process P   (one server process, running revision rev1)
│
├── run1: ProcessCustomers([Ada, Bo, Cy], "aug-batch")          succeeded, 8.4s
│   │     (this root call is call1)
│   ├── [thread2, spawned] call2 WriteAuditLog("aug-batch")     succeeded, 0.3s
│   ├── call3 ProcessCustomer(Ada)                              succeeded, 1.15s
│   │   └── call4 ClassifyCustomer(Ada)                         succeeded, 1.10s → Approve
│   ├── call5 ProcessCustomer(Bo)                               succeeded, 0.95s → Review (fallback)
│   │   └── call6 ClassifyCustomer(Bo)                          failed,    0.90s (provider HTTP 500)
│   └── call7 ProcessCustomer(Cy)                               succeeded, 6.25s
│       └── call8 ClassifyCustomer(Cy)                          succeeded, 6.20s → Approve   ← slow
│
└── run2: ProcessCustomers([Dee], null)                         running
```

Later docs represent this same picture at a million times the volume.

## Process

A **process** is the operating-system process the program runs in: a
server, a CLI invocation, a test runner. It hosts the BAML runtime, which
records what executes inside it **[built]**. A process hosts zero or more
runs; they start and finish independently. `P` has finished `run1` while
`run2` is still running. The process is the host, not the unit of work:
`P` being alive says nothing about whether any work succeeded.
<!-- fresh definition (verified safe): "process" has no canonical glossary entry; composed from profiler-tape facts, proposed as a glossary addition -->

## Run

A **run** is one top-level BAML execution: everything caused by one
outside invocation of the program (one `baml run` of `main()`, one served
request, one test case) **[built]**. Every call and thread that invocation
causes belongs to the run, recorded as one causal graph the runtime owns
end to end. (Runtime internals call the root of a run a *boundary*; the
engineering docs use that word.)
<!-- source: vocabulary-lifecycle pack (GLOS L15/L65, PROD L21–25); profiler-tape pack (boundary = run) -->
<!-- founder sign-off needed: teaching the internal word "boundary" here is not sanctioned by the vocabulary ramp (unlike CCT, sanctioned for docs 00/03); cut the parenthetical if not approved -->

A run has its own outcome: `run1` succeeded, `run2` is still running.
**A run's outcome is independent of its calls' outcomes.** `run1`
succeeded even though `call6` inside it failed: `ProcessCustomer` caught
the provider HTTP 500 and returned the `Review` fallback. A handled error
is a fact about a call, not a verdict on the run.

Run `run3`, also hosted by `P`, shows the other direction. (Call IDs are
scoped to their run: `run3`'s `call3` is a different call from `run1`'s
`call3`.)

```text
└── run3: ProcessCustomers([Eve], "retry-batch")                failed, 0.2s
    ├── [thread2, spawned] call2 WriteAuditLog("retry-batch")   cancelled at run end
    └── call3 ProcessCustomer(Eve)                              failed (ValidationError: malformed email)
        (ClassifyCustomer never ran)
```

Eve's email is malformed, `validate_email` throws, and nothing catches
the error, so it reaches the root and the run fails. A run fails when an
error escapes it, not when something inside it goes wrong. Runs, calls,
and threads each end with their own status: succeeded, failed, cancelled,
and rarer endings such as crashed. The status spellings in the query
surface are not frozen yet; current code uses overlapping
`failed`/`crashed` terms in places **[open]**.
<!-- source: toy-program.md (r3); vocabulary-lifecycle pack (PROD L60–71: execution states; spelling freeze) -->

### Runs are not processes

`P` hosted a succeeded run, a running run, and a failed run at once. The
process is where work happens; a run is one unit of that work with its
own beginning, end, and outcome.

## Entrypoint, call, and child call

The **entrypoint** is the function whose invocation roots a run: here
`ProcessCustomers`. Every run has exactly one entrypoint call; in `run1`
it is `call1`.

A **call** is one function invocation inside a run. A **child call** is a
call made by another call: `call4 ClassifyCustomer(Ada)` is a child of
`call3 ProcessCustomer(Ada)`. Every call except the entrypoint call has
exactly one parent, and every call ends with its own status. `run1`
contains eight calls: root `call1`, spawned `call2`, and three
helper-plus-classify pairs `call3`–`call8`. The runtime records each
call's start and end as it happens **[built]**.

`customers.map` over three customers is one run with many calls, not
three runs: Ada, Bo, and Cy are calls `call3`–`call8` inside the `run1`
invocation. A batch of a million customers is still one run: one
entrypoint call, roughly two million calls under it.

**`total_calls`** counts the entrypoint call plus every descendant, on
every thread of the run. For `run1`, `total_calls = 8` and
`total_errors = 1`: a handled error still counts, so `call6` is included.
For `run3`, `total_calls = 3` and `total_errors = 2`: one
`ValidationError` ended both `call3` and root `call1` as failed.
`total_errors` counts calls that ended in an error, not distinct errors;
one propagating error can end several calls. Later docs use these
counters as columns.

Accounting note: `validate_email` runs inside every `ProcessCustomer`,
but this set folds its work into the caller's frame instead of recording
a separate call. That is why `run1` counts eight calls, not eleven, and
why `run3`'s failing frame is `call3` even though `validate_email` threw.
Doc 07 returns to this with the code-identity tables.
<!-- source: toy-program.md; total_calls semantics per reader-brief §1 -->
<!-- derived: toy accounting (merged into toy-program.md) -->

## The call tree

The **call tree** (or *call structure*) is the full set of parent/child
relationships among a run's calls: the indented picture above. The
runtime does not reconstruct the tree from clues after the fact. It
produces the tree, because it creates every call **[built]**. The "Runs,
not traces" section below rests on this point.

## Logical thread

A **logical thread** is a lane of concurrent execution scheduled by the
BAML runtime: concurrency as the program sees it, not an OS thread.
`spawn` creates one. In `run1`, `WriteAuditLog` runs on logical thread
`thread2` while the map proceeds on the main thread. The runtime may move
the work across OS threads; the logical thread is the stable identity of
the concurrent activity. Thread starts and ends are recorded like call
starts and ends **[built]**.
<!-- source: profiler-tape pack (StartThread 0x03 / EndThread 0x04 records; engine threads, not OS threads), verified; "logical thread" is a canon vocabulary gap, teaching definition composed for this set -->

`spawn` is the verb; a logical thread is what it creates. Internal design
docs historically used "spawn" for both.

In `run3`, the run ended while the audit thread was still running, so the
runtime cancelled it. Cancellation is a normal ending, not an error.

## Revision, the short version

A **revision** is the exact compiled program a run executed: the content
identity of the BAML sources, schema, and compiler inputs. Any source or
type-definition edit produces a new revision. Deployment, release, and
git labels are dimensions attached to a revision, not the identity
itself. Every run records the revision that produced it **[built]**:
`run1`, `run2`, and `run3` all ran on `rev1`, so when their behavior
differs, the code is not the variable. Doc 07 covers how functions keep
their identity across revisions and what a revision can and cannot prove.
<!-- source: vocabulary-lifecycle pack (program snapshot, GLOS L53, PROD L42–54) -->

## Run boundaries

### One run cannot contain another

Calls nest; runs do not. If the program calls another service that itself
runs BAML, that service's work is a second run in its own process, linked
to the first by an explicit relation rather than merged into one graph
**[v1]**. Two processes have two clocks, and Studio does not merge two
clocks into one timeline. `total_calls` never includes anything from a
related run.
<!-- source: design/08 decision register ("Cross-process execution | Related runs with explicit links"), design/01 L25, glossary Run/Causal-run, verified; same-process "calls nest, runs do not" derives from the boundary definition and matches reader-brief total_calls semantics -->

### A long-running server hosts many runs

Each hosted run finishes on its own schedule, and a run is visible while
it is still running: `run2` appears today, with its so-far numbers, not
after it ends **[built]**. If the process dies mid-run, the run does not
get an invented ending: it is classified as crashed when read, from the
absence of a recorded completion **[built]**.
<!-- source: vocabulary-lifecycle pack (GLOS L66; ROOT L7); profiler-tape pack (crashed classification at read time) -->

## Runs, not traces

The industry word for the picture of one request's execution is a
*trace*; doc 01 introduced how trace-shaped tools see the world. Studio
does not use the term, for the following reasons.

A trace is a *correlation*. Instrumented code emits spans, context IDs
propagate across function and service hops, and a backend later stitches
matching IDs into a tree. The structure is inferred from the stitching,
so it is only as complete as the propagation: a missed span orphans its
subtree, an unpropagated ID splits one execution in two, and under
sampling most requests produce no tree at all.
<!-- fresh analysis: review; the trace-mechanics characterization extends doc 01's prior-art ground; no recorded canon argument for "why not the word trace" exists (vocabulary-lifecycle fact pack §5 flags the gap) -->

A run is not a correlation. The runtime creates every call and every
logical thread, so it does not discover the structure; it produces it.
There is nothing to stitch and nothing to sample away: the tree is
recorded as it is created, never inferred after the fact **[built]**.
Structure can still be lost (a process can die mid-run, and a recorder
under extreme load can drop records), but the failure differs in kind:
the loss is recorded as a typed, queryable fact, and the tree is marked
incomplete rather than silently smaller (doc 06 shows where those facts
land) **[v1]**.
<!-- source: design/03 "Crash and corruption behavior": declared-loss persistence on every path is v1 work, not built -->

Calling this a "trace" would mislead in both directions. It would
undersell the guarantee: readers would assume sampled, stitched,
possibly-incomplete trees and design around silent holes that do not
occur here. It would oversell the scope: "trace" implies one merged
picture across services, which Studio does not fabricate; cross-process
work is a *related run*, linked and separately clocked, never spliced
into one timeline **[v1]**.
<!-- no-merged-clocks stance: vocabulary-lifecycle pack (PROD L25); the naming argument itself is fresh analysis; review -->

The runtime sees every call the moment it happens but does not keep every
call. Doc 03 describes counting every call without storing every call.

## Terms defined here

- **Process**: the OS process hosting the runtime; hosts zero or more runs.
- **Run**: one top-level BAML execution with its own causal graph and outcome.
- **Entrypoint**: the function whose invocation roots a run.
- **Call / child call**: one function invocation in a run; calls nest, runs do not.
- **Call tree**: the parent/child structure of a run's calls, produced rather than reconstructed; any loss is declared, never silent.
- **Logical thread**: a runtime-scheduled lane of concurrency; `spawn` creates one.
- **Revision**: the exact compiled program a run executed (details in doc 07).
