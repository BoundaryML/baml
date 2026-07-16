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


// -----------------------------------------------------------------------------
// 1b. The concrete transport errors these providers deliberately raise.
//
// Under the new error model there is no `type Error` / `baml.errors.LlmClient`.
// Each capability method declares `baml.ExtendUnknownError<CapErr>`; a deliberate
// domain failure is a CONCRETE class implementing the capability interface(s).
//
// A warm `wss://` socket can drop a `SocketClosed` on the request/response path of
// `ResponsesWS` (an HttpProvider whose call_with / stream both ride one socket) AND a
// duplex `wss://`/WebRTC socket can drop it mid-session (inside `run` /
// `drive_realtime_events`), so it implements CallError, StreamError AND RealtimeError.
// NOTE (provider-as-marker): its CallError impl is load-bearing because of the GENUINE
// request/response warm socket (ResponsesWS) — NOT because of a dishonest realtime
// `call`. Realtime providers no longer carry `call` at all, so OQ1's "this CallError is
// dead weight" complaint is gone. A plain HTTP non-2xx is call/stream only.
// -----------------------------------------------------------------------------

// A non-2xx HTTP status from a request/response transport. Classified: it is
// neither a network failure nor a rate limit nor a parse error per se — it is a
// transport-domain status the app may branch on (e.g. 401 vs 503).
class TransportHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return false; }
  }
}

// The socket closed before the turn / session reached a terminal frame. This is a
// genuine domain failure: connection loss = state loss (Reason #5). It can surface on
// the request/response path of the WARM socket (ResponsesWS.call_with), mid-stream
// (ResponsesWS.stream), and mid-session (Realtime.run), so it implements all three.
class SocketClosed {
  detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return true; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return true; }
  }
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return true; }
  }
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function                              // response_format = json_schema of T
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp = baml.http.send(request);
      if (!resp.ok()) { throw TransportHttpError { status: resp.status_code }; }   // non-2xx → typed
      return resp;
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "chathttp send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "chathttp parse failed");
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp = baml.http.send(request);
      if (!resp.ok()) { throw TransportHttpError { status: resp.status_code }; }   // non-2xx → typed
      return resp;
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "chatsse send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "chatsse parse failed");
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError> {
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
//    implements `HttpProvider` (call/call_with) and `Streaming` — the SAME
//    capabilities as ChatSSE. Under provider-as-marker, `call`/`call_with` live on
//    `HttpProvider`; the `Provider` marker carries no interaction.
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

  // ★ It implements HttpProvider — request/response on a warm wire — NOT Realtime.
  // Under provider-as-marker, `call`/`call_with` live on HttpProvider (the marker
  // `Provider` carries no interaction). "One in-flight response per connection" maps
  // exactly onto call's 1:1 contract: one response.create -> one terminal
  // response.completed, then the socket is free for the next turn. The warm socket is
  // request/response, just on a persistent wire — so this provider is genuinely an
  // HttpProvider whose transport happens to be a `wss://` upgrade, not an HTTP POST.
  //
  // We DON'T use the build_request/send/parse/meta_of codec default here (the warm
  // socket has no single Request and no single Response Body — that is OQ3, flagged in
  // evaluation.md). Instead we override call_with directly with the hand-rolled drain,
  // and derive `call` from it (project nothing). call_with is the PRIMITIVE; the late
  // usage/finish-reason on response.completed rides the projected ResponseMeta.
  implements HttpProvider {
    type Body = baml.http.Response   // unused by this override; the warm drain is hand-rolled

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Borrow the warm socket from the host pool (dials lazily, guards one-in-flight).
      let ws: WsSocket = self.warm_socket();
      // First (or continuation) turn: a normal Responses body wrapped as a
      // response.create event, carrying previous_response_id for the warm cache.
      // The host pool supplies the last id so each turn chains the warm cache.
      let body: string = self.encode_response_create<T>(prompt, self.last_id());
      ws.send(body);

      // Drain server->client events until response.completed. This is a 1:1
      // request/response loop *over* a duplex socket — we never send mid-response,
      // which is exactly why this is `call_with` and not Realtime.
      let acc: string = "";
      while (true) {
        let frame: string? = ws.recv();
        match (frame) {
          null => { throw SocketClosed { detail: "socket closed mid-response" }; }   // typed CallError
          let raw: string => {
            // Decode the frame into a typed event class (partial — unknown fields ignored).
            let ev: ResponsesWsFrame = baml.json.from_string<ResponsesWsFrame>(raw);
            let etype: string = ev.type ?? "";
            if (etype == "response.output_text.delta") {
              acc = acc + (ev.delta ?? "");
            }
            if (etype == "response.completed") {
              // record the id in the host pool so the NEXT call chains the warm cache.
              let rid: string = match (ev.response) { null => "", let r: ResponsesWsResponse => r.id ?? "" };
              self.remember_id(rid);
              // late sidecar (usage/finish-reason) lands on the terminal frame:
              let meta: ResponseMeta = ResponsesWsMeta { raw: raw };
              return (baml.sap.parse<T>(acc), project(meta));
            }
          }
        }
      }
      baml.sys.panic("unreachable")
    } catch (e) {
      // Foreign Io from ws.send/ws.recv (and any host failure) normalizes here;
      // the deliberate SocketClosed passes through unchanged.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "responsesws call failed");
    }

    // `call` derives from call_with — project nothing, drop the sidecar (spine default,
    // restated here because we overrode call_with rather than the codec primitives).
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  implements Streaming {
    // Same warm socket, but yield deltas as they arrive. Still server->client only
    // within a turn — a delivery refinement, identical capability to ChatSSE.
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError> {
      $rust_io_function
    }
  }
  // NO `implements Realtime`: there is no duplex, no barge-in, no audio. The socket
  // is a latency trick, not an interaction shape.

  // Pool accessors — host-backed, like MemoryStore's store methods. The lifecycle
  // (lazy dial, ~60-min cap, one-in-flight guard) lives in the host, NOT in a
  // mutable BAML field.
  function warm_socket(self) -> WsSocket throws baml.ExtendUnknownError<baml.errors.CallError> { $rust_io_function }
  function last_id(self) -> string? { $rust_io_function }
  function remember_id(self, id: string) -> null { $rust_io_function }
  function encode_response_create<T>(self, prompt: baml.llm.PromptAst, prev: string?)
      -> string throws baml.ExtendUnknownError<baml.errors.CallError> { $rust_io_function }
}

