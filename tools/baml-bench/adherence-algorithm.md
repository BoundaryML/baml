# The BAML Adherence Score

> **Status: superseded by intentionbench** — the pipeline in `baml_src/` now implements
> the second-generation design (see README.md): the flat chunk table of §3 became a
> hierarchical CodeBlock tree (program → file → decl → member/var), the edge table
> became first-class InteractionBlocks (by_depth containment, by_level siblings,
> static references) each carrying their own inferred intention, intentions are
> inferred bottom-up on every block (leaves batched per parent), routing is fully
> deterministic (the grading prompt arbitrates borderline routes), and interaction
> blocks are themselves routed and graded. §§5-8 (anchored rubric, evidence gate,
> refutation, omission scan, weighted aggregation) carry over intact, with
> `weight = card status × kind_weight × size_factor`. The typed LLM functions are
> `InferLeafIntents`, `InferBlockIntent`, `InferInteractionIntents` (intent.baml) and
> `GradeCards`, `RefuteFinding`, `ScanOmissions` (grade.baml).

**Goal:** replace the vibe of "this codebase is slop" with a measurable question — *is this codebase using BAML's primitives the way we designed them to be used?* We are in a unique position: the BEPs record not just what each feature does but *why it exists and what it was for*. Adherence to that recorded intent is our definition of quality.

This doc specifies the algorithm. The principle catalog it grades against lives in `design-principles.md`; the raw corpus is in `data/beps/`.

---

## 1. Definitions

- **Chunk** — a graded unit of code. Kinds: `llm_function`, `function`, `type` (class/enum/interface), `test`, `file` (as a namespace/organization unit). Chunks may overlap (a function is also inside its file chunk) — that's intentional: some principles are about expressions, some about organization.
- **Principle card** — one entry from `design-principles.md`: `{id, source_bep, principle, intended_usage, anti_patterns, applicability_trigger, detectability}`.
- **Intention** — a one-sentence inference of *what the author is trying to achieve* with a chunk, plus the *mechanism* they chose. The gap between goal and mechanism is where slop lives.
- **Adherence grade** — 1–10 for one `(chunk, principle)` pair, anchored (see §5).

## 2. Pipeline overview

```
codebase
  │  (a) chunk via compiler AST          — deterministic
  ├─ chunk table
  │  (b) build interaction graph          — deterministic
  ├─ interaction table
  │  (c) infer intention per chunk        — LLM, graph-grounded
  ├─ intention table
  │  (d) route principles → chunks        — static prefilter + semantic gate
  ├─ (chunk, principle) worklist
  │  (e) grade adherence                  — LLM w/ anchored rubric
  │  (f) adversarially verify low grades  — LLM skeptics
  ├─ finding list
  │  (g) codebase-level omission scan     — LLM over whole-graph summary
  └─ (h) aggregate → score + slop report
```

Stages (a)–(b) are pure static analysis. (c)–(g) are LLM stages, each cheap and parallel. Nothing is a single giant "review the repo" prompt — that's how you get vibes back out.

## 3. Chunking and the two tables (stages a–c)

**Chunking uses the compiler, not text.** We own the parser; `baml describe` / the AST gives exact symbol boundaries, kinds, and spans. Text chunking would immediately reintroduce noise into a system whose whole point is precision.

**Interaction table** — edges extracted statically:

| from_chunk | to_chunk | edge_kind |
|---|---|---|
| `extract_invoice` | `Invoice` | `returns` |
| `main` | `extract_invoice` | `calls` |
| `Invoice` | `LineItem` | `contains` |
| `test invoice_parse` | `extract_invoice` | `exercises ($parse)` |

Edge kinds: `calls`, `returns`, `accepts`, `contains`, `implements`, `exercises`, `imports`. This table is what lets the grader see a chunk *in situ* — most misuse is only visible relationally (e.g. a type that exists only to be immediately destructured back into primitives).

**Intention table** — one LLM call per chunk, given the chunk source plus its 1-hop neighborhood from the interaction table:

| chunk | intention (goal) | mechanism (how) |
|---|---|---|
| `parse_date_str` | turn model output into a comparable date | manual string splitting on "-" |

Structured output: `{goal: string, mechanism: string, confidence: 1-5}`. Keep both fields — the goal is what routes principles, the mechanism is what gets graded. (This is exactly the `parse_date_str` failure: goal is legitimate, mechanism ignores BEP-21.)

## 4. Principle routing (stage d)

Grading every chunk against every principle is O(chunks × principles) LLM calls and dilutes the score with irrelevant pairs. Route instead:

1. **Static prefilter.** Each principle card carries an applicability trigger, many of which compile to AST/grep predicates ("chunk mentions a timestamp-like value", "class has >3 optional fields", "test block contains no `assert.*`"). Cheap, high recall.
2. **Semantic gate.** For principles whose trigger is semantic ("chunk is doing retry orchestration"), a small LLM call over the intention table (not the source) decides applicability. Batched — the intention table is tiny.

Output is a worklist of `(chunk, principle, trigger_evidence)` triples. **Record the routing decision** — "N principles were deemed applicable" is itself a health signal, and it makes the score auditable.

