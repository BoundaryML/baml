// scenario 26 — transports: ../../../../../llm-provider/ideas/scenarios/26-transports/README.md
//
// LIBRARY / std-lib-author code — the offline fakes the deterministic tier drives.
//
// The design's thesis (implement.baml): TRANSPORT (HTTP / SSE / chunked-JSON / WS /
// WebRTC) is a property of the PROVIDER, not of the call. ONE capability model spans
// every wire; transport is an implementation detail. Two providers speaking the SAME
// capability can sit on different wires, and ONE wire (a `wss://` socket) can carry
// two OPPOSITE capabilities (warm request/response vs duplex Realtime) — so transport
// and capability are ORTHOGONAL axes, and Realtime is its OWN capability, never a
// refinement of `call` (the provider-as-marker payoff: a realtime provider carries no
// `call`).
//
// How that maps onto what actually ships (REALIZED.md §26 — ALL THREE transports are
// real): the request/response path is `HttpProvider` (native `baml.http.send`), the
// SSE token stream is `Streaming` (native `stream_messages` over `baml.http.fetch_sse`),
// and the WebSocket duplex is `Realtime` (native `OpenAiRealtime` over `baml.ws`). One
// LLM function rides all three by swapping only the `client =` — proven offline here
// (four fakes, one per transport SHAPE) and live below (OpenAi HTTP / OpenAi $stream /
// OpenAiRealtime $live).
//
// This file authors the OFFLINE fakes — one provider per transport shape:
//   - s26_HttpTransport     — HTTP request/response (Http / client-owned).  The floor.
//   - s26_StreamTransport   — SSE token stream       (HttpSse / client-owned).
//   - s26_WarmSocketTransport — warm `wss://` request/response (WebSocket / server-stored-by-id).
//   - s26_RealtimeTransport — WebSocket duplex        (WebSocket / server-session).
// plus a `s26_TransportInfo` read-interface (the design's TransportInfo: wire ⊥ state)
// and `s26_SocketClosed`, the one error that rides ALL THREE channels (connection loss =
// state loss — the design's SocketClosed on CallError + StreamError + RealtimeError).
//
// The two `WebSocket`-wire fakes (s26_WarmSocketTransport, s26_RealtimeTransport) are the
// scenario's sharpest claim made concrete: SAME wire, OPPOSITE capability, DIFFERENT
// state-location — you cannot read "is this realtime?" off "is this a socket?".
//
// BLOCKED (net-new host surface the design needed, not on the real offline surface):
//   - Constructing a `baml.llm.Stream` offline (the design's SSE drain) needs the
//     host stream primitives (`PrimitiveClient`/`StreamAccumulator`/`SseStream`), which
//     no pure-BAML fake can build — so s26_StreamTransport.stream_messages THROWS the
//     typed `s26_SocketClosed` (a StreamError) instead of yielding a `Stream`. The
//     degrade ROUTING (drive_call -> the Streaming arm) is still proven offline (the
//     stream-only provider surfaces s26_SocketClosed, NOT `Unsupported`); the
//     value-returning `.final()` degrade is proven LIVE. This mirrors scenarios 04/06.
//   - WebRTC (the design's 5th row) is folded into the WebSocket-duplex fake: the design
//     itself says the WebRTC data-plane event loop is "byte-for-byte the §5 WS loop", so
//     the Realtime capability is identical and only the media plane (P8 host audio)
//     differs — not modelled offline.


// -----------------------------------------------------------------------------
// The transport-vs-state taxonomy, as TYPES (the design's TransportInfo). Transport
// and state-location are ORTHOGONAL: `wire()` says which host primitive the provider
// reaches for; `state_location()` says who owns the conversation — read INDEPENDENTLY,
// never one off the other.
// -----------------------------------------------------------------------------

/// Which wire the provider speaks. (WebRtc kept for completeness; folded into the WS
/// duplex fake offline — same Realtime capability, only the media plane differs.)
enum s26_Wire { Http, HttpSse, WebSocket, WebRtc }

