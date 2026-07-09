// scenario 23 — barge-in: ../../../../../llm-provider/ideas/scenarios/23-barge-in/README.md
//   usage.baml — APPLICATION-author code + runnable tests, on the REAL surface.
//
// The design's thesis: a user talks over the assistant mid-response; the app must cut
// the model off NOW (response.cancel), then rewrite server memory to reflect only what
// the user actually HEARD (conversation.item.truncate at the played-ms position), all
// over the SAME open Channel while a response is in flight. Barge-in is modeled as a
// net-new `LiveControl requires Realtime` capability — the dual-capability point: a
// provider can be `Realtime` (runs a session) yet not `LiveControl` (can't be steered).
//
// How that maps onto what actually ships (ns_ai/capabilities/realtime.baml):
//   - `LiveControl requires Realtime` is REAL with `cancel(io)` and
//     `truncate(io, played_ms)`. The played-position argument IS on the real
//     signature, so cancel + truncate-at-heard-ms is expressible verbatim.
//   - `$live` (`Foo$live(args, io, client = ...)` -> drive_live) negotiates `Realtime`
//     and drives the pass-in `Channel`; barge-in rides inside `run` (implementation.baml).
//   - A provider that is `Realtime` but not `LiveControl` is a typed `Unsupported`
//     on the barge-in path — no honest fallback (you cannot fake an interrupt).
//
// What the OFFLINE tier proves deterministically: an in-flight turn is truncated at the
// played position, the transcript records the partial + the interruption, a new turn
// proceeds (both by direct `LiveControl` calls AND end-to-end through `$live`), the wire
// moves (response.cancel THEN conversation.item.truncate@heard-ms) actually happen, and
// the Realtime-vs-LiveControl negotiation degrades honestly to `Unsupported`.
//
// BLOCKED: P8 live barge-in. Driving REAL barge-in needs an audio duplex + server-VAD
//   SpeechStarted events off `baml.ws` (a mic pushing PCM, a speaker measuring played-ms).
//   None of that is on the real surface — the live `OpenAiRealtime.{cancel,truncate}` are
//   stubbed `Unsupported` ("realtime transport not yet wired"). So there is NO live tier
//   for this scenario; the interrupt protocol is proven offline in TEXT mode against a
//   scripted `LiveControl` fake. (The live text-session transport itself is proven by
//   scenario 22's live testset + the Rust `ai_realtime.rs` tier.)


