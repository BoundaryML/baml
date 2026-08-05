# BQL vs SQL: the query-surface decision, investigated fully

**Status:** analysis for review · 2026-08-04
**Inputs:** `TASK/design.md` (shipped observability design, §8), `TASK/studio-design/baml-studio-design.md` (tentative cloud Studio design, §5–§7), `TASK/IMPLEMENTATION.md` (as-built ledger), `crates/bex_query/` (as-built BQL v1). Produced from a multi-agent research pass: five deep readers, an SQL steelman, and two adversarial critiques (anti-BQL and anti-SQL), each verifying claims against repo files.

---

## 0. The one-paragraph answer

"BQL vs SQL" is the wrong dichotomy, and both of our design efforts independently proved it. The real architecture is three layers: **SQL engines as backends** (ClickHouse in the cloud, SQLite/DataFusion in studio's local plans, Parquet/DuckDB as the export escape hatch), **a semantic contract in the middle** (mandatory tenancy/budgets/coverage/typed errors — the layer that carries trust), and **syntax on top** (BQL text for humans and agents, StudioQueryV1 JSON for machines, a typed `baml.obs` API inside BAML — all projections of the same AST). SQL was never rejected as an engine; it was rejected as *the contract*, once by design.md's competitive bake-off and once, independently, by studio-design §6.1. The genuinely decisive arguments are physical and semantic, not aesthetic: a ~100 KiB wasm budget that no SQL engine fits, a sans-io fold engine no SQL engine can replicate, and a capture contract whose honest behavior ("this emptiness is unknowable; here are three remedies") *violates* the semantics of a SQL table scan. The genuinely honest concessions: the design's own agent-lens judge preferred SQL, BQL v1's algebra is today below SQL, and the org currently has **two** bespoke semantic contracts (BQL and StudioQueryV1) that have never been reconciled — this document proposes the reconciliation.

---

## 1. What was actually decided, twice

**design.md §8** ran three candidate surfaces competitively — a pipeline DSL, SQL over DataFusion virtual tables, and a typed stdlib API. The DSL won the human lens (8.5) and implementer lens (8); **the agent-lens judge scored SQL highest (8.5)** and was overruled with a recorded concession: "the DSL's trust machinery, made mandatory, resolves SQL's worst agent hazards (silent-empty results, opt-in completeness)" (design.md:750). The losing candidates both survive inside the architecture: SQL as the ClickHouse compile target (§8.6, [launch]-gated with a cross-engine golden corpus) and as the Parquet/DuckDB export; the stdlib API as the layering endgame (`baml.obs`, §8.6).

**studio-design §6.1** — written independently; the two documents never cite each other's query language — rejects "physical SQL" as the public contract for five reasons, verbatim: local and hosted storage differ; physical tables change across projection generations; tenant scope and budgets must be mandatory; BAML types, value paths, coverage, and three-valued results need domain semantics; a local Codex agent should construct a safe plan without learning private database layouts. Its answer is `StudioQueryV1`, "a versioned semantic AST" in JSON, with SQLite/DataFusion/ClickHouse explicitly *underneath* and a conformance corpus requiring identical results and coverage locally and hosted (§10.7).

Two uncoordinated teams, same conclusion: **SQL engines yes, SQL contract no.** That convergence is evidence about the domain, not about taste.

## 2. The three decisive constraints (physical and semantic)

These are the arguments that survive adversarial review; each was verified against the repo.

**2.1 The wasm wall.** `bridge_wasm` is gated at 4.5 MiB gzip with a committed baseline of 4.4 MiB — ~100 KiB of headroom for *all* future features (`baml_language/.cargo/size-gate.toml`, `.ci/size-gate/wasm32-unknown-unknown.toml`). DataFusion is ~2–4 MB compressed alone; DuckDB-wasm is larger. The playground/VSCode surface is primary, not marginal. Every SQL mitigation either forks the data plane (proxy wasm to a native server — violating "one engine, one wire", design.md:109) or hand-rolls a SQL-subset interpreter — i.e., rebuilds BQL wearing SQL syntax, inheriting SQL's semantic expectations without the capacity to honor them.

**2.2 The sans-io engine.** bex_query's fold runs identically over native mmap, wasm linear memory, and HTTP-Range remote reads via `Poll::NeedData` (design.md §9.2; `source.rs`). No off-the-shelf SQL engine has that I/O model: DataFusion's scan is async, SQLite's VFS is sync, DuckDB owns its I/O. Measured stakes: open-to-first-frame is 2.62 ms vs the legacy 1.65 s *because* the engine folds the storage directly; a SQL surface would sit on a materialization pipeline instead — and the storage it would materialize (session-epoch-local node ids, checkpoint+delta folds, `node_birth` parent-chase, CAS value DAG) is precisely the shape the design chose because per-call rows were the measured disease (38.5 TB/day extrapolated, §1). **The `calls` table the SQL steelman assumes was deliberately abolished.**

**2.3 Silent-empty is SQL semantics, not a SQL bug.** The capture contract is asymmetric: aggregates always exist; exact instances exist only where an exact source covers the scope (design.md §8.2). BQL encodes this in its type system and fails closed — `E_NO_EXACT_SOURCE` with named remedies at three verified sites (bql.rs values/instances gating). In SQL, a scan over an uncovered scope *correctly* returns zero rows; the relational contract requires it. An agent reading that empty result concludes "no errors occurred" — the exact hazard the design names. A coverage-table-join convention can't fix this because nothing *forces* the join; the moment the parser rejects queries that skip it, you have a restricted non-SQL dialect (Sentry's SnQL move) and have forfeited the familiarity dividend that was SQL's whole case. Corollary, verified structurally: BQL's completeness footer travels on **every** result — the BQF1 wire format reserves a meta row so even an empty result is a 1-row frame carrying its footer. SQL result sets have no metadata side channel; two independent teams both had to invent non-SQL envelopes (the local footer; studio's coverage object).

## 3. Query-capability matrix: what each side cannot do

From the steelman's full-catalog pass (design.md §8.5 + §8.3 + studio §3; ~30 distinct asks: ≈55% natural SQL, 20% awkward, 25% engine-help under *either* surface).

**Not doable in SQL (without rebuilding the engine as UDFs/extensions):**
- **Budgeted value hydration with Merkle descent** — `get(max_bytes, depth)` returning elided-subtree CIDs as navigation handles; a `read_value()` UDF is opaque to the optimizer and its degradation semantics have no SQL-level representation.
- **Live queries over the open window** — `live(interval=2s)` reads the consumer's in-RAM delta blocks; any materialization is behind the group commit by construction.
- **Capture-contract gating** — fail-closed `E_NO_EXACT_SOURCE` (see 2.3), and `E_REVISION_MISMATCH`: a cross-revision `GROUP BY function_id` in SQL silently compares disjoint id spaces and returns plausible wrong rows.
- **The mandatory trust envelope** — footer/coverage on every result, surviving empty results and (designed) composition; SQL's is a separate query by convention.
- **Snapshot-pinned reproducibility as a first-class flag** (`--snapshot W`), though DuckDB-over-immutable-Parquet gets close for sealed data.
- **Three-valued results** — studio's `include_unknown` (`true/false/unknown(reason)`); SQL's NULL-in-WHERE collapses unknown→false, converting coverage gaps into confident negatives.
- **`critical_path()`** — iterative graph algorithm; procedural in any architecture.

**Not doable in BQL v1 (today; some designed, some not):**
- **General joins** — including against users' own data (billing exports, deploy logs, feature flags). `lookup(file, on=)` is one hardcoded join shape; SQL's `read_csv`/warehouse joins are strictly superior here.
- **Boolean predicate composition** — `where()` is a single comparison; no AND/OR, no expressions.
- **Window functions, subqueries, cross-run aggregation** — "error rate per fn across the last 20 runs, excluding CI users, joined to deploys" is one SQL statement and outside BQL's algebra.
- **Nested value-path predicates** — studio §3's "email ends in `.edu`" needs the structured value paths (studio §6.3) BQL doesn't have yet.
- **User-defined named abstractions** — SQL views compose; `.bql` files are templates.
- **Series/diff/events stages** — designed (§8.3) but unbuilt.

**Where SQL is genuinely *better*, honestly:** the CAS design manufactures SQL's favorite primitive — a content-addressed join key. Duplicate-prompt detection (`GROUP BY cid`), verify-my-fix (join on input CID), and spend-by-model (CSV join) are *more fluent* in SQL than in BQL's equivalents. The flagship agent stories are SQL's home turf once CIDs are columns. This cuts against any claim that the workload is un-SQL-shaped; it is the trust/liveness/hydration contract that is un-SQL-shaped.

## 4. The honest concessions (what a decision doc must not hide)

1. **The agent-lens judge preferred SQL** (8.5, design.md:750), and agents are the product's sharpening persona (§2.3). The overruling rationale — mandatory trust machinery — is only partially built: the as-built footer is `{sealed, torn, ts-range, degraded[]}` vs the designed `{complete, watermarks, capture_loss[], sources_consulted, truncated, next_cursor, warnings, snapshot}`; `--schema`, snapshot pinning, and the MCP tool are unbuilt. Until they ship, we are defending BQL with features SQL was rejected for lacking that we also lack.
2. **The bake-off's provenance is unauditable.** The losing designs and judge transcripts are not in the repo; the scores are self-reported by the winning design. A fresh measured eval (agents writing BQL-from-`--schema` vs SQL-over-materialized-tables, scored on the catalog *including* trap cases: cross-revision grouping, uncovered scopes, unknown-vs-false) would convert assertion into evidence — for whichever side wins.
3. **"Agents know SQL" is real but weaker than it sounds.** Syntax priors transfer; schema semantics don't. On this schema, SQL priors *actively generate confident wrong answers* (epoch-local node ids, checkpoint folds, revision-scoped function ids, unknown→false). BQL's failure mode is loud (parse error + typed remedy); SQL's is silent (plausible rows, wrong). But this claim, too, is currently asserted, not measured — see the eval above.
4. **Ecosystem isolation is permanent.** No Metabase/Grafana/dbt/notebook will ever speak BQL. The sanctioned answer is Parquet export — which today is unimplemented and exits the trust envelope (no footer travels with a Parquet file). Mitigation: the export is scoped to sealed, immutable segments — the regime where the footer has nothing to warn about — and studio explicitly holds the door open for "a later tenant-dedicated SQL capability" (studio :2805). The industry arc (Datadog → DDSQL) suggests customer pressure will eventually force a read-only SQL projection; we should plan for it rather than deny it.
5. **The maintenance bill is real.** bql.rs is 2,558 lines for a fraction of §8's designed surface; full BQL (series, diff, events, path patterns, language service) is years of language work, *plus* the BQL→ClickHouse compiler, *plus* the cross-engine golden corpus. SQL-first would not have collapsed this to one dialect (the cloud contract can't be physical SQL regardless — studio §6.1), but the count of things-we-maintain is the strongest structural argument the SQL side has.

