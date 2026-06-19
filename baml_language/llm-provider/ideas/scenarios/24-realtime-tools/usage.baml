// ============================================================================
// 24 — Tools in a realtime session  ::  usage.baml  (APPLICATION AUTHOR code)
// ============================================================================
//
// The promise: the app author declares tools ONCE (the same `Tool` shape as the
// synchronous loop in scenario 09), writes a config-only `client`, and calls the
// `.run_tools_live` companion with a Channel it owns. Everything that diverges —
// OpenAI's two-event splice (function_call_output + response.create) vs Gemini
// Live's single implicit toolResponse, the schema dialect, ordering-with-audio,
// id synthesis — stays inside the provider, under the RealtimeTools seam.
//
// What the app author DOES still own, and can't be hidden:
//   * constructing/owning the Channel (mic in, speaker out) — pass-in, per OQ#2.
//   * the tool handlers themselves (registered with the ExecutionContext).
// ============================================================================


// --- Tool argument shapes: ordinary BAML classes -> first-class `type` ---
class WeatherArgs   { city: string }
class CalendarArgs  { day: string }

// Non-model state the tools need (the user's auth, calendar handle) but the model
// never sees. Injected via ExecutionContext.deps.
class VoiceDeps {
  user_id: int
  calendar_token: string
}

// The tools the voice agent may call. Identical declaration to the chat loop —
// `parameters` is a reflected `type`; the provider turns it into JSON Schema
// (OpenAI Realtime) or the Gemini OpenAPI subset (Gemini Live).
function VoiceTools() -> Tool[] {
  [
    Tool {
      name: "get_weather",
      description: "Look up current weather for a city. Call this and keep talking; the result will arrive shortly.",
      parameters: reflect.type_of<WeatherArgs>(),
    },
    Tool {
      name: "check_calendar",
      description: "Check the signed-in user's calendar for a given day.",
      parameters: reflect.type_of<CalendarArgs>(),
    },
  ]
}


// ----------------------------------------------------------------------------
// CLIENTS — config only. Sugar for `function Name() -> Provider`.
// `voice` is a bespoke OpenAIRealtime field; it dissolves in (no options blob).
// The live Channel is NOT a client field — it's handed in at the .run_tools_live
// call (pass-in), because it doesn't exist until the phone rings.
// ----------------------------------------------------------------------------
client VoiceGpt() {
  OpenAIRealtime { model: "gpt-4o-realtime-preview", voice: "alloy", api_key: env.OPENAI_API_KEY }
}

client VoiceGemini() {
  GeminiLive { model: "gemini-2.0-flash-live-001", api_key: env.GEMINI_API_KEY }
}

// Choose the realtime provider once at session open; no mid-call failover.
client VoiceResilient() {
  RealtimeFallback { strategy: [VoiceGpt(), VoiceGemini()] }
}


// ----------------------------------------------------------------------------
// THE FUNCTION + its realtime-tools companion.
//
// `Concierge(persona)` is the plain form: it would use `.call` (a degraded,
// text-only single turn). The interesting companion is `.run_tools_live`, which
// `match`es the client's provider for the RealtimeTools capability. There is NO
// honest fallback — you cannot fake a duplex tool session with a single `.call`
// — so the `_` arm ERRORS, exactly like `.live` and `.run_tools`.
// ----------------------------------------------------------------------------
function Concierge(persona: string) -> Transcript {
  client: VoiceGpt()
  prompt #"You are a {{ persona }} voice concierge. Be brief and natural."#
}

// Generated companion (shown for clarity; normally compiler-synthesized — note it
// threads BOTH the pass-in `io` channel AND the `ctx` carrying deps + handlers):
//
//   function Concierge$run_tools_live(persona: string, tools: Tool[],
//                                     io: Channel, ctx: ExecutionContext) -> Transcript {
//     let p = VoiceGpt();
//     match (p) {
//       let rt: RealtimeTools => rt.run_tools_live(render_prompt(persona), tools, io, ctx),
//       _                     => throw baml.errors.Unsupported { message: "client is not realtime-tools" },
//     }
//   }
//
//   call site:   Concierge.run_tools_live("hotel", VoiceTools(), io, ctx)


