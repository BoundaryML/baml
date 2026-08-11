# Project Studio

Project Studio is the BAML observability initiative: always-on, low-overhead profiling; retained exact evidence and values; a local debugger; and a target portable SQL surface shared by local and hosted deployments.

This directory is the design authority. It reconciles the [archived source corpus](archive/README.md) as of 2026-08-10. The former root source folder now lives under **CANONICAL/archive**. When an archived document conflicts with the active design, the active design wins.

> **Today:** the branch has the profiler/CAS/history/fold substrate, a runs/run-detail playground, **baml q** with BQL, and a separate **baml studio** compatibility command. It does not have DataFusion, **baml_query**, **baml query**, or the hosted Project Studio data plane.
>
> **Target v1:** one local product entry point, **baml playground**; one portable SQL command, **baml query**; and the hosted evidence/query system described here. Compatibility commands are removed only after parity.

## What users will be able to do

From one BAML project, a user will be able to:

- open the playground and see current and historical runs;
- find hot or failing calling contexts without storing one row per call;
- inspect the exact retained inputs, outputs, errors, logs, and nearby events when policy captured them;
- distinguish “no match” from “the value was not captured, was redacted, was lost, or exceeded a query budget”;
- query the same versioned logical catalog locally and in the hosted service;
- compare observed behavior across revisions by stable logical function
  identity while keeping each exact program revision separate;
- ask an agent a natural-language question and let it inspect the schema, issue SQL, and cite the runs and values behind its answer;
- upload immutable evidence to the hosted service without putting network work on the runtime hot path;
- rebuild every analytical projection from canonical artifacts; and
- explicitly erase hosted evidence through an authorized deletion workflow.

Project Studio is the initiative name. The target user-facing local product remains **baml playground**, with **baml query** as its SQL command. There is no separate **baml studio** command after v1 consolidation.

## Start with a question

The query snippets below are illustrative target usage, not commands that work
on this branch today. Names omit version suffixes for readability. Exact
columns/types/nullability, lifecycle enum spellings, parameter binding,
value-helper names, and the outcome wire shape freeze with catalog v1.

For the complete proposed relation schemas, growth model, physical storage
patterns, and review query set, see [Data model and query examples](PROJECT_STUDIO_QUERY_EXAMPLES.md).

### Which recent runs recorded errors?

~~~sql
-- :from_time is illustrative; binding syntax freezes with the CLI/API contract.
SELECT
  run_id,
  started_at,
  status,
  revision_id,
  total_errors,
  structure_state,
  value_state,
  integrity_state,
  projection_state
FROM runs
WHERE started_at >= :from_time
  AND total_errors > 0
ORDER BY started_at DESC
LIMIT 100;
~~~