// The warm-socket sidecar: usage/finish-reason normalized LAZILY off the terminal
// response.completed frame. Same ResponseMeta shape as OpenAIResponse in the spine —
// the warm socket is request/response, so it has a perfectly ordinary call sidecar.
class ResponsesWsMeta {
  raw: string                                          // the response.completed frame JSON
  implements ResponseMeta {
    function usage(self) -> Usage {
      // Decode the terminal frame into the typed event class and read the nested usage.
      let ev: ResponsesWsFrame = baml.json.from_string<ResponsesWsFrame>(self.raw);
      match (ev.response) {
        null => Usage { input: 0, output: 0 },
        let r: ResponsesWsResponse => match (r.usage) {
          null => Usage { input: 0, output: 0 },
          let u: ResponsesWsUsage => Usage { input: u.input_tokens ?? 0, output: u.output_tokens ?? 0 },
        },
      }
    }
    function finish_reason(self) -> string {
      let ev: ResponsesWsFrame = baml.json.from_string<ResponsesWsFrame>(self.raw);
      match (ev.response) { null => "", let r: ResponsesWsResponse => r.status ?? "" }
    }
    function logprobs(self)      -> Supported<Logprob[]> { Unsupported { reason: "responses ws" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "responses ws" } }
    function reasoning(self)     -> Supported<string>     { Unsupported { reason: "responses ws" } }
  }
}

