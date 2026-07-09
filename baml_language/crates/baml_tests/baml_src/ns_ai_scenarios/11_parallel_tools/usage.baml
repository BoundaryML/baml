// scenario 11 — parallel tools: ../../../../../llm-provider/ideas/scenarios/11-parallel-tools/usage.baml
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The headline: choosing "parallel" is choosing a dispatch CLOSURE and handing it
// to the run — exactly like choosing retry/fallback is choosing a combinator. The
// app never touches a provider, transcript, or wire code. Per the "prefer LLM
// functions" directive, the primary spelling is a PLAIN LLM function driven by a
// `baml.ai.ToolLoop` client (the loop is implied by the provider), and the
// `Foo$run_tools` companion; the dispatch closure is where parallelism lives.
//
// Reuses (read-only, shared namespace): common/fakes.baml's NoCapProvider;
// scenario 10's ScriptedTools (one-call-per-turn) is NOT enough for the parallel
// case, so this scenario's own `s11_MultiCallTools` (implementation.baml) scripts
// MULTI-call turns.
//
// Live tier = testset "integ-test-11-parallel-tools" (native OpenAi tools, the
// model actually emitting several tool calls in one turn).


// ── 1. The tool argument shapes + the tools, declared ONCE via `Tool.from_type`.
// D6 typed entry: the wire schema is lowered from the BAML `type`, and the
// dispatcher SAP-coerces `ToolCall.args` back into it via `baml.ai.invoke_tool<A>`.
class s11_SearchArgs {
    query: string,
}
class s11_WeatherArgs {
    city: string,
}
// Two required fields of DISTINCT types so a schema mismatch genuinely fails
// (single-string-field classes SAP-accept almost anything).
class s11_ChargeArgs {
    account: string,
    cents: int,
}

function s11_search_tool() -> baml.ai.Tool {
    baml.ai.Tool.from_type("search_web", "Search the web for a query.", reflect.type_of<s11_SearchArgs>()) catch (e) {
        _ => baml.ai.Tool { name: "search_web", description: "", parameters: "{}" },
    }
}
function s11_weather_tool() -> baml.ai.Tool {
    baml.ai.Tool.from_type("get_weather", "Current weather for a city.", reflect.type_of<s11_WeatherArgs>()) catch (e) {
        _ => baml.ai.Tool { name: "get_weather", description: "", parameters: "{}" },
    }
}
function s11_charge_tool() -> baml.ai.Tool {
    baml.ai.Tool.from_type("charge_card", "Charge a customer's card.", reflect.type_of<s11_ChargeArgs>()) catch (e) {
        _ => baml.ai.Tool { name: "charge_card", description: "", parameters: "{}" },
    }
}

// The effect declaration the design put on `Tool.effect`, re-homed as a read-only
// NAME set (the stdlib Tool can't be extended here). Omitting a tool ⇒ Write ⇒
// sequenced (the safe default): `charge_card` is deliberately absent.
function s11_read_only_tools() -> string[] {
    ["search_web", "get_weather"]
}


// ── 2. The per-call unit of work (`invoke_one`): the leaf the strategies schedule.
// SAP-coerces args into the DECLARED type via `baml.ai.invoke_tool<A>` (D6), and
// injects out-of-band deps by CAPTURE (an `audit` sink here — the model never sees
// it). A `flaky` tool THROWS to exercise partial-failure capture.
function s11_research_invoker(audit: string[]) -> ((baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError) {
    (c: baml.ai.ToolCall) -> baml.ai.ToolResult {
        audit.push(c.name);           // deps/telemetry injected out-of-band, by capture
        if (c.name == "search_web") {
            return baml.ai.invoke_tool<s11_SearchArgs>(c, (a: s11_SearchArgs) -> string {
                "results for " + a.query
            });
        }
        if (c.name == "get_weather") {
            return baml.ai.invoke_tool<s11_WeatherArgs>(c, (a: s11_WeatherArgs) -> string {
                "sunny in " + a.city
            });
        }
        if (c.name == "charge_card") {
            return baml.ai.invoke_tool<s11_ChargeArgs>(c, (a: s11_ChargeArgs) -> string {
                "charged " + a.cents.to_string() + " to " + a.account
            });
        }
        if (c.name == "flaky") {
            throw baml.errors.UnknownError { data: null, message: ["backend down"] };
        }
        baml.ai.ToolResult { id: c.id, output: "{\"error\": \"unknown tool\"}" }
    }
}


// ── 3. The plain LLM function + a ToolLoop client (the design-true spelling). ──
// `s11_Research(q)` is an ordinary one-shot function; handed a `ToolLoop` client
// it runs the whole begin/step/submit loop and returns the final value. The body
// is identical no matter which provider (or dispatch strategy) the loop wraps —
// that IS the portability claim. Offline we default to a dead client so every
// test passes an explicit deterministic provider override.
client<llm> s11_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s11_Research(question: string) -> string {
    client s11_Dead
    prompt #"
        Answer using the tools. You may call multiple tools at once.
        Question: {{ question }}
    "#
}


// ── HEADLINE: run-all parallel fan-out, order preserved, partial failure -> error.
test "11: parallel dispatch fans out a multi-call turn, preserving call order" {
    let audit: string[] = [];
    let agent = baml.ai.ToolLoop {
        inner: s11_MultiCallTools {
            batch: [
                baml.ai.ToolCall { id: "t1", name: "search_web", args: "{\"query\": \"baml\"}" },
                baml.ai.ToolCall { id: "t2", name: "get_weather", args: "{\"city\": \"Tokyo\"}" },
                baml.ai.ToolCall { id: "t3", name: "search_web", args: "{\"query\": \"rust\"}" },
            ],
            final_json: "answered from 3 fanned-out tools",
        },
        tools: [s11_search_tool(), s11_weather_tool()],
        dispatch: s11_parallel_dispatch(s11_research_invoker(audit)),
        stop_when: null,
    };
    let r = s11_Research("what's up?", client = agent) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "answered from 3 fanned-out tools")
    // all three ran concurrently; the model-visible order is emission order.
    assert.equal(audit.length(), 3)
}

