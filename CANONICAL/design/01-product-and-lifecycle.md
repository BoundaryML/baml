# Product and lifecycle

**Status:** Product contract. The profiler core is built; the complete local and hosted experience is v1 target work.

## Product statement

Project Studio helps a BAML developer answer:

- What ran?
- What failed, slowed down, retried, or changed?
- Which calling context produced the behavior?
- What exact retained inputs, outputs, errors, logs, or nearby events explain it?
- Is the answer population-true, limited to retained instances, or incomplete because evidence was unavailable?
- What changed across program revisions, releases, models, providers, or environments?
- Can I reopen, compare, or export the evidence—and, later, rerun it or turn it into a test?

Studio is observation-centered for discovery and run-centered for debugging. A failed model attempt, tool call, or BAML call may be the item a user first finds; opening it reveals the containing causal run.

## Core concepts

### Run

One runtime-owned causal graph rooted at a BAML boundary. It includes calls, logical threads, spawn edges, values, errors, logs, loss markers, source/revision identity, and independent state axes.

A cross-process hop produces a related run with an explicit relation. Studio does not merge independent clocks and streams into a fictional exact graph.

### Observation

A user-discoverable retained operation, such as a run, retained BAML call, model attempt, tool invocation, or resource operation. Observation kinds appear only when the runtime or a versioned adapter emits enough identity, parentage, timing, and terminal evidence.

Studio does not materialize a default observation row for every BAML call. Every call contributes to population aggregates; only explicitly retained calls become individually discoverable instances.

### Population and retained-instance grains

- **Population grain** answers complete count, error, timing, histogram, and LLM-usage questions from the CCT tally.
- **Retained-instance grain** answers exact “which call/value/event?” questions for calls kept by capture policy, exact windows, promotion, or another explicit retention mechanism.

Counting retained rows never becomes a population claim. This rule is reflected in relation names and schema comments.

### Program snapshot

The content identity of the BAML source/schema and compiler inputs that explain
a run. A source or type-definition edit creates a new revision. Dense function
IDs are revision-local. Cross-revision comparison uses **definition_key** and
may annotate the artifact's **def_content_hash**, exposed publicly as
**local_definition_hash**.

That hash covers one function's own compiled signature and bytecode. It does
not recursively include the contents of referenced types, callees, clients,
prompts, or other definitions. An equal local hash is therefore not proof of
equal effective behavior across revisions; the revision remains the exact
program boundary.

Deployment, git revision, application build, service, and release are dimensions attached to a program snapshot; they are not the snapshot identity itself.

### Evidence and availability

Evidence is not one boolean. At minimum the UI and query catalog preserve:

| Axis | Representative states |
|---|---|
| Execution | pending, running, waiting, succeeded, failed, cancelled, panicked, abandoned |
| Structural completeness | open, complete, incomplete/gapped, diagnostic, stale_open |
| Value availability | pending, available, not_captured, omitted, redacted, lost, truncated, corrupt, unsupported |
| Integrity | unverified, verified, truncated, corrupt, conflicting, quarantined |
| Projection | pending, active, delayed, failed, rebuilding |
| Retention | live, tombstoned, deleting, backup-expiry-pending, deleted, legal_hold |

A successful execution may still have lost values and delayed projection. A complete SQL query may include running executions because completeness is relative to the bound query snapshot, not to future execution terminality.