// Typed shapes for the warm-socket frames `ResponsesWS` decodes (partial — only the
// fields these methods read; SAP's structural decode ignores the rest of the frame).
class ResponsesWsFrame    { type: string?, delta: string?, response: ResponsesWsResponse? }
class ResponsesWsResponse { id: string?, status: string?, usage: ResponsesWsUsage? }
class ResponsesWsUsage    { input_tokens: int?, output_tokens: int? }


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
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      let req = baml.http.Request {
        method: "GET",
        url: "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview",
        headers: { "authorization": "Bearer " + self.api_key, "openai-beta": "realtime=v1" },
        body: "",
      };
      let ws: WsSocket = baml.ws.connect(req);

      // configure the server-authoritative session (voice, modalities, VAD).
      ws.send(self.session_update_frame());

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
      ws.send(self.initial_prompt_frame(prompt));

      // ---- the receive loop: REASONS #2, #3, #4 all live here. ----
      // It accumulates the provider-owned Transcript. N user turns, each yielding 0/1/
      // several responses -> there is no single "the response". (REASON #3: 1:N.)
      let transcript: Transcript = empty_transcript();
      while (true) {
        let frame: string? = ws.recv();
        match (frame) {
          // ---- REASON #5: connection loss = state loss. ----
          // null = socket closed. There is NO session id to resume; the server-held
          // conversation is gone. We return whatever Transcript we accumulated and
          // surface the loss — we cannot reconnect to "the same session".
          null => { return transcript; }
          let raw: string => {
            // Decode the frame into the shared typed realtime event class.
            let ev: RealtimeFrame = baml.json.from_string<RealtimeFrame>(raw);
            let etype: string = ev.type ?? "";

            // ---- REASON #2: server-initiated events with NO triggering request. ----
            // rate_limits.updated / speech_started / a mid-session error / session.updated
            // are unsolicited pushes. A request/response shape has NOWHERE to put them;
            // here they are just frames the duplex loop dispatches to the caller's io.
            if (etype == "input_audio_buffer.speech_started") { io.send(SpeechStartedOut {}); }
            if (etype == "rate_limits.updated") { io.send(RateLimitOut { raw: raw }); }
            if (etype == "error") {
              let msg: string = match (ev.error) { null => "", let e: RealtimeErrorBody => e.message ?? "" };
              io.send(ServerErrorOut { message: msg });
            }

            // model audio out -> caller's speaker (the OTHER half of the duplex).
            if (etype == "response.audio.delta") {
              io.send(AudioOutEvent { pcm: ev.delta ?? "" });
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
    } catch (e) {
      // Foreign Io from baml.ws.connect / .send / .recv (and any host failure)
      // normalizes onto the Realtime channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "realtimews run failed");
    }
  }

  // Under provider-as-marker, RealtimeWS does NOT implement `call`. `Realtime requires
  // Provider` (the bare marker), and the marker carries no interaction method — so the
  // class IS a Provider purely by being composable, and exposes ONLY its real
  // interaction (`run`, above). The old "best-effort single turn" (dial a fresh socket,
  // send one prompt, drain to the first response.done, close) is GONE: it threw away
  // everything that makes the provider realtime (no io, no barge-in, no duplex) and was
  // the concrete form of the now-RESOLVED OQ1 footgun. A function over a realtime client
  // simply has no `.call` companion; `Foo.live(args, io)` matches `Realtime` (no degrade).

  // host-boundary frame encoders — the divergent wire grammar stays in the provider.
  // These are pure host-side JSON encoders (a genuine host boundary), so they throw
  // the FOREIGN host error `baml.errors.Io`; `run` (the Realtime channel) normalizes
  // that Io into `baml.UnknownError` in its trailing catch.
  function session_update_frame(self) -> string throws baml.errors.Io { $rust_io_function }
  function initial_prompt_frame(self, prompt: baml.llm.PromptAst) -> string throws baml.errors.Io { $rust_io_function }
  function reconfigure_frame(self, c: ControlEvent) -> string throws baml.errors.Io { $rust_io_function }
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
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      // 1. mint the ephemeral key server-side.
      let ek: string = mint_ephemeral_key(self.ephemeral_key_endpoint);

      // 2. SDP offer/answer -> two planes. The host wires the MEDIA plane to the
      //    caller's mic/speaker DIRECTLY (the audio bytes bypass BAML entirely).
      let offer_req = baml.http.Request {
        method: "POST", url: "https://api.openai.com/v1/realtime",
        headers: { "authorization": "Bearer " + ek, "content-type": "application/sdp" }, body: "",
      };
      let rtc: RtcSession = baml.webrtc.connect(offer_req, ek);

      // 3. media plane: hand the track to the caller's hardware. AUDIO NEVER becomes
      //    a BAML value here — no response.audio.delta over WebRTC (background).
      rtc.media.attach_mic();
      rtc.media.attach_speaker();

      // 4. data plane: the SAME duplex event loop as RealtimeWS §5, on rtc.data.
      //    This is the proof that the event vocabulary is transport-independent.
      return drive_realtime_events(rtc.data, prompt, io,
        self.session_update_frame(), self.initial_prompt_frame(prompt));
    } catch (e) {
      // Foreign Io from mint_ephemeral_key / baml.webrtc.connect (and any host
      // failure) normalizes onto the Realtime channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "realtimewebrtc run failed");
    }
  }

  // Under provider-as-marker, RealtimeWebRTC does NOT implement `call`. The old
  // "best-effort single turn" here was actively absurd — spin up an ephemeral key +
  // SDP/ICE negotiation just to read one text answer. `Realtime requires Provider` (the
  // marker), so the class IS a Provider with NO fabricated call; it exposes only `run`.
  // The most acute form of the now-RESOLVED OQ1 footgun is simply deleted.

  function session_update_frame(self) -> string throws baml.errors.Io { $rust_io_function }
  function initial_prompt_frame(self, prompt: baml.llm.PromptAst) -> string throws baml.errors.Io { $rust_io_function }
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
) -> Transcript throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  ws.send(session_frame);

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

  ws.send(prompt_frame);

  let transcript: Transcript = empty_transcript();
  while (true) {
    let frame: string? = ws.recv();
    match (frame) {
      null => { return transcript; }                               // state loss
      let raw: string => {
        let ev: RealtimeFrame = baml.json.from_string<RealtimeFrame>(raw);
        let etype: string = ev.type ?? "";
        if (etype == "input_audio_buffer.speech_started") { io.send(SpeechStartedOut {}); }
        if (etype == "rate_limits.updated") { io.send(RateLimitOut { raw: raw }); }
        if (etype == "error") {
          let msg: string = match (ev.error) { null => "", let e: RealtimeErrorBody => e.message ?? "" };
          io.send(ServerErrorOut { message: msg });
        }
        if (etype == "response.audio.delta") { io.send(AudioOutEvent { pcm: ev.delta ?? "" }); }
        if (etype == "conversation.item.created") { transcript = append_server_item(transcript, raw); }
        if (etype == "response.done") { transcript = append_completed_response(transcript, raw); }
      }
    }
  }
  baml.sys.panic("unreachable")
} catch (e) {
  // Foreign Io from ws.send/ws.recv (and the transcript helpers' host failures)
  // normalizes onto the Realtime channel — the one error home for both transports.
  _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "realtime event loop failed");
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

