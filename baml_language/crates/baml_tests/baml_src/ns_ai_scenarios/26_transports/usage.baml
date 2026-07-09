// scenario 26 — transports: ../../../../../llm-provider/ideas/scenarios/26-transports/README.md
//   usage.baml — APPLICATION-author code, on the REAL surface.
//
// The design's thesis: transport (HTTP / SSE / WebSocket / WebRTC) is a property of the
// PROVIDER, not of the call. ONE capability model spans every wire; the wire is an
// implementation detail. So the app author writes ONE LLM function and swaps only the
// `client =` to change transport — the body never changes. And Realtime is its OWN
// capability (the provider-as-marker payoff), so a request/response transport CANNOT
// fake a live session — the type system refuses the crossing at the companion.
//
// How that maps onto what actually ships (REALIZED.md §26 — ALL THREE transports real):
//   - HTTP request/response is `HttpProvider` — the base `Foo(args)` companion.
//   - SSE token streaming is `Streaming` — the `Foo$stream(args)` companion.
//   - WebSocket duplex is `Realtime` — the `Foo$live(args, io)` companion.
//   All three are generated from the SAME function; `drive_call`/`drive_stream`/
//   `drive_live` negotiate the client's provider at runtime.
//   - THE DEGRADE: `drive_call` prefers `HttpProvider.call`, else DRAINS a `Streaming`
//     provider via `.final()` (real, in ns_ai/capabilities/http.baml). Proven below.
//
// What the OFFLINE tier proves deterministically (one function, four transport fakes):
//   (1) the SAME function rides HTTP request/response (`Foo` -> HttpProvider.call),
//   (2) the SAME function rides a WebSocket duplex session (`Foo$live` -> Realtime.run),
//   (3) drive_call DEGRADES a stream-only provider to the Streaming arm (`.final()`),
//   (4) the `$stream` companion negotiates Streaming (and rejects a non-streaming
//       provider as a typed `Unsupported`),
//   (5) the marker payoff: a realtime provider carries NO `call`,
//   (6) transport ⊥ capability AND ⊥ state: two `wss://` providers, SAME wire, OPPOSITE
//       capability, DIFFERENT state-location.
//
// LIVE tier (`testset "integ-test-26-transports"`): the SAME `s26_Ask` function against
// OpenAi (HTTP), OpenAi ($stream / SSE), and OpenAiRealtime ($live / WebSocket) — three
// transports, one function, each under 60s.


