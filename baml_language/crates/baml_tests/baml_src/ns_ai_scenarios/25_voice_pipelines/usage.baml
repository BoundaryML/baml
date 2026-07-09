// scenario 25 — voice pipelines: ../../../../../llm-provider/ideas/scenarios/25-voice-pipelines/README.md
//   usage.baml — APPLICATION-author code + runnable tests.
//
// The design's thesis: a voice agent built the CASCADED way is three independent
// single-capability providers — STT -> LLM -> TTS — wired into one flow, where the
// LLM is "any Provider" (the brain stage). Swapping any stage is a field swap; the
// cascaded<->integrated-STS fork (scenario 22) is a latency/quality tradeoff you make
// as STRUCTURE. This file drives that cascade on the REAL surface.
//
// PREFER LLM FUNCTIONS: the middle (brain) stage rides a PLAIN LLM function —
// `VoiceReply` — not a raw `h.call`. The cascade threads whichever provider it is
// handed into that function via `client = ...`. So:
//   - OFFLINE: the whole cascade runs on fakes — scripted STT/TTS + an `EchoProvider`
//     brain (deterministic, no network, no keys).
//   - LIVE: the SAME cascade runs with scripted STT/TTS but the REAL LLM middle stage
//     (`baml.ai.OpenAi`) — the pipeline structure is real even when the audio edges
//     are faked (see the `testset "integ-test-25-voice-pipelines"` block below).
//
// Interfaces + fakes (`s25_Stt`/`s25_Tts`, `s25_FakeStt`/`s25_FakeTts`/`s25_SsmlTts`,
// `s25_CompositeVoice`) live in this dir's implementation.baml.


// --- the brain stage: a PLAIN LLM function. ---------------------------------
// `s25_DeadBrain` is a dead declared default — never reached at runtime, because
// every call below overrides it with `client = <the cascade's brain provider>`
// (an `EchoProvider` offline, a real `baml.ai.OpenAi` live).
client<llm> s25_DeadBrain {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

/// The middle stage of the cascade: recognized transcript in, spoken reply out.
/// An ordinary LLM function — the cascade rides it, swapping the client per tier.
function VoiceReply(transcript: string) -> string {
  client s25_DeadBrain
  prompt #"
    You are a terse phone agent. Reply to the caller in one short sentence.
    Caller said: {{ transcript }}
  "#
}


// --- the cascade: STT -> LLM -> TTS wired into one ordered flow. -------------
// The design's `CascadedVoice` combinator, on the real surface: the STT and TTS
// stages are the net-new capability interfaces (scripted, audio host blocked P8),
// and the brain stage is the plain `VoiceReply` LLM function riding whatever
// `brain` provider is handed in. `log.info` per stage per Aaron's rule.
function s25_cascaded_voice(
    stt: s25_Stt,
    brain: baml.ai.Provider,
    tts: s25_Tts,
    audio_url: string,
) -> string throws baml.errors.CallError | baml.errors.LlmClient | baml.errors.InvalidArgument | baml.errors.RenderPrompt | baml.errors.StreamError | baml.errors.UnknownError {
    // Stage 1 — STT: audio clip -> recognized transcript.
    let transcript = stt.transcribe(audio_url);
    log.info("25: STT transcript in = " + transcript + " (from " + audio_url + ")");

    // Stage 2 — LLM: the brain. Rides the plain `VoiceReply` LLM function; the
    // handed-in `brain` provider is swapped in via `client =` (Echo offline, real
    // OpenAi live). This is the "brain stage is any Provider" claim, made real.
    let reply = VoiceReply(transcript, client = brain);
    log.info("25: LLM reply = " + reply);

    // Stage 3 — TTS: reply text -> synthesized audio (opaque token, P8).
    let audio = tts.synthesize(reply);
    log.info("25: synthesis request = " + audio);
    audio
}


// ============================================================================
// Offline tier — deterministic, no network, no keys. Whole cascade on fakes.
// ============================================================================

// (A) The core claim: the cascade flows through all three stages in order. Scripted
// STT recognizes the caller, the Echo brain answers, scripted TTS synthesizes — the
// pipeline STRUCTURE is proven end to end with the audio edges faked.
test "25: the cascaded pipeline flows STT -> LLM -> TTS" {
    let out = s25_cascaded_voice(
        s25_FakeStt { transcript: "I'd like to speak to billing." },
        EchoProvider { reply: "Sure, connecting you to billing now." },
        s25_FakeTts {},
        "call.wav",
    ) catch (e) {
        _ => "pipeline failed",
    };
    log.info("25 offline: cascade out = " + out);
    assert.equal(out, "audio<Sure, connecting you to billing now.>")
}

// (B) Swap = field swap. The SAME cascade body, one stage swapped (FakeTts ->
// SsmlTts): the combinator only sees the `s25_Tts` interface, so portability is real
// (the design's ElevenLabs -> Cartesia one-field change, minus the host audio surface).
test "25: swapping the TTS stage is a one-field change, cascade body untouched" {
    let out = s25_cascaded_voice(
        s25_FakeStt { transcript: "What are your hours?" },
        EchoProvider { reply: "We are open nine to five." },
        s25_SsmlTts {},
        "call.wav",
    ) catch (e) {
        _ => "pipeline failed",
    };
    assert.equal(out, "<speak>We are open nine to five.</speak>")
}

// (C) The unified-verb adapter: `s25_CompositeVoice` (STT in + TTS out, no LLM)
// forwards `listen`/`speak` to its members behind one object (Mastra's CompositeVoice).
test "25: a CompositeVoice forwards the unified listen and speak verbs" {
    let voice = s25_CompositeVoice {
        input: s25_FakeStt { transcript: "hello there" },
        output: s25_FakeTts {},
    };
    let heard = voice.transcribe("clip.wav");
    let spoken = voice.synthesize("goodbye");
    assert.equal(heard, "hello there")
    assert.equal(spoken, "audio<goodbye>")
}

// (D) The capability hole, made visible (design §(c), OQ1 resolved). A
// `CompositeVoice` is a `Provider` (marker) but carries NO `HttpProvider` `call`.
// Driving an LLM function (the `.call` path) on it discovers the absence at
// negotiation and throws a typed `Unsupported` — the hole is in the companion match,
// not a forged provider method.
test "25: driving an LLM function on a model-less CompositeVoice is a typed Unsupported" {
    let voice: baml.ai.Provider = s25_CompositeVoice {
        input: s25_FakeStt { transcript: "x" },
        output: s25_FakeTts {},
    };
    let label = VoiceReply("hi", client = voice) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        let c: baml.errors.CallError => "call-error",
        _ => "wrong error",
    };
    assert.equal(label, "unsupported")
}

