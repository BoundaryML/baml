// Scenario 02 — Sessions: turns, snapshots, resume.
// Mirrors pages/02_guides/02_sessions.md.

testset "s02 sessions" {
    test "a reply yields the turn; the conversation continues to Done" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [root.ai.assistant("Do you prefer trains or flights?")],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":10,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        let s = plan_trip_session("2 weeks in Japan", fake);

        match (s.run()) {
            let r: root.ai.Replied => assert.contains(r.message, "trains"),
            _ => assert.is_true(false),
        }

        s.send("trains please");
        match (s.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.destination, "Japan"),
            _ => assert.is_true(false),
        }
    }

    test "arguments are session constants, recorded in SessionStarted" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [] };
        let s = plan_trip_session("10 days in Kyoto", fake);
        match (s.journal.entries.at(0)?.event) {
            let st: root.ai.SessionStarted => {
                assert.equal(st.function_name, "PlanTrip");
                assert.contains(st.args_json, "10 days in Kyoto")
            },
            _ => assert.is_true(false),
        }
    }

    test "snapshot mid-conversation, resume elsewhere, finish" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [[root.ai.assistant("How many days?")]] };
        let s = plan_trip_session("Japan trip", fake);
        let _ = s.run();
        let snap = s.snapshot(); // one string — the whole session

        // "another machine": fresh client, fresh policy, same journal
        let fake2 = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":10,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights)] };
        let s2 = root
            .ai
            .resume_session<Itinerary, never>(
                snap,
                fake2,
                tb,
                root.ai.ToolLoop<never> { max_steps: 12, task_mode: false },
                output_schema = itinerary_schema(),
            );
        s2.send("10 days");
        match (s2.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.destination, "Japan"),
            _ => assert.is_true(false),
        }
    }

    test "the resumed journal still contains the pre-snapshot history" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [[root.ai.assistant("noted")]] };
        let s = plan_trip_session("Japan", fake);
        s.send("budget is $3000");
        let _ = s.run();
        let j2 = root.ai.resume_journal<root.ai.Event>(s.snapshot());
        assert.contains(root.ai.render_transcript(j2), "budget is $3000")
    }
}
