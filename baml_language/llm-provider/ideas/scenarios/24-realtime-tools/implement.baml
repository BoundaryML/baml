// ============================================================================
// 24 — Tools in a realtime session  ::  implement.baml  (LIBRARY AUTHOR code)
// ============================================================================
//
// Canonical primitives reused VERBATIM from _conventions.md:
//   interface Provider     — call<T>
//   interface Realtime     — run(prompt, io: Channel) -> Transcript   (pass-in channel)
//   interface Tools        — begin / step / submit / default run_tools (SYNC loop)
//   interface Channel      — on(handler) / send(data) / close()
//   class Tool             — { name, description, parameters: type }
//   class ToolCall         — { id, name, args }   (id OPAQUE to the caller)
//   class ToolResult       — { id, output }
//
// THE TEACHING POINT of this file:
//
//   The synchronous `Tools` seam (begin -> step -> submit) is a REQUEST/RESPONSE
//   shape: `step` blocks until the model's turn ends, returns `T | ToolCalls`,
//   and `submit` opens the NEXT request with the results in its body. That shape
//   is structurally impossible in a realtime session:
//
//     * The tool call arrives as an EVENT on the Channel mid-stream.
//     * The response is NOT paused — the model may still be emitting audio.
//     * There is NO synchronous return slot. You don't "return" a value into a
//       waiting request; you EMIT a `function_call_output` event carrying the
//       call_id, then SEPARATELY emit `response.create` to ask the model to
//       continue. Correlation is by call_id only.
//
//   So realtime tool-calling is NOT the `Tools` capability. We model it as a
//   distinct capability, `RealtimeTools`, that `requires Realtime` (NOT Tools).
//   One concrete provider implements BOTH Realtime and RealtimeTools — they
//   co-exist because RealtimeTools is just "the event wiring for tools, layered
//   on the same duplex channel Realtime already drives."
//
//   Net-new surface (flagged in evaluation.md):
//     * interface RealtimeTools requires Realtime          (the event-driven loop)
//     * class ToolEvent  { call_id, name, args_json }      (the inbound event)
//     * ExecutionContext.emit / .dispatch_async            (event-emit surface)
//     * InEvent / OutEvent payload shapes for tool plumbing
// ============================================================================


// ----------------------------------------------------------------------------
// The realtime channel's event vocabulary.
//
// In _conventions.md `Channel.on` takes `(InEvent) -> void` and `Channel.send`
// takes `OutEvent`. Those are the host's opaque transport frames. We narrow the
// two we care about for tools.  An InEvent is anything the server pushes
// (audio deltas, transcripts, response.done, AND function-call events). An
// OutEvent is anything we push (user audio, conversation.item.create,
// response.create).
//
// NET-NEW: the tool-specific event payloads. These are the realtime analogue of
// `Tools`' ToolCall/ToolResult, but they travel as EVENTS, not as a return value
// and a request-body slot.
// ----------------------------------------------------------------------------

// An inbound function-call event, reconstructed from the server's event sequence
// (response.output_item.added -> function_call_arguments.delta* -> .done).
// `call_id` is the server's correlation handle — OPAQUE to us, echoed back.
class ToolEvent {
  call_id: string                   // server-assigned; we never interpret it
  name: string
  args_json: string                 // the accumulated `arguments` string from .done
}

// What we splice back into the server-side conversation: a function_call_output
// item carrying the SAME call_id. There is no "slot" — this is an outbound event.
class ToolOutputEvent {
  call_id: string                   // MUST equal the inbound ToolEvent.call_id
  output_json: string
}


// ----------------------------------------------------------------------------
// NET-NEW host surface: ExecutionContext gains an EVENT-emit + async-dispatch
// surface, distinct from the synchronous `dispatch` used by `Tools.run_tools`.
//
// `Tools.run_tools` calls `ctx.dispatch(calls) -> ToolResult[]` — synchronous,
// returns a value the loop threads into `submit`. Realtime has no place to
// return TO. Instead `dispatch_async` runs the tool off the audio path and,
// when it resolves, the provider EMITS the output event itself. We model the
// "run tool, hand result back as an event" pair as one call that returns the
// ToolOutputEvent to emit — the provider owns the emit so it can also sequence
// the follow-up `response.create`.
// ----------------------------------------------------------------------------
class ExecutionContext {
  function_name: string
  args: map<string, unknown>
  deps: unknown                     // injected dependencies the model never sees