## 5. The real open question: BQL vs StudioQueryV1 — and the reconciliation

The investigation's most important finding is an internal one: **grep confirms zero occurrences of "BQL" in studio-design and zero of "StudioQueryV1" in design.md.** Two bespoke semantic contracts, each claiming canonicity, mutually unaware. Left unreconciled, "BQL over SQL" actually means "two proprietary languages plus internal ClickHouse SQL plus Parquet SQL" — four surfaces, eroding the "one engine, one wire" principle both docs profess.

They are structurally unifiable, because they are the same *kind* of thing: versioned, typed, budget-mandatory semantic ASTs with trust envelopes. Concretely:

- **One semantic AST, three syntaxes.** BQL's `PlanStage` list *is* a semantic AST already; StudioQueryV1 is a semantic AST without a text form. Commit to: BQL text ⟶ parses to the semantic AST; StudioQueryV1 ⟶ the JSON serialization of that same AST (per studio §7.3, "query representation is the integration boundary" — agents and skills speak the AST, humans speak BQL text, "view as BQL" gives every UI view a copyable text form); the future `baml.obs` stdlib API ⟶ the typed in-BAML projection (pipeline stages are method-chaining-isomorphic: `source | stage | stage` ≡ `source().stage().stage()` — this is why the design calls the wrapper "thin," and it is the composition SQL can never have: `values.get<Invoice>()` returning the user's own class, checked against the capturing revision, with studio's three-valued results mapping onto BAML unions instead of NULL).
- **One trust envelope.** The local footer and studio's coverage object are the same idea on different axes (evidence-plane completeness vs fact-universe accounting). Studio's reason taxonomy is a superset of local capture-loss; local's snapshot pin is stronger than studio's. The union — per-source `{watermarks, sealed/torn, capture_loss[]}` + merged `unknownByReason` + compound snapshot — is constructible and should be specified once, used by both.
- **One conformance corpus.** design.md's local-vs-ClickHouse golden corpus and studio's local-vs-hosted conformance corpus are the same discipline; merge them.
- **The join keys already align and are compiler-owned:** `boundary_id` (ULID), `revision_id`/program snapshot digest, `definition_key` as the cross-revision function key, `(run, thread, call)` call identity, CIDs — all landed in P1/P5. The schemas compose; note one policy tension to resolve: local BQL uses raw CIDs as query primitives (`stats(by=cid)`), while hosted multitenant indexing mandates tenant-keyed HMAC equality tokens (studio §23.5) — federated dedup queries must run in the per-tenant HMAC space.
- **The data-model gap to write down** (it is not a language question): studio's per-observation rows assume per-call facts; the local design abolished always-on per-call events. The reconciliation is that studio's observations are a cloud-ingest projection of *exact-window* artifacts (values, flight dumps, opt-in traces, provider-branch records), with coverage honestly reporting the windows' bounds — i.e., BQL's CtxSet/CallSet split and studio's coverage reasons are one contract in two vocabularies. Say so explicitly in both docs.