// ----------------------------------------------------------------------------
// The host entry point an app actually writes: it OWNS the Channel (wires mic ->
// io and io -> speaker), builds the ExecutionContext with deps + tool handlers,
// and drives the live tool loop. `run_tools_live` blocks until the caller hangs
// up (closes `io`) and returns the server-authoritative Transcript.
// ----------------------------------------------------------------------------
function AnswerCall(persona: string, user_id: int) -> Transcript throws baml.errors.LlmClient {
  // Host boundary: a Channel bound to this phone call's audio (G.711 in/out).
  let io: Channel = open_phone_channel();

  let ctx = ExecutionContext {
    function_name: "Concierge",
    args: { "persona": persona },
    deps: VoiceDeps { user_id: user_id, calendar_token: env.CALENDAR_TOKEN },
  };

  // Same app code regardless of provider. Swap VoiceGpt() -> VoiceGemini() in the
  // client and the entire OpenAI-vs-Gemini event divergence (two-event splice vs
  // single implicit toolResponse, schema dialect, id handling) is absorbed under
  // the seam. The app sees one call.
  Concierge.run_tools_live(persona, VoiceTools(), io, ctx)
}


// ----------------------------------------------------------------------------
// SAME app code, the resilient client. RealtimeFallback picks GPT-realtime, or
// Gemini Live if GPT can't open a session — but only AT OPEN. The app is unaware
// which member answered.
// ----------------------------------------------------------------------------
function AnswerCallResilient(persona: string, user_id: int) -> Transcript throws baml.errors.LlmClient {
  let io: Channel = open_phone_channel();
  let ctx = ExecutionContext {
    function_name: "Concierge",
    args: { "persona": persona },
    deps: VoiceDeps { user_id: user_id, calendar_token: env.CALENDAR_TOKEN },
  };
  let p = VoiceResilient();
  match (p) {
    let rt: RealtimeTools => rt.run_tools_live(render_prompt(persona), VoiceTools(), io, ctx),
    _                     => throw baml.errors.Unsupported { message: "client is not realtime-tools" },
  }
}


// ----------------------------------------------------------------------------
// Custom dispatch POLICY without touching event threading. The app wants a hard
// latency budget — a slow tool is dead air on a phone call — so it wraps
// dispatch with a deadline. It does this by overriding the LOOP (handling tool
// events itself), NOT by reaching into configure/decode/emit. The wire splice
// (function_call_output + response.create on OpenAI; toolResponse on Gemini)
// stays inside emit_tool_output, untouched.
// ----------------------------------------------------------------------------
function BudgetedConcierge(persona: string, ctx: ExecutionContext) -> Transcript throws baml.errors.LlmClient {
  let io: Channel = open_phone_channel();
  let p = VoiceGpt();
  match (p) {
    let rt: RealtimeTools => {
      // Open the session by declaring tools, same as the default loop.
      rt.configure_tools(io, VoiceTools());

      io.on(raw => {
        match (rt.decode_tool_event(raw)) {
          let ev: ToolEvent => {
            // APP POLICY: race the tool against a deadline. On timeout, emit a
            // best-effort "still working" so there is something to say, instead of
            // silence. Threading is untouched — we still hand emit_tool_output a
            // ToolOutputEvent keyed by ev.call_id.
            let out: ToolOutputEvent = run_with_deadline(ctx, ev, 1500)
              catch (e) {
                _ => { ToolOutputEvent { call_id: ev.call_id,
                                         output_json: "{\"status\":\"still looking that up\"}" } }
              };
            rt.emit_tool_output(io, out) catch (e) { _ => { } };
          },
          _ => { },
        }
      });

      rt.run(render_prompt(persona), io)
    },
    _ => throw baml.errors.Unsupported { message: "client is not realtime-tools" },
  }
}


// --- host boundaries the app supplies ---
function open_phone_channel() -> Channel { $rust_io_function }   // mic<->speaker over G.711
function run_with_deadline(ctx: ExecutionContext, ev: ToolEvent, ms: int) -> ToolOutputEvent throws baml.errors.LlmClient { $rust_io_function }
