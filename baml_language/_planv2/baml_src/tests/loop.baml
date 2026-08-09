// Offline tests: the scripted client drives the full loop with no network.
// Mirrors _planv2/pages/03_how_to/02_test_without_a_network.md.

function scripted_two_turn() -> ScriptedClient {
    let call_args: map<string, unknown> = {};
    let _ = call_args.set("origin", "SFO");
    let _ = call_args.set("destination", "NRT");
    let _ = call_args.set("month", "2026-10");
    ScriptedClient.new(
        [
            // turn 1: "the model" calls a tool
            ai.ModelTurn {
                content: [ai.ToolUse { id: "call_1", name: "search_flights", args: call_args }],
                stop_reason: ai.StopReason.ToolUse,
                usage: ai.Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cached_input_tokens: null,
                    reasoning_tokens: null,
                },
            },
            // turn 2: the final answer; the runner parses this as Itinerary
            ai.ModelTurn {
                content: [
                    ai.Text {
                        text: `{"flights": [{"airline": "ANA", "price_usd": 950.0}], "hotels": [], "total_cost_usd": 950.0}`,
                    },
                ],
                stop_reason: ai.StopReason.Complete,
                usage: ai.Usage {
                    input_tokens: 20,
                    output_tokens: 15,
                    cached_input_tokens: null,
                    reasoning_tokens: null,
                },
            },
        ],
    )
}

test "scripted client drives the tool loop to a typed result" {
    let scripted = scripted_two_turn();
    let spec = PlanTrip@spec(trip_request = "2 weeks in Japan");
    let result = ai.Agent<Itinerary>.new(client = scripted).run(spec);
    assert.equal(result.value.total_cost_usd, 950.0);
    assert.equal(result.value.flights.length(), 1);
    assert.equal(result.usage.input_tokens, 30);
    // the second invocation's journal must contain the correlated tool result
    let has_result = scripted.received().at(1)?.journal?.entries()?.some((e: ai.Event) -> {
        match (e) {
            let c: ai.ToolCompleted => c.id == "call_1",
            _ => false,
        }
    })
        ?? false;
    assert.is_true(has_result)
}

test "direct call runs the same loop and unwraps the value" {
    // Calling the LLM function itself desugars to the default ai.Agent over
    // `PlanTrip@spec`; the compiler-injected `client` parameter overrides the
    // function's declared default client.
    let scripted = scripted_two_turn();
    let value = PlanTrip(trip_request = "2 weeks in Japan", client = scripted);
    assert.equal(value.total_cost_usd, 950.0);
    assert.equal(value.flights.length(), 1);
    // the loop really ran: two invocations, tool result correlated
    assert.equal(scripted.received().length(), 2)
}

test "tool errors report to the model by default" {
    // turn 1 calls a tool with bad arguments; turn 2 finishes anyway
    let bad_args: map<string, unknown> = {};
    let _ = bad_args.set("bogus", 1);
    let scripted = ScriptedClient.new(
        [
            ai.ModelTurn {
                content: [ai.ToolUse { id: "c1", name: "search_flights", args: bad_args }],
                stop_reason: ai.StopReason.ToolUse,
                usage: null,
            },
            ai.ModelTurn {
                content: [ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 0.0}` }],
                stop_reason: ai.StopReason.Complete,
                usage: null,
            },
        ],
    );
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let result = ai.Agent<Itinerary>.new(client = scripted).run(spec);
    // the run completed, and the journal shows the failure the model saw
    let failed = result.journal.entries().some((e: ai.Event) -> {
        match (e) {
            let f: ai.ToolFailed => f.id == "c1",
            _ => false,
        }
    });
    assert.is_true(failed)
}

test "step budget throws typed" {
    let call_args: map<string, unknown> = {};
    let _ = call_args.set("origin", "SFO");
    let _ = call_args.set("destination", "NRT");
    let _ = call_args.set("month", "2026-10");
    // every turn calls a tool; the loop can never finish
    let scripted = ScriptedClient.new(
        [
            ai.ModelTurn {
                content: [ai.ToolUse { id: "c1", name: "search_flights", args: call_args }],
                stop_reason: ai.StopReason.ToolUse,
                usage: null,
            },
            ai.ModelTurn {
                content: [ai.ToolUse { id: "c2", name: "search_flights", args: call_args }],
                stop_reason: ai.StopReason.ToolUse,
                usage: null,
            },
        ],
    );
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let outcome = ai.Agent<Itinerary>.new(max_steps = 2, client = scripted).run(spec) catch_all (e) {
        let b: ai.errors.StepBudgetExceeded => null,
        _ => throw e,
    };
    assert.is_true(outcome == null)
}
