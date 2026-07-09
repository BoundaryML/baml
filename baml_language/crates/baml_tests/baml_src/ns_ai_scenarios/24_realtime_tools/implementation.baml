// scenario 24 — tools in a realtime session: ../../../../../llm-provider/ideas/scenarios/24-realtime-tools/README.md
//   implementation.baml — LIBRARY/std-lib-author code.
//
// The design's implement.baml introduced a whole net-new capability —
// `RealtimeTools requires Realtime`, its own `baml.errors.RealtimeToolError`
// channel, `ToolEvent`/`ToolOutputEvent` payloads, and an `ExecutionContext`
// with `dispatch_async`/`emit` — to model tool calls that arrive as EVENTS on the
// duplex channel (no synchronous return slot). That surface does NOT ship in
// `baml.ai`. On the REAL surface the scenario's TEACHING POINT — a model emits a
// `function_call` mid-session, the app dispatches, the result is threaded back
// into the live session, and the conversation continues — is expressible as a
// single provider that negotiates BOTH capabilities that DO ship:
//   - `baml.ai.Realtime` (the live duplex session; `run(prompt, io) -> Transcript`),
//   - `baml.ai.Tools`    (the in-session round trip; `begin`/`step`/`submit` +
//     the default `run_tools`, where `step -> ToolCalls` IS "the model emitted a
//     function_call mid-session" and `submit(results)` IS "the tool result
//     threaded back in; the conversation continues").
//
// DESIGN DIVERGENCE (net-new, noted not built): the design deliberately makes
// realtime tools a DISTINCT capability from the sync `Tools` seam, because in a
// real duplex socket the call is a push event (no return slot) and the two
// providers' continuation shapes diverge (OpenAI's `function_call_output` +
// `response.create` two-event splice vs Gemini Live's single implicit
// `toolResponse`). `baml.ai` has no `RealtimeTools`/event-emit surface, so that
// event/ordering-with-audio lesson lives BELOW the seam and is not modeled here;
// the sync `Tools` loop stands in for the round-trip semantics offline.
//
// The scripted fake is the whole offline driver: the design's two live wire
// providers (OpenAI Realtime, Gemini Live) collapse to one deterministic fixture.

/// A Channel that swallows everything — the offline stand-in for a live socket.
class s24_NullChannel {
    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            null
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}

/// A REALTIME-ONLY provider — a live session with no tool loop (the lattice's
/// "realtime, not tools" corner).
class s24_VoiceOnly {
    transcript: string,

    implements baml.ai.Provider {}

    implements baml.ai.Realtime {
        function run(self, prompt: string, io: baml.ai.Channel) -> baml.ai.Transcript
            throws baml.errors.RealtimeError | baml.errors.UnknownError {
            baml.ai.Transcript { text: self.transcript + " <- " + prompt }
        }
    }
}

/// The provider-owned `Tools` transcript for the scripted in-session round trip.
/// `tool_done` gates the one function_call the model emits; `tool_output` is the
/// dispatched result THREADED BACK into the session (so the continued answer is
/// derived from it — a genuine round trip, not a canned reply).
class S24Session {
    tool_done: bool,
    tool_output: string,
}

/// A dual-capability fake: ONE provider that negotiates BOTH `Realtime` (a live
/// duplex session) and `Tools` (the in-session tool round trip). This is the
/// scenario's dual-capability lattice made concrete — the same value answers
/// `match (p) { let r: Realtime => … }` AND `match (p) { let t: Tools => … }`.
class S24LiveTools {
    voice: string,

    implements baml.ai.Provider {}

    // The live duplex session. Offline this just echoes what a session would
    // transcribe; the point is that the SAME provider also runs the tool loop.
    implements baml.ai.Realtime {
        function run(self, prompt: string, io: baml.ai.Channel) -> baml.ai.Transcript
            throws baml.errors.RealtimeError | baml.errors.UnknownError {
            log.info("24 realtime session opened (voice=" + self.voice + ")");
            baml.ai.Transcript { text: "live[" + self.voice + "]: " + prompt }
        }
    }

    // The in-session tool round trip, on the real `Tools` seam:
    //   step  -> ToolCalls   ==  "the model emitted a function_call mid-session"
    //   submit(results)      ==  "the tool result is threaded back into the session"
    //   step  -> T           ==  "the conversation continues to the spoken answer"
    implements baml.ai.Tools {
        type Transcript = S24Session

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> S24Session throws never {
            log.info("24 tool session begun on live provider");
            S24Session { tool_done: false, tool_output: "" }
        }

        function step<T>(self, t: S24Session) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (!t.tool_done) {
                // Mid-session: the model asks to call `get_weather` and keeps the
                // floor — the app dispatches off-path and threads the result back.
                log.info("24 in-session function_call emitted: get_weather");
                return baml.ai.ToolCalls {
                    calls: [baml.ai.ToolCall { id: "call_rt_1", name: "get_weather", args: "{\"city\": \"Paris\"}" }],
                };
            }
            // Continuation: the result is in the session; produce the spoken answer.
            let spoken = "the weather is " + t.tool_output;
            log.info("24 conversation resumed with tool result: " + spoken);
            baml.sap.parse<T>(spoken) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["24 final parse failed"] },
            }
        }

        function submit(self, t: S24Session, results: baml.ai.ToolResult[]) -> S24Session throws never {
            let out = match (results.at(0)) {
                null => "unknown",
                let r: baml.ai.ToolResult => r.output,
            };
            log.info("24 tool result returned into live session: " + out);
            S24Session { tool_done: true, tool_output: out }
        }
    }
}
