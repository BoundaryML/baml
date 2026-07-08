// scenario 24 — realtime tools (+ 26 transports): ../../../../../llm-provider/ideas/scenarios/24-realtime-tools/README.md
// A realtime session that ALSO dispatches tools = a provider implementing
// both capabilities; negotiation composes. Live WebSocket = ai_realtime.rs.

class ScriptedLiveTools {
    implements baml.ai.Provider {}

    implements baml.ai.Realtime {
        function run(self, prompt: string, io: baml.ai.Channel) -> baml.ai.Transcript
            throws baml.errors.RealtimeError | baml.errors.UnknownError {
            baml.ai.Transcript { text: "live: " + prompt }
        }
    }

    implements baml.ai.Tools {
        type Transcript = ScriptState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> ScriptState throws never {
            ScriptState { turns_wanted: 0, turns_done: 0, final_json: "tool answer" }
        }
        function step<T>(self, t: ScriptState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["parse"] },
            }
        }
        function submit(self, t: ScriptState, results: baml.ai.ToolResult[]) -> ScriptState throws never {
            t
        }
    }
}

test "24: one provider negotiates BOTH Realtime and Tools" {
    // Two bindings: an outer match narrows `p`, so the inner re-match needs
    // its own existential view of the same value.
    let p1: baml.ai.Provider = ScriptedLiveTools {};
    let p2: baml.ai.Provider = ScriptedLiveTools {};
    let both = match (p1) {
        let r: baml.ai.Realtime => match (p2) {
            let tp: baml.ai.Tools => true,
            _ => false,
        },
        _ => false,
    };
    assert.is_true(both)
}

test "26: the same capability model spans transports — HTTP fake vs realtime fake" {
    let http_side: baml.ai.Provider = EchoProvider { reply: "x" };
    let duplex_side: baml.ai.Provider = ScriptedLiveTools {};
    let labels = match (http_side) {
        let h: baml.ai.HttpProvider => match (duplex_side) {
            let r: baml.ai.Realtime => "http+realtime",
            _ => "http-only",
        },
        _ => "neither",
    };
    assert.equal(labels, "http+realtime")
}
