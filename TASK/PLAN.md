# Implementation Plan

**Date:** 2026-08-06. **Role:** execution order for the remaining build. The design detail stays in `profiling-design.md` / `studio-design.md` (canonical); this plan sequences the work and records the decisions that close them out. Each decision is labeled by provenance: **[confirmed]** = agreed in discussion · **[rec-applied]** = my recommendation, applied, flag to flip · **[research]** = outcome of a verified research pass · **[shipped]** = already built.

## 1. Decisions

| Decision | Pick | Provenance |
|---|---|---|
| Capture substrate (CCT + CAS values + windows) | done, unchanged | [shipped] |
| Hosted analytics engine | ClickHouse Cloud, as designed (studio §37) | [confirmed] — leverage CH's query-performance work; rent tenancy/quotas |
| Local query engine | **chDB (embedded ClickHouse)** via chdb-rust over the stable C ABI; clickhouse-local retained as the fallback behind a thin engine seam | [confirmed] — empirically verified 2026-08-06: dialect-complete on our surface, 0.4 ms warm queries (~300× vs subprocess), 150→508 MB distribution (smaller than clickhouse-local), Windows = WSL either way |
| Dialect story | One engine family both sides; pin chdb-core and hosted to the same version family; the conformance corpus tests the documented catalog; drift outside it is best-effort, not a treaty | [confirmed] — sameness inherited from the engine, not maintained by us |
| Flagship query pattern | search values → `USING(cid)` → `value_roots_v1` (functions/roles) → `definition_key` → `cct_population_v1` (true rates); the docs, demo, and corpus feature it | [confirmed] |
| Full-value search indexing (`index_full_scalar_bounded` → `value_nodes_v1`) | **ON by default locally**; per-project opt-in hosted | [rec-applied] — local data is the user's own; privacy/cost posture belongs at the hosted boundary |
| Structural-exhaustion default (studio Q1) | **`fail_run`** (typed capacity error, host alive, evidence kept); `abort_process` strict opt-in; `continue_incomplete` diagnostic-only | [rec-applied] — the standing recommendation of both designs; unblocks P0-A |
| Value bodies | canonical in CAS/S3; searchable *decoded* form in the analytical store (previews + value_nodes); chunk-KV in CH permitted as the hosted hydration-cache variant (private, gated) | [research] |
| Hosted CID tokenization | keep (per-tenant PRF); promotion is the cross-plane join; FTS unaffected | [research] |
| DataFusion | not adopted in any shape; ideas harvested (below); DF-everywhere recorded as the named exit with triggers in studio §67 | [research] — `thoughts-on-query/review.md` |
| Cross-plane querying | no federation machinery: promote-then-query, `baml query --hosted/--both`, export+`file()` idioms | [research] |
| LLM/semantic evaluation in queries | never in-plan; P2 = classification dataset keyed (fn, version, model-config, input CID), joined into SQL | [research] + standing no-LLM-in-data-plane rule |
| BQL deletion timing | at `baml query` parity, not before — the demo never breaks | [confirmed logic] |
| In-app SQL box | was deferred; with a resident chDB at 0.4 ms/query it becomes cheap — **M4 stretch**, pulled in if M2 lands smoothly | [rec-applied] |
| Windows | WSL, documented plainly; native is off the table for the whole ClickHouse family (upstream has no Windows) | [research — measured/verified] |
| Near-live SQL freshness | ~1–2 s (flush→project→query) stands; live views stay on the fold-engine RPC plane | [shipped design] |

**Harvested from the DataFusion review** (engine-neutral, scheduled below): `--hydrate --where <sql>`; interactive scan mode with early-stop-at-N; `--hosted/--both`; classification dataset (P2); their tests 1–6/11–13 into the corpus; local cancellation semantics; reader-concurrency contract.

## 2. Already shipped (do not rebuild)

Records/rings/consumer; CCT engine (74.4 ns/call paired, 52,224×–70,200× disk); all formats golden-pinned; value CAS (frozen codec, budgeted decoder, function_id-carrying captures); retention + GC; fold engine + playground UI (runs/CCT/values panels). Committed on `paulo/cct-1`. BQL v1 keeps working until M3.

## 3. Milestones

### M0 — Doc deltas (first, so the designs stay canonical)
1. profiling §10.4/§10.7/§16-Q2: local engine = embedded chDB (measured numbers, hardening list, clickhouse-local fallback seam); startup-latency framing updated (warm 0.4 ms; cold wash).
2. studio §38.2: indexing defaults — local ON, hosted opt-in. §65: Q1 resolved `fail_run`. §67: DF-everywhere exit + triggers.
3. profiling §10.1 loss list: + interactive row-granular early-stop body predicates (interim: M4 scan mode). §8.6: reader-concurrency contract paragraph (GC/compaction/retention vs long-running readers; resident-engine handle semantics).
4. studio §8/§37.1: `baml query --hosted/--both`; local cancellation statement. §37.5: chunk-KV variant note. Dialect-identity language demoted to "one engine family, pinned + catalog-tested" (profiling §10.5).
5. Cross-plane idioms page stub in the schema docs (promote-then-query / export+`file()` / two-queries-merge).