// Typed shape for the raw server->client realtime frames the event loops decode.
// Partial — only the fields the loops read (`type`, `delta`, `error.message`); SAP's
// structural decode ignores the rest of each frame's wire grammar.
class RealtimeFrame     { type: string?, delta: string?, error: RealtimeErrorBody? }
class RealtimeErrorBody { message: string? }


// -----------------------------------------------------------------------------
// 9. A combinator that respects the transport/capability split.
//
// A `WarmThenCold` fallback: prefer the warm-socket ResponsesWS for latency, fall
// back to plain ChatHTTP if the socket is capped/dead. BOTH are request/response
// (`HttpProvider` — call/call_with — + `Streaming`), so this fallback is HONEST: it
// forwards capabilities its members actually share. Critically it does NOT claim
// Realtime, because neither member is Realtime. Contrast the proposal's generic Fallback (§6), which claims
// EVERY capability and degrades at call time; here we hand-build a fallback that
// only forwards what's actually shared, to make the transport boundary a compile-ish line.
// -----------------------------------------------------------------------------

class WarmThenCold {
  warm: Provider         // ResponsesWS — warm socket
  cold: Provider         // ChatHTTP — the floor

  // Combinators declare the SAME channel as the capability they forward — no
  // narrow / widen / forward. The warm member's CallError (or UnknownError) is
  // recovered at the consumer's catch, not re-typed here. Under provider-as-marker,
  // request/response forwarding lives on HttpProvider (call + call_with), inherited
  // factories (with_retry/fallback_to/cached) ride the `Provider` marker via `requires`.
  implements HttpProvider {
    type Body = unknown                                // routed to a member; this combinator has no codec

    // call_with is the PRIMITIVE — forward the projection to whichever member serves the
    // turn, so usage/finish-reason/cost are NOT dropped (the P1 fix). Both members are
    // HttpProvider here (ResponsesWS + ChatHTTP), so each `.call_with` carries its own
    // ResponseMeta; the warm member's sidecar is preferred, the cold member's on fallback.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.warm) {
        let h: HttpProvider => {
          let r: (T, U) = h.call_with<T, U>(prompt, project) catch (e) {
            _ => {                                      // socket capped/dead -> cold POST
              match (self.cold) {
                let c: HttpProvider => { return c.call_with<T, U>(prompt, project); },
                _                   => { throw baml.errors.Unsupported { message: "cold member is not http" }; },
              }
            }
          };
          return r;
        },
        _ => {
          match (self.cold) {
            let c: HttpProvider => { return c.call_with<T, U>(prompt, project); },
            _                   => { throw baml.errors.Unsupported { message: "no http member" }; },
          }
        },
      }
    }

    // `call` derives from call_with — project nothing (drop the sidecar).
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError> {
      match (self.warm) {
        let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
        _               => {},
      }
      match (self.cold) {
        let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
        _               => { throw baml.errors.Unsupported { message: "no streaming member" }; },
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
function append_server_item(t: Transcript, raw: string) -> Transcript throws baml.errors.Io { $rust_io_function }
function append_completed_response(t: Transcript, raw: string) -> Transcript throws baml.errors.Io { $rust_io_function }

// frame encoders — opaque-string JSON events. Divergent grammar stays host-side.
function append_audio_frame(pcm: string) -> string { $rust_io_function }
function cancel_frame() -> string { $rust_io_function }
function truncate_frame(item_id: string, audio_ms: int) -> string { $rust_io_function }
function function_call_output_frame(call_id: string, output: unknown) -> string throws baml.errors.Io { $rust_io_function }
function control_frame(c: ControlEvent) -> string throws baml.errors.Io { $rust_io_function }

// WebRTC ephemeral-key mint (server-side).
function mint_ephemeral_key(endpoint: string) -> string throws baml.errors.Io { $rust_io_function }
