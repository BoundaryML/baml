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
            // D5: run_tools returns the honest sum T | Budget<T>; no budget was
            // set here, so the Budget arm is structurally unreachable.
            let outcome = tp.run_tools<string>("go", lookup_tools(), echo_dispatch) catch (e) {
                _ => "loop failed"
            };
            match (outcome) {
                let b: baml.ai.Budget<string> => "unexpected budget",
                let v: string => v,
            }
        },
        _ => "no tools capability",
    };
    assert.equal(r, "loop done")
}

test "09: run_tools with stop_when returns the Budget arm, not a throw (D5)" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 5, final_json: "never reached" };
    let r = match (p) {
        let tp: baml.ai.Tools => {
            let outcome = tp
                .run_tools<string>(
                    "go",
                    lookup_tools(),
                    echo_dispatch,
                    stop_when = baml.ai.step_count_is(2),
                ) catch (e) {
                _ => "loop failed"
            };
            match (outcome) {
                let b: baml.ai.Budget<string> => "budget@" + b.steps_taken.to_string(),
                let v: string => v,
            }
        },
        _ => "no tools capability",
    };
    assert.equal(r, "budget@2")
}

/// D6 fixture: two required fields of distinct types — SAP's single-string-field
/// passthrough leniency can't swallow arbitrary JSON into this shape, so schema
/// mismatches genuinely fail (a one-string-field class accepts almost anything).
class S09ForecastArgs {
    city: string,
    days: int,
}

test "09: invoke_tool coerces args to the handler's declared type (D6 standard path)" {
    let call = baml.ai.ToolCall { id: "c1", name: "forecast", args: "{\"city\": \"Paris\", \"days\": 3}" };
    let r = baml.ai.invoke_tool<S09ForecastArgs>(call, (a: S09ForecastArgs) -> string {
        a.days.to_string() + "-day forecast for " + a.city
    });
    assert.equal(r.id, "c1")
    assert.equal(r.output, "3-day forecast for Paris")
}

test "09: invoke_tool returns an error result on schema mismatch (model self-corrects)" {
    let call = baml.ai.ToolCall { id: "c2", name: "forecast", args: "{\"temperature\": 12}" };
    let r = baml.ai.invoke_tool<S09ForecastArgs>(call, (a: S09ForecastArgs) -> string {
        "unreachable"
    });
    assert.equal(r.id, "c2")
    assert.is_true(r.output.includes("error"))
}

test "09: validate_args checks calls against the STORED params type (D6 integrity)" {
    let t = baml.ai.Tool.from_type("forecast", "Weather forecasts.", reflect.type_of<S09ForecastArgs>()) catch (e) {
        _ => baml.ai.Tool { name: "failed", description: "", parameters: "", params_type: null },
    };
    let good = baml.ai.ToolCall { id: "c3", name: "forecast", args: "{\"city\": \"Oslo\", \"days\": 2}" };
    let bad = baml.ai.ToolCall { id: "c4", name: "forecast", args: "{\"temperature\": 12}" };
    assert.is_true(t.validate_args(good))
    assert.is_true(!t.validate_args(bad))
    // A hand-constructed tool (app-provided wire schema) has nothing to validate against.
    let handmade = baml.ai.Tool { name: "x", description: "", parameters: "{}" };
    assert.is_true(handmade.validate_args(bad))
}

test "09: parse_type parses against a runtime type value and narrows via match" {
    let v: unknown = baml.sap.parse_type(reflect.type_of<S09WeatherArgs>(), "{\"city\": \"Kyoto\"}") catch (e) {
        _ => null,
    };
    let city = match (v) {
        let w: S09WeatherArgs => w.city,
        _ => "no-parse",
    };
    assert.equal(city, "Kyoto")
}
