// scenario 09 — tool calling: ../../../../../llm-provider/ideas/scenarios/09-tool-calling/README.md
// The Tools interface's default run_tools loop + typed tool params via
// Tool.from_type. Live tier = tools_loop_live / typed_tool_agent_live.

class S09WeatherArgs {
    city: string,
}

test "09: Tool.from_type lowers a BAML type to the wire schema" {
    let t = baml.ai.Tool.from_type("get_weather", "Weather lookups.", reflect.type_of<S09WeatherArgs>()) catch (e) {
        _ => baml.ai.Tool { name: "failed", description: "", parameters: "" },
    };
    assert.equal(t.name, "get_weather")
    assert.is_true(t.parameters.includes("city"))
}

test "09: the interface's default run_tools loop drives begin/step/submit" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 2, final_json: "loop done" };
    let r = match (p) {
        let tp: baml.ai.Tools => {
            tp.run_tools<string>("go", lookup_tools(), echo_dispatch) catch (e) {
                _ => "loop failed"
            }
        },
        _ => "no tools capability",
    };
    assert.equal(r, "loop done")
}
