// Dedup/classify — collapses N worker reports into a stable issue list. Ports
// services/baml_dedup/prompts.py. The classification rule is verbatim; the only
// change is mechanics: instead of instructing an agent to write issues.json, this
// is a typed BAML function whose return type IS the schema (rendered by
// {{ ctx.output_format }}), so there's no prompt-string + JSON-parse + retry.
//
// kind/category stay `string` (not enums) to match the Convex Issue wire format
// ("skill"/"language", "bug"/"suggestion") byte-for-byte.

class DedupEvidence {
    report_id: string,
    call_index: int?,
    // The finding's `anchor.turn_index` from the report's Findings json, when
    // present — it anchors the dashboard's run link to the exact turn.
    turn_index: int?,
}

class DedupIssue {
    // Include only when extending an existing open issue; omit for new issues.
    id: string?,
    kind: string, // "skill" | "language"
    category: string, // "bug" | "suggestion"
    title: string,
    description: string,
    suggestion: string,
    // The verified minimal repro block, copied VERBATIM, or omitted if none.
    repro: string?,
    evidence: DedupEvidence[],
}

class DedupResult {
    issues: DedupIssue[],
}

function Dedup(reports: string, open_issues: string) -> DedupResult {
    client: Sonnet
    prompt: #"
        You are deduplicating BAML benchmark agent reports into a stable issue list.

        # Inputs
        - `reports`: N reports. Each report is one agent's own write-up of a run with the BAML skill. They often complain about the same things in different words. Each report begins with `--- report_id: <id> ---` and lists candidate findings, each with a `call N` reference into that run's transcript, a definitive `suggestion` (the fix the agent proposed), and sometimes a `## Minimal artifacts (verified)` section: a verified minimal BAML repro. Those are gold, they isolate the language issue. Each report may also end with a `## Suggested improvements` block: standalone skill/language improvement ideas not tied to one error.
        - `open_issues`: M currently-open issues. Each has { id, kind, title }. Match a finding to one of these by its title when it is the same underlying issue, and extend it (emit the issue with its `id`).

        # Your job
        Produce a deduplicated issue list. Each issue is either:
          - kind="skill"    -- the SKILL.md is wrong, incomplete, or unclear
          - kind="language" -- the BAML language/compiler itself has a real bug or limitation
        Promote the `## Suggested improvements` items to issues too (category="suggestion"), deduped against everything else, so broader proposals reach the board alongside the bugs.

        # Classification rule (CRITICAL)
        If fixing the language would obviate the skill change, it's a LANGUAGE fix. Examples:
          - "skill should mention that <X feature> doesn't work"           -> language
          - "skill example uses deprecated syntax"                          -> skill
          - "compiler crashes on union types of size > 4"                   -> language
          - "skill doesn't explain how to write a multi-class function"     -> skill
          - "should warn users that BAML doesn't support async generators"  -> language

        # Rules
        - Include `id` only when extending an existing open issue from open_issues. Omit `id` for new issues.
        - `category` is "bug" for issues derived from an error/friction, or "suggestion" for a standalone improvement proposal. Default "bug".
        - `suggestion` is a definitive, actionable fix in the skill or the language. Carry the agent's suggestion through; sharpen it if several reports agree. Always set it.
        - Titles under 80 chars.
        - Write `description` as a CONCISE Markdown bug report -- this becomes the issue body in Linear. Cover what's wrong and why it matters in ~2-4 sentences or a few short bullets. Be brief and information-dense: no padding, no restating the title, no multi-section essays, no narrating the agent's run. At most one `##` subheading, and only if it genuinely helps. Use `` `inline code` `` for identifiers. The reproduction goes in `repro`, never here. Keep each description well under ~150 words.
        - `repro` is how almost every bug-category issue gets its reproduction. When a finding you cite has a `## Minimal artifacts (verified)` block, COPY that block into the issue's `repro` field VERBATIM -- character for character, exactly as it appears in reports (the `$ <command>` line, the `--- file ---` sections, and the `--- output ---` tail). Do NOT reformat it, summarize it, or invent one. Pick the single most relevant verified block when several apply. Omit `repro` only when none of the cited findings has a verified block. The system validates your paste against the real verified repros and drops anything that doesn't match exactly, so copying precisely is what makes it stick. Do NOT put the repro in the `description` -- it renders in its own Reproduction section.
        - Each evidence entry's `report_id` must be a real id from reports.
        - Set `call_index` to the exact `(call N)` number shown for the finding in reports (it links the issue to the finding and is the fallback path for attaching the repro). Use `null` only for a standalone `suggestion` with no specific failing call. Never guess a number you don't see.
        - Set `turn_index` to the finding's `anchor.turn_index` from the report's Findings json when it is present; `null` otherwise. Never guess.
        - If a report has NO actionable skill/language issue, do not invent one.
        - reports may end with a `# human transcript comments` section: comments human reviewers left on run transcripts. Treat each as a trusted reviewer finding — dedup it against everything else, classify skill/language by the same rule, and cite the comment's run id as the evidence `report_id` (with its turn number as `turn_index` when shown). A comment with no actionable issue (praise, a question) produces nothing.

        # reports
        {{ reports }}

        # open_issues
        {{ open_issues }}

        {{ ctx.output_format }}
    "#
}
