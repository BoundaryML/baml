// Task sketch: s23_VoiceTurn(system) -> string.

// Manual expansion reference for task `s23_VoiceTurn`:
// - `s23_VoiceTurn_request(...)` models `s23_VoiceTurn.request(...)`.
// - `s23_VoiceTurn_live_manual(...)` models the custom realtime consumer
//   `root.ai.open_live(s23_VoiceTurn.request(...), channel)`. Barge-in acts on
//   the returned Live resource; it is not a compiler task modifier.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s23_VoiceTurn_request(
  system: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s23-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s23_VoiceTurn", "ns_ai_scenarios", { "system": system },
    [], root.ai.RequestOptions {}, {}, bepv2_prompt_template(system, false),
  )
}

function s23_VoiceTurn_live_manual(
  system: string,
  channel: root.ai.Channel,
  provider: root.ai.Provider,
) -> root.ai.Live throws baml.errors.UnknownError {
  root.ai.open_live<string>(s23_VoiceTurn_request(system, provider = provider), channel)
}

function s23_script() -> root.ai.LiveEvent[] throws never {
  [
    root.ai.TranscriptDelta { text: "The capital is Par" },
    root.ai.UserSpeechStarted { played_ms: 800 },
    root.ai.TranscriptDelta { text: "Yes, Paris." },
    root.ai.LiveClosed {},
  ]
}

test "23: barge-in cancels, truncates at heard audio, then the next turn proceeds" {
  let channel = s23_RecordingChannel { events: [] };
  let live = s23_VoiceTurn_live_manual(
    "answer briefly", channel, s23_ScriptedBargeIn { script: s23_script() },
  );
  let result = s23_drive_live(live);
  let frames = channel.events.join("|");
  assert.is_true(result.interrupted)
  assert.contains(result.transcript, "Yes, Paris")
  assert.contains(frames, "response.cancel")
  assert.contains(frames, `"audio_end_ms":800`)
}

test "23: barge-in runs through the hand-written live companion" {
  let live = s23_VoiceTurn_live_manual(
    "be a terse barista", NullChannel {},
    s23_ScriptedBargeIn { script: s23_script() },
  );
  let result = s23_drive_live(live);
  assert.is_true(result.interrupted)
}

test "23: a non-realtime provider cannot open a Live resource" {
  let label = s23_VoiceTurn_live_manual("x", NullChannel {}, s23_PlainProvider {}) catch (e) {
    let u: baml.errors.UnknownError => "unsupported",
  };
  assert.equal(label, "unsupported")
}

test "23: controls are negotiated by holding a Live resource" {
  let provider: root.ai.Provider = s23_ScriptedBargeIn { script: [] };
  let live = s23_VoiceTurn_live_manual("x", NullChannel {}, provider);
  assert.equal(s23_manual_barge_in(live, 400), "barged-in")
}

test "23: push-to-talk sends cancel before truncate" {
  let channel = s23_RecordingChannel { events: [] };
  let live = s23_VoiceTurn_live_manual(
    "x", channel, s23_ScriptedBargeIn { script: [] },
  );
  let _ = s23_manual_barge_in(live, 640);
  assert.is_true(channel.events[0].includes("response.cancel"))
  assert.is_true(channel.events[1].includes(`"audio_end_ms":640`))
}

// No live testset: real barge-in still needs an audio duplex and a measured
// UserSpeechStarted event. Scenario 22 covers the live realtime transport.
