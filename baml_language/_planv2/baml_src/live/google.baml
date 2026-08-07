// Live smoke run against the real Gemini API. Not a test — run with
// `baml-cli run -e 'live_google()'` and GOOGLE_API_KEY in the environment.

function live_google() -> string {
    let result = root.ai.Agent<Itinerary>.new().run(
        plan_trip_spec("2 weeks in Japan, mid-range budget", root.google.GoogleClient.new()),
    );
    let completed = 0;
    for (let e in result.journal.entries()) {
        match (e) {
            let c: root.ai.ToolCompleted => {
                completed += 1;
                null
            },
            _ => null,
        };
    }
    `flights=${result.value.flights.length()} total_cost_usd=${result.value.total_cost_usd} tool_completed_events=${completed}`
}
