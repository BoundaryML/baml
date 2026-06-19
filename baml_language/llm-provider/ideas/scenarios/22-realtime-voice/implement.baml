// ============================================================================
// implement.baml — Realtime / voice: a persistent bidirectional connection
//
// Library / std-lib-author code. Builds on the canonical primitives from
// ../_conventions.md (Provider, Realtime, Channel, baml.llm.PromptAst).
//
// What this file has to show the model absorbing (or failing to):
//   - The Realtime capability: ONE long-lived connection, both sides push
//     typed events. There is no request/response `call` here — the connection
//     IS the session.
//   - The Channel is handed in AT the .live call (pass-in), not a client field.
//     A voice channel does not exist until you connect; it is the duplex the
//     provider drives, not config.
//   - Server VAD (the SERVER initiates responses) vs manual push-to-talk (the
//     client commits + asks). Most capabilities assume the client triggers
//     every generation; here that assumption breaks, and the break lives
//     entirely inside run().
//   - Audio is NOT a result. PCM chunks arriving 50ms apart cannot be a return
//     value; they ride the Channel as a side channel. run() returns a
//     Transcript (the record), never the audio.
//   - Items vs messages: OpenAI's typed conversation items, Gemini's turn-based
//     server_content. Neither is the role/content array of Chat Completions.
//     Both collapse to the same provider-owned Transcript here.
//   - Why config-only client + call-time io is the right split.
// ============================================================================


// ----------------------------------------------------------------------------
// 0. Net-new host + library surface this scenario needs.
//
// The conventions give us `interface Realtime` and `interface Channel`, but
// they are deliberately thin: Channel is generic over `InEvent`/`OutEvent` and
// run() returns an opaque `Transcript`. A voice session needs those holes
// *filled in* with a concrete event vocabulary, because the whole scenario is
// about what flows over the wire. Everything in this section is flagged in
// evaluation.md as net-new.
//
// (a) The duplex event types. The Channel handed to run() is bidirectional:
//     the provider registers `on(InEvent -> void)` to DRAIN server frames, and
//     calls `send(OutEvent)` to PUSH client frames. InEvent / OutEvent are the
//     associated types of Channel; a realtime provider pins them to these.
// ----------------------------------------------------------------------------

// Frames the SERVER pushes down. A single user turn produces dozens of these.
// This is a normalized union — each provider's wire grammar maps onto it inside
// its on()-handler (OpenAI's `response.audio.delta`, Gemini's `response.data`).
class InEvent {
  kind: InKind
  // exactly one of these is populated, keyed by `kind`:
  audio: uint8array?          // decoded output-audio PCM chunk (kind == OutputAudio)
  text: string?               // a text/caption delta (kind == OutputText | InputTranscript)
  item_id: string?            // server item id, for truncation correlation (barge-in)
  ms_played_window: int?      // server-reported audio position for this item, if any
  tool_call: ToolCall?        // the model is calling a tool (kind == ToolCall)
  usage: Usage?               // kind == TurnDone
  error_message: string?      // kind == Error: a structured mid-session error
}

enum InKind {
  SessionReady               // handshake / session.updated applied
  SpeechStarted              // server VAD: user started talking (barge-in trigger)
  SpeechStopped              // server VAD: user went silent, buffer auto-committed
  InputTranscript            // Whisper transcript of what the USER said
  OutputAudio                // a PCM chunk of the MODEL's voice (the side channel)
  OutputText                 // model's own-speech caption delta
  ToolCall                   // model wants a tool run mid-session
  TurnDone                   // response.done: one response finished (many per session)
  Error                      // structured error, session continues
}

// Frames the CLIENT pushes up.
class OutEvent {
  kind: OutKind
  audio: uint8array?          // kind == AppendAudio: a mic PCM chunk (many/sec)
  text: string?               // kind == SendText | UpdateInstructions
  voice: string?              // kind == UpdateSession: live voice swap
  item_id: string?            // kind == Truncate: which assistant item
  audio_end_ms: int?          // kind == Truncate: how much the user actually HEARD
  tool_result: ToolResult?    // kind == SendToolResult
}

