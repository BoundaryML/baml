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
      "evidence": [ { "report_id": "<id>", "call_index": 7 } ] },
    { "kind": "language", "category": "suggestion", "title": "...", "description": "...",
      "suggestion": "concrete improvement",
      "evidence": [ { "report_id": "<id>", "call_index": 3 },
                    { "report_id": "<id>", "call_index": null } ] }
  ]
}

Rules:
- Include `id` only when extending an existing open issue from open_issues.json. Omit `id` for new issues.
- `category` is "bug" for issues derived from an error/friction, or "suggestion" for a standalone \
improvement proposal. Default "bug".
- `suggestion` is a definitive, actionable fix in the skill or the language. Carry the agent's \
suggestion through; sharpen it if several reports agree. Always set it.
- Titles under 80 chars. Descriptions 2-5 sentences.
- For `kind="language"` issues: if any cited evidence report has a `## Minimal artifacts (verified)` \
section with an artifact for this issue, append the single most relevant one VERBATIM to the end of \
the `description`, under a `Minimal repro / demo:` heading. Do NOT invent a repro or alter verified BAML.
- Each evidence entry's `report_id` must be a real id from reports.md.
- `call_index` is the call in the report where the failure was described. If you cannot pin a \
specific call, set `call_index` to null. Do not guess.
- If a report has NO actionable skill/language issue, do not invent one.
"""

DEDUP_USER_PROMPT = (
    "Read reports.md and open_issues.json from the working directory. Think carefully about "
    "classification, then write the final issue list to issues.json. Do not write anything else."
)
