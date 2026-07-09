// scenario 24 — tools in a realtime session: ../../../../../llm-provider/ideas/scenarios/24-realtime-tools/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The design's promise: a voice agent calls a function (weather, calendar) and
// keeps talking naturally while it runs. The call arrives MID-SESSION, the app
// dispatches, the result is threaded BACK into the live session, and the
// conversation continues. The app declares its tools ONCE (the same `Tool` shape
// as the sync loop in scenario 09) and drives one loop; the provider absorbs the
// wire divergence below the seam.
//
// On the REAL surface (see implementation.baml for why the design's net-new
// `RealtimeTools` capability collapses to Realtime + Tools negotiation):
//   - the in-session round trip is the `Tools` loop (`run_tools` / begin-step-submit),
//   - typed-args coercion is `baml.ai.invoke_tool<A>` (raw wire JSON -> a class),
//   - the dual-capability lattice is a runtime `match` for each capability.
//
// LIVE tier: BLOCKED (design divergence — see the note above the DIAGNOSTICS
// section at the bottom). The GA OpenAI Realtime wire DOES carry function_call
// events, but `baml.ai.OpenAiRealtime` does not surface tool session config, so a
// live in-session tool round trip cannot be driven without editing the provider.
//
// Fixtures: own `s24_VoiceOnly` / `s24_NullChannel` + `S24LiveTools` (this dir's
// implementation.baml); reused stable fixtures scenario 10's `ScriptedTools` and
// common/fakes.baml's `NoCapProvider`.


// ── The tool's argument shape: an ordinary BAML class -> first-class `type`. ──
// The app author never hand-writes a schema; `Tool.from_type` lowers it and the
// dispatcher SAP-coerces the wire args back into it via `invoke_tool`.
class S24WeatherArgs {
    city: string,
}

// The tools the voice agent may call, declared ONCE via the first-class `type`.
function s24_voice_tools() -> baml.ai.Tool[] {
    [
        baml.ai.Tool.from_type(
            "get_weather",
            "Look up current weather for a city. Call this and keep talking; the result arrives shortly.",
            reflect.type_of<S24WeatherArgs>(),
        ) catch (e) {
            _ => baml.ai.Tool { name: "get_weather", description: "", parameters: "{}" },
        },
    ]
}

// The app's dispatcher: the canonical inbound-args step (D6). `invoke_tool<A>`
// SAP-coerces the model's raw JSON args into the DECLARED `S24WeatherArgs` and
// runs the handler; out-of-band deps would be injected by capture here.
function s24_dispatch(calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
    let out: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let r = baml.ai.invoke_tool<S24WeatherArgs>(c, (a: S24WeatherArgs) -> string {
            log.info("24 tool dispatched off-path: get_weather(city=" + a.city + ")");
            "sunny 22C in " + a.city
        });
        out.push(r);
    }
    out
}

// The dual-capability lattice: label a provider by what it negotiates. Each
// `match` starts from the existential `Provider`, so the two capability probes are
// independent (a provider can answer BOTH).
function s24_negotiate(p: baml.ai.Provider) -> string {
    let is_realtime = match (p) {
        let r: baml.ai.Realtime => true,
        _ => false,
    };
    let is_tools = match (p) {
        let t: baml.ai.Tools => true,
        _ => false,
    };
    if (is_realtime && is_tools) { return "both"; }
    if (is_realtime) { return "realtime"; }
    if (is_tools) { return "tools"; }
    "neither"
}


// ── The in-session tool round trip (the scenario's teaching point). ──────────
// A function_call arrives mid-session, the app dispatches (typed via invoke_tool),
// the result is threaded back into the session, and the conversation continues to
// a spoken answer DERIVED from the tool output.
test "24: in-session tool round trip — function_call mid-session, result threaded back, conversation continues" {
    let p: baml.ai.Provider = S24LiveTools { voice: "alloy" };
    let answer = match (p) {
        let tp: baml.ai.Tools => {
            let outcome = tp.run_tools<string>("what's the weather?", s24_voice_tools(), s24_dispatch) catch (e) {
                _ => "loop failed",
            };
            match (outcome) {
                let b: baml.ai.Budget<string> => "unexpected budget",
                let h: baml.ai.HandoffRequest => "unexpected handoff",
                let v: string => v,
            }
        },
        _ => "no tools capability",
    };
    log.info("24 spoken answer after round trip: " + answer);
    // "sunny 22C in Paris" was dispatched and threaded back -> the continued answer carries it.
    assert.is_true(answer.includes("Paris"))
    assert.is_true(answer.includes("weather"))
}


