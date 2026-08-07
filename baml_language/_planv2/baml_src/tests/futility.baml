// The futility breaker: a tool call that fails identically three times ends
// the run with ToolFailedError instead of burning the whole step budget.

test "identical repeated tool failures stop the run early" {
    let bad_args: map<string, unknown> = {};
    let _ = bad_args.set("bogus", 1);
    // every scripted turn repeats the same doomed call with a fresh id
    let turns: root.ai.ModelTurn[] = [];
    let i = 0;
    while (i < 6) {
        turns.push(
            root.ai.ModelTurn {
                content: [root.ai.ToolUse { id: `c${i}`, name: "search_flights", args: bad_args }],
                stop_reason: root.ai.StopReason.ToolUse,
                usage: null,
            },
        );
        i = i + 1;
    }
    let scripted = ScriptedClient.new(turns);
    let spec = plan_trip_spec("anywhere", scripted);
    let outcome = root.ai.Agent<Itinerary>.new().run(spec) catch_all (e) {
        let t: root.ai.errors.ToolFailedError => `stopped: ${t.name}`,
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
            root.ai.ModelTurn {
                content: [root.ai.ToolUse { id: "c1", name: "search_flights", args: bad_args }],
                stop_reason: root.ai.StopReason.ToolUse,
                usage: null,
            },
            root.ai.ModelTurn {
                content: [root.ai.ToolUse { id: "c2", name: "search_flights", args: good_args }],
                stop_reason: root.ai.StopReason.ToolUse,
                usage: null,
            },
            root.ai.ModelTurn {
                content: [root.ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 0.0}` }],
                stop_reason: root.ai.StopReason.Complete,
                usage: null,
            },
        ],
    );
    let spec = plan_trip_spec("anywhere", scripted);
    let result = root.ai.Agent<Itinerary>.new().run(spec);
    assert.equal(result.value.total_cost_usd, 0.0)
}
