// 25 — STT -> typed task -> TTS; transport-specific audio stays outside Provider.

// Task sketch: VoiceReply(transcript) -> string.

// This is the desugared code for VoiceReply ---------------------------------
function VoiceReply_request(
  transcript: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "voice-brain", reply: `"hello"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "VoiceReply", "ns_ai_scenarios", { "transcript": transcript },
    [], root.ai.RequestOptions {}, {},
    bepv2_prompt_template(`Reply to this caller in one short sentence: ${transcript}`, false),
  )
}

function VoiceReply_manual(
  transcript: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "voice-brain", reply: `"hello"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(VoiceReply_request(transcript, provider = provider))
}

function s25_cascaded_voice(
  stt: s25_Stt,
  brain: root.ai.Provider,
  tts: s25_Tts,
  audio: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  let transcript = stt.transcribe(audio);
  let reply = VoiceReply_manual(transcript, provider = brain);
  tts.synthesize(reply)
}

test "25: the pipeline composes STT, one semantic request, and TTS" {
  let out = s25_cascaded_voice(
    s25_FakeStt { transcript: "connect me to billing" },
    BepV2FixtureGenerate { name: "brain", reply: `"Connecting you now."`, attributes: {} },
    s25_FakeTts {},
    "call.wav",
  );
  assert.equal(out, "audio<Connecting you now.>")
}

test "25: the request is reusable without audio transport state" {
  let request = VoiceReply_request("hello");
  assert.equal(request.identity?.name, "VoiceReply")
  assert.equal(request.tools.length(), 0)
}

testset "integ-test-25-voice-pipelines" {
  test "25 live: the cascade runs a real LLM middle stage between scripted STT/TTS" {
    let out = s25_cascaded_voice(
      s25_FakeStt { transcript: "Reply with exactly the lowercase word pong." },
      root.ai.OpenAi { model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null },
      s25_FakeTts {},
      "call.wav",
    );
    assert.is_true(out.to_lower_case().includes("pong"))
  }
}
