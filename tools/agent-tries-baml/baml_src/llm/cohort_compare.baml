// Cohort-compare — the skill-arena synthesizer. Reads per-variant run write-ups
// (one per baml-skill branch, same task + same baml binary) and produces a single
// comparison: which skill version served the agent best, why, and the concrete
// skill/language changes the differences imply. Ports
// services/cohort_compare/prompts.py. Single-shot synthesis -> one typed LLM call.

class CompareFinding {
    kind: string, // "skill" | "language"
    title: string, // <= 80 chars
    description: string, // verbose Markdown
    call_index: int?, // null for arena findings (compare runs, not one call)
    suggestion: string,
}

class CompareSuggestion {
    target: string, // "skill" | "language"
    suggestion: string,
    rationale: string,
}

class Comparison {
    report_md: string, // per-variant verdict, winning branch, key differentiator
    summary: string, // 2-4 sentences
    what_went_well: string[],
    what_failed: string[],
    findings: CompareFinding[],
    suggestions: CompareSuggestion[],
}

function Compare(arena: string) -> Comparison {
    client: Sonnet
    prompt: #"
        You are comparing BAML skill-arena runs to improve the BAML skill.

        # Inputs
        - `arena`: one task run N times, once per BAML *skill version* (each a different git branch of the baml-skill repo). Each variant block begins with `--- variant: <branch> ---` and contains that run's outcome, metrics, the agent's own summary, what went well / what failed, its full report, and its candidate findings (with `call N` references and any verified repro). The SAME prompt and the SAME baml binary were used across variants -- the ONLY thing that differed is the skill version. So any difference in how smoothly the run went is attributable to the skill.

        # Your job
        Compare the variants and decide which skill version handled the task best (fewest dead-ends, less friction, more correct/idiomatic BAML), and explain WHY in terms of what each skill version did or didn't say. Then turn the differences into concrete improvements:
          - kind="skill"    -- a change to the BAML skill/docs (e.g. "the winning branch's skill explained X, which the others lacked; fold that explanation into the skill").
          - kind="language" -- a real BAML compiler/CLI bug or limitation surfaced by the runs (a skill change can't fix it).
        A finding should capture something worth changing, anchored in the variant evidence -- most often a skill improvement the comparison revealed.

        # Rules
        - Always set `report_md` and `summary`; the report is the cohort's record.
        - Titles under 80 chars; write `description` as well-structured Markdown (it becomes the issue body).
        - Use `call_index: null` for arena findings (they compare runs rather than pin one call).
        - If the variants are effectively tied, say so in `report_md` and still surface any shared friction as findings. Do not invent differences that the evidence doesn't support.

        # arena
        {{ arena }}

        {{ ctx.output_format }}
    "#
}
