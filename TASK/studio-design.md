# BAML Studio (Playground) — Canonical Design

**Status:** Canonical. Supersedes `stale-studio-design.md` (the pre-2026-08-04 draft) and applies the aligned query-surface decisions of 2026-08-04.
**Companion:** `profiling-design.md` owns capture, the CCT engine, on-disk formats, the value CAS, and the local SQL tier mechanics. This document owns the *product*: what users see and do, the observation/run model, capture-to-upload delivery, the hosted platform, and operational correctness.
**State:** the local substrate this product reads (formats, fold engine, playground UI skeleton, values panel) is shipped; the product surfaces in this document are design + phases (§23).

**The 2026-08-04 decisions, applied throughout:**
1. One user-facing query language everywhere: the **ClickHouse SQL dialect over versioned, grain-named views**. The BQL pipeline DSL and the StudioQueryV1 JSON AST are deleted, not deprecated. Natural-language questions are answered by a local agent (Codex/Claude skill) that *generates SQL* against the documented view schema. Query-coverage response machinery (coverage modes, `query explain`, the eligible/examined/matched envelope) is deleted; the underlying evidence states survive as **queryable columns**.
2. Hosted querying = a **`(version, sql)` endpoint** routing to versioned views on ClickHouse Cloud, tenancy by **RLS row policies**, budgets by **role-level settings profiles and quotas**, CID-equality search gated behind value-read authorization.
3. Local = the existing Rust **fold engine** (`bex_query`) for the UI plus **`clickhouse-local` over Parquet projections** for SQL. No DataFusion, no analytical SQLite.
4. The UI runs on **~6 private RPC methods** over the fold engine (files + in-process RAM tap) — internal plumbing, not a query surface.
5. **One product: `baml playground`.** No separate `baml studio` command or app; SQL lives under `baml query`. No wasm/browser-only querying; promptfiddle-class browser hosts become **server-backed** (one `baml` binary per session).

---

# Part I — The product

## 1. What this is

BAML Studio is the observability product over BAML's always-on profiling substrate, delivered in three forms that share one implementation authority (same Rust decoders, run reconstruction, value semantics, and view schemas):

1. **Local** — `baml playground`: the CLI + browser UI + agent surface over `.baml/` artifacts on the developer's machine, no account, no server dependency. `baml query` runs SQL locally.
2. **Hosted** — the multi-tenant service: durable ingest of the same artifacts, fleet-scale querying, sharing, retention, and operations.
3. **Offline operations** — the same binary reading exported/imported artifact sets for support, audit, and disaster recovery.

(“Studio” is the initiative/brand name only; the shipped command surface is `baml playground` + `baml query` — there is no `baml studio` command.)

Any ClickHouse row, Parquet file, or browser cache is a *projection*; the artifacts are the evidence. Nothing user-visible is ever manufactured that the runtime did not emit.

## 2. The user journey

The product is **observation-centered for discovery, run-centered for debugging**. An *observation* is a bounded, queryable summary row of one operation (a run, a BAML call, later a model attempt or tool invocation). A *run* is the semantic debugging unit: the causal graph around one root execution.

Journey: see recent operations (live and terminal, unified) → filter by function/status/release/model/tag → select an observation → open its run → inspect the exact call tree, threads, timeline, values, logs, errors → then compare with another run, reconstruct evidence, rerun, or mint a test.

Five distinct "replay" verbs, each a separate command and audit event, never conflated: **Reconstruct** (re-decode evidence, no execution), **Reindex** (rebuild projections), **Reopen** (ordinary read of an old run), **Rerun** (new run identity, prerequisite report first), **Create-a-test** (reviewed fixture generation).