/// Who owns the canonical conversation — the OTHER coordinate, off a field not the wire.
enum s26_State { ClientOwned, ServerStoredById, ServerSession }

/// A purely descriptive read-interface: ask a provider "what wire?" and "where's the
/// state?" WITHOUT a closed provider-kind enum. `requires baml.ai.Provider` so a
/// `match (p) { let info: s26_TransportInfo => ... }` arm binds (a bare interface would
/// bind `never`).
interface s26_TransportInfo requires baml.ai.Provider {
    function wire(self) -> s26_Wire throws never
    function state_location(self) -> s26_State throws never
}


// -----------------------------------------------------------------------------
// The one error that rides ALL THREE transports: connection loss = state loss
// (the design's SocketClosed). A warm socket can drop it on the request/response path
// (CallError), an SSE reader mid-stream (StreamError), and a duplex session mid-turn
// (RealtimeError) — so it implements every channel. `Failure` is mandatory on each.
// -----------------------------------------------------------------------------

class s26_SocketClosed {
    detail: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { true }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.CallError {
        function is_network_error(self) -> bool { true }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
    implements baml.errors.StreamError {
        function is_network_error(self) -> bool { true }
    }
    implements baml.errors.RealtimeError {
        function is_network_error(self) -> bool { true }
    }
}


// =============================================================================
// TRANSPORT 1 — HTTP request/response. Wire = Http, state = ClientOwned. The floor:
// `(request) -> response`, then close — the only shape request/response models
// perfectly. Pure HttpProvider; the base `call` companion routes straight here.
// =============================================================================
class s26_HttpTransport {
    /// The answer this fake returns (SAP-parsed into the caller's `T`).
    reply: string,

    implements baml.ai.Provider {}

    implements s26_TransportInfo {
        function wire(self) -> s26_Wire throws never { s26_Wire.Http }
        function state_location(self) -> s26_State throws never { s26_State.ClientOwned }
    }

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s26 http transport builds no request offline"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s26 http transport does not send offline"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s26 http parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        // The primitive: value + projected sidecar. A 1:1 request/response turn.
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s26 http parse failed"] },
            };
            log.info("26 offline: transport = HTTP request/response (HttpProvider.call)");
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// =============================================================================
// TRANSPORT 2 — SSE token stream. Wire = HttpSse, state = ClientOwned. Streaming is a
// richer DELIVERY of the SAME call — server->client only, no client->server mid-stream
// frame (the SSE "the client's mouth is taped shut" fact). Implements ONLY Streaming.
//
// drive_call DEGRADES to this: `HttpProvider.call` absent -> drain `Streaming.final()`.
// Offline we can't CONSTRUCT a `Stream` (host-gated), so stream_messages throws the
// typed s26_SocketClosed — which proves the degrade ROUTING (the stream-only provider
// reaches the Streaming arm, NOT `Unsupported`). The value-returning `.final()` degrade
// is proven LIVE (see usage.baml).
// =============================================================================
class s26_StreamTransport {
    implements baml.ai.Provider {}

    implements s26_TransportInfo {
        function wire(self) -> s26_Wire throws never { s26_Wire.HttpSse }
        function state_location(self) -> s26_State throws never { s26_State.ClientOwned }
    }

    implements baml.ai.Streaming {
        function stream_messages<TStream, TFinal>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.llm.Stream<TStream, TFinal> throws baml.errors.StreamError {
            log.info("26 offline: transport = SSE token stream (Streaming; drive_call degrades here)");
            // BLOCKED: P8 — a pure-BAML fake cannot build a `baml.llm.Stream` (needs the
            // host PrimitiveClient/StreamAccumulator/SseStream). The degrade routing is
            // still proven: this typed StreamError surfaces via drive_call's Streaming arm.
            throw s26_SocketClosed { detail: "offline SSE cannot construct a Stream; live tier proves .final()" }
        }
    }
}


// =============================================================================
// TRANSPORT 3 — RESPONSES OVER A WARM `wss://` SOCKET. Wire = WebSocket, state =
// ServerStoredById. THE TEACHING POINT: this is a socket, yet it is NOT Realtime — it
// is request/response (HttpProvider) on a persistent wire, one in-flight response per
// connection (the design's ResponsesWS). Same wire technology as TRANSPORT 4, OPPOSITE
// capability. Proof that "is this realtime?" cannot be read off "is this a socket?".
// =============================================================================
class s26_WarmSocketTransport {
    reply: string,