  // SYNC path (reused by the chat Tools loop, unchanged from scenario 09/10).
  function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.LlmClient {
    let out: ToolResult[] = [];
    for (let c in calls) {
      let r: ToolResult = self.invoke_one(c) catch (e) {
        _ => { ToolResult { id: c.id, output: { "error": "tool execution failed" } } }
      };
      out = out.append(r);
    }
    out
  }

  // ASYNC path (realtime). Validate `ev.args_json` against the registered tool's
  // `parameters: type`, inject `self.deps`, run the handler — possibly on a slow
  // network. Returns the ToolOutputEvent the provider will splice back via the
  // Channel. Crucially this DOES NOT block the audio path: the provider awaits it
  // off the event loop. Latency here is dead air, so a real impl wraps a deadline.
  function dispatch_async(self, ev: ToolEvent) -> ToolOutputEvent throws baml.errors.LlmClient {
    let output: string = self.invoke_one_async(ev) catch (e) {
      // Surface the failure to the MODEL (it can apologize over audio) rather than
      // killing the session — a thrown error here is dead air with no explanation.
      _ => { baml.sap.to_json({ "error": "tool failed; tell the user you couldn't complete it" }) }
    };
    ToolOutputEvent { call_id: ev.call_id, output_json: output }
  }

  // Host boundaries: resolve handler by name, validate args, inject deps, invoke.
  function invoke_one(self, c: ToolCall) -> ToolResult throws baml.errors.LlmClient { $rust_io_function }
  function invoke_one_async(self, ev: ToolEvent) -> string throws baml.errors.LlmClient { $rust_io_function }
}


// ============================================================================
// NET-NEW CAPABILITY — RealtimeTools requires Realtime
// ============================================================================
//
// This is NOT `Tools`. `Tools` is begin/step/submit (request/response). This is
// the event-driven loop: subscribe to function-call events on the channel, run
// the tool off-path, emit a function_call_output event, then ask the model to
// continue. The `run` method drives the whole duplex session AND the tool loop
// because in realtime they are the SAME loop — tool events are interleaved with
// audio events on one channel.
//
// `requires Realtime` (transitively `requires Provider`) means: a RealtimeTools
// provider already drives a duplex `run(prompt, io)`. RealtimeTools adds the
// tool-event handling ON TOP of that same channel. It does NOT add a second
// channel — there is exactly one duplex connection per session.
// ----------------------------------------------------------------------------
interface RealtimeTools requires Realtime {

  // Declare the tools for the session. Realtime declares tools up front in
  // `session.update` (or per response.create), not per-call — so this is config
  // sent once when the session opens, not a parameter threaded through a loop.
  function configure_tools(self, io: Channel, tools: Tool[]) -> null throws Error

  // Reconstruct a complete ToolEvent from the raw event the channel delivered, or
  // null if `raw` is not a function_call.done event. Each provider owns the
  // event-name vocabulary (`response.function_call_arguments.done`) and the
  // delta-accumulation BELOW this seam.
  function decode_tool_event(self, raw: InEvent) -> ToolEvent? throws Error

  // Splice a tool result back into the SERVER-SIDE conversation as an event, then
  // ask the model to continue. This is the realtime replacement for `Tools.submit`
  // — but note it takes NO transcript: the conversation lives on the SERVER, not in
  // a client-held transcript. The client is stateless about history here.
  function emit_tool_output(self, io: Channel, output: ToolOutputEvent) -> null throws Error

  // The default realtime tool loop. Compare with `Tools.run_tools`:
  //   * `Tools.run_tools` is a WHILE loop over `step` (pull, blocking).
  //   * this is an ON handler (push, non-blocking) — the channel drives US.
  // The session ends when the host closes the channel (hangup); `run` returns the
  // server-authoritative Transcript at that point.
  function run_tools_live(self, prompt: baml.llm.PromptAst, tools: Tool[],
                          io: Channel, ctx: ExecutionContext) -> Transcript throws Error {
    // 1. Open the session by declaring the tools up front.
    self.configure_tools(io, tools);

    // 2. Subscribe to EVERY inbound event. Audio events flow straight through to
    //    the host's speakers (the host wired io to its audio sink); we only ACT on
    //    function-call events. The model keeps talking while a tool runs.
    io.on(raw => {
      match (self.decode_tool_event(raw)) {
        let ev: ToolEvent => {
          // Run the tool OFF the audio path. The model may still be speaking the
          // turn that requested this call — we do not pause it.
          let out: ToolOutputEvent = ctx.dispatch_async(ev) catch (e) {
            _ => { ToolOutputEvent { call_id: ev.call_id, output_json: "{\"error\":\"dispatch failed\"}" } }
          };
          // Splice the result back as an event keyed by call_id, then (inside
          // emit_tool_output) ask the model to continue. ORDERING-WITH-AUDIO
          // discipline lives in the provider impl of emit_tool_output.
          self.emit_tool_output(io, out) catch (e) { _ => { } };
        },
        _ => { },   // not a tool event (audio delta, transcript, response.done, ...)
      }
    });

    // 3. Drive the session via Realtime.run on the SAME channel. `run` blocks until
    //    the host closes the channel; the `on` handler above fires throughout.
    self.run(prompt, io)
  }
}