## 5. Adherence grading (stage e)

One LLM call per worklist triple. Input: chunk source, its intention row, its 1-hop interaction rows, and the full principle card. Output:

```
{
  grade: 1-10,
  verdict: "adherent" | "neutral" | "fighting" | "reinventing",
  evidence: string,        // quoted line(s) from the chunk
  suggested_form: string?  // what the intended usage would look like here
}
```

**The scale must be anchored or the average is meaningless:**

- **9–10** — uses the primitive as the BEP intends; the code reads like a BEP usage example.
- **7–8** — right primitive, minor deviation from intended form.
- **5–6** — neutral: principle applies but the code neither exploits nor fights it.
- **3–4** — *fighting*: works around the primitive (stringly-typed data past the type system, catch-and-ignore around error design, prompt-string concatenation where interpolation/templates were designed for it).
- **1–2** — *reinventing*: reimplements a primitive by hand (hand-rolled JSON parsing vs the native `json` type, manual date math vs the time stdlib, ad-hoc retry loops vs designed orchestration).

Require the evidence quote. A grade without a pointable line is a vibe, and vibes are the thing being replaced.

## 6. Verification (stage f)

Every grade ≤ 4 gets an independent skeptic pass: a second LLM call prompted to *refute* the finding ("is there a legitimate reason this chunk can't use the primitive — host-boundary constraint, feature not yet implemented at this BAML version, deliberate perf choice?"). Findings the skeptic kills are re-graded neutral. This is the single highest-leverage step for making the score trustworthy — low grades are the ones people will argue with, and false accusations of slop destroy the metric's credibility faster than missed slop does.

The skeptic must know the **BAML version / feature availability**: a principle from a `draft` or `proposed` BEP can't be violated yet. The catalog tags every principle with its BEP status; only `implemented` (and cautiously `accepted`) principles carry scoring weight. Rejected-BEP anti-patterns (e.g. BEP-16, BEP-41) grade as violations when the *rejected* design shows up in user code.

## 7. Omission scanning (stage g)

Chunk-level grading catches *misuse* but structurally misses *avoidance* — the codebase that never touches the `test` block, never uses streaming, hand-rolls every enum as string constants. No chunk triggers the principle because the primitive never appears.

So run a separate pass over the whole intention table + interaction graph summary: for each `implemented` principle, ask "does this codebase have goals this primitive was designed for, while never using it?" Output the same finding shape, kind `omission`, attributed to the file/namespace level. These are often the strongest slop signals — slop is more about what's absent than what's present.

## 8. Aggregation (stage h)

Do **not** report a single flat mean. The report:

```
adherence_score      = weighted mean over graded pairs
                       weight = trigger_confidence × chunk_weight (log LOC)
commission_score     = mean over misuse-eligible pairs only
omission_score       = 1 - (omission findings / applicable principles)
per_principle        = table: principle → mean grade, n, worst finding
per_file             = table: file → mean grade, findings
slop_report          = every verdict ∈ {fighting, reinventing, omission}
                       with evidence + suggested_form
coverage             = % of chunks with ≥1 applicable principle
```

The headline number is fine for tracking a benchmark over time, but the *slop report* is the product: each entry says "here is the line, here is the BEP it ignores, here is what it should look like." That's what turns the score into a feedback loop for both agents and language design.

**Calibration before anyone believes the number:** run the pipeline on (a) our own examples/integ-tests (should score 8+), (b) a deliberately slopped variant of the same programs (should score ≤4), (c) real agent-tries-baml outputs. If (a) and (b) don't separate cleanly, fix the rubric/routing before scoring anything real. Re-run (a) whenever the principle catalog changes.

## 9. Cost & determinism notes

- LLM call count ≈ chunks (intention) + worklist (grading) + low-grades (verify) + principles (omission). For a typical ATB solution (~10–40 chunks, ~100 catalog principles, sparse routing) this is a few hundred small calls — fine to run per-benchmark-solution.
- Every LLM stage takes structured output with low temperature; the static stages are deterministic. Grade drift across runs is measured during calibration (grade the same corpus 3×; per-pair grades should move ≤1).
- The pipeline itself is a natural BAML program: each stage is a typed LLM function (`InferIntention`, `RouteApplicability`, `GradeAdherence`, `RefuteFinding`), the tables are classes, and the whole grader becomes a dogfooding artifact — the quality tool for BAML, written in BAML, graded by itself.

## 10. Relationship to ATB / bamlcode

- **ATB:** run the pipeline on every agent solution; the adherence score becomes a per-run metric next to pass/fail. Diffs in per-principle scores across model versions show *which* primitives agents fight — that's the data-driven language-design loop.
- **bamlcode:** same pipeline on human submissions; the human-vs-agent per-principle delta quantifies "humans and agents want different things."
- **Language design feedback:** a principle that *everyone* scores poorly on is not a quality signal, it's a design signal — the intended usage may be wrong. Persist per-principle aggregates across the whole corpus and review them like BEP feedback.