enum OutKind {
  Commit                     // input_audio_buffer.commit  (manual mode only)
  RequestResponse            // response.create            (manual mode only)
  AppendAudio                // input_audio_buffer.append  (always, continuously)
  SendText                   // conversation.item.create, text user message
  SendToolResult             // function_call_output item
  Cancel                     // response.cancel            (barge-in)
  Truncate                   // conversation.item.truncate (barge-in: reflect what was heard)
  UpdateInstructions         // session.update instructions, mid-call
  UpdateSession              // session.update voice/temperature, mid-call
}

class Usage { input_tokens: int, output_tokens: int }

// (b) Tool seam reused verbatim from the conventions — ToolCall / ToolResult.
class ToolCall   { id: string, name: string, args: map<string, unknown> }
class ToolResult { id: string, output: unknown }

// (c) The record run() returns. NOT the audio — the audio already left over the
//     Channel. This is the durable conversation: a list of typed ITEMS, which
//     is the OpenAI model, and which Gemini's turn stream is folded into. An
//     item is not a chat message: a function_call and its output are their own
//     items, ordered, server-authoritative.
class Transcript {
  items: Item[]
  ended_reason: string        // "client_closed" | "server_closed" | "error"
}

class Item {
  id: string
  role: string                // "user" | "assistant" | "tool"
  itype: string               // "message" | "function_call" | "function_call_output"
  text: string?               // transcript text for message items (NOT the audio)
  tool_name: string?          // function_call / function_call_output items
}

// (d) NET-NEW host primitive: opening the wire. A realtime provider does not
//     use baml.http.send — it needs a duplex socket. We give the host a single
//     opener that returns a low-level frame socket; the provider wraps it into
//     the normalized Channel above. This is the realtime analogue of
//     baml.http.fetch_sse, but full-duplex.
//
//   class WireSocket { _handle: $rust_type }
//     .send_text(json: string) -> null throws baml.errors.Io   // push a wire frame
//     .recv()  -> string? throws baml.errors.Io                // block for next frame (null == closed)
//     .close() -> null
//
//   baml.realtime.open(url, headers) -> WireSocket throws baml.errors.Io
//
// (e) NET-NEW host primitive: the two concurrent pumps. The background's #1
//     "what's hard" is concurrency — you must pump mic audio UP and drain
//     server events DOWN at the same time, forever, over one socket, or you
//     deadlock. BAML run() is single-threaded straight-line code; it cannot
//     express "do these two loops concurrently" without a host primitive.
//     So we add a spawn that runs a closure on a host task and returns a handle
//     the provider can join/abort:
//
//   class Task { _handle: $rust_type }   // .join() -> null throws Io ; .abort() -> null
//   baml.sys.spawn(body: () -> null) -> Task throws baml.errors.Io
// ----------------------------------------------------------------------------


// ----------------------------------------------------------------------------
// 1. OpenAI Realtime — the WebSocket / typed-items provider.
//
//    A config-only class (voice/instructions/turn mode are FIELDS). The live
//    Channel is NOT here; it arrives at run(). `manual_turn` selects server VAD
//    vs push-to-talk: the single most divergent realtime behavior, and it lives
//    as one boolean that reshapes the body of run().
// ----------------------------------------------------------------------------
class OpenAIRealtime {
  model: string               // "gpt-4o-realtime-preview"
  voice: string               // "alloy" — NOT interchangeable across providers
  api_key: string
  instructions: string?       // the "system prompt", sent via session.update
  manual_turn: bool           // false => server_vad (server initiates); true => push-to-talk

  function from_voice(voice: string) -> OpenAIRealtime {     // static factory
    OpenAIRealtime {
      model: "gpt-4o-realtime-preview", voice: voice,
      api_key: env.OPENAI_API_KEY, instructions: null, manual_turn: false,
    }
  }