These are semantic states, not yet frozen literal SQL enum spellings. Current code uses overlapping **failed**/**crashed** terms in some surfaces. Catalog v1 must publish one mapping without collapsing the independent axes.

## User journey

~~~text
recent and running operations
-> filter or ask a question
-> inspect the terminal query outcome
-> select a retained operation
-> open the containing run
-> inspect graph, threads, timing, values, logs, source, and evidence state
-> compare, export, reopen, or explicitly erase
-> later: rerun or create a test
~~~

### Find what went wrong

1. Query population run/error facts to find affected cohorts.
2. Select retained failed instances where exact evidence exists.
3. Open the run debugger.
4. Inspect error, calling context, source, captured arguments, exact-event window, and capture losses.
5. Report unavailable evidence as typed reasons, not blanks or NULLs.

### Understand a slow execution

1. Query CCT population rows grouped by **definition_key** and revision.
2. Separate inclusive, self, and await time.
3. Open representative retained runs.
4. Inspect timeline, tree/flame, spawn relationships, and provider timing when emitted.
5. Do not label self/await time as operating-system CPU sampling.

### Inspect values without loading everything

1. Use resident scope filters first: project, time, revision, function, run, status.
2. Hydrate distinct canonical values lazily in bounded batches.
3. Navigate canonical paths and apply typed predicates.
4. Stop only when the query completes, is cancelled, or hits a query-global budget.
5. Read the mandatory terminal outcome before treating the result as complete.

There is no standing value-body, preview, scalar-path, or text index in ClickHouse. A future external value index is a separate deferred system.

### Compare behavior

Population comparisons join revisions through **definition_key**. Exact comparisons use retained evidence. Value identity across local and hosted planes is not promised until the hosted opaque-reference decision is resolved.

### Work with an agent

The agent:

1. reads the v1 schema and capability catalog;
2. writes portable BAML/DataFusion SQL;
3. executes within an authenticated query scope;
4. checks the query outcome;
5. opens selected runs, values, and source;
6. cites stable identifiers; and
7. states material unknowns.

The data plane never invokes an LLM.

## Independent lifecycle axes

~~~mermaid
flowchart LR
  subgraph EXECUTION["Execution axis"]
    PENDING["pending"] --> RUNNING["running / waiting"]
    RUNNING --> SUCCEEDED["succeeded"]
    RUNNING --> FAILED["failed / panicked"]
    RUNNING --> CANCELLED["cancelled"]
    RUNNING --> ABANDONED["abandoned"]
  end

  subgraph STRUCTURE["Structural-completeness axis"]
    OPEN["open"] --> COMPLETE["complete"]
    OPEN --> GAPPED["incomplete / gapped"]
    OPEN --> STALE["stale_open classification"]
    STALE --> OPEN
    STALE --> COMPLETE
    STALE --> GAPPED
  end

  subgraph RETENTION["Retention axis"]
    LIVE["live"] --> TOMBSTONED["tombstoned: reads denied"]
    TOMBSTONED --> DELETING["deleting"]
    DELETING --> BACKUP["backup-expiry-pending, if applicable"]
    DELETING --> DELETED["verified deleted"]
    BACKUP --> DELETED
    HOLD["legal_hold"] -. blocks .-> DELETING
  end
~~~

The axes advance independently. **Stale open** is an observability classification, never an invented execution outcome. Tombstoning denies reads but does not rewrite how execution ended. Running and terminal facts are both queryable when durably committed at the bound snapshot.

## History verbs

The product keeps these actions separate:

| Verb | Meaning | V1 status |
|---|---|---|
| Reopen | Inspect retained evidence without executing | Core v1 |
| Reconstruct | Decode canonical artifacts again and compare semantic hashes | Core/offline tooling |
| Reindex | Rebuild projections from a fixed evidence barrier | Core hosted operation |
| Export | Emit bounded, policy-aware JSON/JSONL/Parquet/OTLP projections | Core bounded export; richer forms later |
| Rerun | Execute a new run from selected historical inputs/configuration | Deferred |
| Create a test | Produce a reviewable regression fixture from evidence | Deferred |

Rerun always creates a new identity and requires a prerequisite/side-effect report. Test creation always requires user review before writing a fixture.

## V1 boundaries

V1 includes:

- the existing profiler/CAS/fold substrate;
- local run/value/source inspection;
- a versioned portable SQL catalog and mandatory query outcome;
- local providers over canonical artifacts and rebuildable resident stores;
- hosted commitment, projection, query, value hydration, authorization, audit, and deletion;
- one local browser product under **baml playground**.

V1 does not include user-defined query code, a second query language, a raw ClickHouse dialect, value content in ClickHouse, local/hosted federation, an LLM in query plans, durable background query jobs, rerun/test generation, multi-cell ad-hoc SQL, billing, collaboration, BYOK, or Kubernetes packaging. See [Deferred](10-deferred.md).
