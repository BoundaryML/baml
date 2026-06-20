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

// (b') Concrete domain error: the server pushed a session-fatal structured
// `error` frame mid-session (InKind.Error with a usable message). This is NOT a
// transport failure and NOT the answer — it is a deliberate domain decline the
// provider raises on purpose, so it gets a concrete class on the realtime
// channel. It implements only baml.errors.RealtimeError now: under the marker
// model there is no degenerate `call` path to surface on, so the old CallError
// impl (and the "surfaces on call too" framing) is gone. (No network/rate-limit
// classification — false.)
class RealtimeServerError {
  message: string
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return false; }
  }
}

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
// (e) Concurrency — the two concurrent pumps. The background's #1 "what's hard"
//     is concurrency: you must pump mic audio UP and drain server events DOWN at
//     the same time, forever, over one socket, or you deadlock. This is NOT
//     net-new surface: BAML has first-class structured concurrency today
//     (BEP-034, baml_std/baml/ns_spawn + ns_future). `spawn { body }` starts the
//     down-pump loop concurrently and returns a `baml.future.Future<null, E>`
//     immediately; `await f` blocks on it and re-throws its error; `f.cancel()`
//     tears it down. So "do these two loops concurrently" is expressed in the
//     language — `run` spawns the down-pump and awaits it, and the up-pump rides
//     the app's own spawn (see usage.baml). No host concurrency primitive is
//     introduced here.
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

  // NOTE: OpenAIRealtime has NO `implements Provider { call }`. Under the
  // marker model (../provider-as-marker.md) `Provider` is a bare marker with no
  // interaction method; `call`/`call_with` belong to `HttpProvider`. A realtime
  // model does request/response over a duplex socket only as a degenerate fake
  // (connect, one text turn, drain one caption, hang up) — no audio, no
  // streaming — so we DROP that lie. The class `requires Provider` via the
  // `Realtime` capability below, so it still IS a Provider (marker); its REAL,
  // honest interaction is `run`, and nothing forces a fake `call` onto it.
  // (This closes the old OQ1 "degenerate call" complaint for realtime.)

  // THE capability. io is a PARAMETER (pass-in). The provider drives the duplex
  // and returns the Transcript record when the session ends. Audio never
  // appears in the return type — it left over `io`.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      let sock = self.open_socket();
      self.configure(sock);                          // session.update: instructions, voice, VAD

      // If the caller seeded the session with an initial system/user text turn,
      // splice it as an item. (prompt is the function's `prompt #"..."#`.)
      let seed = baml.llm.render_text(prompt);
      if (seed != "") {
        sock.send_text(self.encode_session_instructions(seed));
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
      // app over the SAME io. Two concurrent loops, one socket — `spawn`
      // (BEP-034) is what makes this not a deadlock: it runs the drain loop
      // concurrently with the app's up-pump and returns a Future immediately.
      let pump: baml.future.Future<null, baml.errors.Io> = spawn {
        for (let frame = sock.recv(); frame != null; frame = sock.recv()) {
          let ev = self.decode_in(frame);
          self.fold(session, ev);                    // server-authoritative state update
          io.send_in(ev);                            // hand the normalized InEvent to the app
        }
      };

      // run() blocks here for the WHOLE session — many responses long — until
      // either the app closes io (client_closed) or the socket drops
      // (server_closed). There is no "answer" to wait for; the wait IS the call.
      // `await` re-throws the pump's error; on failure we mark the session.
      (await pump) catch (e) { _ => { session.ended_reason = "error"; } };
      sock.close();

      Transcript { items: session.items, ended_reason: session.ended_reason }
    } catch (e) {
      // Foreign socket Io / spawn failures are normalized onto the realtime channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "openai realtime run failed");
    }
  }

  // ---- provider-private helpers (the real divergence lives here) ----

  // These helpers feed `run` (the provider's ONLY interaction), so they ride the
  // RealtimeError channel — there is no call path under the marker model, so no
  // CallError surface here. A connect/configure failure is a realtime failure.
  function open_socket(self) -> WireSocket
      throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
    baml.realtime.open(
      "wss://api.openai.com/v1/realtime?model=" + self.model,
      { "Authorization": "Bearer " + self.api_key, "OpenAI-Beta": "realtime=v1" },
    ) catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "openai realtime connect failed"); } }
  }

  // session.update — the closest thing to a system prompt. The turn_detection
  // field is where server-VAD vs manual diverges on the wire.
  function configure(self, sock: WireSocket) -> null
      throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
    // turn_detection is a native json value: `null` under manual mode, or a
    // server_vad config object under VAD mode. No string-escaping needed — it is
    // a real `map<string, json>` (or `null`) nested directly into the session.
    let turn_detection: baml.json.json = if (self.manual_turn) {
      null
    } else {
      {
        "type": "server_vad", "threshold": 0.5,
        "silence_duration_ms": 500, "create_response": true,
      }
    };
    let frame: baml.json.json = {
      "type": "session.update",
      "session": {
        "modalities": ["text", "audio"],
        "voice": self.voice,
        "instructions": self.instructions,
        "input_audio_format": "pcm16",
        "output_audio_format": "pcm16",
        "input_audio_transcription": { "model": "whisper-1" },
        "turn_detection": turn_detection,
      },
    };
    sock.send_text(baml.json.stringify(frame))
      catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "openai session.update failed"); } }
  }

  // Forward one app OutEvent onto the OpenAI wire. THIS is where the OpenAI
  // event names live; the app never sees them. Note Commit / RequestResponse
  // are honored in BOTH modes — the app simply doesn't send them under VAD.
  function forward_out(self, sock: WireSocket, out: OutEvent) -> null {
    // Each arm builds a native `map<string, json>` (a `baml.json.json` value);
    // `out.audio` is a uint8array, base64-encoded via the real `.to_base64()`.
    let audio_b64: baml.json.json = match (out.audio) { null => "", let a: uint8array => a.to_base64() };
    let frame: baml.json.json = match (out.kind) {
      OutKind.AppendAudio => {
        "type": "input_audio_buffer.append",
        "audio": audio_b64,
      },
      OutKind.Commit          => { "type": "input_audio_buffer.commit" },
      OutKind.RequestResponse => { "type": "response.create" },
      OutKind.Cancel          => { "type": "response.cancel" },
      OutKind.Truncate        => {
        "type": "conversation.item.truncate",
        "item_id": out.item_id,
        "content_index": 0,
        "audio_end_ms": out.audio_end_ms,           // what the user ACTUALLY heard
      },
      OutKind.SendText => {
        "type": "conversation.item.create",
        "item": {
          "type": "message", "role": "user",
          "content": [{ "type": "input_text", "text": out.text }],
        },
      },
      OutKind.SendToolResult => {
        "type": "conversation.item.create",
        "item": {
          "type": "function_call_output",
          "call_id": out.tool_result.id,            // id-correlation: opaque, echoed back
          "output": baml.json.to_string(out.tool_result.output),
        },
      },
      OutKind.UpdateInstructions => {
        "type": "session.update",
        "session": { "instructions": out.text },
      },
      OutKind.UpdateSession => {
        "type": "session.update",
        "session": { "voice": out.voice },
      },
    };
    sock.send_text(baml.json.stringify(frame)) catch (e) { _ => { return null; } }    // best-effort; errors surface as InKind.Error
  }

  // Decode ONE OpenAI server frame into the normalized InEvent. The fan-in of
  // ~12 wire event types onto 9 InKinds. The audio delta is base64 PCM that we
  // decode to bytes HERE so the app gets raw audio, not a wire string.
  //
  // `baml.json.parse` turns the wire text into a `json` value once; the
  // `json_*` helpers (below) navigate the resulting `map`/`json[]` by dotted
  // path. Base64 audio is decoded with the real `uint8array.from_base64`.
  function decode_in(self, frame: string) -> InEvent {
    let root: baml.json.json = baml.json.parse(frame) catch (e) { _ => null };
    let t = json_str(root, "type");
    if (t == "response.audio.delta") {
      InEvent { kind: InKind.OutputAudio, audio: uint8array.from_base64(json_str(root, "delta")),
                text: null, item_id: json_str(root, "item_id"),
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else if (t == "response.audio_transcript.delta") {
      InEvent { kind: InKind.OutputText, audio: null, text: json_str(root, "delta"),
                item_id: json_str(root, "item_id"), ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (t == "conversation.item.input_audio_transcription.completed") {
      InEvent { kind: InKind.InputTranscript, audio: null, text: json_str(root, "transcript"),
                item_id: json_str(root, "item_id"), ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (t == "input_audio_buffer.speech_started") {
      InEvent { kind: InKind.SpeechStarted, audio: null, text: null,
                item_id: json_str(root, "item_id"), ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (t == "input_audio_buffer.speech_stopped") {
      InEvent { kind: InKind.SpeechStopped, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else if (t == "response.function_call_arguments.done") {
      InEvent { kind: InKind.ToolCall, audio: null, text: null, item_id: json_str(root, "item_id"),
                ms_played_window: null,
                tool_call: ToolCall {
                  id: json_str(root, "call_id"),       // OpenAI HAS ids
                  name: json_str(root, "name"),
                  args: json_args(json_str(root, "arguments")),
                },
                usage: null, error_message: null }
    } else if (t == "response.done") {
      InEvent { kind: InKind.TurnDone, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: null,
                usage: Usage {
                  input_tokens: json_int(root, "response.usage.input_tokens"),
                  output_tokens: json_int(root, "response.usage.output_tokens"),
                },
                error_message: null }
    } else if (t == "error") {
      InEvent { kind: InKind.Error, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: null, usage: null, error_message: json_str(root, "error.message") }
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
        s.items.push(Item {
          id: ev.item_id, role: "user", itype: "message", text: ev.text, tool_name: null });
      },
      InKind.OutputText => {
        // captions stream as deltas; append into the active assistant item.
        s.active_item_id = ev.item_id;
        s.items = upsert_assistant_text(s.items, ev.item_id, ev.text);
      },
      InKind.ToolCall => {
        s.items.push(Item {
          id: ev.tool_call.id, role: "assistant", itype: "function_call",
          text: null, tool_name: ev.tool_call.name });
      },
      _ => { return null; }   // audio frames don't mutate the durable transcript
    }
    null
  }

  // encode_session_instructions stays — it splices the seed prompt into the live
  // session inside run(). (The call-only helpers encode_text_turn /
  // encode_request_response / drain_one_caption were removed with the degenerate
  // call: the marker model means OpenAIRealtime no longer fakes request/response.)
  function encode_session_instructions(self, text: string) -> string {
    let frame: baml.json.json = { "type": "session.update", "session": { "instructions": text } };
    baml.json.stringify(frame)
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
// JSON navigation helpers — read a parsed `baml.json.json` value by dotted path.
//
// These replace the invented `baml.json.get/get_int/has/parse_map` readers. They
// are built on the REAL surface: `baml.json` is `null | bool | int | float |
// string | json[] | map<string, json>`, navigated with `Map.get` / `Array.at`
// after `match`-narrowing each segment. A path segment that is all-digits indexes
// an array; anything else is a map key. A miss yields `null` (never throws), so
// decode_in stays total over malformed/partial frames.
// ----------------------------------------------------------------------------
function json_get(root: baml.json.json, path: string) -> baml.json.json {
  let cur: baml.json.json = root;
  for (let seg in path.split(".")) {
    cur = match (cur) {
      let m: map<string, json> => match (m.get(seg)) { null => null, let v: json => v },
      let a: json[] => if (seg.is_ascii_numeric()) {
        match (a.at(int.from_string(seg) catch (e) { _ => -1 })) { null => null, let v: json => v }
      } else { null },
      _ => null,
    };
  }
  cur
}

// A string at `path`, or "" when absent / not a string.
function json_str(root: baml.json.json, path: string) -> string {
  match (json_get(root, path)) { let s: string => s, _ => "" }
}

// An int at `path`, or 0 when absent / not an int.
function json_int(root: baml.json.json, path: string) -> int {
  match (json_get(root, path)) { let i: int => i, _ => 0 }
}

// True when `path` resolves to a present (non-null) value.
function json_has(root: baml.json.json, path: string) -> bool {
  match (json_get(root, path)) { null => false, _ => true }
}

// The map at `path` (tool-call args), or an empty map when absent / not a map.
function json_map_at(root: baml.json.json, path: string) -> map<string, unknown> {
  match (json_get(root, path)) {
    let m: map<string, json> => m,
    _ => {},
  }
}

// Parse a JSON-encoded arguments string (OpenAI delivers function args as a JSON
// STRING) into a map. A parse failure or non-object yields an empty map.
function json_args(args: string) -> map<string, unknown> {
  match (baml.json.parse(args) catch (e) { _ => null }) {
    let m: map<string, json> => m,
    _ => {},
  }
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

  // Like OpenAIRealtime: NO `implements Provider { call }`. The marker model
  // (../provider-as-marker.md) drops the degenerate best-effort single turn;
  // GeminiLive `requires Provider` via `Realtime` and exposes only `run`.

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      let sock = baml.realtime.open(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent",
        { "x-goog-api-key": self.api_key },
      );

      // Gemini's setup frame: response_modalities + system_instruction.
      let setup: baml.json.json = {
        "setup": {
          "model": "models/" + self.model,
          "generation_config": {
            "response_modalities": ["AUDIO"],
          },
          "system_instruction": self.instructions,
        },
      };
      sock.send_text(baml.json.stringify(setup));

      let session = RealtimeSession { items: [], active_item_id: null, ended_reason: "client_closed" };

      // Same UP pump: app OutEvents -> Gemini realtimeInput frames.
      io.on(out => { self.forward_out(sock, out); });

      // Same DOWN pump, Gemini decode. `spawn` (BEP-034) runs it concurrently.
      let pump: baml.future.Future<null, baml.errors.Io> = spawn {
        for (let frame = sock.recv(); frame != null; frame = sock.recv()) {
          let ev = self.decode_in(frame);
          self.fold(session, ev);
          io.send_in(ev);
        }
      };

      (await pump) catch (e) { _ => { session.ended_reason = "error"; } };
      sock.close();
      Transcript { items: session.items, ended_reason: session.ended_reason }
    } catch (e) {
      // Foreign socket Io / setup / spawn failures normalized onto the realtime channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "gemini live run failed");
    }
  }

  // Gemini's realtimeInput uses a mime-typed audio blob, not append/commit.
  // In automatic mode there is no commit/response.create AT ALL — sending audio
  // IS the turn. So Commit / RequestResponse become no-ops here: the app code
  // that emits them under manual mode is simply ignored under Gemini auto-VAD.
  // (This is a real portability seam — see evaluation.md.)
  function forward_out(self, sock: WireSocket, out: OutEvent) -> null {
    let audio_b64: baml.json.json = match (out.audio) { null => "", let a: uint8array => a.to_base64() };
    // `null` is the drop sentinel (Gemini auto-VAD has no wire frame for some
    // OutKinds); every other arm builds a native `map<string, json>` value.
    let frame: baml.json.json = match (out.kind) {
      OutKind.AppendAudio => {
        "realtimeInput": {
          "mediaChunks": [{
            "mimeType": "audio/pcm;rate=16000",
            "data": audio_b64,
          }],
        },
      },
      OutKind.SendText => {
        "clientContent": {
          "turns": [{
            "role": "user",
            "parts": [{ "text": out.text }],
          }],
          "turnComplete": true,
        },
      },
      OutKind.SendToolResult => {
        "toolResponse": {
          "functionResponses": [{
            "name": out.tool_result.id,    // Gemini correlates by NAME, not a call id
            "response": baml.json.to_string(out.tool_result.output),
          }],
        },
      },
      // Gemini auto-VAD has no explicit commit/response.create/cancel/truncate
      // wire frame. We drop them rather than fake them.
      _ => null,
    };
    match (frame) {
      null => {},
      let f: baml.json.json => { sock.send_text(baml.json.stringify(f)) catch (e) { _ => { return null; } }; },
    }
    null
  }

  // Gemini turn-based server_content -> the same normalized InEvent. Output
  // audio arrives as raw bytes (response.data), captions/interruptions differ.
  function decode_in(self, frame: string) -> InEvent {
    let root: baml.json.json = baml.json.parse(frame) catch (e) { _ => null };
    if (json_has(root, "serverContent.modelTurn.parts.0.inlineData")) {
      InEvent { kind: InKind.OutputAudio,
                audio: uint8array.from_base64(json_str(root, "serverContent.modelTurn.parts.0.inlineData.data")),
                text: null, item_id: null, ms_played_window: null,
                tool_call: null, usage: null, error_message: null }
    } else if (json_has(root, "serverContent.interrupted")) {
      // Gemini signals barge-in as a flag + discards its own queued audio.
      InEvent { kind: InKind.SpeechStarted, audio: null, text: null, item_id: null,
                ms_played_window: null, tool_call: null, usage: null, error_message: null }
    } else if (json_has(root, "toolCall")) {
      InEvent { kind: InKind.ToolCall, audio: null, text: null, item_id: null, ms_played_window: null,
                tool_call: ToolCall {
                  id: json_str(root, "toolCall.functionCalls.0.name"),  // NO id; name IS the id
                  name: json_str(root, "toolCall.functionCalls.0.name"),
                  args: json_map_at(root, "toolCall.functionCalls.0.args"),
                },
                usage: null, error_message: null }
    } else if (json_has(root, "serverContent.turnComplete")) {
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
        s.items.push(Item {
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
function VoiceChat$live(system: string, io: Channel) -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  let p = VoiceChat$client();
  match (p) {
    let r: Realtime => r.run(baml.llm.render_text_prompt(system), io),
    // Unsupported implements every capability interface, so it is legal on the
    // RealtimeError channel and is caught by its concrete type — NOT routed
    // through UnknownError.
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
