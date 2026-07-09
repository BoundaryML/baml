// scenario 22 — realtime voice: ../../../../../llm-provider/ideas/scenarios/22-realtime-voice/README.md
//   usage.baml — APPLICATION-author code for a realtime voice session, on the REAL surface.
//
// The design's thesis: the CONNECTION is the session. There is no request/response
// and no session id, so the client block is pure config and the live `Channel` is
// handed in AT the `.live` call — never a client field, because the duplex does not
// exist until you connect. Audio is NOT a result: it rides the `Channel` as a side
// channel while `run` returns only the durable `Transcript`. A one-shot `.call`
// cannot fake a duplex session, so `.live` on a non-realtime client is a typed
// `Unsupported` — the cleanest "interaction-shape change, no honest degrade" case.
//
// How that maps onto what actually ships (REALIZED.md §22):
//   - `Realtime requires Provider` + `run(self, prompt, io: Channel) -> Transcript`
//     is REAL (ns_ai/capabilities/realtime.baml). A realtime provider implements
//     ONLY `Realtime` — no degenerate `call` (the marker resolves OQ1).
//   - The `$live` companion is generated on every LLM function and desugars to
//     `baml.ai.drive_live<T>(client, prompt, io)`: it `match`es the client's
//     provider for `Realtime`, hands in the call-time `io`, and throws a typed
//     `Unsupported` for a non-realtime client (no fallback). `T` (the function's
//     return type) is unused — a live session returns the session `Transcript`.
//   - The LIVE `baml.ai.OpenAiRealtime` provider drives a real TEXT exchange over
//     the GA Realtime protocol on `baml.ws` (proven in the Rust `ai_realtime.rs`
//     tier). The live testset below proves the SAME thing through the scenario's
//     own `VoiceChat$live` spelling.
//
// What the OFFLINE tier proves deterministically: the `$live` negotiation (a scripted
// `Realtime` fake is driven; a non-realtime client is a typed `Unsupported`), the
// config-only-client + call-time-io split (the `Channel` is handed in at `$live` and
// the provider drives it), and `Transcript` accumulation across a scripted event feed.
//
// BLOCKED (net-new host surface the design needed, not on the real surface):
//   - the typed duplex event vocabulary (InEvent/OutEvent + InKind/OutKind) and the
//     audio side channel (PCM frames, `baml.audio.open_channel`/`play`, mic pump).
//     The real `Channel` carries a bare `string` feed and `Transcript` is `{ text }`,
//     so the scenario runs in TEXT mode. Recorded as `// BLOCKED: P8 ...` below.
//   - server-VAD vs push-to-talk (`manual_turn`) and barge-in (`Cancel`/`Truncate`,
//     scenario 23's `LiveControl`) — the real `OpenAiRealtime` drives one text turn.