// ============================================================================
// PROVIDER A — OpenAI Realtime (GA realtime API over WebSocket)
// ============================================================================
//
// Wire facts this provider owns BELOW the seam:
//   * tools declared in:   session.update  (event type "session.update")
//   * call arrives as:     response.output_item.added (item.type=="function_call")
//                          -> response.function_call_arguments.delta* (accumulate)
//                          -> response.function_call_arguments.done   (call_id,name,arguments)
//   * pause semantics:     NONE — generation continues; we choose when to reply.
//   * result threading:    conversation.item.create with item.type
//                          "function_call_output" { call_id, output }
//   * continuation:        a SEPARATE response.create event.
//   * ordering w/ audio:   wait for response.done before response.create, else the
//                          follow-up interleaves with in-flight audio.
// ----------------------------------------------------------------------------
class OpenAIRealtime {
  model: string                     // e.g. "gpt-4o-realtime-preview"
  voice: string                     // bespoke field — dissolves in, no options blob
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  // Realtime requires Provider, so we also owe a best-effort single-turn `call`.
  // For a realtime socket model, `call` opens a throwaway session, sends the
  // prompt as text, drains to the first complete text response, closes. Honest
  // but lossy (no audio) — this is the Open Question #1 "best-effort single turn".
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function    // open socket, text-only turn, SAP-parse, close
    }
  }

  implements Realtime {
    // The duplex driver. Wires inbound audio/transcript events to whatever sink
    // the HOST attached to `io`, and accepts outbound user audio. Returns the
    // server-authoritative Transcript when the host closes `io`.
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      // session.update with instructions + voice, then stream. The tool handler (if
      // any) was layered on by RealtimeTools.configure_tools BEFORE we got here.
      OpenAIRealtime.open_session(self, io, prompt);   // host boundary: socket + audio bridge
      OpenAIRealtime.pump_until_closed(self, io)       // blocks; returns Transcript
    }
  }

  implements RealtimeTools {
    function configure_tools(self, io: Channel, tools: Tool[]) -> null throws baml.errors.LlmClient {
      // DIALECT: each Tool's `parameters: type` -> JSON Schema, wrapped in the
      // realtime session tool entry { type:"function", name, description, parameters }.
      // Sent ONCE as a session.update event — not per turn.
      let entries: string[] = [];
      for (let tool in tools) {
        let schema: string = baml.reflect.type_to_json_schema(tool.parameters);
        entries = entries.append(
          baml.llm.openai_realtime_tool_entry(tool.name, tool.description, schema)
        );
      }
      io.send(OpenAIRealtime.session_update_tools_event(entries))   // OutEvent
        catch (e) { _ => { throw baml.errors.LlmClient { message: "session.update failed" }; } };
      null
    }

    function decode_tool_event(self, raw: InEvent) -> ToolEvent? throws baml.errors.LlmClient {
      // We only fire on the TERMINAL event. The host's transport layer accumulates
      // response.function_call_arguments.delta into the final `arguments` string and
      // tags the frame; here we just test the event type and pull the fields.
      if (OpenAIRealtime.event_type(raw) == "response.function_call_arguments.done") {
        return ToolEvent {
          call_id: OpenAIRealtime.event_field(raw, "call_id"),
          name: OpenAIRealtime.event_field(raw, "name"),
          args_json: OpenAIRealtime.event_field(raw, "arguments"),
        };
      }
      null   // audio delta / transcript / response.done / etc.
    }

    function emit_tool_output(self, io: Channel, output: ToolOutputEvent) -> null throws baml.errors.LlmClient {
      // THREADING: two SEPARATE events, in order.
      // (1) conversation.item.create with a function_call_output item carrying the
      //     SAME call_id — this is the "no slot, correlate by id" reality.
      io.send(OpenAIRealtime.function_call_output_event(output.call_id, output.output_json))
        catch (e) { _ => { throw baml.errors.LlmClient { message: "item.create failed" }; } };

      // (2) ORDERING-WITH-AUDIO: block for response.done of the CURRENT turn before
      //     asking for the next one, so the follow-up audio doesn't interleave with
      //     the audio the model is still speaking. This wait is the provider's
      //     concern, not the app's. It is also where latency becomes dead air.
      OpenAIRealtime.await_response_done(io);

      // (3) response.create — ask the model to continue WITH the spliced result.
      io.send(OpenAIRealtime.response_create_event())
        catch (e) { _ => { throw baml.errors.LlmClient { message: "response.create failed" }; } };
      null
    }
  }

  // --- statics: host bridges + event encoders, all BELOW the seam ---
  function open_session(self: OpenAIRealtime, io: Channel, prompt: baml.llm.PromptAst) -> null { $rust_io_function }
  function pump_until_closed(self: OpenAIRealtime, io: Channel) -> Transcript throws baml.errors.LlmClient { $rust_io_function }
  function await_response_done(io: Channel) -> null { $rust_io_function }   // blocks off the audio path
  function event_type(raw: InEvent) -> string { $rust_io_function }
  function event_field(raw: InEvent, key: string) -> string { $rust_io_function }
  function session_update_tools_event(entries: string[]) -> OutEvent { $rust_io_function }
  function function_call_output_event(call_id: string, output_json: string) -> OutEvent { $rust_io_function }
  function response_create_event() -> OutEvent { $rust_io_function }
  function openai_realtime_tool_entry(name: string, description: string, schema: string) -> string { $rust_io_function }
}


