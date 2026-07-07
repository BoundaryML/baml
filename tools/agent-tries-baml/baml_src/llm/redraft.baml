// Redraft — revise an issue's write-up to address reviewer feedback (Linear
// comments), keeping it faithful to the verified repro. Ports
// services/baml_redraft/prompts.py. In Python this ran as a file-shuffling agent;
// it's really a single text transformation, so here it's one typed LLM call.

class RedraftResult {
    title: string, // under 80 chars
    kind: string, // "skill" | "language"
    category: string, // "bug" | "suggestion"
    description: string, // rich Markdown Linear body
    suggestion: string, // definitive, actionable fix
}

function Redraft(issue: string, feedback: string) -> RedraftResult {
    client: Sonnet
    prompt: #"
        You are revising a single BAML benchmark issue to address reviewer feedback.

        # Inputs
        - `issue`: the current issue -- { title, kind, category, description, suggestion, repro }. `kind`, `category`, and `repro` are read-only context. `repro` (when present) is a VERIFIED minimal reproduction; never alter or contradict it.
        - `feedback`: a reviewer's comments from the Linear board explaining what's wrong with the current write-up (unclear, inaccurate, missing context, wrong classification framing, etc.).

        # Your job
        Rewrite the issue's `title`, `description`, and `suggestion` so they fully address the feedback, while staying faithful to the underlying bug/idea and the verified repro. Do not invent new evidence. If the feedback asks for something the evidence does not support, reflect that honestly rather than fabricating.

        # Rules
        - Keep `kind` the same unless the feedback clearly demands a change.
        - Write `description` as a thorough, well-structured Markdown bug report -- this becomes the Linear issue body. Use short paragraphs, `##` subheadings, `-` bullets, **bold**, `` `inline code` ``, and ```fenced code``` where they help. Directly resolve each point of feedback.
        - Do NOT paste the repro into the description; it is attached separately and rendered in its own Reproduction section.
        - Write `suggestion` as a definitive, actionable fix in structured Markdown with exactly these three parts: `**What to change:**` one sentence naming the fix; `**Where:**` the specific skill section, compiler area, or file the change lands in; `**Change sketch:**` 2-5 bullets or a short fenced before/after snippet showing the shape of the change. ~50-200 words. Always set it.

        # issue
        {{ issue }}

        # feedback
        {{ feedback }}

        {{ ctx.output_format }}
    "#
}
