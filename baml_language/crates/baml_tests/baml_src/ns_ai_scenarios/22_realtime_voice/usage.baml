// Task sketch: VoiceChat(system) -> string.

// Manual expansion reference for task `VoiceChat`:
// - `VoiceChat_request(...)` models `VoiceChat.request(...)`.
// - `VoiceChat_live_manual(...)` models the custom realtime consumer
//   `root.ai.open_live(VoiceChat.request(...), channel)`. Live resources are
//   reached through `.request`; `.live` is not a compiler task modifier.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function VoiceChat_request(
  system: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s22-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "VoiceChat", "ns_ai_scenarios", { "system": system },
    [], root.ai.RequestOptions {}, {}, bepv2_prompt_template(system, false),
  )
}

function VoiceChat_live_manual(
  system: string,
  channel: root.ai.Channel,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s22-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Live throws baml.errors.UnknownError {
  root.ai.open_live<string>(VoiceChat_request(system, provider = provider), channel)
}

test "22: the live companion opens a resource and transcript deltas accumulate" {
  let channel = s22_RecordingChannel { events: [] };
  let live = VoiceChat_live_manual("be brief", channel, provider = ScriptedRealtime { transcript: "pong" });
  assert.equal(s22_collect_text(live), "pong")
  assert.is_true(channel.events.length() > 0)
  live.cleanup();
}

test "22: live on a non-realtime client reports unsupported execution" {
  let label = VoiceChat_live_manual("be brief", NullChannel {}) catch (e) {
    let u: baml.errors.UnknownError => "unsupported",
  };
  assert.equal(label, "unsupported")
}

test "22: a realtime provider does not need to implement Generate" {
  let provider: root.ai.Provider = ScriptedRealtime { transcript: "x" };
  let label = match (provider) {
    let generate: root.ai.Generate => "generate",
    let realtime: root.ai.Realtime => "realtime-only",
    _ => "neither",
  };
  assert.equal(label, "realtime-only")
}

test "22: interruption controls belong to the opened Live resource" {
  let live = VoiceChat_live_manual("be brief", NullChannel {}, provider = ScriptedRealtime { transcript: "x" });
  live.cancel_response();
  live.truncate_assistant_audio(250);
  assert.equal(s22_collect_text(live), "x")
}

testset "integ-test-22-realtime-voice" {
  test "22 live: VoiceChat$live drives a real OpenAiRealtime text exchange" {
    let channel = s22_RecordingChannel { events: [] };
    let live = VoiceChat_live_manual(
      "Reply with exactly the lowercase word: pong",
      channel,
      provider = root.ai.OpenAiRealtime {
        model: "gpt-realtime", voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
    );
    assert.contains(s22_collect_text(live).to_lower_case(), "pong")
    assert.is_true(channel.events.length() > 0)
    live.cleanup();
  }
}
