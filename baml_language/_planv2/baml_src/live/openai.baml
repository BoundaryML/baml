// Live smoke run against the real OpenAI API. Not a test — run with
// `baml-cli run -e 'live_openai()'` and OPENAI_API_KEY in the environment.

function live_openai() -> string {
    // No client override: this exercises the spec's lazily-resolved default
    // client from the function's `client "openai/gpt-4o-mini"` string.
    let result = ai.Agent<Itinerary>.new().run(
        PlanTrip@spec(trip_request = "2 weeks in Japan, mid-range budget"),
    );
    let completed = 0;
    for (let e in result.journal.entries()) {
        match (e) {
            let c: ai.events.ToolCompleted => {
                completed += 1;
                null
            },
            _ => null,
        };
    }
    `flights=${result.value.flights.length()} total_cost_usd=${result.value.total_cost_usd} tool_completed_events=${completed}`
}
