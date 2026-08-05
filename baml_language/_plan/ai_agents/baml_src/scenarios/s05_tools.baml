// Scenario 05 — Tools: plain functions; schemas from reflection;
// validation from reflect.call_any; closures capture state.
// Mirrors pages/02_guides/05_tools.md.

testset "s05 tools" {
    test "schemas come from the signature and docstring" {
        let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights), root.ai.tool(search_hotels)] };
        let rendered = tb.render();
        assert.contains(rendered, "search_flights(origin: string, dest: string)");
        assert.contains(rendered, "Search available flights");
        assert.contains(rendered, "search_hotels")
    }

    test "arguments are validated before the tool runs; errors are model-readable" {
        let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights)] };
        let good: map<string, unknown> = { "origin": "SFO", "dest": "NRT" };
        assert.contains(tb.call("search_flights", good), "ANA");
        //# hallucinated parameter: the tool never runs; the throw becomes the
        //# ToolFailed event the model reads (see s09 for the loop-level view)
        let bad: map<string, unknown> = { "origin": "SFO", "wrong_param": "NRT" };
        let bad_err = tb.call("search_flights", bad) catch_all (e) {
            _ => e.to_string()
        };
        assert.contains(bad_err, "bad arguments");

        let none: map<string, unknown> = {};
        let missing = tb.call("no_such_tool", none) catch_all (e) {
            _ => e.to_string()
        };
        assert.contains(missing, "no such tool");
        //# the error names the available tools, so the model can correct
        assert.contains(missing, "search_flights")
    }

    test "closures are tools: session-bound state without globals" {
        let user_id = "u42";
        let booked: string[] = [];
        let book = root
            .ai
            .tool(
                (hotel: string, nights: int) -> {
                    booked.push(`${hotel}:${nights}:${user_id}`);
                    `confirmed ${hotel} for ${nights} nights`
                },
                name = "book_hotel",
            );
        let tb = root.ai.Toolbox { entries: [book] };
        let args: map<string, unknown> = { "hotel": "Kyoto Ryokan", "nights": 3 };
        assert.contains(tb.call("book_hotel", args), "confirmed");
        assert.equal(booked.at(0) ?? "", "Kyoto Ryokan:3:u42")
    }

    test "anonymous tools require a name" {
        let failed = (
            root.ai.tool((x: int) -> {
                x + 1
            }) catch_all (e) {
                _ => {
                    root.ai
                        .tool(
                            (x: int) -> {
                                x
                            },
                            name = "fallback",
                        )
                },
            }
        );
        assert.equal(failed.name, "fallback")
    }
}