### M1 — Projector + manifest (profiling Q1)
Parquet projection of sealed artifacts → `.baml/proj/v1/<view>/run_id=…` for: runs, cct_population, llm_population, spawn views, value_roots, value_scalars (previews), **value_nodes (decoded scalars — local default ON)**, capture_losses, exact_windows, call_instances (from dumps), functions, revisions. Append-only manifest with seal-CRC drift detection; retention/tombstone handling; hot-tail regeneration; compaction at ~500 files/view.
**Gates:** projections rebuild byte-stable from fixtures; drift/tombstone scenarios pass; preview + scalar projection cost measured per root (per-run byte cap enforced, counted).

### M2 — `baml query` on chDB (profiling Q2)
- Engine: chdb-rust against a **vendored, checksummed** libchdb (no unchecksummed build.rs download); version pinned to the hosted family; `chdb_set_signal_handlers_enabled(false)`; memory-limit behavior (`max_memory_usage` fails the query, never the process) as a CI test; one-connection-per-process respected; thin engine seam so `--engine=clickhouse-local` fallback stays alive.
- Surface: generated init script (explicit schemas, view DDL, the two integer-math UDFs, memory cap); `--schema` rendering grain/trap docs; freshness footer (stderr); `--format json|jsonl`; Ctrl-C cancellation (stated semantics); `--hydrate run=/role=` **and `--hydrate --where <sql>`**; first-use download flow with checksums + `BAML_CHDB_LIBRARY` override; WSL note for Windows.
**Gates:** the user-story catalog passes end-to-end, featuring the flagship pattern; warm-query p50 measured and pinned (expect ~tens of ms dominated by scan, not engine); cold first-run measured; near-live ~1–2 s measured; runaway query fails loudly under the memory cap without killing the CLI.

### M3 — Parity, deletion, conformance, demo (profiling Q0 + Q3)
Delete bql.rs/tests, `baml q`, BqlTable frames + TS decode **after** M2 covers the demo flows. Rewrite `/root/dev/demo/baml-q.md` → `baml-query.md`; refresh demo AGENTS.md. Conformance corpus in CI: catalog + trap cases (instance-vs-population, `ORDER BY run_id`, cross-revision `function_id`, NaN/±0.0, integer quantiles) + harvested tests (resident-only queries do zero blob reads; per-role hydration isolation; distinct-CID-once; availability-state-not-NULL; cancellation stops all stages). Agent eval against `--schema` docs alone before freezing view schema v1.
**Gates:** corpus green in CI; eval recorded; CI-hardware confirmation of the ≤60 ns p99-shape leg (blocking); no `bql` symbol in tree; demo end-to-end on the new surface.

### M4 — Playground + local hardening (studio P0-A slice)
`fail_run` landed as the exhaustion default (+ `abort_process` opt-in flag, `continue_incomplete` for diagnostic-admitted runs); capture-health surfaces; interactive small-scope scan with early-stop-at-N (`baml playground scan`, availability-state-aware output); reader-concurrency contract implemented (open readers survive GC/compaction — hold-off or epoch-swap); resident-engine invalidation keyed on the projection manifest. **Stretch:** in-app SQL box on the resident chDB session (0.4 ms/query makes it interactive-grade).
**Gates:** §14 pressure scenarios behave as specified; index/projection loss rebuilds; local RPC, CLI, and SQL agree semantically.

### M5 — Hosted (studio P0-C + profiling Q4)
As designed, unchanged: ingest (spool → presigned single-PUT → receipt → outbox → projector), same view DDL on ClickHouse Cloud, `(version, sql)` endpoint, grant-profile identities + row policies + column-scoped grants + CONST/MAX profiles + quotas, per-tenant CID tokens, previews + opt-in scalar indexing, promotion path, `--hosted/--both`. Pin hosted to the chdb-core version family; corpus runs pinned-local × Cloud staging.
**Benchmark-owned during M5:** chunk-KV hydration cache vs CID-index+ranged-GET; CH text index over value_nodes; physical layout (§66 list).

### P1/P2 (after)
Deferred scans (full form); rerun / create-test; classification dataset; indexed-path policies; multi-cell.

## 4. Not doing (settled)
DataFusion (any shape) · federation machinery · LLM inside query plans · wasm/browser-local querying · native Windows (WSL documented) · a second query language of any kind.

## 5. Remaining open items
Only benchmark-owned physical choices (§66) and the M4 stretch call on the SQL box. Everything else above is picked; flip any [rec-applied] row by saying so.
