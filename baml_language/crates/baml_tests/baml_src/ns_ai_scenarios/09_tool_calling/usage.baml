// scenario 09 — tool calling: ../../../../../llm-provider/ideas/scenarios/09-tool-calling/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The design's promise: the app author declares tools ONCE (a `Tool` whose
// `parameters` is a first-class `type`), writes a function, and drives the loop —
// while the provider absorbs every wire divergence below the `Tools` seam (one
// schema -> JSON Schema for OpenAI/Anthropic vs the Gemini OpenAPI-3 subset;
// id-vs-name correlation; three result-threading shapes + Anthropic's positional
// ordering). Swapping the client changes the provider and NOTHING in the app code.
//
// On the real surface the design's `Weather.run_tools(...)` companion is spelled
// two design-true ways (per the explicit "prefer LLM functions" directive):
//   - a plain LLM function called with a `ToolLoop` CLIENT (the loop is implied by
//     the provider, not a call-site mode) — `s09_Weather(city, client = ToolLoop{…})`,
//   - the interface's default `run_tools` loop / `Foo$run_tools` companion.
// Hand-rolled `begin`/`step`/`submit` appears ONCE — the approval gate — because
// there the loop internals ARE the lesson (the design's ApprovedSupport).
//
// Live tier = testset "integ-test-09-tool-calling" (native OpenAi tools).
//
// Reuses (read-only, shared namespace): scenario 10's ScriptedTools /
// echo_dispatch / lookup_tools; common/fakes.baml's EchoProvider / NoCapProvider.
// Preserves the D5 (default loop + stop_when Budget) and D6 (invoke_tool coercion
// + self-correct, validate_args integrity, parse_type narrowing) behaviors.


// ── The tool's argument shape: an ordinary BAML class -> first-class `type`. ──
// The app author never hand-writes a schema: `Tool.from_type` lowers the class
// `type` to the wire schema (and keeps it for `validate_args`).
class S09WeatherArgs {
    city: string,
}

// D6 fixture: two required fields of DISTINCT types, so SAP's single-string-field
// passthrough leniency can't swallow arbitrary JSON — schema mismatches genuinely
// fail here (a one-string-field class accepts almost anything).
class S09ForecastArgs {
    city: string,
    days: int,
}

// Dependency-injection shapes (the design's Pydantic-AI RunContext[Deps]).
// `s09_AccountBalanceArgs` is what the MODEL supplies (schema-typed); the deps
// (`s09_BankingDeps`) are what the model NEVER sees — injected out-of-band.
class s09_AccountBalanceArgs {
    include_pending: bool,
}
class s09_BankingDeps {
    db_url: string,
    customer_id: int,
}

// The tools the model may call, declared ONCE via the first-class `type`.
// `Tool.from_type` throws `Unsupported` if a type can't be lowered; we fold that
// into a placeholder so these are ordinary (throws-free) tool factories.
function s09_weather_tools() -> baml.ai.Tool[] {
    [
        baml.ai.Tool.from_type("get_weather", "Get the current weather for a city.", reflect.type_of<S09WeatherArgs>()) catch (e) {
            _ => baml.ai.Tool { name: "get_weather", description: "", parameters: "{}" },
        },
    ]
}
function s09_banking_tools() -> baml.ai.Tool[] {
    [
        baml.ai.Tool.from_type(
            "account_balance",
            "Look up the signed-in customer's balance. `include_pending` toggles pending transactions.",
            reflect.type_of<s09_AccountBalanceArgs>(),
        ) catch (e) {
            _ => baml.ai.Tool { name: "account_balance", description: "", parameters: "{}" },
        },
    ]
}


// ── Tool.from_type: one BAML type -> the wire schema (D6). ───────────────────
test "09: Tool.from_type lowers a BAML type to the wire schema" {
    let t = baml.ai.Tool.from_type("get_weather", "Weather lookups.", reflect.type_of<S09WeatherArgs>()) catch (e) {
        _ => baml.ai.Tool { name: "failed", description: "", parameters: "" },
    };
    assert.equal(t.name, "get_weather")
    assert.is_true(t.parameters.includes("city"))
}


// ── The plain LLM function + a ToolLoop client (the design-true spelling). ───
// `s09_Weather(city)` is an ordinary one-shot function; handed a `ToolLoop`
// client it runs the whole begin/step/submit loop and returns the final value.
// The body is identical no matter which provider the ToolLoop wraps — that IS the
// portability claim. Offline we default to a dead client so every test passes an
// explicit deterministic provider override.
client<llm> s09_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s09_Weather(city: string) -> string {
    client s09_Dead
    prompt #"What's the weather in {{ city }}? Use your tools."#
}