## 6. Verdicts by deployment shape

| Shape | Verdict |
|---|---|
| **Local-only (CLI, playground, wasm)** | BQL, decisively. Shipped, fits the wasm gate, native trust contract, O(pixels) responses. SQL-as-surface fails the wasm host outright. |
| **Studio online (cloud)** | A semantic layer regardless — studio's own §6.1 rules out physical SQL even with no local component. The open choice there is text-BQL vs JSON-AST, which are complementary forms of one contract (§5), not rivals. ClickHouse remains the engine underneath. |
| **Federated local + cloud** | The strongest case *for* this architecture and its least-built part: one planner, per-source trust entries, shared compiler-owned keys, associative merge of partial aggregates. SQL federation would require schema identity that doesn't exist and cannot carry coverage. Missing today: the ClickHouse compiler, the federation planner, the unified envelope. |
| **User BYO-warehouse / BI** | SQL wins, by design: Parquet/JSONL export is the sanctioned exit, with the stated boundary that exports are projections, never canonical evidence — and the honest caveat that the trust envelope must be materialized as sidecar columns or the consumer loses it. |

## 7. Recommendations (commitments that make the decision defensible)

1. **Resolve BQL-vs-StudioQueryV1 in writing** — one semantic AST, BQL text + JSON wire + `baml.obs` typed API as its three projections; assign an owner; make design.md and studio-design cite each other.
2. **Ship the trust machinery the decision leans on**: `--schema` self-description, snapshot pinning, the MCP tool, and the widened footer. Until then, soften claims accordingly.
3. **Run the agent eval** (BQL-from-schema vs SQL-over-tables, trap cases included) and attach the results here. It is the single cheapest way to convert the overruled 8.5 into settled evidence.
4. **Close the expressiveness gap on a schedule**: boolean `where`, structured value-path predicates (adopt studio §6.3's canonical paths verbatim), series/diff, and a general `join(on=cid|key)` for the cases where SQL is currently better — these are the SQL steelman's legitimate wins, absorbable into BQL's algebra.
5. **Implement the export with a coverage sidecar** (Parquet + a completeness table), so the escape hatch stops being an exit from the contract.
6. **Build the ClickHouse compiler prototype before the cloud API freeze** (already [launch]-gated) — it is the forcing function for the unification in (1).
7. **Plan for eventual read-only tenant SQL** in studio (the Datadog arc), explicitly as a *projection* surface with the same mandatory envelope injected — on our terms, before customer pressure sets them.

## Appendix: sharpest lines from the adversarial review

*Anti-BQL:* "You scored SQL highest for the user segment (agents) you say the product is for, then overruled it on a wasm binary-size constraint affecting one host, and your mitigation is to ship SQL anyway — twice: as an unguarded export and as a compiler target."

*Anti-SQL:* "The pro-SQL case survives only on the ~55% of the catalog that was never in dispute, by assuming into existence tables the architecture deliberately abolished, and by proposing mitigations — restricted dialect, mandatory wrappers, native-only proxy — that each reconstruct the DSL decision under a different name."

Both are right. The decision holds because the constraints in §2 are physical and semantic rather than stylistic — and it *stays* defensible only if the commitments in §7 actually land.
