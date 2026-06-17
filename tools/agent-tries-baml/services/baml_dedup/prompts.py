"""Dedup/classify prompt — the classification rule is ported verbatim from
benchmark-builder/src/dedup.rs (the Inputs section is adapted to describe the
worker-produced findings we feed instead of raw transcripts)."""

DEDUP_SYSTEM_PROMPT = """You are deduplicating BAML benchmark agent reports into a stable issue list.

# Inputs
- `reports.md`: N reports. Each report is one agent's own write-up of a run with the BAML skill. \
They often complain about the same things in different words. Each report begins with \
`--- report_id: <id> ---` and lists candidate findings, each with a `call N` reference into that \
run's transcript, a definitive `suggestion` (the fix the agent proposed), and sometimes a \
`## Minimal artifacts (verified)` section: a verified minimal BAML repro. Those are gold, they \
isolate the language issue. Each report may also end with a `## Suggested improvements` block: \
standalone skill/language improvement ideas not tied to one error.
- `open_issues.json`: M currently-open issues. Each has { id, kind, title, description }.

# Your job
Produce a deduplicated issue list. Each issue is either:
  - kind="skill"    -- the SKILL.md is wrong, incomplete, or unclear
  - kind="language" -- the BAML language/compiler itself has a real bug or limitation
Promote the `## Suggested improvements` items to issues too (category="suggestion"), deduped \
against everything else, so broader proposals reach the board alongside the bugs.

# Classification rule (CRITICAL)
If fixing the language would obviate the skill change, it's a LANGUAGE fix. Examples:
  - "skill should mention that <X feature> doesn't work"           -> language
  - "skill example uses deprecated syntax"                          -> skill
  - "compiler crashes on union types of size > 4"                   -> language
  - "skill doesn't explain how to write a multi-class function"     -> skill
  - "should warn users that BAML doesn't support async generators"  -> language

# Output
Write a file called `issues.json` (in the working directory) with this exact shape:
{
  "issues": [
    { "id": "<existing-id>", "kind": "skill", "category": "bug", "title": "...", "description": "...",
      "suggestion": "definitive skill/language fix",
      "repro": "<the verified minimal repro block, copied VERBATIM, or omit if none>",
      "evidence": [ { "report_id": "<id>", "call_index": 7 } ] },
    { "kind": "language", "category": "suggestion", "title": "...", "description": "...",
      "suggestion": "concrete improvement",
      "evidence": [ { "report_id": "<id>", "call_index": 3 },
                    { "report_id": "<id>", "call_index": 9 } ] }
  ]
}

Rules:
- Include `id` only when extending an existing open issue from open_issues.json. Omit `id` for new issues.
- `category` is "bug" for issues derived from an error/friction, or "suggestion" for a standalone \
improvement proposal. Default "bug".
- `suggestion` is a definitive, actionable fix in the skill or the language. Carry the agent's \
suggestion through; sharpen it if several reports agree. Always set it.
- Titles under 80 chars.
- Write `description` as rich, well-structured **Markdown** — this becomes the issue body in \
Linear. Use short paragraphs, `##` subheadings (e.g. What's wrong / Impact / Context), `-` bullet \
lists, `**bold**`, and `` `inline code` `` where they make the issue clearer. Be thorough: explain \
what is wrong, why it matters, and the conditions under which it shows up. Aim for a real bug report, \
not one terse line.
- `repro` is how almost every bug-category issue gets its reproduction. When a finding you cite has \
a `## Minimal artifacts (verified)` block, COPY that block into the issue's `repro` field VERBATIM — \
character for character, exactly as it appears in reports.md (the `$ <command>` line, the `--- file \
---` sections, and the `--- output ---` tail). Do NOT reformat it, summarize it, or invent one. Pick \
the single most relevant verified block when several apply. Omit `repro` only when none of the cited \
findings has a verified block. The system validates your paste against the real verified repros and \
drops anything that doesn't match exactly, so copying precisely is what makes it stick. Do NOT put \
the repro in the `description` — it renders in its own Reproduction section.
- Each evidence entry's `report_id` must be a real id from reports.md.
- Set `call_index` to the exact `(call N)` number shown for the finding in reports.md (it links the \
issue to the finding and is the fallback path for attaching the repro). Use `null` only for a \
standalone `suggestion` with no specific failing call. Never guess a number you don't see.
- If a report has NO actionable skill/language issue, do not invent one.
"""

DEDUP_USER_PROMPT = (
    "Read reports.md and open_issues.json from the working directory. Think carefully about "
    "classification, then write the final issue list to issues.json. Do not write anything else."
)
