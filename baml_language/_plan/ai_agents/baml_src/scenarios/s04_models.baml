// Scenario 04 — Models: clients are stateless codecs; the journal stamps
// every assistant message with its producer; switching clients is a
// rendering decision, not a migration. Mirrors pages/02_guides/04_models.md.
//
// The live client (ns_ai/openai.baml) is exercised by shared/plan_trip.baml:
//   infisical run --env=test -- baml run -e 'demo()'

testset "s04 models" {
    test "every assistant message records which provider produced it" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [[root.ai.assistant("first pass", provider = "openai/gpt-5.2")]],
        };
        let s = plan_trip_session("Japan", fake);
        let _ = s.run();
        let providers: string[] = [];
        for (let en in s.journal.entries) {
            if let a: root.ai.AssistantMessage = en.event {
                providers.push(a.provider);
            }
        }
        assert.equal(providers.at(0) ?? "", "openai/gpt-5.2")
    }

    test "switching clients mid-session: same journal, mixed provider stamps" {
        let model_a = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [[root.ai.assistant("draft from model A", provider = "openai/gpt-5.2")]],
        };
        let s = plan_trip_session("Japan", model_a);
        let _ = s.run();
        //# nothing happens to the data — the next render walks the same journal
        let model_b = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":9,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        s.set_client(model_b);
        s.send("finish it");
        match (s.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.destination, "Japan"),
            _ => assert.is_true(false),
        };
        //# the pre-switch history is intact, still stamped with model A
        let transcript = root.ai.render_transcript(s.journal);
        assert.contains(transcript, "draft from model A")
    }

    test "render is stateless: the request is rebuilt from the journal each call" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [] };
        let s = plan_trip_session("Japan", fake);
        let live = root.ai.OpenAiClient {
            model: "gpt-4o-mini",
            api_key: "test-key",
            parser: PlanTripParser {  },
            calls: 0,
        };
        let req1 = live.render(s.journal, s.toolbox, itinerary_schema());
        s.journal.append(root.ai.user("make it 10 days"));
        let req2 = live.render(s.journal, s.toolbox, itinerary_schema());
        //# the new message appears in the rebuilt request; no hidden client state
        assert.is_true(!req1.body_json.includes("make it 10 days"));
        assert.contains(req2.body_json, "make it 10 days")
    }
}
