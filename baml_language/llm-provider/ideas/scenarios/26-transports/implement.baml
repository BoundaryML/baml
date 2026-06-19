// =============================================================================
// Scenario 26 — Transport taxonomy: why request/response cannot model realtime
// implement.baml  —  STD-LIB / library-author code
//
// Builds on the canonical spine (_conventions.md): Provider, HttpProvider,
// Streaming, Realtime, Channel, the combinators.
//
// The thesis this file makes concrete:
//
//   TRANSPORT (HTTP / SSE / chunked-JSON / WS / WebRTC) is a property of the
//   PROVIDER, not of the call. Two providers that speak the SAME application-level
//   capability can sit on different wires; and one wire (a wss:// socket) can carry
//   two completely different capabilities (warm Responses vs duplex Realtime).
//
//   So the axis the proposal already has — "capabilities are interfaces" — is the
//   right axis, but it forces a sharp claim we test here: REALTIME IS ITS OWN
//   CAPABILITY, not a refinement of `call`. The background gives five structural
//   reasons request/response cannot model a live session; this file shows each one
//   landing on a *different* part of the Realtime interface than `call` could reach.
//
// What this file shows absorbing (or failing to absorb):
//   1. The five structural reasons, each tied to a concrete piece of surface.
//   2. Transport-is-provider: ChatHTTP (single-shot), ChatSSE (warm-token stream),
//      ResponsesWS (warm socket, SAME call semantics), RealtimeWS (duplex audio),
//      RealtimeWebRTC (media plane split off the data plane).
//   3. The bidirectionality cliff: the four forcing use cases (barge-in,
//      mid-stream tool injection, side-channel commands, unsolicited server events)
//      are expressible ONLY on Realtime's duplex Channel, never on a Stream.
//   4. State-location is an ORTHOGONAL axis: a `state:` field, not the transport.
//   5. Responses-over-WS is `call`/`Streaming` on a warm socket — NOT Realtime.
//      Realtime-WS is duplex. Same wire technology, opposite capability.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. Net-new host primitives this scenario needs.  (Flagged in evaluation.md)
//
// The conventions ship baml.http.send / fetch_sse / SseStream. Realtime and the
// warm-socket transports need a real bidirectional socket primitive that the SSE
// reader cannot provide (SSE is server->client only — that is the whole cliff).
//
//   // A live, full-duplex frame socket. Frames are opaque strings (JSON events
//   // for OpenAI/Gemini; SDP/RTP is handled below for WebRTC). This is the host
//   // primitive that DOES admit a client->server send mid-stream — the thing SSE
//   // structurally cannot do.
//   class WsSocket { url: string, _handle: $rust_type }
//   namespace baml.ws {
//     function connect(req: baml.http.Request) -> WsSocket throws baml.errors.Io
//     // WsSocket methods (host-backed):
//     //   .send(frame: string) -> null throws baml.errors.Io         // client -> server, ANY time
//     //   .recv() -> string? throws baml.errors.Io                   // server -> client; null = closed
//     //   .close() -> null
//   }
//
//   // WebRTC splits the connection in two. The DATA plane carries the same JSON
//   // event grammar as the WS; the MEDIA plane carries RTP audio the events never
//   // touch. Setup is an SDP offer/answer with an ephemeral key, not a header.
//   class RtcSession {
//     data: WsSocket          // the "oai-events" data channel — SAME frame grammar as baml.ws
//     media: MediaTrack       // RTP audio track; bytes do NOT flow through `data`
//     _handle: $rust_type
//   }
//   class MediaTrack { _handle: $rust_type }   // .attach_mic() / .attach_speaker() — host owns the codec
//   namespace baml.webrtc {
//     function connect(offer_for: baml.http.Request, ephemeral_key: string)
//         -> RtcSession throws baml.errors.Io     // mints SDP offer, posts it, sets remote answer
//   }
//
// `baml.ws` is the load-bearing addition: a duplex frame socket. Everything that
// makes realtime realtime rides on `.send` being callable WHILE `.recv` is pending.
// -----------------------------------------------------------------------------


