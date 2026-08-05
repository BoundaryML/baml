# STEELMAN: SQL as the query surface for BAML observability

Scope note on ground truth: the shipped decision is design.md:750-752 (three surfaces designed competitively; pipeline DSL won human 8.5 / implementer 8 lenses; **the agent-lens judge preferred SQL at 8.5**); the as-built BQL v1 is bql.rs (2,558 lines) implementing only **3 of the 9 designed set kinds** (RunSet, CtxSet, Table; ValueSet folded in later per IMPLEMENTATION.md:621-634) — bql.rs:1-14. The tentative Studio design independently rejects "physical SQL" as the semantic contract (studio-design §6.1, lines 422-434) and bans "arbitrary raw SQL access to multitenant physical databases" (line 251) — but note that rejection targets *physical/multitenant* SQL, not a SQL dialect over sanely materialized tables, which is the steelman here.

Assumed materialization (the steelman's premise): someone runs the already-planned Parquet export (design.md:752 "sealed segments export to Parquet so DuckDB/DataFusion work outside the product") continuously, into tables `runs(run_id, status, started_at, ended_at, revision_id, ...)`, `cct_nodes(ctx_id, parent, fn, path, root_fn, depth, calls, errors, self_ns, awaiting_ns, total_ns, window_start, revision_id, fqn, hist[16], ...)`, `calls(run_id, thread_id, call_id, node_id, fn, kind, model, tokens_in, tokens_out, start_ns, end_ns, status)`, `values(run_id, call_id, role, cid, bytes, status)`, `events(run_id, thread_id, seq, ts, kind, ...)`, `spawn_edges(parent_node, child_fn, spawns, errored, ...)`. Note `path` and `root_fn` as materialized columns is itself a design choice SQL forces you to make up front (CCT stores parent pointers; design.md:412-424) — several "AWKWARD" ratings below become NATURAL only if the ETL pre-computes them.

## PART 1 — The full query catalog in SQL (DuckDB dialect unless noted)

### §8.5 representative queries (design.md:806-842)

**Q1. Exact prompt + raw model output for a wrong answer** (design.md:808)
```sql
WITH latest AS (SELECT run_id FROM runs ORDER BY started_at DESC LIMIT 1)
SELECT c.call_id, v.role, v.cid, v.bytes
FROM calls c JOIN values v USING (run_id, call_id), latest
WHERE c.run_id = latest.run_id AND c.fn = 'ExtractResume' AND c.kind = 'llm'
  AND v.role IN ('input','raw_output','output');
```
Rating: **split**. The listing is NATURAL. The payload — `get(max_bytes=256kb)` — is **IMPOSSIBLE-WITHOUT-ENGINE-HELP**: bodies are BLAKE3-addressed DAG nodes in packs (design.md:728-736), and "hydration is byte-budgeted with child-CID handles for selective descent" (design.md:735). SQL yields the CID; you need a `read_value(cid, budget)` table function/UDF backed by the pack store. DuckDB *can* host that UDF — so engine help, not engine impossibility.

**Q2. Runaway hot loop, live** (design.md:811)
```sql
SELECT n.fn, sum(n.calls) AS calls FROM cct_nodes n
JOIN runs r ON n.run_id = r.run_id WHERE r.status = 'running'
GROUP BY n.fn ORDER BY calls DESC LIMIT 5;  -- rerun every 2s
```
Top-5: NATURAL. `tree()` context: AWKWARD (recursive CTE over `parent`). `live(interval=2s)`: **IMPOSSIBLE-WITHOUT-ENGINE-HELP** — the design's live plane reads the consumer's in-RAM active delta blocks via `LiveMirrorSource` for "~0-latency instead of waiting on group commit" (design.md:886); a Parquet materialization is by definition behind the group commit and the seal. Client-side polling over exported data structurally cannot see the open window. ClickHouse materialized views / live views get closer but still sit behind ingest.

**Q3. Failed run: one bounded evidence bundle** — `run("run_0147") | failure()` (design.md:814)
`failure()` = "error payload + promoted args + sibling ok/error counts + flight-recorder tail, one bounded result" (design.md:851). In SQL that is four queries (values WHERE role IN ('error','promoted'); sibling counts via self-join on parent node; events tail LIMIT N) stapled together by app code. Each leg NATURAL; the *product* — one bounded bundle with an availability taxonomy — is **AWKWARD**, and honestly it's a stored procedure, i.e., you've rebuilt a DSL verb inside SQL. Rating: AWKWARD.

**Q4. Exact events before the failure** (design.md:817)
```sql
WITH trg AS (SELECT seq FROM events WHERE run_id='run_0147' AND kind='trigger_error' LIMIT 1)
(SELECT e.* FROM events e, trg WHERE e.run_id='run_0147' AND e.seq < trg.seq ORDER BY e.seq DESC LIMIT 200)
UNION ALL
(SELECT e.* FROM events e, trg WHERE e.run_id='run_0147' AND e.seq >= trg.seq ORDER BY e.seq LIMIT 20)
ORDER BY seq;
```
Rating: **NATURAL as a query, IMPOSSIBLE-WITHOUT-ENGINE-HELP as a contract.** If no flight dump covers the scope, SQL returns 0 rows — precisely the "silent-empty" agent hazard the design names (design.md:750, 780, 799-800). BQL's `E_NO_EXACT_SOURCE` "raises ... naming remedies (arm flight recorder / @capture / bounded trace) instead of returning zero rows" (design.md:780). Capture-contract gating requires knowing which exact-event artifacts exist for the scope — metadata a bare events table doesn't carry. A `coverage` table + a mandatory-join convention could emulate it, but nothing in SQL *forces* the join: that's the structural gap.

**Q5. Incident window: hot error contexts** (design.md:820)
```sql
SELECT path, sum(calls) calls, sum(errors) errors, sum(self_ns) self_ns, sum(awaiting_ns) awaiting_ns,
       sum(awaiting_ns)::DOUBLE / nullif(sum(self_ns)+sum(awaiting_ns),0) AS wait_share
FROM cct_nodes WHERE window_start BETWEEN '2026-07-30 03:00' AND '2026-07-30 03:10'
GROUP BY path ORDER BY errors DESC LIMIT 20;
```
Rating: **NATURAL** (given materialized `path`; with raw parent pointers, AWKWARD recursive CTE). This is SQL's home turf.

**Q6. Deploy correlation across a revision boundary** (design.md:823-824)
```sql
SELECT time_bucket(INTERVAL 15 MINUTE, w.window_start) AS bucket,
       count(*) FILTER (...) -- calls, errors: plain sums
       , sum(errors) errors, sum(calls) calls, avg(awaiting_ns) mean_awaiting_ns,
       -- p95 from 16-bucket histograms: unnest + cumulative-sum fold
       (SELECT ub FROM (
          SELECT bucket_upper(i) ub, sum(h.cnt) OVER (ORDER BY i) cum, sum(h.cnt) OVER () tot
          FROM unnest(hist) WITH ORDINALITY AS h(cnt, i)) WHERE cum >= 0.95*tot LIMIT 1) AS p95_total_ns
FROM cct_nodes w JOIN fn_dictionary d ON w.fn = d.function_id AND w.revision_id = d.revision_id
WHERE d.fqn_path = 'main>>handle_request>extract_invoice'   -- path pattern: now a LIKE/regex you write by hand
  AND w.revision_id IN ('v418','v419') AND w.window_start > now() - INTERVAL 48 HOUR
GROUP BY bucket, d.fqn ORDER BY bucket;
```
Rating: **AWKWARD** — three compounding pains: (a) p95 from histogram blocks needs unnest gymnastics in DuckDB (ClickHouse with `-State` aggregate columns does `quantileMerge` natively — a real point for ClickHouse); (b) `align=fqn` cross-revision alignment is a dictionary join the user must know to write, and SQL has no `E_REVISION_MISMATCH` (design.md:799) — get the join wrong and you silently compare disjoint node-id spaces; (c) path patterns (`>>` descendant / `>` child, design.md:793) become hand-rolled regex over materialized path strings.

**Q7. Agent: verify my fix** — diff + `compare(match_io=true)` (design.md:827-828)
```sql
WITH a AS (SELECT vi.cid in_cid, vo.cid out_cid, c.status FROM calls c
           JOIN values vi ON (vi.run_id,vi.call_id)=(c.run_id,c.call_id) AND vi.role='input'
           JOIN values vo ON (vo.run_id,vo.call_id)=(c.run_id,c.call_id) AND vo.role='output'
           JOIN runs r ON r.run_id=c.run_id WHERE r.revision_id='ab12' AND c.fn='user.hello.retry'),
     b AS (/* same for cd34 */)
SELECT a.in_cid, a.out_cid = b.out_cid AS same_output, a.status, b.status
FROM a JOIN b USING (in_cid);
```
Rating: **NATURAL — and this is important for the steelman.** The Merkle short-circuit at the top level is just string equality on materialized CID columns; matching calls by input CID across cohorts is a textbook join. SQL does the §8.5 "verify my fix" story *well*. What SQL cannot do is the next click — `vdiff(role, max_nodes)` descending into the DAG to localize *where* two values differ (bql.rs:2074-2076: "bodies are never fetched" until descent) — that's budgeted CAS traversal = ENGINE-HELP.

**Q8. Duplicate prompts — CID as query primitive** (design.md:831)
```sql
SELECT cid, count(*) n FROM values v JOIN calls c USING (run_id, call_id)
WHERE v.role='input' AND c.kind='llm' AND c.run_id = (...latest...)
GROUP BY cid HAVING count(*) > 1;
```
Rating: **NATURAL.** GROUP BY/HAVING is strictly more fluent than BQL's `stats(n=count(), by=cid) | where(n>1)`. The CAS design *creates* SQL-friendly primitives: once CIDs are columns, dedup, cross-run joins, and cache-hit-rate analytics are all plain SQL.

**Q9. LLM spend via lookup** (design.md:834-835)
```sql
SELECT c.root_fn, c.model,
       sum(c.tokens_in * p.in_price + c.tokens_out * p.out_price) AS usd
FROM calls c JOIN read_csv('prices.csv') p USING (model)
WHERE c.kind='llm' AND c.start_ns > ...
GROUP BY c.root_fn, c.model;
```
Rating: **NATURAL, and SQL is outright better here.** `read_csv` (or `read_parquet`, or a warehouse join against the user's own billing tables) is a general capability; BQL's `lookup(file, on=)` (design.md:791) is a bespoke reimplementation of one join shape. (`root_fn` needs materialization or a recursive CTE — ETL choice.)

**Q10. Is 03:00-04:00 trustworthy?** (design.md:838) — `SELECT process, complete_through, capture_loss, shedding, backlog_age, termination FROM health WHERE ...` given a materialized health table: **NATURAL as a standalone query**. The structural difference: BQL puts a completeness footer on *every* result "computed from the blocks the query actually touched" (design.md:797); SQL makes trustworthiness a separate table you must remember to consult, and nothing correlates it to the blocks your other query read. Emulable by convention (views that join coverage), unenforceable by construction.

**Q11. Privacy audit** (design.md:841) — plain GROUP BY over an audit table: **NATURAL.**

### §8.3 stage catalog residue (design.md:784-793)
- `rollup(by=fn|path|file|package)`: GROUP BY + dictionary join — NATURAL.
- `callers(fn)` / `callees(depth)`: recursive CTE over `parent` — AWKWARD in DuckDB (fine), AWKWARD-to-recently-possible in ClickHouse (recursive CTEs only landed ~24.4). Classic, well-understood SQL pain.
- `spawns()`: spawn_edges table — NATURAL.
- `delta(vs=prev|rev)`: self-join or window functions — NATURAL-to-AWKWARD.
- `hist(metric)`: unnest bucket math — AWKWARD (DuckDB) / NATURAL-ish (ClickHouse aggregate states).
- `tree() flame()`: not queries — rendering sinks with pixel-width-bounded LOD (design.md:888, 924). N/A for SQL; app code either way.
- `instances(source=)`: the capture-contract gate — ENGINE-HELP (as Q4).
- `get(as=MyType)`: typed hydration against the capturing revision's schema (design.md:850) — ENGINE-HELP (UDF + dictionary access).
- `live()`: ENGINE-HELP (as Q2).
- `export(parquet)`: SQL native (`COPY TO`) — SQL wins.
- `critical_path()`: iterative graph algorithm over exact events (design.md:853) — IMPOSSIBLE in practical SQL; it's procedural code in any architecture.
- `--snapshot W` bit-identical replay (design.md:801): ENGINE-HELP in both designs — but note Iceberg/Delta snapshot reads or ClickHouse at a fixed parts-set emulate it; DuckDB over immutable sealed Parquet gets it nearly free (sealed segments are immutable; the snapshot is the file list).

### Studio-design §3 "Questions Studio can answer" (studio-design:169-186)
| Question | SQL rating |
|---|---|
| What failed in the last hour? | NATURAL |
| Why did this run fail? | AWKWARD bundle (Q3); the "exact missing/omitted/redacted/lost/corrupt" taxonomy (line 172) is ENGINE-HELP metadata |
| Which function slower after deployment? | AWKWARD (Q6 shape) |
| Which provider attempt timed out before successful retry? | NATURAL (window function over attempt events within run), *if* attempt facts are rows |
| How much did this agent run cost? | NATURAL sum; "provider-omitted usage remains unknown" (line 175) honesty = ENGINE-HELP |
| Tool calls blocked by hook? | NATURAL over typed event rows |
| Outputs whose top-level enum variant is Rejected | ENGINE-HELP or heavy ETL: value bodies are CAS DAG packs, not columns; needs either a hydration UDF or a shredded/indexed projection (studio-design line 177 says "indexed semantic query" — the index is the engine help in *both* designs) |
| `request.customer.email` ends `.edu` | ENGINE-HELP: nested-path scan over CAS with omitted/redacted/expired taxonomy (line 178); after a hydrating ETL it's `json_extract(...) LIKE '%.edu'` — but full hydration defeats the dedup design |
| Work for app user 123 / session abc | NATURAL (tags), with "absent context ≠ no activity" (line 179) = coverage metadata |
| Open the exact value for this call | ENGINE-HELP (point CAS read + retention state) |
| Reproduce old result / failure→test | workflows, not queries — neither language covers them |
| CPU per function / waiting vs executing | "fact not emitted" (lines 183-184) — the honest-refusal contract, ENGINE-HELP in any surface |

**Tally across the full catalog (~30 distinct asks): roughly 55% NATURAL, 20% AWKWARD, 25% ENGINE-HELP — and every ENGINE-HELP item is equally engine-help under BQL** (BQL doesn't make CAS hydration or live mirroring free; it makes them *stages* instead of UDFs). The honest reading: the *query algebra* is mostly SQL-shaped; the *trust/hydration/liveness contract* is engine-shaped in both worlds.

## PART 2 — Real advantages of SQL that the BQL choice forfeits

1. **Agent fluency is a measured, admitted loss.** The design's own agent-lens judge scored SQL 8.5 and preferred it (design.md:750). LLMs have massive SQL priors; BQL competence is bootstrap-from-schema-doc every session (design.md:802 exists precisely to pay this tax). Subtle BQL misuse gets no correction from model priors; subtle SQL misuse does. In the agent era this is the single largest forfeit.
2. **The BI/tooling ecosystem**: Metabase/Superset/Grafana/notebooks/dbt/warehouse federation all speak SQL. BQL gets none; the design's answer is Parquet export (design.md:752) — which exits the trust envelope entirely (exported Parquet has no completeness footer, no fail-closed errors, no snapshot pinning: **the escape hatch is strictly less safe than in-product SQL would have been**).
3. **Arbitrary joins/window functions/subqueries.** BQL has no general join (only `lookup(file, on=)` and fixed diff/vdiff shapes), no window functions, no subqueries, no HAVING-equivalents beyond `where` after `stats`. Questions like "functions whose error rate doubled week-over-week, excluding runs from CI users, joined to my deploys table" are one SQL statement and are simply outside BQL v1's algebra — and much of v2's.
4. **No bespoke parser/planner/executor to maintain.** bql.rs is already 2,558 lines for 3-of-9 set kinds + ValueSet (bql.rs:5, IMPLEMENTATION.md:597,621); the full §8.2/§8.3 surface (SeriesSet, DiffSet, EventSet, SpawnSet, path patterns, `series`, `diff/compare`, `events`, `critical_path`, the BQL language service for completion/hover, design.md:802) is years of language work SQL gets free.
5. **Two dialects anyway.** The design *mandates* a BQL→ClickHouse compiler for the cloud aggregate subset with a golden corpus diffing local vs ClickHouse results, [launch]-blocking (design.md:849, 1016, 1041). So the team maintains: a DSL, a local engine, a SQL compiler, and a cross-engine equivalence corpus. SQL-first collapses this to one dialect + one semantics; the drift risk the golden corpus exists to police is self-inflicted.
6. **Hiring, docs, Stack Overflow, muscle memory.** Every engineer and every support answer starts from zero with BQL. Query-language adoption failure is the historical norm (see Part 3's graveyard of proprietary DSLs users refuse to learn).
7. **Users' own data as a first-class join** — prices.csv is the toy case; the real cases are billing exports, feature-flag tables, customer tiers, deploy logs. `read_csv`/`read_parquet`/warehouse tables vs a single hardcoded `lookup()` verb.
8. **Composability**: SQL views let users name and layer abstractions (`CREATE VIEW slow_llm_calls AS ...`); BQL has parameterized .bql files (design.md:852) — templates, not composable relations.

## PART 3 — What comparable products actually chose (from memory, knowledge cutoff Jan 2026)

- **Honeycomb**: no text query language for most of its life — a visual query builder over a bounded JSON query spec (Query Data API), plus **derived columns** with a small expression language. Chose bounded-spec-over-SQL for high-cardinality columnar safety. Model ≈ studio-design's `StudioQueryV1` JSON AST (studio-design:432-465), *not* ≈ BQL-the-text-language.
- **Datadog**: proprietary metric/log/trace query DSLs; notably added **DDSQL** (a real SQL surface) later as the product matured — the common arc is DSL-first, SQL-added-under-customer-pressure.
- **Grafana Loki (LogQL) / Tempo (TraceQL)**: purpose-built pipeline DSLs modeled on PromQL. TraceQL exists specifically because trace *trees* need structural operators (`>` child, `>>` descendant) that SQL expresses miserably — BQL's path patterns (design.md:793) are near-verbatim TraceQL structural syntax. **Strongest external precedent FOR BQL.**
- **Prometheus PromQL**: the canonical proof that an ops-domain DSL can beat SQL (SQL-over-timeseries contenders like TimescaleDB never displaced it in ops workflows). Precedent for DSL viability, with a caveat: PromQL succeeded partly by being *the* ecosystem standard, a network effect BQL will not have.
- **Jaeger**: no query language at all — parameterized search API + fixed views. Precedent that trace products can ship without any language.
- **pprof / Polar Signals Parca**: **profilers ship no general query language** — pprof has fixed report types (top/flame/peek) plus focus/ignore regexes; Parca has PromQL-style profile selectors + fixed folds; FrostDB is queried via a logical-plan builder. CCT profiling — BAML's core novelty — sits in a domain whose entire history says "fixed folds + selectors, no language." This cuts *against* SQL **and** against building a big DSL: it suggests most of §8.3's tree verbs (`rollup`, `callers`, `flame`, `top`) would have been fine as CLI subcommands/API methods.
- **SigNoz / Uptrace / ClickHouse-native OTel vendors**: expose **raw ClickHouse SQL directly** (SigNoz: builder + ClickHouse SQL + PromQL side-by-side). **Strongest precedent FOR SQL** — but every such vendor shares preconditions BAML lacks: ClickHouse *is* the canonical store, deployment is server-only (no wasm/laptop/sans-io constraint), values are inline span attributes (no CAS DAG), and they accept silent-empty/unbounded-query hazards as the cost of power.
- **Sentry (Snuba/SnQL, Discover)**: sits on ClickHouse but deliberately **interposed a restricted SQL-shaped semantic layer** (SnQL) rather than exposing ClickHouse SQL — for tenancy, bounds, and schema-migration freedom. This is the closest architectural cousin to what BAML built, and it validates the *gating* logic (mirrors studio-design:424-430) while showing the gate can be SQL-shaped rather than pipe-shaped.
- **OTel**: standardized no query language; every backend picks its own — no help either way.

**Which model BAML most resembles**: BAML = continuous profiler (pprof/Parca lineage: CCT, flame, fixed folds) + tracer over trees (TraceQL lineage: path operators, capture gating) + a CAS value store (no industry comparable; nearest kin is Git plumbing, which also exposes *commands*, not a language). The vendors that chose raw SQL all lack BAML's three binding constraints: the 4.5 MiB gzip wasm ceiling with a 4.4 MiB committed baseline (~100 KiB headroom — verified in `.cargo/size-gate.toml`; DataFusion alone is ~2-4 MB compressed, design.md:750), the sans-io `Poll::NeedData` engine that must run identically over mmap/wasm/HTTP-Range (design.md:876-886), and the capture-contract honesty problem (aggregates always exist; exact instances only sometimes — design.md:780). No comparable product with those constraints chose SQL.

## PART 4 — The strongest honest case FOR SQL (what a skeptical reviewer would argue)

1. **The workload is mostly SQL-shaped.** By the catalog tally, ~55% of the product's own defining queries are plain SQL, including the flagship agent stories (verify-my-fix via CID join, dedup via GROUP BY cid, spend via CSV join — Q7/Q8/Q9 are *better* in SQL). The CAS design ironically manufactures SQL's favorite primitive: a content-addressed join key.
2. **Everything genuinely hard is engine help under BOTH surfaces.** Budgeted hydration, Merkle descent, live mirroring, completeness metadata, capture gating are engine capabilities, not language features — BQL's parser doesn't implement them either; its stages call the same engine. They could be exposed to SQL as table functions (`read_value(cid, 65536)`, `events_around(run, trigger, 200, 20)`, `coverage(scope)`) — DuckDB's extension mechanism exists precisely for this. The choice of surface syntax and the trust machinery are orthogonal; the design itself concedes this ("the DSL's trust machinery, **made mandatory**, resolves SQL's worst agent hazards" — design.md:750 — mandatory-footer-wrapping a SQL executor is equally possible, and Sentry's SnQL proves restricted-SQL-with-enforced-bounds ships at scale).
3. **You are building the SQL layer anyway.** The [launch]-gated BQL→ClickHouse compiler + golden equivalence corpus (design.md:849) plus Parquet export plus the studio's DataFusion/SQLite local acceleration (studio-design:1155-1157) means SQL semantics must be maintained regardless — as a *translation target*, the worst of both worlds: users get a DSL, implementers still debug SQL, and the equivalence corpus polices drift between two engines that SQL-first would have unified.
4. **The escape hatch indicts the choice.** Power users are told to export Parquet and use DuckDB *outside* the trust envelope (design.md:752) — i.e., the design already admits BQL's algebra is insufficient, and its remedy hands the hardest queries to exactly the surface (SQL) with none of the safety rails. In-product SQL with mandated footers would keep those users inside the contract.
5. **A concrete counter-architecture exists**: SQL-core (DuckDB native + ClickHouse cloud, one dialect), domain table functions for CAS/events/coverage, a mandatory result-envelope wrapper (footer, budgets, snapshot pinning = querying the immutable sealed-file set), fixed CLI verbs for profiler folds (`baml top/flame/failure` — the pprof model, no language needed), and for the wasm host either (a) proxy queries to the native server (the playground already has a server) or (b) a small interpreted subset for the browser-only case. Wasm size — the "decisive" argument (design.md:750) — then gates only the one host that could degrade gracefully, not the whole product's query surface.
6. **The rebuttal the steelman must acknowledge** (why the reviewer might still lose): (a) the wasm/VSCode host is not marginal in this product — the playground/extension is a primary surface, and ~100 KiB of gzip headroom is a hard wall; (b) sans-io `Poll::NeedData` over HTTP-Range (design.md:876) has no off-the-shelf SQL engine implementation — DuckDB wants its own I/O; (c) `E_NO_EXACT_SOURCE` as a *type-system* property of set kinds (CtxSet vs CallSet, design.md:780) is enforceable-by-construction in a DSL and only enforceable-by-convention in SQL; (d) BQL v1 as-built is small (2.5 KLOC) precisely because it implements a fraction of the design — the fair comparison is full-§8 BQL vs SQL-plus-UDFs, and the full BQL is the thing that costs years. The reviewer's sharpest closing line: *"You scored SQL highest for the user segment (agents) you say the product is for, then overruled it on a wasm binary-size constraint affecting one host, and your mitigation is to ship SQL anyway — twice: as an unguarded export and as a compiler target."*