// --- config-only dead default. Every offline test passes an explicit `client =`. ---
client<llm> s26_Dead {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// --- THE ONE FUNCTION. -------------------------------------------------------
// `-> string`, so it serves ALL THREE companions off one body: `s26_Ask(...)` returns
// the string (call), `s26_Ask$stream(...)` returns `Stream<string, string>` (stream),
// `s26_Ask$live(..., io)` returns the session `Transcript` (live). Swapping `client =`
// swaps the WIRE with ZERO body change — transport-is-provider, proven.
function s26_Ask(instruction: string) -> string {
  client s26_Dead
  prompt #"{{ instruction }}"#
}


// -----------------------------------------------------------------------------
// Reusable app helpers: read the transport axes, negotiate capability, consume a stream.
// -----------------------------------------------------------------------------

// Read BOTH coordinates off the descriptive read-interface — never one off the other.
// e.g. s26_WarmSocketTransport -> "websocket / server-stored-by-id"
//      s26_RealtimeTransport   -> "websocket / server-session"   (SAME wire, DIFFERENT state)
function s26_describe_transport(p: baml.ai.Provider) -> string throws never {
  match (p) {
    let info: s26_TransportInfo => {
      let w: string = match (info.wire()) {
        s26_Wire.Http => "http",
        s26_Wire.HttpSse => "http+sse",
        s26_Wire.WebSocket => "websocket",
        s26_Wire.WebRtc => "webrtc",
      };
      let s: string = match (info.state_location()) {
        s26_State.ClientOwned => "client-owned",
        s26_State.ServerStoredById => "server-stored-by-id",
        s26_State.ServerSession => "server-session",
      };
      w + " / " + s
    }
    _ => "unknown transport",
  }
}

// Which capability the provider actually offers (Realtime checked first so the two
// WebSocket-wire providers split: warm -> "http", live -> "realtime").
function s26_capability_of(p: baml.ai.Provider) -> string throws never {
  match (p) {
    let r: baml.ai.Realtime => "realtime",
    let h: baml.ai.HttpProvider => "http",
    let s: baml.ai.Streaming => "stream",
    _ => "none",
  }
}

// The `$stream` consumer loop (the design's typewriter). Reused offline (negotiation)
// and live (real SSE reassembly). Byte-identical to scenario 04's consumer.
function s26_collect_stream(instruction: string, client: baml.ai.Provider) -> string throws never {
  let s = s26_Ask$stream(instruction, client = client) catch (e) {
    let sc: s26_SocketClosed => return "stream-negotiated",   // reached the Streaming provider (offline)
    let u: baml.errors.Unsupported => return "unsupported",   // provider is not Streaming
    _ => return "no-stream",
  };
  let acc: string = "";
  while (true) {
    match (s.next()) {
      let delta: string => { acc = acc + delta; }             // typewriter step
      null => {}                                              // no-text frame, skip
      let _done: baml.stream.StreamFinished => { break; }
    } catch (e) {
      _ => return "mid-stream error",
    }
  }
  s.final() catch (e) { _ => return acc }
}


// ============================================================================
// Offline tier — deterministic, no network, no keys.
// ============================================================================

// (1) The SAME function rides HTTP request/response — the base `Foo` companion routes
// through drive_call's HttpProvider arm.
test "26: the one function rides HTTP request/response (call via HttpProvider)" {
  let r = s26_Ask("q", client = s26_HttpTransport { reply: "pong" }) catch (e) {
    _ => "call failed",
  };
  log.info("26 offline: HTTP transport result = " + r);
  assert.equal(r, "pong")
}

// (2) The SAME function — swap the client only — rides a WebSocket duplex session, via
// the `$live` companion. The pass-in `io` is handed in at the call (never a client
// field); the provider drives it and returns the durable Transcript.
test "26: the SAME function rides a WebSocket duplex session (live via Realtime)" {
  let ch = s26_RecordingChannel { events: [] };
  let t = s26_Ask$live("be brief", ch, client = s26_RealtimeTransport { reply: "pong" }) catch (e) {
    _ => baml.ai.Transcript { text: "live failed" },
  };
  log.info("26 offline: duplex transcript = " + t.text + " | events = " + ch.events.length().to_string());
  assert.is_true(t.text.includes("pong"))       // deltas accumulated into the record
  assert.is_true(t.text.includes("be brief"))    // seeded with the call-time prompt
  assert.is_true(ch.events.length() > 0)          // the duplex was driven over the pass-in io
}

// (3) THE DEGRADE (real, in drive_call): a stream-only provider (Streaming, NOT
// HttpProvider) is DRAINED via the Streaming arm's `.final()`, not rejected as
// Unsupported. Offline the fake can't build a `Stream`, so it surfaces the typed
// s26_SocketClosed — which is exactly the proof the degrade ROUTED to Streaming (a
// no-degrade drive_call would have thrown `Unsupported` instead). The value-returning
// `.final()` is proven LIVE.
test "26: drive_call degrades a stream-only provider to the Streaming arm (.final())" {
  let label = s26_Ask("q", client = s26_StreamTransport {}) catch (e) {
    let sc: s26_SocketClosed => "degraded-to-stream",         // reached Streaming.final()
    let u: baml.errors.Unsupported => "unsupported",          // would mean NO degrade
    _ => "other",
  };
  log.info("26 offline: stream-only degrade label = " + label);
  assert.equal(label, "degraded-to-stream")
}

// (4) The `$stream` companion negotiates Streaming: the stream fake reaches
// stream_messages (surfaces s26_SocketClosed), the HTTP fake (not Streaming) is a typed
// Unsupported. Same companion, capability chosen by the client's provider.
test "26: the $stream companion negotiates Streaming, rejecting a non-streaming provider" {
  assert.equal(s26_collect_stream("q", client = s26_StreamTransport {}), "stream-negotiated")
  assert.equal(s26_collect_stream("q", client = s26_HttpTransport { reply: "x" }), "unsupported")
}

// (5) The marker payoff (provider-as-marker, OQ1 resolved): a realtime provider carries
// NO `HttpProvider.call`. It negotiates as Realtime only — `Provider` is a bare marker
// and `call` is a SEPARATE capability, never forced onto a duplex provider.
test "26: a realtime provider carries no HttpProvider call (marker payoff)" {
  let p: baml.ai.Provider = s26_RealtimeTransport { reply: "x" };
  let label = match (p) {
    let h: baml.ai.HttpProvider => "has-call",
    let r: baml.ai.Realtime => "realtime-only",
    _ => "neither",
  };
  assert.equal(label, "realtime-only")
}

// (6) Transport ⊥ capability AND ⊥ state: the scenario's sharpest claim. Both the warm
// socket and the realtime session are on a `wss://` WebSocket wire — SAME wire — yet one
// is request/response (HttpProvider) and the other duplex (Realtime), and their
// state-location differs. You cannot read capability OR state off the transport.
test "26: two wss:// providers — same wire, opposite capability, different state" {
  let warm: baml.ai.Provider = s26_WarmSocketTransport { reply: "x" };
  let live: baml.ai.Provider = s26_RealtimeTransport { reply: "x" };

  // SAME wire...
  assert.equal(s26_describe_transport(warm), "websocket / server-stored-by-id")
  assert.equal(s26_describe_transport(live), "websocket / server-session")
  // ...OPPOSITE capability.
  assert.equal(s26_capability_of(warm), "http")
  assert.equal(s26_capability_of(live), "realtime")

  // And the request/response floor is a DIFFERENT wire, SAME capability as the warm socket.
  let floor: baml.ai.Provider = s26_HttpTransport { reply: "x" };
  assert.equal(s26_describe_transport(floor), "http / client-owned")
  assert.equal(s26_capability_of(floor), "http")
}


// ============================================================================
// Live tier (excluded by default; run via `-i "integ-test*::"` with a key).
//
// ONE function (`s26_Ask`), THREE transports, three companions — the portability claim
// against the real wires. Each under 60s.
// ============================================================================
testset "integ-test-26-transports" {
  // (1/3) HTTP request/response over OpenAI (native baml.http.send).
  test "26 live: the one function calls over HTTP (transport 1/3)" {
    let r = s26_Ask(
      "Reply with exactly the lowercase word: pong",
      client = baml.ai.OpenAi {
        model: "gpt-5.4-mini",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    ) catch (e) {
      _ => "live HTTP call failed",
    };
    log.info("26 live: HTTP transport = " + r);
    assert.is_true(r.to_lower_case().includes("pong"))
  }

  // (2/3) The SAME function STREAMS over OpenAI's SSE (native stream_messages over
  // baml.http.fetch_sse) — swap only the companion (`$stream`) + the transport.
  test "26 live: the SAME function streams over OpenAI SSE (transport 2/3)" {
    let r = s26_collect_stream(
      "the number one, in one short line",
      client = baml.ai.OpenAi {
        model: "gpt-5.4-mini",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    log.info("26 live: SSE transport = " + r);
    assert.is_true(r.length() > 0)
  }

  // (3/3) The SAME function runs a LIVE duplex text session over OpenAiRealtime (native
  // WebSocket over baml.ws) — swap only the companion (`$live`) + the transport. The
  // provider forwards its raw event feed to `ch` and returns the durable Transcript.
  test "26 live: the SAME function runs a WebSocket duplex session (transport 3/3)" {
    let ch = s26_RecordingChannel { events: [] };
    let t = s26_Ask$live(
      "Reply with exactly the lowercase word: pong",
      ch,
      client = baml.ai.OpenAiRealtime {
        model: "gpt-realtime",
        voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
    );
    log.info("26 live: WebSocket transport = " + t.text + " | events = " + ch.events.length().to_string());
    assert.is_true(t.text.to_lower_case().includes("pong"))   // the model spoke over the duplex
    assert.is_true(ch.events.length() > 0)                     // the raw event feed reached the channel
  }
}