// ── Typed-args coercion via invoke_tool<A> (raw wire JSON -> WeatherArgs). ────
test "24: typed-args coercion via invoke_tool coerces raw JSON to WeatherArgs" {
    let call = baml.ai.ToolCall { id: "c1", name: "get_weather", args: "{\"city\": \"Berlin\"}" };
    let r = baml.ai.invoke_tool<S24WeatherArgs>(call, (a: S24WeatherArgs) -> string {
        "looked up " + a.city
    });
    assert.equal(r.id, "c1")
    assert.equal(r.output, "looked up Berlin")
}


// ── The dual-capability lattice: realtime-only, tools-only, both, neither. ───
test "24: the dual-capability lattice — realtime-only, tools-only, both, neither" {
    // both: the SAME provider negotiates Realtime AND Tools.
    assert.equal(s24_negotiate(S24LiveTools { voice: "alloy" }), "both")
    // realtime-only: a voice provider with no tool loop.
    assert.equal(s24_negotiate(s24_VoiceOnly { transcript: "voice" }), "realtime")
    // tools-only: a request/response tool loop with no live session.
    assert.equal(s24_negotiate(ScriptedTools { turns_wanted: 1, final_json: "x" }), "tools")
    // neither: a bare marker provider.
    assert.equal(s24_negotiate(NoCapProvider {}), "neither")
}


// ── One provider, both interactions: a live session AND the tool loop. ───────
// The dual-capability point end to end: drive `Realtime.run` (the live session)
// and confirm the SAME value also negotiates `Tools` (the in-session loop).
test "24: the same provider drives a live session and the tool loop" {
    let p: baml.ai.Provider = S24LiveTools { voice: "shimmer" };
    let transcript = match (p) {
        let r: baml.ai.Realtime => r.run("be brief", s24_NullChannel {}) catch (e) {
            _ => baml.ai.Transcript { text: "run failed" },
        },
        _ => baml.ai.Transcript { text: "not realtime" },
    };
    log.info("24 live transcript: " + transcript.text);
    assert.is_true(transcript.text.includes("live[shimmer]"))
    let has_tools = match (p) {
        let t: baml.ai.Tools => true,
        _ => false,
    };
    assert.is_true(has_tools)
}


// ── Negotiation failure: a realtime-only client cannot run the tool loop. ────
// The design's `.run_tools_live` on a non-tools client has no honest degrade; on
// the real surface a realtime-only provider simply fails the `Tools` match.
test "24: a realtime-only client cannot run the tool loop (negotiation miss)" {
    let p: baml.ai.Provider = s24_VoiceOnly { transcript: "voice only" };
    let r = match (p) {
        let tp: baml.ai.Tools => "ran tools",
        _ => "no tools capability",
    };
    assert.equal(r, "no tools capability")
}


// ─────────────────────────────────────────────────────────────────────────────
// DIAGNOSTICS — LIVE TIER BLOCKED (design divergence, not a test gap).
//
// The design's live target is an in-session tool round trip against a real
// realtime provider. The GA OpenAI Realtime WIRE supports this fully:
//   * tools declared in `session.update` (session.tools = [{ type:"function", … }]),
//   * the call arrives as `response.output_item.added` (item.type "function_call")
//     -> `response.function_call_arguments.delta`* -> `.done` (call_id,name,arguments),
//   * the result is spliced via `conversation.item.create`
//     (item.type "function_call_output" { call_id, output }) + a fresh `response.create`.
//
// But `baml.ai.OpenAiRealtime` (providers/openai_realtime.baml) only implements a
// TEXT-mode exchange: `run` hardcodes one `response.create` with
// `output_modalities:["text"]`, never sends a tools `session.update`, never decodes
// `function_call_arguments.*`, and keeps the `baml.ws` handle INTERNAL to `run`
// (there is no seam to emit a `function_call_output` back into the same socket).
// So the class exposes NO tool/function_call session config: a live round trip
// cannot be driven without editing the provider — which is out of scope here.
// Hence NO `testset "integ-test-24-*"` block. WIRE-supports vs CLASS-exposes gap
// recorded in the alignment report.