// -----------------------------------------------------------------------------
// 1. The transport-vs-state taxonomy, made into TYPES (not provider kinds).
//
// Background §"Server state is a *separate* axis": transport and state-location are
// ORTHOGONAL. People conflate "WebSocket => server state". We refuse to. Transport
// shows up as WHICH host primitive the provider reaches for (send vs fetch_sse vs
// ws.connect vs webrtc.connect). State-location shows up as a plain `state:` field
// the provider carries, independent of the wire.
// -----------------------------------------------------------------------------

// Who owns the canonical conversation. This is the OTHER coordinate — read it off
// the provider's field, never off the transport.
enum StateLocation {
  ClientOwned,        // client threads full history every turn (Chat Completions, Messages)
  ServerStoredById,   // server holds it, client passes previous_response_id (Responses)
  ServerSession,      // server-authoritative live session, no resume id (Realtime / Live)
}

// A purely descriptive read-interface so tooling can ask a provider "what wire?"
// and "where's the state?" WITHOUT a closed kind enum. Optional, like
// ChatModelOptions in §4 of the proposal.
enum Wire { Http, HttpSse, HttpChunkedJson, WebSocket, WebRtc }

interface TransportInfo {
  function wire(self) -> Wire
  function state_location(self) -> StateLocation
}


// =============================================================================
// 2. THE FLOOR: HTTP single-shot. Transport = Http, state = ClientOwned.
//
//    Background "Patience" regime. `(request) -> response`, then close. This is
//    the ONLY shape request/response models perfectly — and the baseline the rest
//    diverges from. Pure HttpProvider, gets `call` for free.
// =============================================================================

class ChatHTTP {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }
  implements TransportInfo {
    function wire(self) -> Wire { Http }
    function state_location(self) -> StateLocation { ClientOwned }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function                              // response_format = json_schema of T
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } })
    }
  }
}


// =============================================================================
// 3. THE READING REGIME: HTTP + SSE. Transport = HttpSse, state = ClientOwned.
//
//    Background "Reading" regime + the bidirectionality cliff "It is a cliff, not a
//    slope". SSE *feels* bidirectional but structurally CANNOT send a client->server
//    frame mid-stream. So it implements Streaming (richer DELIVERY of the same call)
//    and NOTHING duplex. This is exactly where the proposal's "Streaming degrades to
//    call" split lives: streaming is a delivery refinement, not an interaction shape.
// =============================================================================

class ChatSSE {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }
  implements TransportInfo {
    function wire(self) -> Wire { HttpSse }
    function state_location(self) -> StateLocation { ClientOwned }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } })
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      // Opens an SSE reader. Server -> client only. There is NO method here, and
      // none possible, that lets the consumer say something back mid-stream:
      // SseStream has .next() and .close(), but no .send(). That absence IS the
      // structural fact the background calls "the client's mouth is taped shut".
      $rust_io_function
    }
  }
  // NOTE: no `implements Realtime`. ChatSSE physically cannot do barge-in.
}


// =============================================================================
// 4. RESPONSES OVER A WARM SOCKET. Transport = WebSocket, state = ServerStoredById.
//
//    Background §"Responses over WebSocket": "a different beast" from Realtime WS.
//    SAME Responses semantics — same body, same tools, same previous_response_id
//    chaining — just over a persistent warm socket for latency. NO audio, NO VAD,
//    NO duplex. One in-flight response per connection.
//
//    THE TEACHING POINT: this is a `wss://` socket, yet it is NOT Realtime. It
//    implements `Provider.call` and `Streaming` — the SAME capabilities as ChatSSE.
//    The wire is WebSocket; the capability is request/response. Proof that transport
//    is orthogonal to capability: you cannot read "is this realtime?" off "is this a
//    socket?". The warm socket is an optimization the application code never sees.
// =============================================================================

class ResponsesWS {
  model: string
  api_key: string
  // The warm socket + last-response-id is a CONNECTION-LOCAL, MUTABLE resource with
  // a lifecycle (lazy dial, ~60-min cap, one-in-flight). The proposal says providers
  // are immutable config, so we DON'T mutate `self` — we follow the std-lib idiom
  // (scenario 17's `MemoryStore._cells`): hide the mutable state behind a host
  // handle and let the host own its lifecycle. `_pool` holds {socket?, last_id?}.
  // This keeps the BAML value pure config while being honest that a warm socket is a
  // resource, not a field. (The tension this papers over is flagged in evaluation.md.)
  _pool: $rust_type