// (E) The marker payoff: a CompositeVoice negotiates as its voice verbs but is NOT an
// HttpProvider — `Provider` is a bare marker and `call` is a separate capability the
// adapter never fakes.
test "25: a CompositeVoice is a Provider but carries no HttpProvider call" {
    let p: baml.ai.Provider = s25_CompositeVoice {
        input: s25_FakeStt { transcript: "x" },
        output: s25_FakeTts {},
    };
    let label = match (p) {
        let h: baml.ai.HttpProvider => "has-call",
        _ => "marker-only",
    };
    assert.equal(label, "marker-only")
}


// ============================================================================
// Live tier (excluded by default; run via `-i "integ-test-25-*::"` with a key).
//
// The SAME cascade, with scripted STT/TTS but the REAL LLM middle stage
// (`baml.ai.OpenAi`). The scripted STT seeds a real utterance the model answers; the
// scripted TTS wraps the real reply. This proves the pipeline STRUCTURE (STT -> LLM
// -> TTS) is real even when the audio edges are faked — the whole point of the
// cascade on a P8-blocked audio surface.
// ============================================================================
testset "integ-test-25-voice-pipelines" {
    test "25 live: the cascade runs a real LLM middle stage between scripted STT/TTS" {
        let out = s25_cascaded_voice(
            s25_FakeStt { transcript: "Reply with exactly the single lowercase word: pong" },
            baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
            s25_FakeTts {},
            "call.wav",
        );
        log.info("25 live: cascade out = " + out);
        // Structure is real: STT text -> real model -> TTS wrapper. The reply rode the
        // `audio<...>` synthesis token, so the real answer is inside it.
        assert.is_true(out.starts_with("audio<"))
        assert.is_true(out.to_lower_case().includes("pong"))
    }
}