// ============================================================================
// PROVIDER B — Gemini Live (BidiGenerateContent over WebSocket)
// ============================================================================
//
// The divergent one. Wire facts BELOW the seam:
//   * tools declared in:   BidiGenerateContentSetup.tools (functionDeclarations,
//                          OpenAPI-3 SUBSET — the lossy transform, not JSON Schema)
//   * call arrives as:     a BidiGenerateContentServerContent message whose
//                          modelTurn contains functionCall parts. NO separate
//                          delta/done lifecycle — the call arrives whole.
//   * call id:             Gemini Live DOES carry an `id` per functionCall (unlike
//                          batch generateContent, which has none) — so we echo it.
//                          But older/2.0 stacks omit it; we synthesize when absent.
//   * pause semantics:     the model can keep generating; we reply when ready.
//   * result threading:    BidiGenerateContentToolResponse { functionResponses:
//                          [{ id, name, response }] } — keyed by id AND name.
//   * continuation:        IMPLICIT. Sending the toolResponse is itself the
//                          continuation — there is NO separate response.create.
//                          (Contrast OpenAI's explicit two-event splice.)
// ----------------------------------------------------------------------------
class GeminiLive {
  model: string                     // e.g. "gemini-2.0-flash-live-001"
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function    // single-turn over a throwaway live socket
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      GeminiLive.open_setup(self, io, prompt);     // BidiGenerateContentSetup
      GeminiLive.pump_until_closed(self, io)
    }
  }

  implements RealtimeTools {
    function configure_tools(self, io: Channel, tools: Tool[]) -> null throws baml.errors.LlmClient {
      // DIALECT: JSON-Schema-shaped `parameters: type` -> Gemini OpenAPI-3 subset
      // (strip $ref, coerce formats, flatten anyOf) — the SAME lossy transform as
      // batch Gemini in scenario 09, just delivered in the Setup message, not a
      // session.update. Declared ONCE at setup; cannot be changed mid-session on
      // Gemini Live, which DIVERGES from OpenAI's per-response tool override.
      let decls: string[] = [];
      for (let tool in tools) {
        let openapi: string = baml.reflect.type_to_gemini_openapi(tool.parameters);
        decls = decls.append(
          baml.llm.gemini_function_declaration(tool.name, tool.description, openapi)
        );
      }
      io.send(GeminiLive.setup_tools_event(decls))   // part of BidiGenerateContentSetup
        catch (e) { _ => { throw baml.errors.LlmClient { message: "setup tools failed" }; } };
      null
    }

    function decode_tool_event(self, raw: InEvent) -> ToolEvent? throws baml.errors.LlmClient {
      // Gemini delivers the functionCall WHOLE inside a serverContent/toolCall
      // message — no delta accumulation to wait for. One server message can carry
      // SEVERAL functionCalls; the host transport fans them into one InEvent each,
      // so we still decode one ToolEvent per raw event.
      if (GeminiLive.is_function_call(raw)) {
        // If the wire `id` is present, echo it; else synthesize one keyed by
        // (name, turn-position) so ExecutionContext's call_id contract holds —
        // exactly the id-synthesis trick from scenario 09, but here it's needed
        // far LESS often because Live usually supplies ids.
        let wire_id: string = GeminiLive.function_call_id_or_synth(raw);
        return ToolEvent {
          call_id: wire_id,
          name: GeminiLive.event_field(raw, "name"),
          args_json: GeminiLive.event_field(raw, "args"),
        };
      }
      null
    }

    function emit_tool_output(self, io: Channel, output: ToolOutputEvent) -> null throws baml.errors.LlmClient {
      // THREADING: ONE event. BidiGenerateContentToolResponse with a single
      // functionResponse keyed by the SAME id (and name). Sending it IS the
      // continuation — there is no response.create. So this branch DROPS OpenAI's
      // await_response_done + response.create two-step. The capability seam is the
      // same; the implementation underneath is genuinely different shapes.
      io.send(GeminiLive.tool_response_event(output.call_id, output.output_json))
        catch (e) { _ => { throw baml.errors.LlmClient { message: "toolResponse failed" }; } };
      null
    }
  }

  function open_setup(self: GeminiLive, io: Channel, prompt: baml.llm.PromptAst) -> null { $rust_io_function }
  function pump_until_closed(self: GeminiLive, io: Channel) -> Transcript throws baml.errors.LlmClient { $rust_io_function }
  function is_function_call(raw: InEvent) -> bool { $rust_io_function }
  function function_call_id_or_synth(raw: InEvent) -> string { $rust_io_function }
  function event_field(raw: InEvent, key: string) -> string { $rust_io_function }
  function setup_tools_event(decls: string[]) -> OutEvent { $rust_io_function }
  function tool_response_event(call_id: string, output_json: string) -> OutEvent { $rust_io_function }
  function gemini_function_declaration(name: string, description: string, openapi: string) -> string { $rust_io_function }
}