This is a run-grain question. It does not depend on retained per-call instances. See [Product and lifecycle](design/01-product-and-lifecycle.md) and [Logical query catalog](design/04-query-system.md#logical-catalog).

### Which calling contexts consumed the most time in each revision?

~~~sql
SELECT
  definition_key,
  revision_id,
  sum(calls_started) AS calls,
  sum(self_ns) AS self_ns,
  sum(await_ns) AS await_ns
FROM cct_population
WHERE run_id IN (
  SELECT run_id
  FROM runs
  WHERE started_at >= :from_time
    AND started_at < :to_time
)
GROUP BY definition_key, revision_id
ORDER BY self_ns DESC
LIMIT 50;
~~~

This produces population-true totals by revision; it does not, by itself, prove a latency regression. Compare per-call/distribution metrics across revisions for that claim. Cross-revision grouping uses **definition_key**, never the revision-local **function_id**. See [Profiler](design/03-profiler.md) and [Query semantics](design/04-query-system.md).

### Which retained calls contain a customer older than 30?

~~~sql
-- Frozen catalog-v1 syntax: args is a named-argument object.
SELECT call_id, run_id, definition_key
FROM retained_calls
WHERE args['customer']['age'] >= 30
LIMIT 100;
~~~

This means “matching retained instances,” not “30% of all calls.” The planner first pushes safe metadata predicates to the resident store, then hydrates distinct values from the canonical CAS, evaluates the BAML predicate, and applies the final limit. See [Query system](design/04-query-system.md#value-query-execution).

`args` is a virtual query field in this example, not a ClickHouse column. The
resident row contains availability metadata and a private lookup handle; the
captured body remains in local evidence or S3/CAS.

DataFusion lowers the ordinary subscript/comparison expression into internal
hydration, traversal, type-checking, and comparison work. Users do not write
those internal helper calls. Whole-value equality is also direct SQL—such as
`args = baml_value_cid('bamlv_1_…')`—and means exact BAML-value equality,
not partial-object or serialized-byte equality.

### Is the answer complete?

Every SQL stream ends with a typed outcome outside the SQL rows. This is an illustrative shape; exact field spelling freezes with catalog v1:

~~~json
{
  "queryCompleted": true,
  "resultState": "incomplete",
  "snapshot": {
    "catalogVersion": "v1",
    "projectedThrough": "..."
  },
  "valueEvaluations": {
    "attempted": 104,
    "available": 100,
    "unavailable": 4,
    "byReason": {
      "redacted": 3,
      "not_captured": 1
    }
  }
}
~~~

Unavailable evaluations are never silently counted as ordinary SQL NULL/non-matches, and the outcome reconciles every unavailable reason. The exact row-level typed-unknown carrier is a catalog freeze gate. A stream without its terminal outcome is not a successful complete result.

### Ask an agent the same question (target commands)

~~~text
baml query --schema --format json
baml query "<portable SQL>" --format jsonl
baml playground runs show <run-id> --format json
baml playground values read <value-ref> --format json
~~~

The agent reads the documented catalog, submits SQL, checks the terminal query outcome, opens selected evidence, and reports what remains unknown. No LLM runs inside the query plan.

## The user lifecycle

~~~mermaid
flowchart LR
  RUN["Run a BAML program"]
  CAPTURE["Profile locally by default"]
  PLAY["Inspect live and retained runs"]
  QUERY["Ask SQL or an agent"]
  UPLOAD["Optionally upload sealed evidence"]
  HOSTED["Query fleet history"]
  ACT["Reopen, compare, export, or explicitly erase"]

  RUN --> CAPTURE --> PLAY --> QUERY
  CAPTURE --> UPLOAD --> HOSTED --> ACT
  QUERY --> ACT
~~~

1. **Run.** The runtime emits fixed-width structural records into per-thread rings. A background consumer maintains population aggregates, bounded exact-event windows, and captured value roots.
2. **Retain locally.** Sealed artifacts land under the project’s **.baml** directory. Canonical values live in a shared content-addressed store.
3. **Inspect.** The playground’s private fold-engine RPC provides the low-latency live debugger. Ordinary SQL binds a fixed durable snapshot and never follows an unbounded live tail.
4. **Query.** The DataFusion/BAML layer owns parsing, logical planning, ordinary SQL operators/subscripts over virtual BAML values, remaining allowlisted functions, budgets, cancellation, provider mappings, and the terminal outcome.
5. **Upload, when configured.** A host adapter spools immutable chunks, uploads them directly to object storage, and reclaims local bytes only through a receipt-backed contiguous watermark.
6. **Project.** PostgreSQL records commitment and workflow state. SQS carries replaceable pointers. Projectors build non-value analytical facts in ClickHouse. Canonical bodies remain in S3/CAS.
7. **Read hosted.** DataFusion pushes semantics-preserving resident work to ClickHouse, hydrates authorized values from S3/CAS in bounded batches, and evaluates residual value predicates itself.
8. **Retain or erase.** Local cleanup uses configured budgets and CAS reachability. Accepted hosted S3 evidence is retained indefinitely by default; only an explicit authorized erasure removes it.

## Initiative ledger

| Area | User outcome | Current state | Canonical document | V1 gate |
|---|---|---|---|---|
| Profiler and local artifacts | Always-on population profiling plus bounded exact evidence and captured values | **Built and C1-hardened**: crash-safe root-pin barrier, exhaustion-policy ladder, explicit saturation evidence, persisted loss diagnostics, bounded slab/defer memory, continuous CLI value drain | [Profiler](design/03-profiler.md), [Local artifacts](design/storage/local-artifacts.md) | Keep crash/perf gates green while the SQL layer lands |
| Playground fold engine | Open runs, CCTs, and captured values at interactive latency | **Built core**; full Studio experience remains target work | [Studio experience](design/06-studio-experience.md) | RPC/CLI/SQL semantic agreement |
| Public SQL | One portable SQL contract, locally and hosted | **Core built (Q1)**: catalog v1 frozen, DataFusion engine, D7 value lowering, budgets/outcomes gate-tested; local providers and CLI are Q2 | [Query system](design/04-query-system.md) | Local provider conformance, then hosted parity |
| Hosted value queries | Filter resident metadata in ClickHouse, hydrate from S3/CAS, finish value work in DataFusion | **Designed; prototype evidence exists; hosted provider not built** | [Query system](design/04-query-system.md), [ClickHouse](design/storage/clickhouse.md) | Pushdown parity, global budgets, authz, cancellation |
| Capture-to-cloud delivery | Durable receipt-backed upload without runtime-network coupling | **Target v1** | [Capture and ingest](design/05-capture-and-ingest.md), [S3](design/storage/s3.md) | Failure injection and reconciliation |
| Hosted control plane | Transactional tenancy, commitment, projection, policy, audit, deletion | **Target v1** | [PostgreSQL](design/storage/postgres.md) | Migrations, RLS, restore test |
| Hosted analytics | Rebuildable non-value projections and fleet queries | **Target v1** | [ClickHouse](design/storage/clickhouse.md) | Tenant isolation and rebuild/conformance |
| Security and operations | Explicit authorization, audit, durability and failure semantics | **Target v1** | [Security and reliability](design/07-security-and-reliability.md) | Cross-boundary suite, canary, runbooks |
| Historical actions and enterprise extras | Durable background queries, rerun, test creation, multi-cell, BYOK, collaboration | **Deferred** | [Deferred](design/10-deferred.md) | Separate product decisions |

## Read by need

- “I am reviewing this for the first time — ease me in.” → [The story: start here](share/story/00-start-here.md)
- “What is the product and how does a user move through it?” → [Product and lifecycle](design/01-product-and-lifecycle.md)
- “How do the components and authorities fit together?” → [System architecture](design/02-system-architecture.md)
- “How does the profiler work, and what is already built?” → [Profiler](design/03-profiler.md)
- “What SQL is public, what gets pushed down, and how are values handled?” → [Query system](design/04-query-system.md)
- “What tables exist, why does each row exist, and which queries are viable?” → [Data model and query examples](PROJECT_STUDIO_QUERY_EXAMPLES.md)
- “How do bytes get from a runtime into hosted storage?” → [Capture and ingest](design/05-capture-and-ingest.md)
- “What screens, commands, APIs, and live-update semantics exist?” → [Studio experience](design/06-studio-experience.md)
- “What is stored where?” → [Storage index](design/storage/README.md)
- “What are the exact settled choices?” → [Decision register](design/08-decisions.md)
- “What is the build order?” → [Delivery plan](design/09-delivery-plan.md)
- “What is explicitly not v1?” → [Deferred](design/10-deferred.md)
- “How were old documents reconciled?” → [Source map](design/11-source-map.md)
- “What does a term mean?” → [Glossary](design/glossary.md)

## Canonical editing rule

Every material change must update all affected surfaces:

1. the relevant component or storage document;
2. the [decision register](design/08-decisions.md) when a settled choice changes;
3. the [deferred register](design/10-deferred.md) when work crosses the v1 boundary;
4. the [delivery plan](design/09-delivery-plan.md) when sequencing or gates change; and
5. this ledger when user outcomes or implementation state change.

Do not copy a historical proposal into the canon without reconciling its authority, grain, evidence semantics, and implementation status.
