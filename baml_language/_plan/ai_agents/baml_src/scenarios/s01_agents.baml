// Scenario 01 — Agents: task mode. Mirrors pages/02_guides/01_agents.md.
// A task runs to the typed return value; the loop is invisible to the caller.

testset "s01 agents" {
    test "task mode: tool round-trip, then a typed result" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [root.ai.tool_requested("t1", "search_flights", `{"origin":"SFO","dest":"NRT"}`)],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":14,"flights":[],"hotel":null,"daily_plan":["arrive"]}`,
                        ),
                ],
            ],
        };
        let trip = plan_trip("2 weeks in Japan", fake);
        assert.equal(trip.days, 14);
        assert.equal(trip.destination, "Japan")
    }

    test "task mode: a conversational reply is not an exit — the loop keeps driving" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [root.ai.assistant("thinking out loud about the route...")],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":7,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        let trip = plan_trip("1 week in Japan", fake);
        assert.equal(trip.days, 7)
    }

    test "every call is recorded: the task's journal is its trace" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [root.ai.tool_requested("t1", "search_flights", `{"origin":"SFO","dest":"NRT"}`)],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":14,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        let s = plan_trip_session("2 weeks in Japan", fake, task_mode = true);
        let _ = s.run();
        let kinds = s.journal.entries.map((en) -> {
            match (en.event) {
                let e: root.ai.SessionStarted => "started",
                let e: root.ai.ToolRequested => "tool_requested",
                let e: root.ai.ToolCompleted => "tool_completed",
                let e: root.ai.FinalProduced => "final",
                _ => "other",
            }
        });
        assert.equal(kinds.at(0) ?? "", "started");
        assert.is_true(kinds.includes("tool_requested"));
        assert.is_true(kinds.includes("tool_completed"));
        assert.is_true(kinds.includes("final"))
    }
}
