//# an agent wrapped as a tool (closure captures the child's client — in the

//# real feature the child session and ChildSpawned/ChildFinished events are

//# wired by the runtime)

function research_tool(child_client: root.ai.Client) -> root.ai.Tool {
    root.ai
        .tool(
            (request: string) -> {
                baml.json.stringify(baml.json.to_json(plan_trip(request, child_client)))
            },
            name = "research_trip",
        )
}

testset "s08 subagents" {
    test "an agent as a tool: the parent's model delegates, the child returns typed data" {
        let child = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Kyoto","days":3,"flights":[],"hotel":null,"daily_plan":["temples"]}`,
                        ),
                ],
            ],
        };
        let tb = root.ai.Toolbox { entries: [research_tool(child)] };
        let args: map<string, unknown> = { "request": "3 days in Kyoto" };
        let out = tb.call("research_trip", args);
        assert.contains(out, "Kyoto");
        assert.contains(out, "temples")
    }

    test "fan-out: independent subagents run concurrently, results in order" {
        let cities = ["Kyoto", "Osaka", "Sapporo"];
        let g = baml.spawn.TaskGroup.new(2); // bounded concurrency
        let futures = cities.map((city) -> {
            spawn with baml.spawn.options(group = g) {
                let child = root.ai.ScriptedClient<never> {
                    i: 0,
                    turns: [
                        [
                            root.ai
                                .final_produced(
                                    `{"destination":"${city}","days":2,"flights":[],"hotel":null,"daily_plan":[]}`,
                                ),
                        ],
                    ],
                };
                plan_trip(`2 days in ${city}`, child)
            }
        });
        let trips = await baml.future.all(futures);
        assert.equal(trips.length(), 3);
        assert.equal(trips.at(0)?.destination ?? "", "Kyoto");
        assert.equal(trips.at(2)?.destination ?? "", "Sapporo")
    }

    test "each subagent has its own journal; the parent only sees the result" {
        let child = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [root.ai.tool_requested("t1", "search_flights", `{"origin":"SFO","dest":"KIX"}`)],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Osaka","days":4,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        let child_session = plan_trip_session("4 days in Osaka", child, task_mode = true);
        let result = child_session.run();
        //# the child's tool traffic lives in the child's journal...
        assert
            .is_true(
                child_session.journal.entries.some((en) -> {
                    en.event is root.ai.ToolRequested
                }),
            );
        //# ...and only the final result crosses the boundary
        match (result) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.destination, "Osaka"),
            _ => assert.is_true(false),
        }
    }
}