Core workflows: find what went wrong (error → failing subtree → exact captured inputs → source); understand a slow execution (self vs await per context — never a false CPU claim); inspect data without loading everything (budgeted, lazy, CID-addressed); compare behavior across runs/releases; drive everything from a local agent; reuse history (yesterday's run opens today).

## 3. Questions the product answers — and how it stays honest

Representative catalog (each row names its mechanism; ▸ = SQL tier):

| Question | Mechanism |
|---|---|
| What failed in the last hour? | observation explorer; ▸ `runs_v1`/`errors_population_v1` |
| Why did this run fail? | run debugger: failing subtree + error values + flight-dump events |
| Slower after the deploy? | ▸ population views joined across `revision_id` on `definition_key` |
| The timed-out attempt before the winning retry? | provider attempt records (all attempts retained, not just winners) |
| What did this agent cost? | ▸ `llm_population_v1` × user's price table (computed, never stored) |
| Which outputs used enum variant X? | value previews/indexed paths; ▸ `value_scalars_v1` filters |
| The exact value read | audited point read, budgeted hydration by CID |
| Reproduce an old result? | rerun with prerequisite report (§16.7) |
| Production failure → test | create-a-test review workflow (§16.7) |

The closing invariant of the old design survives with a new enforcement story: **no matching row ≠ no matching execution.** Honesty now comes from (a) grain-named views (`*_population_*` complete; `*_instances_*` lower bounds), (b) queryable evidence-state columns and coverage ledgers (`exact_windows_v1`, `capture_losses_v1`, availability states), and (c) documentation with explicit wrong/right trap pairs — not from a coverage calculator. A zero-match result over rows that include `capture_disabled`/`redacted`/`lost` states is not a trustworthy negative, and the schema docs teach exactly that idiom. This is a deliberate trade (enforcement → documentation), recorded in profiling-design §10.1.

## 4. Personas

**App developer**: local loop, no account; playground UI + `baml query`; values on by default for roots and LLM calls. **Production operator**: hosted ingest, fleet views, retention/deletion, runbooks. **AI agent**: the sharpening persona — bounded results, stable IDs, machine-checkable evidence, SQL with documented schema; the NL skill loop is: question → read `--schema` docs → write SQL → execute bounded → fetch runs/values/source by ID → cited narrative. Studio supplies typed facts; the agent interprets; neither upgrades partial to complete. **Engineering lead**: spend/behavior drift across releases; privacy/audit posture.

---

# Part II — Concepts and contracts

## 5. Core concepts

**Artifact** — canonical evidence: profiling streams (`.bamlseg`/`.bamlmeta`/`.bamlprof`), value records + CAS blobs, source/schema snapshots, run/root attachments, completion manifests, service-authenticated commit receipts. Immutable once sealed.

**Event** — one immutable fact (call start/end, thread lifecycle, usage update, provider change). Events are not the default browsing rows; several may describe one operation.

**Observation** — the queryable unit: identity, kind + schema version, containing run, parent/root correlation, start/optional end, state + outcome, function/provider/tool/resource identity, value/metadata refs, evidence state, provenance. Kinds today: `run`, `baml_call`; after the provider/runner language contract lands: `model_attempt`, `tool_invocation`, `resource_operation`. Kinds are capability-versioned; unknown kinds are preserved and surfaced as `unsupported`, never dropped.

**Run** — the semantic debugging unit around one root execution: root attachment + BoundaryId, calls, logical threads, explicit parent/spawn edges, values/errors/logs/loss records, source/schema refs — and **six independent state axes** (§18): execution, structural completeness, value completeness, integrity, projection, retention. A run is never timestamp-defined. **Cross-process default**: one run = one runtime-owned causal graph in one engine; cross-service correlation = *related runs with explicit links*, never merged clocks (decision Q2, confirmed default).

**Program snapshot** — content digest over normalized program/source/schema inputs (the compiler's revision identity, `baml_rev_1_…` — see profiling-design §6; this doc's snapshot concept and the profiling doc's RevisionId are the same object). Deployment name/git commit/build/release are separate optional dimensions; byte-identical snapshots may appear in many deployments (Q4, confirmed default).

**Effective schema** — TypeBuilder-style runtime changes mean the declared snapshot may not describe a call. Contract: `program_schema_digest` + `effective_schema_digest` + optional bounded content-addressed overlay ref; Studio never reverse-engineers schemas from values; until the runtime emits this, type-aware queries are documented as partial (Q5, confirmed default).

**Application context** — optional customer correlation (user/conversation/request/workflow/release): bounded indexed tags in P0, possible reserved first-class `user_id`/`session_id` later; frozen at observation start, never rewrites history (Q3, confirmed default). Distinct from Studio auth identity, tenancy, ingest sessions, and BEX threads.

## 6. Evidence-state vocabulary (replaces "coverage")

The deleted coverage machinery was built from real facts; those facts survive as **data**. Canonical vocabulary (the enum space of view columns, RPC DTOs, and UI badges — never collapsed to a boolean "has data"):

- **Execution**: pending / running / waiting / cancelling / succeeded / failed / cancelled / panicked / abandoned.
- **Body/value availability**: pending / available / missing / omitted / redacted / lost / expired / unsupported / corrupt / not_emitted / capture_disabled / not_indexed.
- **Projection**: pending / active / delayed / failed / rebuilding.
- **Capture guarantee**: off / diagnostic / delivery_required / durable_spool.
- **Headline-reason precedence** (non-overlapping, so totals reconcile in user SQL): unsupported > corrupt > capture-lost > redacted > expired > disabled-by-policy > not-indexed > projection-delayed > complete.

Every dataset that summarizes evidence (per-run, per-dataset, per-indexed-path) exposes eligible/evaluated counts, state, reason, policy version, and committed/projected watermarks as columns. What was deleted is only the coupling of these facts to a query-response envelope.

## 7. The query surface

**Public**: SQL. Locally `baml query "<sql>"` (clickhouse-local over Parquet projections — full mechanics in profiling-design §10.4); hosted `POST (version, sql)` (§16.6). The contract is the **versioned, grain-named view catalog** (profiling-design §10.2 for the v1 catalog; §16 here for hosted physicals). View names + columns + version are the stable integration boundary shared by agents, scripts, and editors. Convenience CLI filters (e.g. `runs list --status errored`) are RPC/CLI sugar over the same data — never a second language.

**Private**: the UI's ~6 RPC methods over the fold engine (run list, run snapshot+patches, graph/profile, values list/read, source). Served directly from fold state and projections; not obligated to route through SQL; no stability contract beyond the UI; cursor semantics (bind scope, sort key, schema version, generation, expiry; opaque tokens) live here.

**Canonical value paths** (resolved needs-decision #1): the structured path encoding + scoped digest (role/argument/segments, map-key vs field disambiguation, display strings never authoritative) **survives as a data-model contract** — it keys the indexed-path dataset (a P1 view over the private `value_nodes` table) and path-level policy even though the query-AST syntax that used it is gone. Views expose `path_digest` + canonical path columns.

**NL questions**: no embedded LLM in the data plane. A local skill loop drives `baml query` + playground CLI reads and produces cited narratives. The stale design's claim that the skill "never generates ClickHouse SQL" is deliberately inverted: generating SQL against documented views *is* the integration.

**CID equality** (resolved needs-decision #5): the stale design rejected raw content hashes as an equality index (dictionary/confirmation attacks on low-entropy values) and prescribed tenant-keyed HMAC tokens. The decision supersedes this with a simpler rule made possible by the canonical value CAS: **CID columns are visible only to principals already authorized to read the underlying values** — enforced per surface (column/row policy on hosted views, value-read authz on RPC and scans). Such principals can only confirm plaintexts they could read anyway, which collapses the attack. Consequences, stated plainly: hosted CIDs are tenant-scoped tokens not comparable to local raw CIDs (documented in the schema); any *future* exposure of equality search to principals broader than value-readers must resurrect the HMAC design — that boundary is the contract.

## 8. Command surface

One family. Representative set (P-1 → P1):

```
baml playground                      # serve UI (local); also the VSCode backend
baml playground runs list|show|graph|profile [--format json|jsonl]
baml playground values read <ref>    # audited, budgeted point read
baml playground observations list|show
baml playground artifacts list|validate [--deep]
baml playground reconstruct|reindex|diff RUN_A RUN_B|doctor [--deep]
baml playground export --format json|jsonl|parquet|otlp   # OTLP = lossy interop, never canonical
baml query "<sql>" [--schema] [--hydrate run=<id> role=<r> --max-bytes <n>] [--format ...]
baml playground serve|tail|upload --to PROFILE            # P0-A
baml playground scan|rerun|test create                    # P1
```

Output contract: stdout = structured results, stderr = diagnostics; `--format json` is a versioned envelope, `jsonl` schema-declared records; human output cosmetic; IDs copyable and resolvable. Exit codes distinguish success / no match / corrupt artifact / unsupported version / invalid SQL / authorization / transport / cancelled; SQL partial/limit behavior follows ClickHouse semantics plus documented flags (the old coverage-coupled exit-code rule is gone).

---

# Part III — Capture and delivery

## 9. Capture is not upload (locked)

Four responsibilities with distinct interfaces and failure reporting, whether or not they share a process:

1. **Instrumentation** (the runtime): owns BAML identities and semantics; never sees S3 credentials, retry policy, tenancy, or ClickHouse schemas.
2. **Host drain adapter**: moves bytes off the hot path. Placements: in-process; native background thread (the shipped default); sidecar/extension; cooperative wasm; standalone agent tailing files.
3. **Durable spool** (optional): fsync'd immutable chunks, retry state, reclamation-after-receipt.
4. **Upload transport** (optional): bounded authorization, immutable chunk upload, manifest commit, retain-until-contiguous-watermark.

Why no universal external agent: Lambda can't guarantee a sidecar; Workers lack processes/filesystems; browsers need cooperative draining; embedded hosts may need delivery-before-return; some users refuse daemons. "The agent owns networking" means the *transport layer* owns networking.

## 10. Capability negotiation and capture modes

The adapter declares: capture mode, structural/value/log buffer bytes, spool kind + capacity, fsync availability, remote delivery availability, max chunk bytes/age, shutdown budget, supported artifact versions. The runtime records the *selected* capability + policy version with the run, so readers can distinguish "configured diagnostic" from "durable capture failed".

Four modes: **off**; **diagnostic** (bounded best-effort; incompleteness surfaced — the local default posture, which per the on-by-default initiative is ON with generous bounds); **delivery_required** (an operation isn't observed until evidence is durably accepted; block-within-budget then fail/mark per policy); **durable_spool** (admitted structure survives network loss once locally fsync'd; pause admission + exhaustion policy when full). Invariant: **no storage ⇒ no lossless async telemetry** — choose off, bounded diagnostic, or wait-for-remote; there is no fourth physics.

## 11. Host matrix

- **Native/VM/container**: `durable_spool`. Rings → native drain thread → immutable chunks → uploader; no network on the hot path; structural vs value pressure budgets separate; outages grow a bounded spool; crash leaves a torn artifact reported explicitly.
- **AWS Lambda**: `delivery_required` or `diagnostic`. Memory chunk builder + optional `/tmp` spool (execution-environment lifetime, *not* durability); bounded extension shutdown; small chunks by bytes/records/age/handler-completion; in `delivery_required` the handler awaits the durability watermark; insufficient budget ⇒ stop accepting or fail per policy — never fake an async flush. One-request-per-event is prohibited.
- **Cloudflare Workers/edge**: `delivery_required` or `diagnostic`. Bounded in-isolate builder → app-owned fetch/Queue/DO/R2 adapter; durable ack for the strong guarantee; large values externalized/omitted by policy; the adapter advertises low limits so it degrades before OOM; diagnostic may lose the tail on abrupt termination (recorded).
- **Browser/wasm** (resolved needs-decision #2): with promptfiddle-class hosts server-backed and browser-local querying deleted, the browser ceases to be a *product* storage/query host. Scope that survives: wasm *capture* for embedded wasm SDK users — in-memory, `diagnostic` mode only, cooperative drain, bounded recorder, values inline-only; the app hands chunks to its own transport if it wants durability. The OPFS/IndexedDB durable-spool design is **dropped** (revisit only on concrete embedded-wasm demand). Same-page live rendering via the wasm fold build survives for now (profiling-design §17-Q3).
- **Local CLI/tests/offline import**: no upload requirement; reads immutable artifacts; state is rebuildable (fold state + Parquet projections); evidence is never rewritten to make it queryable.

## 12. Budgets, pressure, and exhaustion

**Budget domains**: structural ring, value/log queues, chunk builder, spool, upload concurrency, live reconstruction state. Rules: no per-event HTTPS/S3/SQS/PG/CH/fsync; structural and value planes never share a queue; values reserve capacity before copy/encode; large bodies become references; chunks close by age AND size (benchmark envelope: 8–32 MiB, 250–1000 ms, 50k–250k records); live UI consumes incremental patches, never whole-run resends; every benchmark reports app-impact (CPU/alloc/latency/memory/failure), not just throughput.

**Exhaustion scenarios (A–F), all with defined behavior:**
- **A — value/log queue full**: skip bodies per class budget; non-overlapping loss counters; structure preserved; UI shows `value_lost`/`log_lost`, never renders absence as `null`.
- **B — structural drain behind, memory available**: grow within admitted budget, increase bounded drain, emit pressure diagnostics; never drop or sample structural records; no network on the producer path.
- **C — spool full, network down**: soft: stop admitting new runs, reserve close-out capacity, surface via doctor/health, bounded reclamation. Hard: policy — **`fail_run` (recommended default, Q1 — still awaiting product confirmation; the only unresolved runtime-behavior decision blocking P0-A)**: typed observability-capacity error, host alive, evidence retained, run terminal incomplete/failed. Alternatives: `abort_process` (strict) and `continue_incomplete` (diagnostic-admitted runs only, permanently marked). Never switch guarantee level without recording it before loss.
- **D — no durable storage + no remote**: `delivery_required` stops before the reserve is exhausted, retries within budget, then fails the operation — never success-and-hope. `diagnostic` retains bounded evidence and reports the undelivered range.
- **E — killed process/isolate/page**: durable open-marker without completion ⇒ `abandoned`/`incomplete`; torn tail ignored, prefix retained; no idle-timer verdicts; last watermark shown; no knowledge claims about markerless vanished bytes.
- **F — hosted overload post-commit**: committed chunks are durable; projection is *delayed, not lost*; upload authorizations slow/stop; agents keep spooling; UI shows `projection_delayed` + the durable watermark; authorized point reads may reconstruct straight from artifacts.

## 13. Provider/tool/agent alignment

The language branch (`aaron/custom-llm-providers-v3`) owns the semantic source: response metadata, usage categories (input/output/cached-input/reasoning/cost), runner/provider boundary, typed agent events, hook roles, error axes, resources/sessions/background jobs. Studio **must** preserve and query emitted facts, attach via explicit identities, retain *every* attempt (not just winners), aggregate usage without replacing attempt records, keep provider metadata as opaque typed/raw body references under capture policy, and display absent facts honestly. Studio **must not** build a second provider execution model, scrape HTTP, invent a speculative exchange schema before the language design lands, treat `Meta.raw` as a stable cross-provider schema, reconstruct session state from final values, treat effectful hooks as passive, or store credentials/auth headers. A versioned adapter in the decode layer maps the landed record family (`ProviderAttemptStarted/Finished/Failed, UsageUpdated, ToolCall*, ProviderChanged, ResourceOperation*, HookDecision, AgentRunFinished`); projections and UI never parse branch BAML source. Raw request/response bodies: only under explicit capture choice, bounded/redacted/lazy, never auth material; availability states (`not_emitted | capture_disabled | redacted | lost | available`), never blanks. Not a P-1 prerequisite.

---

# Part IV — Architecture

## 14. Local architecture

**Engine**: the shipped fold engine (`bex_query`) incrementally reconstructs runs/observations/values from `.baml/` files (mmap, committed-block scan, torn-tail tolerance) and from the in-process RAM tap for same-process live runs. It serves the private RPC endpoints that power the UI, plus bounded CLI reads. Discovery tracks file identity/generation/offset/digest/decoder-version; truncate/replace/prefix-mismatch starts a new diagnostic generation; blob reads are lazy. Artifacts sync/export exactly — never translated into a foreign cloud-event schema.

**SQL**: `baml query` = Parquet projections + clickhouse-local (profiling-design §10.4 owns the mechanics: projector, manifest, hot tail, pinned binary distribution, hydration). New open item carried there: binary packaging/size (~0.5 GB disk, no native Windows).

**Local control state** (resolved needs-decision #3): the analytical catalog (SQLite/DataFusion) is deleted — analytics are fold state + rebuildable Parquet, and the invariant survives retargeted: *delete rebuildable state, rebuild from artifacts, get an identical semantic hash*. But the **non-rebuildable control responsibilities** still need a durable home: local identity; not-yet-canonical root/run attachments; capture/index/upload policies; spool ownership; upload authorizations + receipts; contiguous sync watermarks; pending operations; migration audit. **Decision: a single small `control.sqlite` remains** (the ban was on the analytical stack, not on a control store — nothing else provides transactional single-writer semantics this cheaply). Durability rules carry over verbatim: single writer, full-sync, backup-before-migration; corruption stops upload/reclamation and never silently recreates state; spool creation = temp file + fsync + atomic rename + fsync dir + transactional ownership commit; reclamation records the contiguous accepted watermark before any unlink.

**Local security**: loopback/Unix-socket bind; Host/Origin validation; no wildcard CORS; one-time browser handoff → rotated HttpOnly SameSite session; explicit consent before any hosted page connects to a local agent; no browser-supplied filesystem paths; audited exact-value reads. **Server-backed promptfiddle addition**: each browser session gets its own `baml` binary/server; that per-session server carries the same origin/session discipline, plus session lifetime/resource caps owned by the hosting service.

## 15. Hosted topology

Producer → agent/transport → immutable chunk upload to S3 + session/authorization/commit via the API → PostgreSQL (control) → outbox/dispatch → SQS (pointers) → Rust projectors → active + terminal projections in ClickHouse → API. Browser/CLI/agents reach data via **(a)** private RPC endpoints (UI) and **(b)** the `(version, sql)` endpoint routing to versioned CH views. Authorized exact-body reads go to S3 through the API's point-read decision.

**Authorities** (locked): exact evidence = artifacts/blobs/attachments/completion manifests/receipts in S3. PG = transactional control and correctness (tenancy, authz, commitment, idempotency, attachments, policy, outbox, checkpoints, generations, audit, deletion). SQS Standard = replaceable at-least-once pointers, never evidence. ClickHouse = rebuildable projections, never authoritative for acceptance/ownership/success/retention/identity/deletion. Ten invariants carry over, two rewritten: honesty = documented view semantics + queryable availability/loss/watermark columns (was: coverage envelope); one implementation = one decoder/reconstruction stack + one view schema across local/hosted/offline (was: one semantic query implementation).

**Stack** (locked): Terraform sole infra owner; ECS/Fargate for api/dispatch/projectors/ops; S3 canonical; SQS Standard; managed PG; ClickHouse Cloud; static TanStack SPA → Rust API; OIDC humans + scoped service credentials. Explicit non-requirements: K8s/EKS, Lambda on the data path, Kafka/Kinesis/Redis/SNS/EventBridge/ClickPipes, browser-held DB credentials.

**Regions, cells, lanes**: region = residency/failure boundary chosen at project creation. Cell = bounded data-plane allocation (bucket/prefix + KMS scope; online/replay/scan/admin queues + DLQs; API/projector capacity; CH service/shards; canary + runbooks). A lane pins a producer stream to a cell for life: `(project_id, routing_epoch, ingest_lane_id) → cell_id`; capacity adds mint a new epoch for *new* streams; existing streams stay pinned until drain/copy/verify/cutover. Initial deployment: shared global control + `cell_000`; two cells never claim independence on one PG writer. Single-run reads hit one cell; project-wide analytics fan out over the bounded lane set and merge **typed partial aggregates or ordered cursors in the API** — never raw high-cardinality rows through PG.

**Multi-cell SQL** (resolved needs-decision #4): raw user SQL does not decompose into per-cell partial-aggregate plans the way a typed AST did. **v1 rule: a project's SQL endpoint is scoped to a single cell** (v1 projects are single-cell by default, so this is invisible); multi-cell projects expose per-cell targets (`(version, sql, cell)`) plus the pre-aggregated rollup views that *are* mergeable. ClickHouse-native distributed views over cells are the P2 escalation if multi-cell fleet SQL becomes a real demand — a deliberate deferral, recorded, not an accident.

**Admission and backpressure**: admission ≤50% of measured sustained max across every dimension (events/bytes/s, PG commit + WAL, S3 requests, KMS rate, projector throughput, CH merge/query pressure, ledger bytes, query concurrency + tenant skew). Backpressure ladder by bytes/age: spool → committed-unprojected → decode backlog → CH pressure → PG pressure. Overload: preserve accepted chunks; cap projector concurrency; pause upload reservations; reject new sessions 429/503 + Retry-After + pause watermark; agents spool; ignoring pause ⇒ no more signed keys; out-of-authorization uploads stay uncommitted orphans. Query-side budgets additionally enforced by the CH role profiles/quotas (§16.6).

**Deployable roles**: one multi-call signed Rust image — agent, api, dispatch, projector, operations-worker, migrate-postgres, migrate-clickhouse, replay, reindex, doctor, export; SPA static. Credential separation: API control-tx / API analytical-query (the SQL endpoint's **dedicated read-only CH role** under RLS + profiles; cannot write CH) / agent ingest / projector object-read-decrypt / projector CH-insert / ops scan-export-delete / PG-migrate / CH-migrate / audit-export. SQS messages never grant access. External-dependency matrix: each of S3/PG/SQS/CH/OIDC/KMS/CDN/telemetry/local-FS has a stated authority, unavailability behavior, rebuild boundary, and "not required by P-1" flag; no dependency substitutes for another (CH is not artifact backup; SQS is not a ledger; platform logs are not audit).

## 16. Hosted data plane

### 16.1 Ingest protocol

Chunk envelope `ArtifactChunkEnvelopeV1`: protocol/schema versions; tenant/project/environment/cell/lane; source-artifact identity + generation; byte offset/length/total; media type + runtime version; stream identity/kind/epoch/sequence/predecessor digest; record count + time/causal bounds; plaintext digest; envelope digest; compression/encryption metadata; capture policy + loss deltas; source/schema refs. Framing: `magic | envelope_len | canonical_envelope | payload | auth_tag`; deterministic encoding; compress-then-optionally-encrypt; provider-side encryption always.

Hard decode limits (violations quarantined, never partially accepted): 64 MiB stored chunk; 256 MiB decoded / ≤32× expansion; 500k records; 8 MiB single structural record; depth 128; zero-origin non-wrapping u64 sequences; allowlisted zstd; deterministic CBOR headers; per-task-class streaming budgets.

Flow: session creation (resolves tenant/project/env, lane/cell, policies, admitted rates, authorization window, versions, durability level) → upload authorization (fsync spool first; server reserves + selects immutable sharded keys; presigned create-only PUTs bound to key/expiry/length/checksum; presigned URLs are bearer secrets, never logged; ingest creds cannot overwrite/delete) → client uploads → **batch commit**: one short PG transaction verifying scope/key/length/checksum/quota (no download/decode on the commit path), idempotent insert, conflict rejection, pending deterministic receipt, contiguous-head advance, audit; then a service-authenticated **receipt object** is anchored; only then is durability acknowledged. Client lifecycle: drain → envelope → compress/encrypt → fsync spool → create-only upload → resolve ambiguity by checksum → batch commit → **retain until receipt-backed largest-contiguous-committed sequence** → reclaim only through the contiguous watermark. Completion: source and run completion manifests (expected stream set with required/optional/omitted/lost per stream); completion is never inferred from idle. Root attachment is an immutable `boundary_id → (process, engine, thread, call)` record; reconstruction follows explicit causal connectivity, never filename/timestamp guessing.

Projection dispatch: transactional outbox → SQS pointer JSON (version/tenant/project/env/cell/lane/date/chunk/kind/generation); workers always reload authoritative PG state; four queue classes (online/replay/scan/admin) with pinned contract (20 s long-poll ×10; 4 d/14 d retention; 14 d DLQ; maxReceiveCount 8; visibility max(5 min, 3× p99) renewed at ⅓ remaining; batch deletes; fair-queue by tenant is not the quota system). Projector lifecycle: pointer hint → reload PG requirement → renewable lease with fence epoch → contiguous committed range from durable next_sequence → bounded-parallel streaming decode in deterministic order → full validation → bounded incremental snapshot restore (every 64 chunks / 256 MiB / 30 s) → normalized outputs → deterministic CH batches → read-back verify uncertain writes → checkpoint only after required visibility verifies → delete SQS ≤ durable disposition. Terminal dispositions: `projected | quarantined_corrupt | blocked_unsupported_version | suppressed_tombstoned | retryable_after(ts, reason)`. Never checkpoint/delete after fence loss. A standing reconciliation suite (orphans, unpublished commits, expired leases, SQS/DLQ loss, stream gaps, ambiguous batches, tombstones, obsolete generations, multipart/quarantine cleanup) each with SLO/dashboard/alert/runbook.

### 16.2 PostgreSQL control plane

Two database families: `studio_control` (tenants, projects, environments, lanes, memberships, service principals, credentials, program snapshots + aliases) and `studio_cell_<id>` (ingest sessions/authorizations/receipts; artifact chunks + stream heads — identity `(ledger_date, tenant, project, stream, epoch, sequence)`, same identity + same manifest hash = idempotent, any immutable-field diff = conflict + quarantine; runs with six state axes; attachments/relationships/completions; projection outbox/checkpoints/batches/generations; backlog counters). Key rules: UUIDv7; BAML identities stored separate and complete; tenant+project in every tenant-owned key; binary digests; lookup tables over PG enums; version columns; soft-delete-then-workflow. Ledger compaction: contiguous receipt-anchored ranges compact into content-addressed manifest segments (Merkle root; one PG row per segment); hot partitions dropped only after a verifier proves exactly-once coverage, segment existence, checkpoints, no holds, grace elapsed. Transaction rules: no row locks during decode or CH writes; short renewable leases with fence epochs; PG is not polled as a job queue; outbox is a short-lived handoff. Tenant isolation: forced RLS, non-owner roles without BYPASSRLS, scoped per-transaction tenant context, reviewed SECURITY DEFINER routines (pinned search_path, minimal columns, bounded timeouts), cross-tenant attack tests with deployed roles. Connections: bounded SQLx pools; Terraform rejects plans exceeding 70% of max_connections; pool wait is backpressure; PgBouncer is a tested enterprise option, not baseline.

### 16.3 ClickHouse projections

CH never decides acceptance/ownership/success/retention. Every row carries provenance: tenant/project/env; generation + decoder + projection-schema versions; program snapshot; source artifact/chunk/record identity + digest; logical row id + semantic version; row hash; batch identity; full BAML identity chain; evidence state. Physical tables (private, projector/migration-only): observations active/terminal, runs, calls, threads, graph edges, operation events, function definitions/parameters, call inputs/outputs/errors, captured values, value nodes (indexed paths), logs, capture losses, engine liveness, evidence-state datasets (renamed from `*_coverage_*`), rollups, projection visibility/integrity-conflicts. **Users query only the versioned, grain-named serving views** via the `(version, sql)` endpoint, under row policies + quotas — the central tenet. The stale design's sentence "hosted v1 exposes no arbitrary raw SQL; a later tenant-dedicated SQL capability requires independent roles/policies/quotas/audit" is superseded *in exactly the way it anticipated*: v1 ships that capability, with those controls, and still never exposes physical tables.

**Logical shapes**: `ObservationSummaryV1` (bounded discovery row) / `ObservationDetailV1` (full identity/provenance/refs) — now also the basis of the public view catalog. Physical layout: **one terminal table + selected column projections** (recommended default), full/core split only if the §21 benchmark forces it. Terminal observation columns: scope; identity (observation/kind/schema-version; run/root/parent; call/thread/process); operation (function + call site; provider/model/attempt; tool/resource); result/time; program + deployment; bounded data summary (declared/effective types, root kinds/sizes, body availability + refs, policy-authorized preview); usage (provider-emitted categories, emitted-vs-estimate flags); context (bounded tags + reserved dimensions); evidence/policy states; provenance. Call-argument summaries: one bounded row per call with per-declared-parameter disposition (`supplied|omitted|defaulted`, types, sizes, child counts, optional policy-gated equality token, body availability) — a missing row ≠ a null argument.

**Duplicate safety** (critical now that users run SQL directly): deterministic logical ID + row hash; deterministic batch ID + ordinal; read-back-by-batch before reinsert; identical duplicates collapse to one semantic fact; same-ID-different-hash = conflict, excluded from normal results and surfaced in `projection_integrity_conflicts`; checkpoints advance only after verification; **no query — user SQL included — relies on background-merge timing or a finite dedup window**; the serving views expose only duplicate/conflict-safe results. Mutability: terminal observations immutable; active rows bounded+versioned; events/logs/losses immutable; reinterpretation = new **projection generation** (build B alongside A, dual-project, replay from the immutable barrier, validate counts/hashes/queries, atomic PG pointer flip, A retained as rollback shadow; requests/cursors bind one generation; double storage is a capacity requirement). Versioned view names are the user-visible face of generations. Ordering/partitioning (benchmark-owned start): `PARTITION BY month(started_at)`, `ORDER BY (tenant, project, generation, date, function_family, started_at, observation_id)`; tenant never the partition key. Rollups: scheduled recomputation from verified terminal rows after a lateness watermark — never insert-triggered — with contributing counts + aggregate checksums; closed windows recompute on late corrections; rollups surface as grain-named views.

**Active observations index**: bounded, rebuildable, short-retention index of started-not-terminal operations (identity, parentage, current state, latest causal version, bounded progress preview, committed+projected watermarks, expiry). Terminal shadows active; no idle-timer terminalization; index loss = delayed visibility, never evidence loss; never feeds long-range rollups.

### 16.4 Live updates

Patch kinds: observation/call/thread upsert + terminalize; graph edge add; value/log availability change; evidence-state change; diagnostic add; run state change. Per-run monotone semantic sequence + durable watermark; volatile pre-flush patches are non-resumable and marked. Reconnect: durable cursor → snapshot at cursor → newer patches only; typed recovery on expired/compacted/future cursors; slow consumers disconnected with the latest recoverable cursor; clients reject duplicate/backward/gapped sequences. Hosted delivery: durable state + SSE; PG NOTIFY as a hint only; lost notifications cost latency, never data; keepalives under LB idle timeouts; connection/tenant/byte caps. Locally the same patch stream is fed by the fold engine's RAM tap.

### 16.5 Browser experience

Five screens: **observation explorer** (terminal-recent ∪ active with shadowing; visible time range; URL-shareable state; token cursors, never offsets; availability/loss/projection-delay badges beside results — the replacement for coverage chips); **run debugger** (independent state axes; full BAML identities and graph edges — no string stacks; tree/graph/timeline/flame of one semantic run; collapse/aggregate/virtualize/incremental fetch; source links only with exact evidence; lazy typed values; ordering-ambiguity markers preserved; provider/tool/agent facts in run context without reducing to an LLM-trace; copyable CLI/API ids); **comparison**; **operations** (scans/reruns/tests/exports with audit); **capture health** (adapter capabilities, pressure, losses, spool state). Performance requirements: virtualization; Canvas/WebGL timelines; summary thresholds; lazy/range body reads; abort obsolete requests; byte-capped caches; streaming/pagination; progressive states; bounded hover prefetch.

### 16.6 The `(version, sql)` endpoint

Request: contract version + SQL text (+ optional cell for multi-cell projects, §15). The API authenticates, resolves tenant→CH user, stamps a query id, executes under the tenant's role (row policies on base tables; INVOKER serving views; MAX-constrained settings profile: readonly, execution time, rows/bytes read, result rows/bytes, memory, concurrency; quotas per interval; `compatibility` pinned to the local engine version), streams results, and audits (statement text, actor, scope, cost). Errors are typed (`invalid_sql`, authorization_denied, budget_exceeded, rate_limited, projection_delayed, …) and never leak secrets, presigned URLs, or other tenants' object names. Capability negotiation advertises supported SQL contract versions + view schema versions (replacing the deleted query-operator/coverage-mode advertisement). Full lockdown list and dialect-drift discipline: profiling-design §10.5.

### 16.7 Operations: scans, rerun, tests

**Deferred scans** (P1): queries needing unindexed retained bodies beyond interactive budgets become explicit operations — immutable evidence barrier + generation; artifact/byte/cost estimate; confirmation above limits; cancellable; streamed through the shared decoder; bounded temp results (CH or Parquet); optional proposal of a future indexed path; scheduled expiry; separate queues/capacity so multi-TB scans never delay online projection. The scan request is a typed scan-predicate (SQL-derived WHERE over the value model), not a resurrected query AST. **Reconstruct** = re-decode + semantic hash + diagnostics, no execution/mutation — comparable against projections to catch projector bugs. **Reindex** = deterministic resumable rebuild from a fixed barrier, activated only after validation. **Rerun** = new run identity + prerequisite report (program/source/schema availability; input body availability; runtime/compiler compatibility; provider/tool/resource config; unrecoverable secrets; side-effect/idempotency risk; policy deltas; expected reproducibility level exact/compatible/approximate); links to the source run; never overwrites history; secrets never recovered from telemetry; hosted rerun disabled by default pending sandbox/credential policy. **Create-a-test** = review workflow (inputs, assertions, target, mocks, redaction findings, uncaptured deps, provenance); user approves before any write. Reads vs mutating operations are separate authorization surfaces, all audited.

## 17. Security, privacy, tenancy

Identity: OIDC for humans; hashed/rotatable/expiring scoped service credentials; scope from authenticated context, never request bodies. Defense in depth: API authz → forced PG RLS → CH views/row policies (front line for the SQL endpoint) → S3 IAM/access points → KMS → queue-role separation. Browsers get no DB or broad object-store credentials; SQL goes through the API; bodies download via narrow URLs only after an authorized point-read decision. Data classification: prompts/responses, I/O/errors/captures, logs, tool args, provider raw, source/schemas, app user ids, filenames/tags are sensitive; ids/sizes/timings are tenant-scoped. Capture/redaction policy controls: whole-run admission; guarantee mode; per-class on/off; summary-only; field/path allow/deny/redact/tokenize; size caps; region + durability; retention/export/deletion; policy identity + transformation reason recorded; no silent structural sampling after complete-guarantee admission. Encryption: TLS everywhere; SSE-KMS; encrypted PG/CH/queues/logs/backups/local; key ids in metadata, never material; optional envelope/BYOK behind the same artifact contract; cryptographic erasure only with independent key topology. Audit: value/body reads, exports, scans, reruns, policy changes, generation activation, deletion/holds, migrations, impersonation, **and every SQL-endpoint query** — never stdout-only. Deletion: tombstone → purge live PG/CH/S3 → replicas/exports/scans/caches → backup expiry or legal hold → verified deleted; access denial first; per-store proof; uninstall retains by default, purge is explicit.

## 18. Reliability

Six independent state axes per run (execution / structural completeness / value completeness / integrity / projection / retention): "succeeded run, complete structure, lost values, delayed projection" must display exactly that. The 24-row failure table survives as the canonical matrix (producer death pre/post fsync; expired authorization; ambiguous PUT; object-without-commit; receipt-unanchored; anchored-but-SQS-lost; duplicates/reorders; retention expiry; worker death around CH writes; unsupported decoder; checksum failures; early completion; CH slow/lost; PG restored-behind-receipts → import valid service receipts, quarantine orphans; tombstoned project; projection schema bug; active-index loss; local projection-cache corrupt → rebuild from artifacts; local control corrupt → stop upload/reclamation, never silently recreate; structural exhaustion → §12 policy). No queue-order dependency (persisted `blocked_gap`). Autoscaling by weighted work (pending bytes/age/records, safe per-task rate, CH merge pressure, PG WAL, S3/KMS quotas), clamped by every downstream limit, admission ≤50%. Durability tiers: `regional_anchored` (baseline; survives task/AZ loss, not region-RPO-zero) and `cross_region_anchored` (awaits replication; separate latency/cost; tier choice = D5). Backup/DR: PG Multi-AZ + PITR + *tested* restore + receipt/object reconciliation; S3 versioning/checksums/lifecycle/inventory; CH backups reduce RTO but **full rebuild from canonical evidence is mandatory**; gates require measured restores.

## 19. Platform observability

Three planes: customer telemetry, platform telemetry, security audit — ingest outages must not blind repair metrics. Portable contract: JSON logs, OTLP traces/metrics, Prometheus, /health/{live,ready,dependencies}; bounded dimensions; no tenant/run/chunk ids in metric labels; never log values/credentials/presigned URLs. Correctness metrics: uploaded_not_committed, committed_not_enqueued, enqueued_not_projected, conflicts, gaps, orphan bytes, receipts-not-imported, partial runs, reconciliation age, active generation — plus SQL-endpoint quota/latency per role. Per-cell end-to-end canary: synthetic artifact through the public path → verify count/digest/generation/authz → record event-to-query latency. Initial SLO targets (benchmark-owned): local view p95 <250 ms; chunk→durable p95 <2 s; event→hosted-queryable p50 <2 s / p95 <5 s / p99 <15 s; run detail p95 <1 s; fleet query p95 <3 s; recovery ≥5× steady state; **zero silent acknowledged structural loss**. Page on SLO burn/durability/DLQ/capacity, not CPU. Runbooks versioned beside Terraform (full list carried from the stale doc; adds SQL-endpoint abuse/quota-exhaustion).

## 20. Packaging and developer experience

Hosted reference: Terraform provisions everything (VPC, ECS/Fargate, LB/DNS/TLS/WAF, versioned S3, SQS + DLQs, RDS Multi-AZ + PITR, same-region ClickHouse Cloud + private connectivity, KMS/secrets/IAM/OIDC, dashboards/alarms/canaries); destroy protection; locked state. Enterprise v1: signed amd64/arm64 images by digest, SBOM + provenance, ECS/Fargate module, external PG/CH/S3-compatible/KMS/OIDC contracts, migration/preflight/doctor/conformance/replay/restore/export/deletion tooling, pinned compatibility matrix; Kubernetes deferred to a contracted customer; provider qualification checklist (checksums, create-only, multipart, queue semantics, termination recovery, restore, cross-tenant tests, air-gap). Dev experience: native Rust + TS HMR; Compose deps per phase (pinned PG, pinned CH, narrow S3 container, deterministic dev identity); **clickhouse-local as a dev dependency of `baml query`**; worktree-scoped naming/ports/volumes; `*-reset` explicitly destructive; Testcontainers + real-AWS contract tests (no emulator claimed as AWS evidence); scoped preview environments with TTL cleanup. Migrations: SQLx checked queries + committed metadata; forward-only, expand/backfill/contract; one migration task per store; CH major changes = new versioned tables + generation; rollback via compatibility/pointer, never down-scripts; user-facing view versions follow the same discipline. Deploy order: build/sign → Terraform → PG migrate → CH migrate → API → dispatch → projectors → canary → promote.

---

# Part V — Verification, state, and phases

## 21. Verification strategy

**Golden semantic corpus** (locked): real + generated artifacts across versions; CLI, local agent, hosted projector, and offline reconstruction must produce identical normalized semantic hashes and evidence-state outputs. Contents: deep/parallel trees; equal timestamps + ring migration; `$id`/heartbeats; success/error/cancel/panic/abandon; all value kinds + omission/redaction/loss; provider attempts/retries/usage; truncation/corrupt framing/unknown fields; duplicate/missing/late/reordered/conflicting chunks; old/new schema manifests; all capture modes. (The profiling substrate's golden tests — `bex_events` golden v1/v2, canon, prof-gate — are the shipped foundation this corpus extends.)

**SQL conformance** (replaces StudioQueryV1 conformance): a fixture corpus of SQL statements against the versioned views returns identical normalized rows/order/aggregates on (a) clickhouse-local over local Parquet and (b) hosted ClickHouse Cloud; version-negotiation tests; availability/loss-column reconciliation tests (non-overlapping precedence totals); tenancy probes through views *and* base tables; trap-case tests of the docs' own examples. CI-gated release blocker (profiling-design §10.5).

**PR suite**: Rust unit/property/fuzz/golden; TS unit/component; decoder compatibility; view-schema + generated-client checks; SQLx offline metadata; PG migrations empty-to-head; CH DDL + serving-view duplicate/conflict tests; Testcontainers repositories; commit→outbox→queue→projector→CH→API path; deployed non-owner RLS/authorization attacks extended to the SQL endpoint. **Nightly**: full Compose black-box; real AWS S3/SQS/KMS; previous-release upgrade + N−1; crash at every durable boundary; PITR + receipt import; ambiguous CH insert + conflict quarantine; generation cutover/rollback races; lease fencing + SIGTERM; SSE recovery; browser e2e; noisy neighbor; load/catch-up/recovery; backup restore + deletion/legal hold; adapter termination/pressure per supported host modes.

**Performance corpus**: up to 50k structural events/s; many small tenants + hot projects; tiny→multi-GB runs; value-mode mixes; late/partial uploads; simultaneous live viewers + point reads + fleet analytics + scans + replay. Measured: app impact; spool/hard-boundary UX; commit throughput; per-store pressure; projector throughput; CH insert/merge/compression; query p50/95/99 + scanned bytes; backlog recovery; full reindex time; cost per M records/TB. The full/core + duplicate-safety benchmark decides physical layout with stated acceptance criteria (no unbounded FINAL/latest-version aggregation in common queries; exactly one semantic terminal observation visible; conflicts detected not hidden; zero-match answers distinguishable from unknown **via availability columns**) — all of which must hold *under the user-facing SQL views*. New benchmark-owned item: the clickhouse-local-over-Parquet envelope for `baml query` (file sizing/partitioning/startup).

**Release acceptance**: view-schema compatibility fixtures; semantic-hash parity local/hosted/offline; no cross-tenant access via API, PG, CH, S3, cursor, export, scan, **or SQL endpoint**; every acknowledged chunk recoverable; migration/rollback compatibility; canary SLOs; projection rebuild within RTO; metrics/alerts/runbooks per new failure mode.

## 22. Implementation state (2026-08-04)

**Shipped** (profiling-design Parts I–III, commits through `fa1fd3091` + successors on `paulo/cct-1`): capture substrate, CCT engine, all storage formats, value CAS with function_id-carrying captures, fold engine, playground UI with runs/CCT/values panels, retention/GC. **Built-then-superseded**: BQL v1 + `baml q` (deletion scheduled, profiling-design Q0). **Not built**: everything hosted (this doc §15–§19), the SQL tier (profiling-design §16 Q1–Q5), capture modes beyond the native default, the provider adapter (blocked on the language branch), scans/rerun/tests.

## 23. Phases

Ordering rule (locked): no phase depends on a later phase; P-1 requires no hosted infrastructure, accounts, or tracing changes.

- **P-1 — Local CLI + SQL (with profiling Q0–Q3).** Versioned decoders as the readable contract; `baml playground` command family over existing `.baml/` artifacts (runs/observations/values/artifacts/doctor/reconstruct/export, JSON/JSONL contracts); Parquet projector + `baml query` + documented view schema v1; golden corpus + semantic hash. Handles torn finals, incomplete runs, old artifacts, wasm-exported sets. **Gates**: current artifacts inspectable and SQL-queryable; torn/incomplete/unsupported explicit; deleting rebuildable state reproduces identical semantics; an agent skill answers the §3 catalog via CLI + SQL against documented schema.
- **P0-A — Live local product.** Incremental local service (fold engine + control.sqlite per §14); SPA explorer + run debugger on private RPC; live patches/cursors; graph/thread/timeline/flame; source/value/log views; comparison/reconstruct/reindex/export; capture health UI; native adapter hardening (**blocked on Q1 confirmation**: `fail_run` default). **Gates**: full product with no cloud; §12 pressure scenarios pass; index/projection loss rebuilds; local RPC, CLI, and SQL agree semantically.
- **P0-B — Provider/tool/agent depth.** Versioned adapter over the landed language contract; model_attempt/tool_invocation/resource_operation kinds; UI projections + availability states; raw metadata under policy. **Gates**: every attempt visible; usage reconciles to attempts; parentage/terminal golden fixtures; old runtimes show unsupported/not-emitted, never empty success.
- **P0-C — Hosted platform (with profiling Q4).** Terraform foundation; reserved presigned upload; receipt-backed commit + contiguous watermark; outbox/queues/fenced projectors/reconciliation/quarantine; active + terminal projections; **`(version, sql)` endpoint + versioned views + RLS + role quotas**; SSE; OIDC/credentials/audit/encryption/canary. **Gates**: acknowledged-chunk survival incl. PG restore; no duplicate semantic observations; object-to-CH rebuild; freshness/query/recovery/cost envelope; cross-tenant attack suite incl. SQL endpoint.
- **P1 — Depth.** Deferred scans; rerun prerequisites + provenance; reviewable test fixtures; richer exports; indexed nested paths; program/effective-schema cohort queries.
- **Enterprise.** Signed artifacts, Terraform module, dependency contracts, identity, retention/durability tiers, optional BYOK, conformance suite.

## 24. Decision register

**Locked**: observation-centered discovery / run-centered debugging; P-1 CLI-first; existing artifacts canonical; capture/drain/spool/upload separation; runtime owns BAML facts; artifacts + PG = hosted truth; Terraform/ECS/S3/SQS/PG/CH stack; bounded rebuildable active index; immutable terminal observations; duplicate delivery never duplicates semantic results; collaboration out of core; **one query language = ClickHouse SQL over versioned grain-named views (local clickhouse-local/Parquet; hosted `(version, sql)`); UI on private RPC; NL via local agent generating SQL; honesty via documented schema + queryable evidence columns; no wasm/browser-only querying; promptfiddle server-backed; one command family `baml playground` + `baml query`**.

**Confirmed defaults**: Q2 cross-process = related runs with links; Q3 app context = bounded tags now, reserved ids later, frozen at start; Q4 snapshot = content identity, deployment as dimension; Q5 effective schema = base digest + bounded overlay, no inference from values. **Resolved this revision**: value-path encoding kept as data-model contract (§7); browser durable-spool dropped, wasm capture = diagnostic-only embedded scope (§11); control.sqlite retained for non-rebuildable control state (§14); multi-cell SQL = cell-scoped v1 (§15); CID equality gated by value-read authorization, HMAC design retired to a boundary condition (§7).

**Awaiting confirmation**: **Q1 — structural exhaustion default `fail_run`** (blocks P0-A; the only unresolved runtime-behavior decision). **Benchmark-owned**: one-table vs full/core; duplicate-safe serving mechanics; active-index engine/TTL; chunk thresholds; CH ordering/projections; admission ≤50%; SLO values; recovery factor; clickhouse-local performance envelope. **Deferred (bounded)**: D1 identity provider; D2 one-command enterprise deploy; D3 Kubernetes; D4 BYOK; D5 cross-region durability tier; D6 billing; D7 producer envelope commitment; D8 final SLOs; D9 retention defaults; D10 collaboration (PG-authoritative if ever).

## 25. Handoff checklist

A complete handoff names: artifact/schema/view versions in production; supported observation kinds + adapter versions; active projection generations per cell; capture modes + exhaustion policy per environment; Terraform state location; migration heads (PG/CH); routing epochs/lanes/capacity; queue/DLQ/reconciliation status; receipt/commitment/checkpoint roots; retention/redaction policies; key ownership; dashboards/SLOs/runbooks; latest restore-test results; deletion/hold status; load/cost envelope; golden corpus + conformance results. Unwritten operator knowledge = incomplete handoff.

## Appendix A — Glossary (delta over profiling-design Appendix B)

**Observation** — bounded queryable summary of one operation. **Active index** — rebuildable short-retention index of non-terminal operations. **Chunk** — immutable record-aligned byte range of one source artifact. **Receipt** — service-authenticated acceptance proof; the reclamation trigger. **Lane/cell** — pinned ingest routing / bounded data-plane allocation. **Generation** — one projection interpretation; rebuilds are new generations. **Grain-named view** — `*_population_*` (complete aggregate) vs `*_instances_*` (windowed exact rows). **`(version, sql)` endpoint** — the hosted query surface; version names the (views + dialect subset + engine pin) contract. **Evidence state** — the §6 vocabulary; never a boolean. **Watermark** — highest contiguous *proven* sequence, never merely highest seen.

## Appendix B — Superseded-material index

`stale-studio-design.md` remains as the detailed reference for: full PG DDL-level schema inventory (its §22.4), the 24-row failure matrix (its §26.2), the complete runbook list (its §28), metric-set enumerations (its §27.5), queue-contract numbers (its §15.6), and provider-qualification details (its §29.4) — all of which this document carries in condensed, decision-current form. Where the two disagree, this document wins; the known systematic deltas are the five decision areas in the header plus the §24 resolutions. `old-references/bql-vs-sql.md` records the query-language decision history.