    implements baml.ai.Provider {}

    implements s26_TransportInfo {
        function wire(self) -> s26_Wire throws never { s26_Wire.WebSocket }        // ...a socket, but
        function state_location(self) -> s26_State throws never { s26_State.ServerStoredById }
    }

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s26 warm socket builds no request offline"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s26 warm socket does not send offline"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s26 warm parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        // One `response.create` -> one terminal `response.completed`: call's 1:1
        // contract, just over a persistent wire. We never send mid-response — which is
        // exactly why this is request/response and not Realtime.
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s26 warm parse failed"] },
            };
            log.info("26 offline: transport = warm WebSocket request/response (HttpProvider on a wss:// wire)");
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
    // DELIBERATELY no `implements Realtime`: a warm socket is a latency trick, not an
    // interaction shape. Same wire as TRANSPORT 4, opposite capability.
}


// =============================================================================
// TRANSPORT 4 — REALTIME OVER WEBSOCKET. Wire = WebSocket, state = ServerSession. Same
// wire as TRANSPORT 3, OPPOSITE capability. The model stops being a thing you `call`:
// it implements ONLY Realtime — duplex, server-authoritative, no resume id (connection
// loss = state loss encoded by the ABSENCE of a recoverable handle). The pass-in `io`
// is handed at the `run`/`$live` call, NEVER a client field.
// =============================================================================
class s26_RealtimeTransport {
    /// The line the "model" speaks this session (also its config-only seed).
    reply: string,

    implements baml.ai.Provider {}

    implements s26_TransportInfo {
        function wire(self) -> s26_Wire throws never { s26_Wire.WebSocket }        // ...also a socket, but
        function state_location(self) -> s26_State throws never { s26_State.ServerSession }   // DIFFERENT state axis
    }

    implements baml.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: baml.ai.Channel,
        ) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
            // Session open — the pass-in `io` is the live duplex, handed in at the call.
            log.info("26 offline: transport = WebSocket duplex (Realtime.run), prompt = " + prompt);
            io.send("{\"type\":\"session.created\"}") catch (e) { _ => null };

            // One turn: forward the output-text delta over `io` (the side channel) AND
            // fold it into the durable transcript. Audio deltas are BLOCKED (P8) — text
            // only, exactly what the live OpenAiRealtime tier proves.
            let acc = "";
            io.send("{\"type\":\"response.output_text.delta\",\"delta\":\"" + self.reply + "\"}")
                catch (e) { _ => null };
            acc = acc + self.reply;
            io.send("{\"type\":\"response.done\"}") catch (e) { _ => null };

            log.info("26 offline: transcript tail = " + acc);
            baml.ai.Transcript { text: acc + " <- " + prompt }
        }
    }
    // Under provider-as-marker, s26_RealtimeTransport does NOT implement `call`: a
    // realtime provider IS a Provider (composable, negotiable) WITHOUT faking
    // request/response. `Foo(args)` has no honest degrade to a duplex session.
}


// -----------------------------------------------------------------------------
// A recording `Channel`: captures every raw event the provider forwards over the
// duplex (the app gets one from its host audio I/O and hands it to `$live`). In the
// design this is the host-backed audio channel bound to a mic + speaker — BLOCKED (P8)
// here, so we record the string event feed instead of playing PCM.
// -----------------------------------------------------------------------------
class s26_RecordingChannel {
    events: string[],

    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            null
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            self.events.push(data);
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}