// ============================================================================
// COMBINATOR — RealtimeFallback: forward Realtime + RealtimeTools by delegation
// ============================================================================
//
// A duplex session can't be "retried mid-call" — once the socket is up and audio
// is flowing, falling over to another provider means a fresh session (dead air +
// lost server-side history). So fallback here can ONLY choose at session OPEN.
// After `run_tools_live` is entered, we are committed to the chosen member. This
// is weaker than chat Fallback (which can retry per request) and the combinator
// must say so.
// ----------------------------------------------------------------------------
class RealtimeFallback {
  strategy: Provider[]

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        let r: T = p.call<T>(prompt) catch (e) { _ => { continue; } };
        return r;
      }
      throw baml.errors.LlmClient { message: "all providers failed" }
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        match (p) {
          let r: Realtime => { return r.run(prompt, io); },   // commit to the first realtime member
          _ => { continue; },
        }
      }
      throw baml.errors.LlmClient { message: "no realtime provider available" }
    }
  }

  implements RealtimeTools {
    // Pick the first member that can do realtime tools and run its WHOLE live loop.
    // We cannot split configure/decode/emit across members — they share one socket.
    function configure_tools(self, io: Channel, tools: Tool[]) -> null throws baml.errors.LlmClient {
      // unused on its own path; the default run_tools_live below drives a member
      // directly. Present only to satisfy the interface — see run_tools_live.
      throw baml.errors.LlmClient { message: "configure_tools on RealtimeFallback: call run_tools_live" }
    }
    function decode_tool_event(self, raw: InEvent) -> ToolEvent? throws baml.errors.LlmClient {
      throw baml.errors.LlmClient { message: "decode_tool_event on RealtimeFallback: call run_tools_live" }
    }
    function emit_tool_output(self, io: Channel, output: ToolOutputEvent) -> null throws baml.errors.LlmClient {
      throw baml.errors.LlmClient { message: "emit_tool_output on RealtimeFallback: call run_tools_live" }
    }

    // Override the default loop: choose a member ONCE, then delegate its entire
    // event loop. No mid-session failover.
    function run_tools_live(self, prompt: baml.llm.PromptAst, tools: Tool[],
                            io: Channel, ctx: ExecutionContext) -> Transcript throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        match (p) {
          let rt: RealtimeTools => { return rt.run_tools_live(prompt, tools, io, ctx); },
          _ => { continue; },
        }
      }
      throw baml.errors.LlmClient { message: "no realtime-tools provider available" }
    }
  }
}
