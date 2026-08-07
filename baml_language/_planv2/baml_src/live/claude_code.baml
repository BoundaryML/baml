// Live smoke: the Claude Code CLI as a client, with BAML tools riding the
// outcome envelope. Uses the CLI's own authentication; no API key.

function live_claude_code() -> string {
    let spec = plan_trip_spec(
        "2 weeks in Japan, mid-range budget, flying from SFO in October. Use the tools to look up flights and hotels before answering. Do not ask questions.",
        root.claude_code.ClaudeCodeClient.new(),
    );
    let result = root.ai.Agent<Itinerary>.new().run(spec);
    let tool_events = result
        .journal
        .entries()
        .filter(
            (e: root.ai.Event) -> {
                match (e) {
                    let c: root.ai.ToolCompleted => true,
                    _ => false,
                }
            },
        )
        .length();
    `flights=${result.value.flights.length()} hotels=${result.value.hotels.length()} total_cost_usd=${result.value.total_cost_usd} tool_completed_events=${tool_events}`
}
