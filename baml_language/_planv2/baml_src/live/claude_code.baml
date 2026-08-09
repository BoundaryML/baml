// Live smoke: the Claude Code CLI as a client, with BAML tools riding the
// outcome envelope. Uses the CLI's own authentication; no API key.

function live_claude_code() -> string {
    let spec = PlanTrip@spec(
        trip_request = "2 weeks in Japan, mid-range budget, flying from SFO in October. Use the tools to look up flights and hotels before answering. Do not ask questions.",
    );
    let result = ai.Agent<Itinerary>
        .new(client = claude_code.ClaudeCodeClient.new(), on_event = log_event)
        .run(spec);
    let tool_events = result.journal.entries()
        .filter((e: ai.Event) -> {
            match (e) {
                let c: ai.ToolCompleted => true,
                _ => false,
            }
        })
        .length();
    `flights=${result.value.flights.length()} hotels=${result.value.hotels.length()} total_cost_usd=${result.value.total_cost_usd} tool_completed_events=${tool_events}`
}

// The same live smoke with the observer supplied by the caller — e.g. a Go
// func passed through the generated SDK. The Event union does not cross the
// SDK boundary yet (its classes carry `unknown`-typed fields), so each event
// is delivered as its kind plus a structural json payload, which hosts
// receive as plain values (`any` in Go).
function live_claude_code_observed(on_event: (string, json) -> null throws never) -> string {
    let spec = PlanTrip@spec(
        trip_request = "2 weeks in Japan, mid-range budget, flying from SFO in October. Use the tools to look up flights and hotels before answering. Do not ask questions.",
    );
    let result = ai.Agent<Itinerary>
        .new(client = claude_code.ClaudeCodeClient.new(), on_event = (e: ai.Event) -> {
            let kind = match (e) {
                let x: ai.RunStarted => "RunStarted",
                let x: ai.UserMessage => "UserMessage",
                let x: ai.AssistantMessage => "AssistantMessage",
                let x: ai.ToolRequested => "ToolRequested",
                let x: ai.ToolCompleted => "ToolCompleted",
                let x: ai.ToolFailed => "ToolFailed",
                let x: ai.Usage => "Usage",
                let x: ai.FinalProduced => "FinalProduced",
            };
            let payload = baml.json.to_json(e) catch_all (err) {
                _ => null,
            };
            on_event(kind, payload)
        })
        .run(spec);
    let tool_events = result.journal.entries()
        .filter((e: ai.Event) -> {
            match (e) {
                let c: ai.ToolCompleted => true,
                _ => false,
            }
        })
        .length();
    `flights=${result.value.flights.length()} hotels=${result.value.hotels.length()} total_cost_usd=${result.value.total_cost_usd} tool_completed_events=${tool_events}`
}

testset "live" {
    test "claude code tool loop" {
        let summary = live_claude_code();
        log.info(summary);
        assert.contains(summary, "flights=");
        assert.is_true(!summary.includes("tool_completed_events=0"))
    }
}
