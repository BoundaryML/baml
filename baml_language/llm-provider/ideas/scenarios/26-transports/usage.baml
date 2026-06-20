// =============================================================================
// Scenario 26 — Transport taxonomy
// usage.baml  —  END-USER / application-author code
//
// What the app author writes to pick a transport, and what they CANNOT write —
// which is the point. Transport is chosen by which provider the `client` returns;
// capability is chosen by which companion (.call / .stream / .live) you reach for.
// The two are independent, and the type system stops you from crossing the cliff.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. The clients: each one names ONE provider == ONE transport. Config only.
//    Note how little changes between rows of the taxonomy — same shape, different
//    provider — and how the two `wss://` providers (ResponsesWS, RealtimeWS) look
//    almost identical here yet are NOT interchangeable (different capabilities).
// -----------------------------------------------------------------------------

client Batch() {                                   // Patience regime: the floor
  ChatHTTP { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

client Reading() {                                 // Reading regime: SSE token stream
  ChatSSE { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

client WarmAgent() {                               // warm socket, SAME call semantics
  ResponsesWS.dial("gpt-5", env.OPENAI_API_KEY)    // host-pooled warm socket
}

client Voice() {                                   // Conversational regime: duplex audio
  RealtimeWS { voice: "alloy", api_key: env.OPENAI_API_KEY }
}

client BrowserVoice() {                            // Conversational regime: WebRTC, media split off
  RealtimeWebRTC { voice: "alloy", ephemeral_key_endpoint: "/api/ephemeral-key" }
}

// A latency optimization that is purely orchestration: warm socket, cold fallback.
// Both members are request/response, so this is an HONEST fallback (no Realtime).
client ResilientAgent() {
  WarmThenCold {
    warm: ResponsesWS.dial("gpt-5", env.OPENAI_API_KEY),
    cold: ChatHTTP { model: "gpt-4o", api_key: env.OPENAI_API_KEY },
  }
}


// -----------------------------------------------------------------------------
// 2. The same EXTRACTION function over four request/response transports.
//
//    THE PORTABILITY CLAIM: Batch (HTTP), Reading (SSE), WarmAgent (warm WS) all
//    answer the identical `Extract.call` — because they all implement `HttpProvider`
//    (which OWNS `call`/`call_with`; the `Provider` marker carries no interaction). The
//    app author writes ONE function body; swapping `client:` swaps the wire with
//    ZERO code change. This is transport-is-provider, proven.
// -----------------------------------------------------------------------------

class Resume { name: string, skills: string[] }

function Extract(text: string) -> Resume {
  client: Batch()                                  // swap to Reading() or WarmAgent(): no body change
  prompt #"Extract a resume from: {{ text }}"#
}

// streaming the SAME extraction — degrades to .call on Batch (no Streaming), runs
// incrementally on Reading / WarmAgent. The cliff has NOT been crossed: streaming
// is richer DELIVERY of the same call, not a duplex interaction.
function ExtractLive(text: string) -> Resume {
  client: Reading()
  prompt #"Extract a resume from: {{ text }}"#
}
//   ExtractLive.stream("...")  -> baml.llm.Stream<Partial<Resume>, Resume>


// -----------------------------------------------------------------------------
// 3. A live voice agent — the duplex side of the cliff.
//
//    `VoiceChat.live(system, io)` is the ONLY way to reach barge-in / mid-stream
//    injection / side-channel / server events. The io: Channel is handed in at the
//    call (pass-in, proposal §3), NEVER a client field. The function body is
//    config + prompt; the duplex endpoint arrives at the realtime companion.
// -----------------------------------------------------------------------------

function VoiceChat(system: string) -> Transcript {
  client: Voice()                                  // swap to BrowserVoice(): same code, WebRTC instead
  prompt #"{{ system }}"#
}
//   VoiceChat.live("be a terse barista", io)  ->  RealtimeWS.run(prompt, io)

// The application's duplex endpoint. THIS is where the four cliff use cases are
// pushed in, and where server-initiated events come out. None of these have a
// home on a Stream — they only exist because `io` is a duplex Channel.
function run_barista_session(system: string) -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  let io: Channel = open_local_audio_channel();    // the app's mic+speaker endpoint

  // server -> caller: handle the model's audio + the UNSOLICITED server events.
  io.on(out => {
    match (out) {
      let a: AudioOutEvent    => { play_to_speaker(a.pcm); },
      let s: SpeechStartedOut => {
        // cliff #1 trigger: the moment the USER starts talking, we barge in. We push
        // a client->server BargeInEvent back up the SAME live session.
        io.send(BargeInEvent { item_id: current_response_item(), audio_ms: spoken_ms() });
      },
      let r: RateLimitOut     => { log_rate_limit(r.raw); },
      let e: ServerErrorOut   => { surface_error(e.message); },
    }
  });

  // drive the duplex session to completion; returns the provider-owned Transcript.
  // Consumer catch, most-specific first: the realtime-domain failure the app can
  // act on, then the universal escape hatch. (SocketClosed lands in RealtimeError.)
  VoiceChat.live(system, io) catch (e) {
    let r: baml.errors.RealtimeError => {
      if (r.is_network_error()) { surface_error("session dropped"); throw r; }
      else { throw r; }
    }
    let u: baml.UnknownError => { surface_error(u.message); throw u; }
  }
} catch (e) {
  // open_local_audio_channel's foreign Io normalizes onto the Realtime channel.
  _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "barista session setup failed");
}


// -----------------------------------------------------------------------------
// 4. The cliff is enforced by the companion, not by hope.
//
//    These two functions look nearly identical. The first compiles AND runs. The
//    second compiles (client returns the existential Provider) but THROWS at the
//    .live call, because Batch()'s ChatHTTP does not implement Realtime — there is
//    no honest fallback for a duplex session (proposal §5). The model lets you
//    *write* the crossing, then refuses it at runtime. (See evaluation.md (c).)
// -----------------------------------------------------------------------------

function valid_live(system: string, io: Channel) -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  VoiceChat.live(system, io)                        // Voice() is Realtime -> runs
}

function invalid_live(text: string, io: Channel) -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  // Extract's client is Batch() == ChatHTTP. ChatHTTP is HttpProvider only.
  // .live matches for Realtime, misses, and the `_` arm throws baml.errors.Unsupported.
  // Unsupported implements EVERY capability-error interface, so it is legal on this
  // RealtimeError channel and the app catches it by its concrete type below.
  // A request/response transport CANNOT fake a live session — by design.
  Extract.live(text, io) catch (e) {
    let u: baml.errors.Unsupported => { throw u; }   // surfaced as-is: not a transport failure
    let r: baml.errors.RealtimeError => { throw r; } // any realtime-domain failure
    let x: baml.UnknownError         => { throw x; } // the escape hatch
  }
}


