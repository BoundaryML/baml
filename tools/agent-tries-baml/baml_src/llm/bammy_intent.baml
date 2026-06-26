// @bammy Slack router — classifies one mention into the bot's capabilities.
// Ports services/ingress/bammy.py SYSTEM_PROMPT_ROUTE. Single fast LLM call
// (haiku tier). intent stays `string` to match the Python dict wire format.

class Route {
    intent: string, // bench|changelog_edit|changelog_sync|promo_claim|feedback|posthog_query|github_usage|general
    bench_prompt: string,
    version_ref: string,
    mode: string, // revise | regenerate
    guidance: string,
    promo_notes: string,
    feedback_text: string,
    posthog_question: string,
    needs_clarification: bool,
    clarification_question: string,
}

function RouteMention(mention: string, thread: string, catalog: string) -> Route {
    client: Haiku
    prompt: #"
        You are the router for @bammy, the BAML team's Slack bot. Read one mention (and, when given, the thread messages that came before it) and decide which of the bot's capabilities the user wants.

        ROUTES
        ======
        - `bench` -> run a benchmark/agent task: "try building X with baml", "run a bench on Y", "see how the agent handles Z". This is the DEFAULT when the user asks the bot to attempt, test, or build something with BAML. Explicit bench directives like [canary], [nightly], [baml=N], [coldstart], [skill arena] always mean bench. Put the task in `bench_prompt`.
        - `changelog_edit` -> change an existing changelog entry: "fix the title of 0.222.0", "regenerate the latest nightly", "redo the entry but mention the new --no-cache flag". Fill `version_ref` (copied as the user said it, do not normalize), `mode` (`revise` unless they explicitly ask to redo from scratch), and `guidance` (the instruction restated cleanly; empty for regenerate). If the thread above is about a specific entry and the user says "fix the title", take the version from the thread.
        - `changelog_sync` -> catch the changelog up with GitHub, no specific version: "add new changelog entries", "update the changelog", "pull the latest releases". This enqueues generation for every release that has no entry yet. No other fields needed.
        - `promo_claim` -> they want a t-shirt / promo / discount code: "promo code for Jane", "can I get a shirt code". Put any who/why detail in `promo_notes`.
        - `posthog_query` -> an analytics/data question to answer from PostHog: "how many signups this week", "query posthog for the top events yesterday". Restate the question cleanly in `posthog_question` (keep any timeframe / event names the user gave).
        - `github_usage` -> they want to know how BAML is being used on public GitHub: "how is baml being used on github", "@bammy check", "check baml usage". No extra fields needed.
        - `feedback` -> they are reporting an experience with BAML itself (a bug, a papercut, praise, a confusing error) and want it logged, not acted on now: "feedback: baml fmt is slow on big files". Restate it in `feedback_text`.
        - `general` -> greetings, questions about what the bot can do, or anything that fits no route.

        CLARIFICATION
        =============
        Set `needs_clarification` true (with one short `clarification_question`) only when the route is clear but unusable -- e.g. changelog_edit with no identifiable version anywhere in the message or thread. Ambiguity between routes resolves to the most likely route, not a clarification.

        Fill every required field; use empty strings for the fields your route does not use.

        # changelog catalog (known versions)
        {{ catalog }}

        # thread (prior messages, oldest first)
        {{ thread }}

        # mention
        {{ mention }}

        {{ ctx.output_format }}
    "#
}