test "11: run-all captures a failing sibling as an error ToolResult, siblings survive" {
    let audit: string[] = [];
    let dispatch = s11_parallel_dispatch(s11_research_invoker(audit));
    let calls = [
        baml.ai.ToolCall { id: "a", name: "search_web", args: "{\"query\": \"x\"}" },
        baml.ai.ToolCall { id: "b", name: "flaky", args: "{}" },
        baml.ai.ToolCall { id: "c", name: "get_weather", args: "{\"city\": \"Oslo\"}" },
    ];
    let out = dispatch(calls);
    assert.equal(out.length(), 3)
    // ORDER preserved: results line up with the calls, not with completion order.
    assert.equal(out[0].id, "a")
    assert.equal(out[0].output, "results for x")
    assert.is_true(out[1].output.includes("error"))   // the failed sibling, as a result
    assert.equal(out[2].id, "c")
    assert.equal(out[2].output, "sunny in Oslo")
}

// Completion order != input order still yields input order (the determinism the
// design demands for Anthropic positional matching + prompt-cache stability).
function s11_staggered_invoker() -> ((baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError) {
    (c: baml.ai.ToolCall) -> baml.ai.ToolResult {
        // earlier calls sleep LONGER, so they FINISH last.
        let delay = 1;
        if (c.id == "first") { delay = 40; }
        if (c.id == "second") { delay = 20; }
        baml.sys.sleep(baml.time.Duration.from_milliseconds(delay)) catch (e) { _ => null };
        baml.ai.ToolResult { id: c.id, output: "done:" + c.id }
    }
}

test "11: result order follows call order despite reversed completion order" {
    let dispatch = s11_parallel_dispatch(s11_staggered_invoker());
    let calls = [
        baml.ai.ToolCall { id: "first", name: "search_web", args: "{}" },
        baml.ai.ToolCall { id: "second", name: "search_web", args: "{}" },
        baml.ai.ToolCall { id: "third", name: "search_web", args: "{}" },
    ];
    let out = dispatch(calls);
    assert.equal(out[0].id, "first")
    assert.equal(out[1].id, "second")
    assert.equal(out[2].id, "third")
}


// ── Effect-aware: fan out reads, SEQUENCE writes; reassemble in emission order. ─
test "11: effect-aware dispatch parallelizes reads and sequences the write" {
    let audit: string[] = [];
    let dispatch = s11_effect_aware_dispatch(s11_research_invoker(audit), s11_read_only_tools());
    let calls = [
        baml.ai.ToolCall { id: "0", name: "search_web", args: "{\"query\": \"q\"}" },
        baml.ai.ToolCall { id: "1", name: "charge_card", args: "{\"account\": \"acct\", \"cents\": 500}" },
        baml.ai.ToolCall { id: "2", name: "get_weather", args: "{\"city\": \"Paris\"}" },
    ];
    let out = dispatch(calls);
    assert.equal(out.length(), 3)
    assert.equal(out[0].output, "results for q")             // read (fanned out)
    assert.equal(out[1].output, "charged 500 to acct")       // write (sequenced), original slot
    assert.equal(out[2].output, "sunny in Paris")            // read (fanned out)
}


// ── Concurrency-limited (TaskGroup cap): run-all under a rate limit. ──────────
test "11: TaskGroup-limited dispatch caps concurrency and preserves order" {
    let audit: string[] = [];
    let dispatch = s11_limited_dispatch(s11_research_invoker(audit), 2);
    let calls = [
        baml.ai.ToolCall { id: "1", name: "search_web", args: "{\"query\": \"a\"}" },
        baml.ai.ToolCall { id: "2", name: "search_web", args: "{\"query\": \"b\"}" },
        baml.ai.ToolCall { id: "3", name: "search_web", args: "{\"query\": \"c\"}" },
        baml.ai.ToolCall { id: "4", name: "search_web", args: "{\"query\": \"d\"}" },
    ];
    let out = dispatch(calls);
    assert.equal(out.length(), 4)
    assert.equal(out[3].id, "4")
    assert.equal(audit.length(), 4)   // every call ran, just at most 2 at a time
}


// ── Stop-on-first-error: first failure cancels siblings and THROWS (asymmetric).
test "11: stop-on-first-error re-throws the failing call's concrete error" {
    let audit: string[] = [];
    let calls = [
        baml.ai.ToolCall { id: "a", name: "search_web", args: "{\"query\": \"x\"}" },
        baml.ai.ToolCall { id: "b", name: "flaky", args: "{}" },
    ];
    let label = s11_stop_on_first_error(s11_research_invoker(audit), calls) catch (e) {
        _ => "aborted",
    };
    assert.equal(label, "aborted")
}

test "11: stop-on-first-error returns all results when nothing fails" {
    let audit: string[] = [];
    let calls = [
        baml.ai.ToolCall { id: "a", name: "search_web", args: "{\"query\": \"x\"}" },
        baml.ai.ToolCall { id: "b", name: "get_weather", args: "{\"city\": \"Rome\"}" },
    ];
    let out: baml.ai.ToolResult[] = s11_stop_on_first_error(s11_research_invoker(audit), calls) catch (e) {
        _ => [],
    };
    assert.equal(out.length(), 2)
    assert.equal(out[0].id, "a")
    assert.equal(out[1].output, "sunny in Rome")
}


// ── The `Foo$run_tools` companion spelling: dispatch is a direct argument. ────
test "11: the run_tools companion drives a multi-call turn with parallel dispatch" {
    let audit: string[] = [];
    let agent = s11_MultiCallTools {
        batch: [
            baml.ai.ToolCall { id: "t1", name: "search_web", args: "{\"query\": \"one\"}" },
            baml.ai.ToolCall { id: "t2", name: "get_weather", args: "{\"city\": \"Lima\"}" },
        ],
        final_json: "companion fanned out",
    };
    let outcome = s11_Research$run_tools(
        "go",
        [s11_search_tool(), s11_weather_tool()],
        s11_parallel_dispatch(s11_research_invoker(audit)),
        client = agent,
    ) catch (e) {
        _ => "companion failed",
    };
    let r = match (outcome) {
        let b: baml.ai.Budget<string> => "unexpected budget",
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let v: string => v,
    };
    assert.equal(r, "companion fanned out")
    assert.equal(audit.length(), 2)
}

// Portability, offline: the SAME dispatch closure over a capability-less client is
// a typed Unsupported (the loop can't negotiate Tools).
test "11: a client that cannot run tools is a typed Unsupported" {
    let audit: string[] = [];
    let agent = baml.ai.ToolLoop {
        inner: NoCapProvider {},
        tools: [s11_search_tool()],
        dispatch: s11_parallel_dispatch(s11_research_invoker(audit)),
        stop_when: null,
    };
    let r = s11_Research("x", client = agent) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys). ──
// The design-true spelling against a REAL provider: the plain LLM function
// `s11_Research`, driven by a ToolLoop wrapping NATIVE OpenAi tools, with a
// PARALLEL dispatch closure. The prompt nudges the model to emit several tool
// calls in one turn; the dispatcher fans them out with spawn + baml.future.all
// and SAP-coerces each call's args (D6) — all wire correlation stays below the seam.
testset "integ-test-11-parallel-tools" {
    test "11 live: native OpenAi emits a multi-call turn, dispatched in parallel" {
        let audit: string[] = [];
        let agent = baml.ai.ToolLoop {
            inner: baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
            tools: [s11_search_tool(), s11_weather_tool()],
            dispatch: s11_parallel_dispatch(s11_research_invoker(audit)),
            stop_when: null,
        };
        let answer = s11_Research(
            "Compare the current weather in Tokyo and Paris, searching if needed. Call several tools at once.",
            client = agent,
        ) catch (e) {
            _ => "live parallel loop failed",
        };
        assert.is_true(audit.length() > 0)
        assert.is_true(answer.length() > 0)
    }
}