test "09: the plain LLM function runs the tool loop via a ToolLoop client" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 2, final_json: "sunny in Tokyo" },
        tools: s09_weather_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = s09_Weather("Tokyo", client = agent) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "sunny in Tokyo")
}

// The portability claim, offline: SAME body, a DIFFERENT provider, one-line swap.
// (The live tier proves it against real native OpenAi tools.)
test "09: same app code, a different provider is a one-line client swap" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "clear in Oslo" },
        tools: s09_weather_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = s09_Weather("Oslo", client = agent) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "clear in Oslo")
}

test "09: a client that cannot run tools is a typed Unsupported" {
    let r = s09_Weather("x", client = NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}


// ── The interface's default run_tools loop (D5). ─────────────────────────────
test "09: the interface's default run_tools loop drives begin/step/submit" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 2, final_json: "loop done" };
    let r = match (p) {
        let tp: baml.ai.Tools => {
            // D5: run_tools returns the honest sum T | Budget<T> | HandoffRequest;
            // no budget/handoff here, so only the value arm fires.
            let outcome = tp.run_tools<string>("go", lookup_tools(), echo_dispatch) catch (e) {
                _ => "loop failed"
            };
            match (outcome) {
                let b: baml.ai.Budget<string> => "unexpected budget",
                let h: baml.ai.HandoffRequest => "unexpected handoff",
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
                let h: baml.ai.HandoffRequest => "unexpected handoff",
                let v: string => v,
            }
        },
        _ => "no tools capability",
    };
    assert.equal(r, "budget@2")
}


// ── Argument validation: the canonical inbound-args step (D6). ───────────────
// `invoke_tool<A>` SAP-coerces the wire args into the handler's DECLARED type. A
// mismatch does NOT abort the loop — it returns an error `ToolResult` so the
// model self-corrects on the next turn (the design left this policy unlegislated;
// the real surface makes "feed the error back" the default).
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


// ── Dependency injection: deps ride the dispatcher, never the model's args. ──
// The design threaded `deps` on `ExecutionContext`; the real dispatch is a plain
// `(ToolCall[]) -> ToolResult[]` closure, so out-of-band state (DB handle,
// customer id) is injected by CAPTURE — the model only ever supplies the
// schema-typed `args`. We record what the tool saw to prove the deps reached it.
test "09: deps ride the dispatcher closure, the model never sees them" {
    let deps = s09_BankingDeps { db_url: "postgres://bank", customer_id: 4242 };
    let seen_customer: int[] = [];
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "balance reported" },
        tools: s09_banking_tools(),
        dispatch: (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
            let out: baml.ai.ToolResult[] = [];
            for (let c in calls) {
                // deps injected here, out-of-band; the model never saw customer_id.
                seen_customer.push(deps.customer_id);
                out.push(baml.ai.ToolResult { id: c.id, output: "queried " + deps.db_url });
            }
            out
        },
        stop_when: null,
    };
    let r = agent.call<string>("what is my balance?") catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "balance reported")
    assert.equal(seen_customer.length(), 1)
    assert.equal(seen_customer[0], 4242)
}


// ── Custom dispatch POLICY without touching threading: the approval gate. ────
// The design's ApprovedSupport overrides only the loop body to approve each call
// before dispatch (begin/step/submit and all wire correlation are untouched). On
// the real surface this is a hand-rolled loop over the Tools seam — the ONE place
// the loop internals are the lesson. A denial throws the typed s09_ToolCallDenied
// (implementation.baml), caught concretely by the app.
function s09_run_with_approval(p: baml.ai.Provider, allow: string[], question: string) -> string
    throws baml.errors.ToolError | baml.errors.UnknownError {
    match (p) {
        let tp: baml.ai.Tools => {
            let t = tp.begin<string>(question, lookup_tools());
            while (true) {
                match (tp.step<string>(t)) {
                    let calls: baml.ai.ToolCalls => {
                        for (let c in calls.calls) {
                            if (!allow.includes(c.name)) {
                                throw s09_ToolCallDenied { tool_name: c.name };
                            }
                        }
                        t = tp.submit(t, echo_dispatch(calls.calls));
                    },
                    let value: string => { return value; },
                }
            }
            baml.sys.panic("unreachable")
        },
        _ => throw baml.errors.Unsupported { message: "client cannot run tools" },
    }
}