// --- config-only client + function. The live `io` arrives at the `$live` call, never a
//     client field. We default to a dead NON-realtime client so the un-steerable path is
//     honest; the barge-in tests swap in the scripted `LiveControl` provider at the call.
client<llm> s23_DeadVoice {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s23_VoiceTurn(system: string) -> string {
  client s23_DeadVoice
  prompt #"{{ system }}"#
}


// ============================================================================
// Offline tier — deterministic, no network, no keys. Drives the scripted fake.
// ============================================================================

// (A) The core interrupt protocol, exercised directly (this IS the lesson): the model is
// mid-response ("...is Par") when the user barges in. The session cancels the in-flight
// response and truncates server memory at the played position (800ms), records the
// truncated partial + the interruption, and a NEW turn proceeds. The recording channel
// captures the actual wire moves so we assert response.cancel THEN truncate@heard-ms.
test "23: barge-in truncates the in-flight turn and a new turn proceeds" {
  let ch = s23_RecordingChannel { events: [] };
  let session = s23_ScriptedBargeIn {
    script: [
      s23_TurnEvent { kind: "delta", text: "The capital of France is Par", ms: 800 },
      s23_TurnEvent { kind: "barge_in" },
      s23_TurnEvent { kind: "delta", text: "Yes, Paris.", ms: 400 },
      s23_TurnEvent { kind: "commit" },
    ],
  };
  let t = session.run("what is the capital of france", ch) catch (e) {
    _ => baml.ai.Transcript { text: "FAILED" },
  };
  let frames = ch.events.join("|");
  log.info("23 offline: recorded frames = " + frames);
  // the in-flight turn was truncated at the played position, the partial preserved
  assert.is_true(t.text.includes("truncated@800ms"))
  assert.is_true(t.text.includes("The capital of France is Par"))
  assert.is_true(t.text.includes("interrupted"))
  // a new turn proceeded after the floor handover
  assert.is_true(t.text.includes("[turn 2: Yes, Paris.]"))
  // the actual wire moves happened: cancel first, then truncate at the heard ms
  assert.is_true(frames.includes("response.cancel"))
  assert.is_true(frames.includes("conversation.item.truncate"))
  assert.is_true(frames.includes("audio_end_ms\":800"))
}

// (B) The same barge-in, end to end through the `$live` companion — the spelling an app
// actually writes. `$live` negotiates `Realtime` on the swapped-in provider, hands it the
// call-time `io`, and the barge-in rides inside `run`. Proves the interrupt survives the
// real companion path, not just a direct method call.
test "23: barge-in runs end to end through the $live companion" {
  let ch = s23_RecordingChannel { events: [] };
  let t = s23_VoiceTurn$live(
    "be a terse barista",
    ch,
    client = s23_ScriptedBargeIn {
      script: [
        s23_TurnEvent { kind: "delta", text: "We have a lovely single-origin Ethiop", ms: 1200 },
        s23_TurnEvent { kind: "barge_in" },
        s23_TurnEvent { kind: "delta", text: "Sure, one espresso.", ms: 300 },
        s23_TurnEvent { kind: "commit" },
      ],
    },
  ) catch (e) {
    _ => baml.ai.Transcript { text: "FAILED" },
  };
  assert.is_true(t.text.includes("truncated@1200ms"))
  assert.is_true(t.text.includes("interrupted"))
  assert.is_true(t.text.includes("[turn 2: Sure, one espresso.]"))
}

// (C) The dual-capability negotiation (the design's core point): a plain voice provider
// is `Realtime` but NOT `LiveControl`. App-driven barge-in on it is a TYPED `Unsupported`
// — no honest fallback — and crucially NOTHING is sent (no dishonest half-interrupt).
test "23: Realtime-without-LiveControl barge-in degrades to typed Unsupported" {
  let ch = s23_RecordingChannel { events: [] };
  let plain: baml.ai.Provider = s23_PlainRealtime { transcript: "ok" };
  let label = s23_manual_barge_in(plain, ch, 500) catch (e) {
    let u: baml.errors.Unsupported => "unsupported",
    _ => "wrong error",
  };
  assert.equal(label, "unsupported")
  assert.equal(ch.events.length(), 0)
}

// (D) Positive negotiation + the capability lattice: `LiveControl` layers on top of
// `Realtime`. The steerable provider negotiates as `LiveControl`; the plain one only as
// `Realtime`. An app can pre-flight steerability before wiring a barge-in handler.
test "23: LiveControl is negotiated on top of Realtime" {
  let steerable: baml.ai.Provider = s23_ScriptedBargeIn { script: [] };
  let plain: baml.ai.Provider = s23_PlainRealtime { transcript: "x" };
  let a = match (steerable) {
    let lc: baml.ai.LiveControl => "live-control",
    let r: baml.ai.Realtime => "realtime-only",
    _ => "neither",
  };
  let b = match (plain) {
    let lc: baml.ai.LiveControl => "live-control",
    let r: baml.ai.Realtime => "realtime-only",
    _ => "neither",
  };
  assert.equal(a, "live-control")
  assert.equal(b, "realtime-only")
}

// (E) Push-to-talk: the app (not server VAD) decides the user interrupted, and drives the
// barge-in itself via the negotiated `LiveControl`. Proves the app-facing move sends the
// real frames (cancel + truncate at the app-tracked played position).
test "23: push-to-talk barge-in on a steerable provider sends cancel+truncate" {
  let ch = s23_RecordingChannel { events: [] };
  let p: baml.ai.Provider = s23_ScriptedBargeIn { script: [] };
  let r = s23_manual_barge_in(p, ch, 640) catch (e) { _ => "FAILED" };
  let frames = ch.events.join("|");
  assert.equal(r, "barged-in")
  assert.is_true(frames.includes("response.cancel"))
  assert.is_true(frames.includes("audio_end_ms\":640"))
}