  function dial(model: string, api_key: string) -> ResponsesWS {   // static constructor
    ResponsesWS { model: model, api_key: api_key, _pool: $rust_io_function }
  }

  implements ChatModelOptions { model as model, api_key as api_key }
  implements TransportInfo {
    function wire(self) -> Wire { WebSocket }            // ← a socket...
    function state_location(self) -> StateLocation { ServerStoredById }
  }

  // ★ It implements PLAIN Provider.call (and Streaming) — NOT Realtime. The socket
  // is request/response on a warm wire. "One in-flight response per connection"
  // maps exactly onto call's 1:1 contract: one response.create -> one terminal
  // response.completed, then the socket is free for the next turn.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // Borrow the warm socket from the host pool (dials lazily, guards one-in-flight).
      let ws: WsSocket = self.warm_socket();
      // First (or continuation) turn: a normal Responses body wrapped as a
      // response.create event, carrying previous_response_id for the warm cache.
      // The host pool supplies the last id so each turn chains the warm cache.
      let body: string = self.encode_response_create<T>(prompt, self.last_id());
      ws.send(body) catch (e) { _ => { throw baml.errors.LlmClient { message: "ws send failed" }; } };

      // Drain server->client events until response.completed. This is a 1:1
      // request/response loop *over* a duplex socket — we never send mid-response,
      // which is exactly why this is `call` and not Realtime.
      let acc: string = "";
      while (true) {
        let frame: string? = ws.recv() catch (e) { _ => { throw baml.errors.LlmClient { message: "ws recv failed" }; } };
        match (frame) {
          null => { throw baml.errors.LlmClient { message: "socket closed mid-response" }; }
          let raw: string => {
            let etype: string = baml.json.get_str(raw, "type") ?? "";
            if (etype == "response.output_text.delta") {
              acc = acc + (baml.json.get_str(raw, "delta") ?? "");
            }
            if (etype == "response.completed") {
              // record the id in the host pool so the NEXT call chains the warm cache.
              self.remember_id(baml.json.get_str(raw, "response.id") ?? "");
              return baml.sap.parse<T>(acc);
            }
          }
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  implements Streaming {
    // Same warm socket, but yield deltas as they arrive. Still server->client only
    // within a turn — a delivery refinement, identical capability to ChatSSE.
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      $rust_io_function
    }
  }
  // NO `implements Realtime`: there is no duplex, no barge-in, no audio. The socket
  // is a latency trick, not an interaction shape.

  // Pool accessors — host-backed, like MemoryStore's store methods. The lifecycle
  // (lazy dial, ~60-min cap, one-in-flight guard) lives in the host, NOT in a
  // mutable BAML field.
  function warm_socket(self) -> WsSocket throws baml.errors.LlmClient { $rust_io_function }
  function last_id(self) -> string? { $rust_io_function }
  function remember_id(self, id: string) -> null { $rust_io_function }
  function encode_response_create<T>(self, prompt: baml.llm.PromptAst, prev: string?)
      -> string throws baml.errors.LlmClient { $rust_io_function }
}


// =============================================================================
// 5. REALTIME OVER WEBSOCKET. Transport = WebSocket, state = ServerSession.
//
//    Same wire technology as §4, OPPOSITE capability. This is the row where "the
//    model stops being a thing you call". It implements Realtime — duplex,
//    server-authoritative, 1:N, server-initiated events, state-loss-on-disconnect.
//
//    Each of the FIVE structural reasons from the background lands on a specific
//    part of `run`, and NONE of them can be reached by a `(prompt) -> T` signature:
// =============================================================================

class RealtimeWS {
  voice: string
  api_key: string
  // server-authoritative session: there is NO resume id, so we don't even carry a
  // _last_response_id. Connection loss = state loss is encoded by the ABSENCE of a
  // recoverable handle. (Reason #5, by omission.)

  implements TransportInfo {
    function wire(self) -> Wire { WebSocket }            // ← also a socket, but...
    function state_location(self) -> StateLocation { ServerSession }   // ...DIFFERENT state axis
  }