test "09: an approval gate runs the loop when the tool call is allowed" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 2, final_json: "approved answer" };
    let r = s09_run_with_approval(p, ["lookup"], "answer me") catch (e) {
        _ => "gate failed",
    };
    assert.equal(r, "approved answer")
}

test "09: a denied tool call throws a typed s09_ToolCallDenied, caught concretely" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 2, final_json: "never reached" };
    let r = s09_run_with_approval(p, [], "answer me") catch (e) {
        let d: s09_ToolCallDenied  => "denied:" + d.tool_name,
        let te: baml.errors.ToolError => "generic tool error",
        _ => "wrong error",
    };
    assert.equal(r, "denied:lookup")
}


// ── Resilient tools: pick a Tools-capable provider UP FRONT (the ToolFallback). ─
// The design's ToolFallback forwards the Tools capability by delegation, but the
// evaluation is explicit that for a STATEFUL capability "fallback" can only mean
// "pick a provider up front" (a bound transcript can't fail over mid-loop). So
// this selects the first Tools-capable member and drives it; none capable is a
// typed Unsupported (the design's NoToolProvider case).
function s09_resilient_tools(strategy: baml.ai.Provider[], question: string) -> string
    throws baml.errors.ToolError | baml.errors.UnknownError {
    for (let p in strategy) {
        match (p) {
            let tp: baml.ai.Tools => {
                let outcome = tp.run_tools<string>(question, lookup_tools(), echo_dispatch) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["resilient tools run failed"] },
                };
                return match (outcome) {
                    let b: baml.ai.Budget<string> => "budget",
                    let h: baml.ai.HandoffRequest => "handoff",
                    let v: string => v,
                };
            },
            _ => { continue; },
        }
    }
    throw baml.errors.Unsupported { message: "no tools-capable provider in strategy" };
}

test "09: resilient tools picks the first Tools-capable member up front" {
    let strategy: baml.ai.Provider[] = [
        NoCapProvider {},
        ScriptedTools { turns_wanted: 1, final_json: "picked" },
    ];
    let r = s09_resilient_tools(strategy, "go") catch (e) {
        _ => "failed",
    };
    assert.equal(r, "picked")
}

test "09: resilient tools with no Tools-capable member is a typed Unsupported" {
    let strategy: baml.ai.Provider[] = [NoCapProvider {}, EchoProvider { reply: "not tools" }];
    let r = s09_resilient_tools(strategy, "go") catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys). ──
// The design-true spelling against a REAL provider: the plain LLM function
// `s09_Weather`, driven by a ToolLoop wrapping NATIVE OpenAi tools. Tools are
// declared once via `Tool.from_type` (typed) and the dispatcher SAP-coerces the
// wire args back into `S09WeatherArgs` (D6) — the schema dialect, call ids, and
// tool-result threading all stay inside the OpenAi provider, below the seam.
testset "integ-test-09-tool-calling" {
    test "09 live: the plain LLM function drives native OpenAi tools via a ToolLoop client" {
        let seen_cities: string[] = [];
        let weather_tool = baml.ai.Tool.from_type(
            "get_weather",
            "Get the current weather for a city",
            reflect.type_of<S09WeatherArgs>(),
        ) catch (e) {
            _ => baml.ai.Tool { name: "get_weather", description: "", parameters: "{}" },
        };
        let agent = baml.ai.ToolLoop {
            inner: baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
            tools: [weather_tool],
            dispatch: (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
                let out: baml.ai.ToolResult[] = [];
                for (let c in calls) {
                    // D6: coerce the wire args into the DECLARED type via SAP.
                    let args: S09WeatherArgs = baml.sap.parse<S09WeatherArgs>(c.args) catch (e) {
                        _ => S09WeatherArgs { city: "unknown" },
                    };
                    seen_cities.push(args.city);
                    out.push(baml.ai.ToolResult { id: c.id, output: "cloudy, 18C in " + args.city });
                }
                out
            },
            stop_when: null,
        };
        let answer = s09_Weather("Tokyo", client = agent) catch (e) {
            _ => "live tool loop failed",
        };
        assert.is_true(seen_cities.length() > 0)
        assert.is_true(answer.includes("18") || answer.to_lower_case().includes("cloud"))
    }
}
