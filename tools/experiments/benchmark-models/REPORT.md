# Adherence-by-model — experiment report (2026-07-07)

Five models each built the same small BAML project (receipt extractor: types +
LLM function + pure helpers + tests) in a 25-turn tool loop with real `baml`
CLI access, cold — no BAML knowledge beyond what they extracted from
`baml describe` / `baml check`. Every project was then graded by the same
grader (baml-bench v3, adherence-v3 prompts, Sonnet judge via the local
claude-proxy). Total builder spend: **$2.24** of the $10 cap.

## Headline table

| model | adherence | commission | omission | chunks | pairs | slop | **compiles?** |
|---|---|---|---|---|---|---|---|
| gpt-5-nano | **8.71** | 9.45 | 1.00 | 13 | 23 | 0 | **✗ 407 errors** |
| claude-sonnet-4-6 | 7.94 | 8.26 | 1.00 | 5 | 15 | 2 | ✗ 6 errors |
| gpt-5-mini | 7.51 | 8.76 | 0.97 | 6 | 15 | 0 | ✗ 57 errors |
| claude-haiku-4-5 | 6.97 | 7.90 | 0.98 | 9 | 23 | 4 | **✓ clean** |
| gpt-5.1 | 5.60 | 5.80 | 0.98 | 7 | 26 | 7 | ✗ 12 errors |

## The headline finding: adherence and validity are orthogonal

The adherence score measures whether the *text* uses BAML's primitives the way
the BEPs intend. It does not gate on compilation — and the ranking proves those
are different axes:

- **gpt-5-nano "won" with unparseable pseudo-BAML.** 407 compile errors. It
  wrote fluent, idiomatic-*looking* code that pattern-matches the catalog's
  intended forms (hence 0 slop findings) but is not valid BAML. High adherence,
  zero validity.
- **claude-haiku-4-5 is the only model whose project compiles**, and it ranks
  4th on adherence. It shipped working code using legacy test syntax and a
  prompt that hand-describes the output shape — real, honest slop in real code.

**Takeaway for the benchmark:** report adherence *alongside* a compile gate
(and arguably score `adherence × compiles`). A model that writes beautiful
non-code should not outrank a model that ships working code. This is now the
top follow-up for the harness.

## Per-model breakdown

### gpt-5-nano — 8.71, 0 slop, 0 omissions, 407 compile errors
Wrote the most code (13 chunks, 17 file writes) with the least learning
(5 describes) and never reacted to its 5 failing `baml check` runs. The
grader found nothing to object to *by design intent* — the code shape mirrors
the cards — but the toolchain rejects nearly every line. Fluency without
grounding.

### claude-sonnet-4-6 — 7.94, 2 slop, 6 compile errors
The most deliberate learner: 31 `baml describe` calls before/while writing 5
files (it burned its first, un-anchored run entirely on exploring the
neighboring repo — fixed by seeding `baml.toml`). Had a clean `baml check`
mid-build, then broke it with a late edit and hit the turn cap. Slop: an
untested LLM function (P-036-1) and a braceless lambda in a `reduce`
(P-017-2).

### gpt-5-mini — 7.51, 0 slop, 57 compile errors
Explorer profile (19 describes, 5 writes, one check). Zero per-chunk slop but
four omissions: hand-rolled `if`/`includes` chains where pattern matching
fits (P-015-2), a stringly `date` field instead of time types (P-021-1),
hand-rolled loops where core methods (P-043-1) and iterators (P-051-1) exist.
Clean-ish text, meaningful gaps, invalid project.

### claude-haiku-4-5 — 6.97, 4 slop, **compiles clean**
The pragmatist: balanced tool mix (8 describe / 9 check / 10 write) and the
only project the compiler accepts. Its slop is legible and real: legacy
declarative `test { functions [...] args {...} }` blocks (P-023-2 ×2), no test
exercising the LLM function (P-036-1), and a prompt that hand-describes the
output JSON instead of `{{ ctx.output_format }}` (P-036-2, grade 1 — the
schema-drift anti-pattern verbatim).

### gpt-5.1 — 5.60, 7 slop, 12 compile errors
Wrote TypeScript in .baml files: `=>` lambdas (P-017-1 ×3), reflexive
`return`s (P-017-3), legacy test syntax (P-023-2), and it reinvented the LLM
call as `call_llm_function<Receipt>(client, prompt_string)` instead of
declaring an LLM function (P-012-1) — the exact "fighting the design" failure
mode the benchmark was built to catch. Also never compiled, and its 5 checks
never converged.

## Method notes & caveats

- **n=1 per model.** One build per model, one task. Treat orderings within
  ~1 point as noise until we run repetitions (the harness makes this cheap:
  `--force` rebuilds, grading is cached per project content).
- **Same grader everywhere** (Sonnet via proxy, temperature 0, content-addressed
  cache), so grader bias is constant across models; differences are builder
  differences.
- **All five models hit MAX_TURNS (25)** — none decided it was done. A higher
  cap or an explicit "stop when check passes" incentive may change compile
  outcomes, especially for sonnet (which was one edit away).
- Two grading pairs for nano filled in on a re-grade after transient proxy
  failures (8.71→8.82 on one re-run); the cache makes this converge
  monotonically. Table shows the current cached state.
- Costs: $1.11 (OpenAI trio, incl. the wasted un-anchored Claude attempts) +
  $1.13 (Claude pair rerun). Grading was $0 (subscription proxy).

## Follow-ups

1. **Compile gate in baml-bench** — run `baml check` during grading, put the
   result in the report, and expose `adherence × compiles` as the headline
   composite. (The nano result makes this non-optional.)
2. Repetitions (3-5 builds/model) for variance bars.
3. Give builders the same turn budget but an explicit reward for stopping
   early on a clean check — tests whether models *can* converge or just don't.
4. Wider roster (Gemini line is one uncomment away; opus tier gated on cost).

## Artifacts

- `results.json` — the table data
- `runs/<provider>-<model>/project/` — each model's BAML project, as written
- `runs/<provider>-<model>/bench/report.md` — full per-model adherence report
- Build transcripts: `/tmp/atb-exp-run.log` (OpenAI trio), `/tmp/atb-exp-run3.log` (Claude pair)