// --- config-only client. -----------------------------------------------------
// The design declares config-only clients (voice, instructions, turn-mode) and hands
// the live `io` in at `.live`. On the real surface a declared `client` is the DEFAULT;
// the swap is a one-line call-site `client =` override (offline via the scripted fake,
// live via the native `OpenAiRealtime` provider). We default to a dead NON-realtime
// client so the negotiation test proves the honest `Unsupported`.
client<llm> s22_DeadVoice {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// --- the function. `client:` takes the config-only provider; the live channel is
//     NOT named here — it arrives on the `$live` companion. VoiceChat's `-> string`
//     return type is unused by `$live` (which returns the session `Transcript`).
function VoiceChat(system: string) -> string {
  client s22_DeadVoice
  prompt #"{{ system }}"#
}


// ============================================================================
// Offline tier — deterministic, no network, no keys. Drives the scripted fake.
// ============================================================================

// (A) The happy path: `$live` negotiates `Realtime` on the swapped-in provider,
// hands it the call-time `io`, and the provider drives the duplex. This proves BOTH
// the config-only-client + pass-in-io split AND `Transcript` accumulation: the
// scripted deltas fold into the returned transcript, and the recording channel
// captured the forwarded event feed (the side channel).
// BLOCKED: P8 audio side channel — the design plays PCM off `io` here; the real
//   Channel is a string feed, so we record events instead of audio frames.
test "22: Foo$live drives a swapped-in Realtime provider and the transcript accumulates" {
  let ch = s22_RecordingChannel { events: [] };
  let t = VoiceChat$live(
    "be brief",
    ch,
    client = ScriptedRealtime { transcript: "pong" },
  ) catch (e) {
    _ => baml.ai.Transcript { text: "live failed" },
  };
  log.info("22 offline: transcript = " + t.text + " | events = " + ch.events.length().to_string());
  assert.is_true(t.text.includes("pong"))          // deltas accumulated into the record
  assert.is_true(t.text.includes("be brief"))       // seeded with the call-time prompt
  assert.is_true(ch.events.length() > 0)             // the duplex was driven over the pass-in io
}

// (B) Interaction-shape change, no honest degrade: `.live` against a NON-realtime
// client (the default dead OpenAI chat client — an HttpProvider, not Realtime) is a
// TYPED `Unsupported`. A one-shot `.call` cannot fake a duplex session.
test "22: Foo$live on a non-realtime client is a typed Unsupported" {
  let ch = s22_RecordingChannel { events: [] };
  let label = VoiceChat$live("be brief", ch) catch (e) {
    let u: baml.errors.Unsupported => "unsupported",
    _ => "wrong error",
  };
  assert.equal(label, "unsupported")
}

// (C) The marker payoff: a realtime provider carries NO `call`. It negotiates as
// `Realtime`, and is NOT an `HttpProvider` — so `Provider` is a bare marker and
// `call` is a separate `HttpProvider` capability (OQ1 resolved: no degenerate call).
test "22: a realtime provider negotiates Realtime and carries no HttpProvider call" {
  let p: baml.ai.Provider = ScriptedRealtime { transcript: "x" };
  let label = match (p) {
    let h: baml.ai.HttpProvider => "has-call",
    let r: baml.ai.Realtime => "realtime-only",
    _ => "neither",
  };
  assert.equal(label, "realtime-only")
}

// (D) Barge-in / mid-session mutation is a SEPARATE capability (`LiveControl`,
// scenario 23) layered on top of `Realtime`. A plain voice provider is Realtime but
// NOT LiveControl — the negotiation distinguishes them, so an app can pre-flight
// whether truncation/cancel is available before wiring a barge-in handler.
test "22: a plain voice provider is Realtime but not LiveControl" {
  let p: baml.ai.Provider = ScriptedRealtime { transcript: "x" };
  let label = match (p) {
    let lc: baml.ai.LiveControl => "live-control",
    let r: baml.ai.Realtime => "realtime-only",
    _ => "neither",
  };
  assert.equal(label, "realtime-only")
}


// ============================================================================
// Live tier (excluded by default; run via `-i "integ-test*::"` with a key).
//
// The scenario's own `VoiceChat$live` spelling, driven against the REAL
// `baml.ai.OpenAiRealtime` provider over the GA Realtime protocol on `baml.ws`.
// One short TEXT exchange: hand a fresh recording Channel to `$live`, ask for a
// single word, and read the accumulated `Transcript`. The provider forwards its raw
// event feed to `ch` (the side channel) and returns the durable text record. Audio
// is BLOCKED (P8) — text mode is exactly what the Rust `ai_realtime.rs` tier proves,
// re-proven here through the companion the application actually calls.
// ============================================================================
testset "integ-test-22-realtime-voice" {
  test "22 live: VoiceChat$live drives a real OpenAiRealtime text exchange" {
    let ch = s22_RecordingChannel { events: [] };
    let t = VoiceChat$live(
      "Reply with exactly the lowercase word: pong",
      ch,
      client = baml.ai.OpenAiRealtime {
        model: "gpt-realtime",
        voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
    );
    log.info("22 live: transcript = " + t.text + " | events = " + ch.events.length().to_string());
    assert.is_true(t.text.to_lower_case().includes("pong"))   // the model spoke over the duplex
    assert.is_true(ch.events.length() > 0)                     // the raw event feed reached the channel
  }
}
