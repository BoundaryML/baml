// Live smoke: the Anthropic client drives the full tool loop against the
// real Messages API. Run with:
//   infisical run --env=test -- baml-cli run -e 'live_anthropic()'

function live_anthropic() -> string {
    let result = ai.Agent<Itinerary>.new(client = anthropic.AnthropicClient.new()).run(
        PlanTrip@spec(
            // The bare brief "2 weeks in Japan, mid-range budget" makes the
            // model ask clarifying questions, and a one-shot run has nobody
            // to answer them; pin the details so it acts autonomously.
            trip_request = "2 weeks in Japan, mid-range budget. Flying from SFO to Tokyo (NRT) in October 2026; hotels in Tokyo up to $200/night. This is a one-shot request: do not ask clarifying questions, make reasonable assumptions, use the tools, and answer with the final itinerary.",
        ),
    );
    let completed = 0;
    for (let e in result.journal.entries()) {
        match (e) {
            let c: ai.ToolCompleted => {
                completed = completed + 1;
                null
            },
            _ => null,
        };
    }
    `flights=${result.value.flights.length()} hotels=${result.value.hotels.length()} total_cost_usd=${result.value.total_cost_usd} tool_completed_events=${completed}`
}