  implements Realtime {
    // io: Channel is a PASS-IN parameter (proposal §3): the caller's duplex endpoint
    // (their mic/speaker, their UI). The provider drives the wire and bridges it to
    // io. The session ends -> return the Transcript record.
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      let req = baml.http.Request {
        method: "GET",
        url: "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview",
        headers: { "authorization": "Bearer " + self.api_key, "openai-beta": "realtime=v1" },
        body: "",
      };
      let ws: WsSocket = baml.ws.connect(req)
        catch (e) { _ => { throw baml.errors.LlmClient { message: "realtime dial failed" }; } };

      // configure the server-authoritative session (voice, modalities, VAD).
      ws.send(self.session_update_frame()) catch (e) { _ => { throw baml.errors.LlmClient { message: "session.update failed" }; } };

      // ---- REASON #1: concurrent duplex. ----
      // The caller's inbound events feed the socket WHILE the socket feeds the caller.
      // io.on registers a handler that runs concurrently with the recv loop below.
      // A (prompt) -> T signature has one input slot; this has two open at once.
      io.on(ev => {
        match (ev) {
          // user audio frames go UP the socket as they arrive — mid-response.
          let a: AudioInEvent => {
            ws.send(append_audio_frame(a.pcm)) catch (e) { _ => {} };
          }
          // ---- REASON, cliff use case #1: BARGE-IN. ----
          // The user interrupts mid-speech. We must cancel + truncate NOW. This is a
          // client->server frame sent WHILE the server is still streaming audio out.
          // SSE cannot do this; it requires .send on a live duplex socket.
          let b: BargeInEvent => {
            ws.send(cancel_frame()) catch (e) { _ => {} };
            ws.send(truncate_frame(b.item_id, b.audio_ms)) catch (e) { _ => {} };
          }
          // ---- cliff use case #3: SIDE-CHANNEL COMMANDS. ----
          // change voice / temperature / VAD sensitivity mid-session.
          let c: ControlEvent => {
            ws.send(self.reconfigure_frame(c)) catch (e) { _ => {} };
          }
          // ---- cliff use case #2: MID-STREAM TOOL-RESULT INJECTION. ----
          // push a function_call_output into the live conversation while the model
          // is still generating — a client->server item create on the open socket.
          let t: ToolResultInEvent => {
            ws.send(function_call_output_frame(t.call_id, t.output)) catch (e) { _ => {} };
          }
        }
      });

      // seed the conversation with the initial prompt (server now owns it).
      ws.send(self.initial_prompt_frame(prompt)) catch (e) { _ => { throw baml.errors.LlmClient { message: "seed failed" }; } };

      // ---- the receive loop: REASONS #2, #3, #4 all live here. ----
      // It accumulates the provider-owned Transcript. N user turns, each yielding 0/1/
      // several responses -> there is no single "the response". (REASON #3: 1:N.)
      let transcript: Transcript = empty_transcript();
      while (true) {
        let frame: string? = ws.recv() catch (e) { _ => { throw baml.errors.LlmClient { message: "recv failed" }; } };
        match (frame) {
          // ---- REASON #5: connection loss = state loss. ----
          // null = socket closed. There is NO session id to resume; the server-held
          // conversation is gone. We return whatever Transcript we accumulated and
          // surface the loss — we cannot reconnect to "the same session".
          null => { return transcript; }
          let raw: string => {
            let etype: string = baml.json.get_str(raw, "type") ?? "";

            // ---- REASON #2: server-initiated events with NO triggering request. ----
            // rate_limits.updated / speech_started / a mid-session error / session.updated
            // are unsolicited pushes. A request/response shape has NOWHERE to put them;
            // here they are just frames the duplex loop dispatches to the caller's io.
            if (etype == "input_audio_buffer.speech_started") { io.send(SpeechStartedOut {}); }
            if (etype == "rate_limits.updated") { io.send(RateLimitOut { raw: raw }); }
            if (etype == "error") { io.send(ServerErrorOut { message: baml.json.get_str(raw, "error.message") ?? "" }); }

            // model audio out -> caller's speaker (the OTHER half of the duplex).
            if (etype == "response.audio.delta") {
              io.send(AudioOutEvent { pcm: baml.json.get_str(raw, "delta") ?? "" });
            }

            // ---- REASON #4: server-authoritative state. ----
            // The conversation lives on the server as an ordered item list. We never
            // hold the canonical copy; conversation.item.created tells us what the
            // SERVER now believes the state is. We append to a LOCAL mirror for the
            // returned record, but the server's copy is authoritative.
            if (etype == "conversation.item.created") {
              transcript = append_server_item(transcript, raw);
            }

            // a response (one of the N) completed. Loop continues — more turns may come.
            if (etype == "response.done") {
              transcript = append_completed_response(transcript, raw);
            }
          }
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  // Realtime requires Provider, so we MUST supply call. Open Question 1 made
  // concrete: a "best-effort single turn over the connection". We make it honest by
  // dialing, sending one prompt, draining to the first response.done, and closing.
  // It works, but it throws away everything that makes the provider a realtime one
  // (no io, no barge-in, no duplex) — see evaluation.md (c).
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let req = baml.http.Request {
        method: "GET", url: "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview",
        headers: { "authorization": "Bearer " + self.api_key, "openai-beta": "realtime=v1" }, body: "",
      };
      let ws: WsSocket = baml.ws.connect(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "dial failed" }; } };
      ws.send(self.initial_prompt_frame(prompt)) catch (e) { _ => { throw baml.errors.LlmClient { message: "seed failed" }; } };
      ws.send(self.response_create_text_frame()) catch (e) { _ => { throw baml.errors.LlmClient { message: "create failed" }; } };
      let acc: string = "";
      while (true) {
        let frame: string? = ws.recv() catch (e) { _ => { throw baml.errors.LlmClient { message: "recv failed" }; } };
        match (frame) {
          null => { ws.close(); throw baml.errors.LlmClient { message: "closed before response.done" }; }
          let raw: string => {
            if ((baml.json.get_str(raw, "type") ?? "") == "response.text.delta") {
              acc = acc + (baml.json.get_str(raw, "delta") ?? "");
            }
            if ((baml.json.get_str(raw, "type") ?? "") == "response.done") {
              ws.close();
              return baml.sap.parse<T>(acc);
            }
          }
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  // host-boundary frame encoders — the divergent wire grammar stays in the provider.
  function session_update_frame(self) -> string throws baml.errors.LlmClient { $rust_io_function }
  function initial_prompt_frame(self, prompt: baml.llm.PromptAst) -> string throws baml.errors.LlmClient { $rust_io_function }
  function response_create_text_frame(self) -> string { $rust_io_function }
  function reconfigure_frame(self, c: ControlEvent) -> string throws baml.errors.LlmClient { $rust_io_function }
}


// =============================================================================
// 6. REALTIME OVER WEBRTC. Transport = WebRtc, state = ServerSession.
//
//    Background §"The WebRTC media-vs-data split". SAME Realtime capability, SAME
//    JSON event grammar as §5 — but the connection SPLITS in two: a DATA plane
//    (events) and a MEDIA plane (RTP audio the events never carry). Setup is an
//    ephemeral key + SDP offer/answer, not a header on a socket upgrade.
//
//    The teaching point: capability (Realtime) is identical; ONLY the transport
//    differs. The data-plane code is byte-for-byte the §5 event loop. So Realtime
//    really is "one event grammar across two transports" — exactly what the
//    background says the design must allow. The media bytes never appear in BAML
//    values: the host's audio stack owns echo-cancel / jitter / PLC on the track.
// =============================================================================

class RealtimeWebRTC {
  voice: string
  // ephemeral key is minted server-side (NOT a long-lived api_key on the client).
  ephemeral_key_endpoint: string

  implements TransportInfo {
    function wire(self) -> Wire { WebRtc }
    function state_location(self) -> StateLocation { ServerSession }   // same state axis as §5
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      // 1. mint the ephemeral key server-side.
      let ek: string = mint_ephemeral_key(self.ephemeral_key_endpoint)
        catch (e) { _ => { throw baml.errors.LlmClient { message: "ephemeral key failed" }; } };

      // 2. SDP offer/answer -> two planes. The host wires the MEDIA plane to the
      //    caller's mic/speaker DIRECTLY (the audio bytes bypass BAML entirely).
      let offer_req = baml.http.Request {
        method: "POST", url: "https://api.openai.com/v1/realtime",
        headers: { "authorization": "Bearer " + ek, "content-type": "application/sdp" }, body: "",
      };
      let rtc: RtcSession = baml.webrtc.connect(offer_req, ek)
        catch (e) { _ => { throw baml.errors.LlmClient { message: "webrtc connect failed" }; } };

      // 3. media plane: hand the track to the caller's hardware. AUDIO NEVER becomes
      //    a BAML value here — no response.audio.delta over WebRTC (background).
      rtc.media.attach_mic();
      rtc.media.attach_speaker();

      // 4. data plane: the SAME duplex event loop as RealtimeWS §5, on rtc.data.
      //    This is the proof that the event vocabulary is transport-independent.
      return drive_realtime_events(rtc.data, prompt, io,
        self.session_update_frame(), self.initial_prompt_frame(prompt));
    }
  }

  // best-effort single turn: text-only over the data plane, no media.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // WebRTC for a single text turn is absurd in practice; we degrade to the same
      // text drain as §5 over the data channel. Honest but pointless — flagged.
      $rust_io_function
    }
  }