// -----------------------------------------------------------------------------
// 5. Reading the OTHER axis at runtime: state-location is NOT the transport.
//
//    Both WarmAgent (ResponsesWS) and Voice (RealtimeWS) are on a `wss://` socket
//    (wire == WebSocket). But their state location differs: ServerStoredById vs
//    ServerSession. You CANNOT read "who owns the conversation" off "is it a socket".
//    The TransportInfo read-interface exposes both coordinates independently.
// -----------------------------------------------------------------------------

function describe_transport(p: Provider) -> string {
  match (p) {
    let info: TransportInfo => {
      let w: string = match (info.wire()) {
        Http => "http", HttpSse => "http+sse", HttpChunkedJson => "http+chunked-json",
        WebSocket => "websocket", WebRtc => "webrtc",
      };
      let s: string = match (info.state_location()) {
        ClientOwned => "client-owned", ServerStoredById => "server-stored-by-id",
        ServerSession => "server-session",
      };
      // e.g. ResponsesWS -> "websocket / server-stored-by-id"
      //      RealtimeWS  -> "websocket / server-session"   (SAME wire, DIFFERENT state)
      w + " / " + s
    },
    _ => "unknown transport",
  }
}


// -----------------------------------------------------------------------------
// 6. host-boundary helpers the app uses to wire its own audio I/O.
// -----------------------------------------------------------------------------

function open_local_audio_channel() -> Channel throws baml.errors.Io { $rust_io_function }
function play_to_speaker(pcm: string) -> null { $rust_io_function }
function current_response_item() -> string { $rust_io_function }
function spoken_ms() -> int { $rust_io_function }
function log_rate_limit(raw: string) -> null { $rust_io_function }
function surface_error(message: string) -> null { $rust_io_function }