  // Realtime requires Provider, so we MUST supply call<T>. A single-turn over a
  // realtime model is a degenerate session: connect, send one text item, ask
  // for one response, drain to the first TurnDone, parse the caption, hang up.
  // This is the "best-effort single turn" of Open Question 1. It is real but
  // lossy — no audio, no streaming — so almost nobody calls Voice() directly.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let sock = self.open_socket();
      self.configure(sock);
      sock.send_text(self.encode_text_turn(baml.llm.render_text(prompt)))
        catch (e) { _ => { throw baml.errors.LlmClient { message: "realtime send failed" }; } };
      sock.send_text(self.encode_request_response())
        catch (e) { _ => { throw baml.errors.LlmClient { message: "response.create failed" }; } };
      // Drain frames until the model's own caption is done, then SAP-parse it.
      let caption: string = self.drain_one_caption(sock);
      sock.close();
      baml.sap.parse<T>(caption)
    }
  }

  // THE capability. io is a PARAMETER (pass-in). The provider drives the duplex
  // and returns the Transcript record when the session ends. Audio never
  // appears in the return type — it left over `io`.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      let sock = self.open_socket();
      self.configure(sock);                          // session.update: instructions, voice, VAD

      // If the caller seeded the session with an initial system/user text turn,
      // splice it as an item. (prompt is the function's `prompt #"..."#`.)
      let seed = baml.llm.render_text(prompt);
      if (seed != "") {
        sock.send_text(self.encode_session_instructions(seed))
          catch (e) { _ => { throw baml.errors.LlmClient { message: "seed failed" }; } };
      }

      // The mutable session record. run() OWNS it while draining; this is the
      // stateful accumulator (like the streaming reassembler), folded forward
      // by every server frame.
      let session = RealtimeSession {
        items: [], active_item_id: null, ended_reason: "client_closed",
      };

      // --- PUMP UP: mic frames the APP pushes via io get forwarded to the wire.
      // The app calls io.send(AppendAudio{...}) from its capture loop; we route
      // each OutEvent onto the socket. In server-VAD mode we NEVER send Commit
      // or RequestResponse — the server does both. In manual mode the app sends
      // them explicitly and we forward them.
      io.on(out => { self.forward_out(sock, out); });

      // --- PUMP DOWN: server frames get decoded, folded into `session`, and
      // the interesting ones (audio, captions, transcripts) re-emitted to the
      // app over the SAME io. Two concurrent loops, one socket — the host
      // spawn is what makes this not a deadlock.
      let pump = baml.sys.spawn(() => {
        for (let frame = sock.recv(); frame != null; frame = sock.recv()) {
          let ev = self.decode_in(frame);
          self.fold(session, ev);                    // server-authoritative state update
          io.send_in(ev);                            // hand the normalized InEvent to the app
        }
      }) catch (e) { _ => { throw baml.errors.LlmClient { message: "pump spawn failed" }; } };

      // run() blocks here for the WHOLE session — many responses long — until
      // either the app closes io (client_closed) or the socket drops
      // (server_closed). There is no "answer" to wait for; the wait IS the call.
      pump.join() catch (e) { _ => { session.ended_reason = "error"; } };
      sock.close();

      Transcript { items: session.items, ended_reason: session.ended_reason }
    }
  }

  // ---- provider-private helpers (the real divergence lives here) ----

  function open_socket(self) -> WireSocket throws baml.errors.LlmClient {
    baml.realtime.open(
      "wss://api.openai.com/v1/realtime?model=" + self.model,
      { "Authorization": "Bearer " + self.api_key, "OpenAI-Beta": "realtime=v1" },
    ) catch (e) { _ => { throw baml.errors.LlmClient { message: "connect failed" }; } }
  }

  // session.update — the closest thing to a system prompt. The turn_detection
  // field is where server-VAD vs manual diverges on the wire.
  function configure(self, sock: WireSocket) -> null throws baml.errors.LlmClient {
    let turn_detection = if (self.manual_turn) {
      "null"
    } else {
      "{\"type\":\"server_vad\",\"threshold\":0.5,\"silence_duration_ms\":500,\"create_response\":true}"
    };
    let frame = baml.json.object({
      "type": "session.update",
      "session": baml.json.object({
        "modalities": baml.json.array(["text", "audio"]),
        "voice": self.voice,
        "instructions": self.instructions,
        "input_audio_format": "pcm16",
        "output_audio_format": "pcm16",
        "input_audio_transcription": baml.json.object({ "model": "whisper-1" }),
        "turn_detection": baml.json.raw(turn_detection),
      }),
    });
    sock.send_text(frame)
      catch (e) { _ => { throw baml.errors.LlmClient { message: "session.update failed" }; } }
  }

  // Forward one app OutEvent onto the OpenAI wire. THIS is where the OpenAI
  // event names live; the app never sees them. Note Commit / RequestResponse
  // are honored in BOTH modes — the app simply doesn't send them under VAD.
  function forward_out(self, sock: WireSocket, out: OutEvent) -> null {
    let frame = match (out.kind) {
      OutKind.AppendAudio => baml.json.object({
        "type": "input_audio_buffer.append",
        "audio": baml.media.b64_encode(out.audio),
      }),
      OutKind.Commit          => baml.json.object({ "type": "input_audio_buffer.commit" }),
      OutKind.RequestResponse => baml.json.object({ "type": "response.create" }),
      OutKind.Cancel          => baml.json.object({ "type": "response.cancel" }),
      OutKind.Truncate        => baml.json.object({
        "type": "conversation.item.truncate",
        "item_id": out.item_id,
        "content_index": 0,
        "audio_end_ms": out.audio_end_ms,           // what the user ACTUALLY heard
      }),
      OutKind.SendText => baml.json.object({
        "type": "conversation.item.create",
        "item": baml.json.object({
          "type": "message", "role": "user",
          "content": baml.json.array([baml.json.object({ "type": "input_text", "text": out.text })]),
        }),
      }),
      OutKind.SendToolResult => baml.json.object({
        "type": "conversation.item.create",
        "item": baml.json.object({
          "type": "function_call_output",
          "call_id": out.tool_result.id,            // id-correlation: opaque, echoed back
          "output": baml.json.to_string(out.tool_result.output),
        }),
      }),
      OutKind.UpdateInstructions => baml.json.object({
        "type": "session.update",
        "session": baml.json.object({ "instructions": out.text }),
      }),
      OutKind.UpdateSession => baml.json.object({
        "type": "session.update",
        "session": baml.json.object({ "voice": out.voice }),
      }),
    };
    sock.send_text(frame) catch (e) { _ => { return null; } }    // best-effort; errors surface as InKind.Error
  }

  // Decode ONE OpenAI server frame into the normalized InEvent. The fan-in of
  // ~12 wire event types onto 9 InKinds. The audio delta is base64 PCM that we
  // decode to bytes HERE so the app gets raw audio, not a wire string.
  function decode_in(self, frame: string) -> InEvent {
    let t = baml.json.get(frame, "type");
    if (t == "response.audio.delta") {
      InEvent { kind: InKind.OutputAudio, audio: baml.media.b64_decode(baml.json.get(frame, "delta")),
                text: null, item_id: baml.json.get(frame, "item_id"),
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else if (t == "response.audio_transcript.delta") {
      InEvent { kind: InKind.OutputText, audio: null, text: baml.json.get(frame, "delta"),
                item_id: baml.json.get(frame, "item_id"), ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (t == "conversation.item.input_audio_transcription.completed") {
      InEvent { kind: InKind.InputTranscript, audio: null, text: baml.json.get(frame, "transcript"),
                item_id: baml.json.get(frame, "item_id"), ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (t == "input_audio_buffer.speech_started") {
      InEvent { kind: InKind.SpeechStarted, audio: null, text: null,
                item_id: baml.json.get(frame, "item_id"), ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (t == "input_audio_buffer.speech_stopped") {
      InEvent { kind: InKind.SpeechStopped, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else if (t == "response.function_call_arguments.done") {
      InEvent { kind: InKind.ToolCall, audio: null, text: null, item_id: baml.json.get(frame, "item_id"),
                ms_played_window: null,
                tool_call: ToolCall {
                  id: baml.json.get(frame, "call_id"),       // OpenAI HAS ids
                  name: baml.json.get(frame, "name"),
                  args: baml.json.parse_map(baml.json.get(frame, "arguments")),
                },
                usage: null, error_message: null }
    } else if (t == "response.done") {
      InEvent { kind: InKind.TurnDone, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: null,
                usage: Usage {
                  input_tokens: baml.json.get_int(frame, "response.usage.input_tokens"),
                  output_tokens: baml.json.get_int(frame, "response.usage.output_tokens"),
                },
                error_message: null }
    } else if (t == "error") {
      InEvent { kind: InKind.Error, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: null, usage: null, error_message: baml.json.get(frame, "error.message") }
    } else if (t == "session.created" || t == "session.updated") {
      InEvent { kind: InKind.SessionReady, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else {
      // rate_limits.updated, response.created, *.delta we don't surface, etc.
      InEvent { kind: InKind.SessionReady, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    }
  }

  // Fold a server frame into the server-authoritative item list. THIS is the
  // items-not-messages model: an assistant message item, a function_call item,
  // and a function_call_output item are three different items, not one turn.
  function fold(self, s: RealtimeSession, ev: InEvent) -> null {
    match (ev.kind) {
      InKind.InputTranscript => {
        s.items = baml.list.push(s.items, Item {
          id: ev.item_id, role: "user", itype: "message", text: ev.text, tool_name: null });
      },
      InKind.OutputText => {
        // captions stream as deltas; append into the active assistant item.
        s.active_item_id = ev.item_id;
        s.items = upsert_assistant_text(s.items, ev.item_id, ev.text);
      },
      InKind.ToolCall => {
        s.items = baml.list.push(s.items, Item {
          id: ev.tool_call.id, role: "assistant", itype: "function_call",
          text: null, tool_name: ev.tool_call.name });
      },
      _ => { return null; }   // audio frames don't mutate the durable transcript
    }
    null
  }

  function encode_text_turn(self, text: string) -> string {
    baml.json.object({
      "type": "conversation.item.create",
      "item": baml.json.object({
        "type": "message", "role": "user",
        "content": baml.json.array([baml.json.object({ "type": "input_text", "text": text })]),
      }),
    })
  }
  function encode_request_response(self) -> string {
    baml.json.object({ "type": "response.create" })
  }
  function encode_session_instructions(self, text: string) -> string {
    baml.json.object({ "type": "session.update", "session": baml.json.object({ "instructions": text }) })
  }
  function drain_one_caption(self, sock: WireSocket) -> string throws baml.errors.LlmClient {
    let acc = "";
    for (let frame = sock.recv(); frame != null; frame = sock.recv()) {
      let ev = self.decode_in(frame);
      match (ev.kind) {
        InKind.OutputText => { acc = acc + ev.text; },
        InKind.TurnDone   => { return acc; },
        InKind.Error      => { throw baml.errors.LlmClient { message: ev.error_message }; },
        _ => {},
      }
    }
    acc
  }
}

// The provider-owned mutable session state. Not part of any public interface.
class RealtimeSession {
  items: Item[]
  active_item_id: string?
  ended_reason: string
}

function upsert_assistant_text(items: Item[], id: string, delta: string) -> Item[] {
  $rust_io_function   // append `delta` to the assistant message item with `id`, or create it
}


// ----------------------------------------------------------------------------
// 2. Gemini Live — the SAME Realtime interface, a DIFFERENT conversation model.
//
//    Gemini Live is turn-based (server_content), not item-typed. There is no
//    server_vad boolean shaped the same way; modalities are declared as
//    response_modalities; tool calls arrive without the same call ids. The
//    payoff: the divergence is ABSORBED inside configure/decode_in/fold — the
//    Realtime interface and the Channel vocabulary are identical, so the app
//    that talks to OpenAIRealtime talks to GeminiLive unchanged (almost — see
//    evaluation.md on the voice catalog leak).
// ----------------------------------------------------------------------------
class GeminiLive {
  model: string               // "gemini-2.0-flash-live-001"
  api_key: string
  instructions: string?
  // Gemini's automatic activity detection is the default; manual is "activity
  // handling" with explicit start/end. Same boolean role, different wire.
  manual_turn: bool

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // best-effort single turn, same shape as OpenAIRealtime.call
      $rust_io_function
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      let sock = baml.realtime.open(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent",
        { "x-goog-api-key": self.api_key },
      ) catch (e) { _ => { throw baml.errors.LlmClient { message: "connect failed" }; } };

      // Gemini's setup frame: response_modalities + system_instruction.
      sock.send_text(baml.json.object({
        "setup": baml.json.object({
          "model": "models/" + self.model,
          "generation_config": baml.json.object({
            "response_modalities": baml.json.array(["AUDIO"]),
          }),
          "system_instruction": self.instructions,
        }),
      })) catch (e) { _ => { throw baml.errors.LlmClient { message: "setup failed" }; } };

      let session = RealtimeSession { items: [], active_item_id: null, ended_reason: "client_closed" };

      // Same UP pump: app OutEvents -> Gemini realtimeInput frames.
      io.on(out => { self.forward_out(sock, out); });

      // Same DOWN pump, Gemini decode.
      let pump = baml.sys.spawn(() => {
        for (let frame = sock.recv(); frame != null; frame = sock.recv()) {
          let ev = self.decode_in(frame);
          self.fold(session, ev);
          io.send_in(ev);
        }
      }) catch (e) { _ => { throw baml.errors.LlmClient { message: "pump spawn failed" }; } };

      pump.join() catch (e) { _ => { session.ended_reason = "error"; } };
      sock.close();
      Transcript { items: session.items, ended_reason: session.ended_reason }
    }
  }

  // Gemini's realtimeInput uses a mime-typed audio blob, not append/commit.
  // In automatic mode there is no commit/response.create AT ALL — sending audio
  // IS the turn. So Commit / RequestResponse become no-ops here: the app code
  // that emits them under manual mode is simply ignored under Gemini auto-VAD.
  // (This is a real portability seam — see evaluation.md.)
  function forward_out(self, sock: WireSocket, out: OutEvent) -> null {
    let frame = match (out.kind) {
      OutKind.AppendAudio => baml.json.object({
        "realtimeInput": baml.json.object({
          "mediaChunks": baml.json.array([baml.json.object({
            "mimeType": "audio/pcm;rate=16000",
            "data": baml.media.b64_encode(out.audio),
          })]),
        }),
      }),
      OutKind.SendText => baml.json.object({
        "clientContent": baml.json.object({
          "turns": baml.json.array([baml.json.object({
            "role": "user",
            "parts": baml.json.array([baml.json.object({ "text": out.text })]),
          })]),
          "turnComplete": baml.json.raw("true"),
        }),
      }),
      OutKind.SendToolResult => baml.json.object({
        "toolResponse": baml.json.object({
          "functionResponses": baml.json.array([baml.json.object({
            "name": out.tool_result.id,    // Gemini correlates by NAME, not a call id
            "response": baml.json.to_string(out.tool_result.output),
          })]),
        }),
      }),
      // Gemini auto-VAD has no explicit commit/response.create/cancel/truncate
      // wire frame. We drop them rather than fake them.
      _ => "",
    };
    if (frame != "") {
      sock.send_text(frame) catch (e) { _ => { return null; } };
    }
    null
  }

  // Gemini turn-based server_content -> the same normalized InEvent. Output
  // audio arrives as raw bytes (response.data), captions/interruptions differ.
  function decode_in(self, frame: string) -> InEvent {
    if (baml.json.has(frame, "serverContent.modelTurn.parts.0.inlineData")) {
      InEvent { kind: InKind.OutputAudio,
                audio: baml.media.b64_decode(baml.json.get(frame, "serverContent.modelTurn.parts.0.inlineData.data")),
                text: null, item_id: null, ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (baml.json.has(frame, "serverContent.interrupted")) {
      // Gemini signals barge-in as a flag + discards its own queued audio.
      InEvent { kind: InKind.SpeechStarted, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else if (baml.json.has(frame, "toolCall")) {
      InEvent { kind: InKind.ToolCall, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: ToolCall {
                  id: baml.json.get(frame, "toolCall.functionCalls.0.name"),  // NO id; name IS the id
                  name: baml.json.get(frame, "toolCall.functionCalls.0.name"),
                  args: baml.json.parse_map(baml.json.get(frame, "toolCall.functionCalls.0.args")),
                },
                usage: null, error_message: null }
    } else if (baml.json.has(frame, "serverContent.turnComplete")) {
      InEvent { kind: InKind.TurnDone, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: null, usage: Usage { input_tokens: 0, output_tokens: 0 }, error_message: null }
    } else {
      InEvent { kind: InKind.SessionReady, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    }
  }

  function fold(self, s: RealtimeSession, ev: InEvent) -> null {
    match (ev.kind) {
      InKind.ToolCall => {
        s.items = baml.list.push(s.items, Item {
          id: ev.tool_call.id, role: "assistant", itype: "function_call",
          text: null, tool_name: ev.tool_call.name });
      },
      _ => { return null; }
    }
    null
  }
}


// ----------------------------------------------------------------------------
// 3. The .live companion (capability negotiation) and a Channel adapter.
//
//    .live is generated from a function the way .stream / .run_tools are. It
//    match-es the client's provider against Realtime and, CRUCIALLY, there is
//    NO honest fallback: a one-shot .call cannot fake a duplex session, so the
//    `_` arm throws Unsupported. This is the proposal's §5 split: delivery
//    refinements degrade, interaction-shape changes error.
// ----------------------------------------------------------------------------

// Conceptually-generated for `function VoiceChat(...) { client: ...; prompt ... }`:
function VoiceChat$live(system: string, io: Channel) -> Transcript throws baml.errors.LlmClient {
  let p = VoiceChat$client();
  match (p) {
    let r: Realtime => r.run(baml.llm.render_text_prompt(system), io),
    _               => throw baml.errors.Unsupported { message: "client is not realtime" },
  }
}

// A host-backed Channel implementation the app gets from its audio I/O. The app
// does NOT write this — it asks the host for one bound to a mic source + a
// speaker sink. Shown here so the surface is concrete.
class AudioChannel {
  _handle: $rust_type          // mic source + speaker sink + an InEvent dispatch table

  implements Channel {
    // The provider registers its server-frame handler here.
    function on(self, handler: (InEvent) -> void) -> null { $rust_io_function }
    // The provider (or the app) pushes an OutEvent up the wire.
    function send(self, data: OutEvent) -> null throws baml.errors.Io { $rust_io_function }
    function close(self) -> null { $rust_io_function }
  }

  // App-facing convenience: register a typed handler for a specific InKind.
  // Built on `on`. This is how the app plays audio / shows captions.
  function on_kind(self, kind: InKind, handler: (InEvent) -> void) -> null { $rust_io_function }

  // The provider's down-pump uses this to deliver a decoded InEvent to the app
  // handlers. (send pushes UP to the wire; send_in delivers DOWN to the app.)
  function send_in(self, ev: InEvent) -> null { $rust_io_function }
}
