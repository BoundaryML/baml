# Test without a network

`ScriptedClient` returns pre-written turns in order and records every
input it receives, so an agent loop runs deterministically with no
provider. This page tests the travel agent:

```baml
function PlanTrip(trip_request: string) -> Itinerary {
    client: "openai/gpt-5.6"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${trip_request}
        ${ctx.output_format}
    `
}
```

Script one tool-calling turn and one final turn, run the spec against
the scripted client, and assert on the result and on what the runner
sent:

```baml
test plan_trip_runs_the_tool_loop {
    let scripted = ai.clients.ScriptedClient {
        turns: [
            // turn 1: "the model" calls a tool
            ModelTurn {
                content: [ToolUse { id: "call_1", name: "search_flights", args: { "origin": "SFO", "destination": "NRT", "month": "2026-10" } }],
                stop_reason: StopReason.ToolUse,
                usage: null,
            },
            // turn 2: the final answer; the runner parses this as Itinerary
            ModelTurn {
                content: [Text { text: itinerary_json }],
                stop_reason: StopReason.Complete,
                usage: null,
            },
        ],
    };

    let result: RunResult<Itinerary> = ai.Agent<Itinerary>
        .new(client = scripted)
        .run(PlanTrip@spec(trip_request = "2 weeks in Japan"));

    assert.equal(result.value.flights.length(), 1);
    // the second invocation's journal must contain the correlated tool result
    let second = scripted.received().at(1);
    assert.is_true(second?.journal.entries().some((e) -> { e is ToolCompleted }) ?? false)
}
```

The scripted client fakes the model only. Tools execute for real, so
point `tools:` at deterministic implementations in tests or construct
the spec's toolbox from fakes via a custom runner.

To test a client's own rendering and parsing, skip the loop entirely
and call its pure functions with literal inputs
(`../02_guides/03_clients/03_writing_a_client.md`).
