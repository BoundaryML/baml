// Changelog — the two-agent draft<->critique pipeline. Ports
// services/changelog_worker/prompts.py. The processor loops DraftEntry ->
// (run code blocks) -> CritiqueEntry until "approve" or an attempt cap. Both are
// single-shot typed LLM calls (opus tier). `context` is the assembled release
// context (version/date/predecessor/commit-log/file-diff) the processor builds.

class ChangelogDraft {
    version: string,
    date: string, // YYYY-MM-DD
    title: string,
    body: string,
    authors: string[], // GitHub logins from the commit log; empty if unsure
}

class Critique {
    grounding: string, // fail|poor|ok|good|great
    completeness: string,
    specificity: string,
    usefulness: string,
    style: string,
    runnable: string,
    verdict: string, // approve | revise
    issues: string[], // empty when approve
    rewrite_hints: string, // empty when approve
}

function DraftEntry(context: string) -> ChangelogDraft {
    client: Opus
    prompt: #"
        You write the BAML changelog. BAML is a typed language for reliable LLM functions. The reader is a developer who uses BAML, not someone implementing BAML. Treat them like an adult.

        You are given the context for ONE release: the version, the date, the predecessor release on the SAME CHANNEL (so a nightly's predecessor is the prior nightly; a canary's predecessor is the prior canary), the commit log for that range, AND the actual file-level diff between those two refs. READ THE DIFF. Commit subjects tell you what people intended; the diff tells you what actually shipped. Every concrete claim in your entry must be traceable to a real change in the diff.

        THE BODY IS THE PRODUCT
        =======================
        The body is what a BAML user reads to decide whether this release matters to them. It is not a code review. Write for the user, not the implementer.

        1. **Lede (first sentence).** Name the most important user-visible change with a specific identifier in backticks: a function, keyword, CLI command, type name, or config field that they will actually type. Bad: "improvements to type checking". Good: "`Self`-typed parameters in interface default methods now resolve correctly".
        2. **Highlights (when warranted).** A short bulleted list with bolded labels, one bullet per substantive change. Each bullet names what changed (a specific identifier), says how the user's code is affected, and skips internal-only refactors unless they change observable behavior.
        3. **Code examples.** Include a short fenced block ONLY when syntax, a CLI command, or an API shape changed. The example must use identifiers and syntax that appear in the diff. Tag the language explicitly (```baml, ```python, ```typescript, ```rust, ```bash, ```json). A `baml` block is ACTUALLY RUN by the harness, so make it self-contained and runnable (a complete expression that evaluates, or a complete top-level declaration). Do NOT show a bare statement referencing an undefined variable. If you cannot write a correct, self-contained example, omit the code block.
        4. **Proportionality.** A trivial release (version bump only, rename) gets a one-sentence body. A substantive release gets several bullets. Padding a quiet release with invented detail is the worst failure mode.
        5. **No internals unless they leak.** Implementation function names belong in commits and PRs, not the body. Mention them only when the user can observe them (an error code or diagnostic name).

        OTHER FIELDS
        ============
        - `version` and `date`: copy from the context as-is.
        - `title`: one short sentence, same standard as the lede. Filler titles ("Various improvements", "Patch release with minor fixes") are unacceptable. If the release is genuinely empty, say so precisely.
        - `authors`: GitHub logins from the commit log. Do not invent. Empty list if unsure.

        PUNCTUATION (strict)
        ====================
        Never use em dashes or en dashes. Use a period, a comma, or split into two sentences. Avoid semicolons. No emoji. No marketing words ("exciting", "powerful", "supercharge", "revolutionize", "seamlessly").

        GROUNDING (most important)
        ==========================
        If the diff shows a refactor with no observable behavior change, your entry must say so. Do not invent user-facing changes. Do not generalize a one-line fix into a feature. Do not name a symbol that does not appear in the diff.

        # release context
        {{ context }}

        {{ ctx.output_format }}
    "#
}

function CritiqueEntry(context: string, draft: string, code_check: string) -> Critique {
    client: Opus
    prompt: #"
        You audit a draft BAML changelog entry against the same release context the drafter saw (commit log + file diff between the previous release on this channel and this release). You are skeptical by default. Your bar is "would a careful BAML user open this changelog and trust it".

        Score each dimension `fail | poor | ok | good | great`. Then emit `verdict = "approve"` ONLY when every dimension is `good` or `great`. Otherwise `verdict = "revise"` with a SPECIFIC list of issues and concrete `rewrite_hints` the drafter can act on.

        DIMENSIONS
        ==========
        GROUNDING: every concrete claim, identifier, command flag, file path, error code, type name, and code example must appear in the diff. A name that "sounds plausible" but isn't in the diff is a hallucination and is `fail`. Quote the hallucinated thing in `issues`.
        COMPLETENESS: does the body cover the substantive user-visible changes in the diff? If the diff adds three new public functions and the body only mentions one, that is `poor`. If the diff is just a version bump and the body is one honest sentence, that is `great`.
        SPECIFICITY: does every paragraph and bullet name a real identifier the user will type? Vague phrasing ("improved performance", "various fixes") is `poor` unless that's genuinely all the diff shows. Internal-only function names in the body is `poor`, not `great`.
        USEFULNESS: would a BAML user reading this know (a) what changed, (b) how their code is affected, and (c) whether they need to do anything? If the entry reads like a code review or a patch note, it's `poor`.
        STYLE: no em dashes, no en dashes, no emoji, no marketing words. Title is not a filler phrase. Body is proportional to the actual change.
        RUNNABLE: every fenced code block in the body actually runs. You cannot execute code, so do not judge by eye. The CODE CHECK section below is the AUTHORITATIVE result of running each block. Score `fail` if any block is marked FAIL, otherwise `great`. When a block FAILED, reflect it in `issues` and `rewrite_hints` (quote the compiler error and tell the drafter to fix or drop that block).

        PROPORTIONALITY CHECK
        =====================
        - If the diff has zero/near-zero substantive changes and the body has many bullets and a code block, that is `fail` on `completeness` and `specificity` (the drafter padded). Flag every invented bullet.
        - If the diff has substantive changes and the body is one generic sentence, that is `fail` on `completeness`. Name the missing items.

        Be specific in `issues` (quote the bad phrase or claim) and concrete in `rewrite_hints`. `issues` must be empty and `rewrite_hints` must be empty when verdict is `approve`.

        # release context
        {{ context }}

        # draft entry
        {{ draft }}

        # CODE CHECK (authoritative results of running each fenced block)
        {{ code_check }}

        {{ ctx.output_format }}
    "#
}