  function session_update_frame(self) -> string throws baml.errors.LlmClient { $rust_io_function }
  function initial_prompt_frame(self, prompt: baml.llm.PromptAst) -> string throws baml.errors.LlmClient { $rust_io_function }
}


// -----------------------------------------------------------------------------
// 7. The shared duplex event loop — proof that Realtime is one capability, two wires.
//
// Both RealtimeWS (over baml.ws) and RealtimeWebRTC (over rtc.data) drive the SAME
// frame grammar. We factor the loop so the divergence is ONLY "which socket", never
// "which events". This is the background's "one event grammar across WS and WebRTC".
// -----------------------------------------------------------------------------

function drive_realtime_events(
  ws: WsSocket,                      // baml.ws socket OR the WebRTC data channel
  prompt: baml.llm.PromptAst,
  io: Channel,
  session_frame: string,
  prompt_frame: string,
) -> Transcript throws baml.errors.LlmClient {
  ws.send(session_frame) catch (e) { _ => { throw baml.errors.LlmClient { message: "session.update failed" }; } };

  io.on(ev => {
    match (ev) {
      let a: AudioInEvent     => { ws.send(append_audio_frame(a.pcm)) catch (e) { _ => {} }; }
      let b: BargeInEvent     => {
        ws.send(cancel_frame()) catch (e) { _ => {} };
        ws.send(truncate_frame(b.item_id, b.audio_ms)) catch (e) { _ => {} };
      }
      let t: ToolResultInEvent => { ws.send(function_call_output_frame(t.call_id, t.output)) catch (e) { _ => {} }; }
      let c: ControlEvent      => { ws.send(control_frame(c)) catch (e) { _ => {} }; }
    }
  });

  ws.send(prompt_frame) catch (e) { _ => { throw baml.errors.LlmClient { message: "seed failed" }; } };

  let transcript: Transcript = empty_transcript();
  while (true) {
    let frame: string? = ws.recv() catch (e) { _ => { throw baml.errors.LlmClient { message: "recv failed" }; } };
    match (frame) {
      null => { return transcript; }                               // state loss
      let raw: string => {
        let etype: string = baml.json.get_str(raw, "type") ?? "";
        if (etype == "input_audio_buffer.speech_started") { io.send(SpeechStartedOut {}); }
        if (etype == "rate_limits.updated") { io.send(RateLimitOut { raw: raw }); }
        if (etype == "error") { io.send(ServerErrorOut { message: baml.json.get_str(raw, "error.message") ?? "" }); }
        if (etype == "response.audio.delta") { io.send(AudioOutEvent { pcm: baml.json.get_str(raw, "delta") ?? "" }); }
        if (etype == "conversation.item.created") { transcript = append_server_item(transcript, raw); }
        if (etype == "response.done") { transcript = append_completed_response(transcript, raw); }
      }
    }
  }
  baml.sys.panic("unreachable")
}


// -----------------------------------------------------------------------------
// 8. The duplex event grammar (NET-NEW): the In/Out events on the Channel.
//
// The conventions' Channel speaks InEvent (caller -> provider) and OutEvent
// (provider -> caller). This scenario PINS those unions, because the four cliff
// use cases ARE specific InEvent variants. They have no analogue on a Stream — a
// Stream is pull-only, server->client. That asymmetry is the whole point.
// -----------------------------------------------------------------------------

// InEvent: things the CALLER pushes into the live session. Each is a client->server
// action that a one-shot call has no place for.
type InEvent = AudioInEvent | BargeInEvent | ToolResultInEvent | ControlEvent

class AudioInEvent      { pcm: string }                              // mic audio up
class BargeInEvent      { item_id: string, audio_ms: int }           // cliff #1: interrupt now
class ToolResultInEvent { call_id: string, output: unknown }         // cliff #2: mid-stream injection
class ControlEvent      { key: string, value: string }              // cliff #3: side-channel command

// OutEvent: things the PROVIDER pushes to the caller. The server-initiated ones
// (cliff #4) are unsolicited — no triggering request.
type OutEvent = AudioOutEvent | SpeechStartedOut | RateLimitOut | ServerErrorOut

class AudioOutEvent   { pcm: string }                                // model audio down
class SpeechStartedOut {}                                            // cliff #4: server-initiated
class RateLimitOut    { raw: string }                                // cliff #4: server-initiated
class ServerErrorOut  { message: string }                           // cliff #4: mid-session error


// -----------------------------------------------------------------------------
// 9. A combinator that respects the transport/capability split.
//
// A `WarmThenCold` fallback: prefer the warm-socket ResponsesWS for latency, fall
// back to plain ChatHTTP if the socket is capped/dead. BOTH are request/response
// (`call` + `Streaming`), so this fallback is HONEST — it forwards capabilities its
// members actually share. Critically it does NOT claim Realtime, because neither
// member is Realtime. Contrast the proposal's generic Fallback (§6), which claims
// EVERY capability and degrades at call time; here we hand-build a fallback that
// only forwards what's actually shared, to make the transport boundary a compile-ish line.
// -----------------------------------------------------------------------------

class WarmThenCold {
  warm: Provider         // ResponsesWS — warm socket
  cold: Provider         // ChatHTTP — the floor

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let r: T = self.warm.call<T>(prompt) catch (e) {
        _ => { return self.cold.call<T>(prompt); }    // socket capped/dead -> cold POST
      };
      return r;
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      match (self.warm) {
        let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
        _               => {},
      }
      match (self.cold) {
        let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
        _               => { throw baml.errors.LlmClient { message: "no streaming member" }; },
      }
    }
  }
  // DELIBERATELY no `implements Realtime`: a warm/cold HTTP fallback is not a live
  // session and must not pretend to be. This is the "transport boundary as a type
  // line" the proposal can express only by hand-restricting the combinator.
}


// -----------------------------------------------------------------------------
// 10. host-boundary frame builders + transcript helpers used above.
// -----------------------------------------------------------------------------

// the Realtime Transcript is provider-owned + opaque (proposal §7 / conventions).
class Transcript { _items: $rust_type }
function empty_transcript() -> Transcript { Transcript { _items: $rust_io_function } }
function append_server_item(t: Transcript, raw: string) -> Transcript throws baml.errors.LlmClient { $rust_io_function }
function append_completed_response(t: Transcript, raw: string) -> Transcript throws baml.errors.LlmClient { $rust_io_function }

// frame encoders — opaque-string JSON events. Divergent grammar stays host-side.
function append_audio_frame(pcm: string) -> string { $rust_io_function }
function cancel_frame() -> string { $rust_io_function }
function truncate_frame(item_id: string, audio_ms: int) -> string { $rust_io_function }
function function_call_output_frame(call_id: string, output: unknown) -> string throws baml.errors.LlmClient { $rust_io_function }
function control_frame(c: ControlEvent) -> string throws baml.errors.LlmClient { $rust_io_function }

// WebRTC ephemeral-key mint (server-side).
function mint_ephemeral_key(endpoint: string) -> string throws baml.errors.Io { $rust_io_function }
