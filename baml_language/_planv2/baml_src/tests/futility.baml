// The futility breaker: a tool call that fails identically three times ends
// the run with ToolFailedError instead of burning the whole step budget.

test "identical repeated tool failures stop the run early" {
    let bad_args: map<string, unknown> = {};
    let _ = bad_args.set("bogus", 1);
    // every scripted turn repeats the same doomed call with a fresh id
    let turns: ai.ModelTurn[] = [];
    let i = 0;
    while (i < 6) {
        turns.push(
            ai.ModelTurn {
                content: [ai.content.ToolUse { id: `c${i}`, name: "search_flights", args: bad_args }],
                stop_reason: ai.content.StopReason.ToolUse,
                usage: null,
            },
        );
        i = i + 1;
    }
    let scripted = ScriptedClient.new(turns);
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let outcome = ai.Agent<Itinerary>.new(client = scripted).run(spec) catch_all (e) {
        let t: ai.errors.ToolFailedError => `stopped: ${t.name}`,
        _ => "wrong error",
    };
    assert.equal(outcome, "stopped: search_flights");
    // three turns, not the full budget of twelve
    assert.equal(scripted.received().length(), 3)
}

test "a model that changes its failing call is not stopped" {
    let bad_args: map<string, unknown> = {};
    let _ = bad_args.set("bogus", 1);
    let good_args: map<string, unknown> = {};
    let _ = good_args.set("origin", "SFO");
    let _ = good_args.set("destination", "NRT");
    let _ = good_args.set("month", "2026-10");
    let scripted = ScriptedClient.new(
        [
            // fails once, adapts, finishes: no breaker
            ai.ModelTurn {
                content: [ai.content.ToolUse { id: "c1", name: "search_flights", args: bad_args }],
                stop_reason: ai.content.StopReason.ToolUse,
                usage: null,
            },
            ai.ModelTurn {
                content: [ai.content.ToolUse { id: "c2", name: "search_flights", args: good_args }],
                stop_reason: ai.content.StopReason.ToolUse,
                usage: null,
            },
            ai.ModelTurn {
                content: [ai.content.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 0.0}` }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            },
        ],
    );
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let result = ai.Agent<Itinerary>.new(client = scripted).run(spec);
    assert.equal(result.value.total_cost_usd, 0.0)
}
