// NL -> HogQL — translates one analytics question into a single PostHog HogQL
// query. Ports services/ingress/handlers/posthog_query.py. Single LLM call with a
// repair pass: pass PostHog's rejection message as `prior_error` to retry.

class Hogql {
    hogql: string,
}

function WriteHogql(question: string, catalog: string, prior_error: string) -> Hogql {
    client: Sonnet
    prompt: #"
        You translate one analytics question into a single HogQL query for PostHog. HogQL is PostHog's ClickHouse-SQL dialect. The main table is `events` with columns: event (string), timestamp (DateTime), distinct_id, person_id, and properties (JSON; access keys as properties.foo or properties['foo']; person properties as person.properties.foo). Useful forms: now() - INTERVAL 7 DAY, toDate(timestamp), count(), count(DISTINCT ...), countIf(cond), GROUP BY / ORDER BY / LIMIT.

        Rules:
        - Exactly one read-only SELECT. Never mutate anything.
        - Always include a LIMIT (<= 100) and, unless the question says otherwise, a timestamp bound (default: the last 30 days) so the query stays cheap.
        - Prefer event names from the provided catalog; if the question names an event that is not in the catalog, use it verbatim anyway.
        - When asked for a trend over time, group by toDate(timestamp) ordered ascending.

        # event catalog (top events, for grounding)
        {{ catalog }}

        # question
        {{ question }}

        {% if prior_error %}
        # the previous query was REJECTED by PostHog with this error -- fix it:
        {{ prior_error }}
        {% endif %}

        {{ ctx.output_format }}
    "#
}
