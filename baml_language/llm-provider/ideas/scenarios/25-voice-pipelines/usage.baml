// ============================================================================
// 25 — Voice pipelines (APPLICATION CODE)
// ============================================================================
//
// What an app author writes to USE the voice stack from implement.baml.
// Clients are sugar for `function … -> Provider`, so each `client` below is a
// config-only factory that returns a Provider (here, a Realtime one). The live
// audio Channel is handed in at the `.live` companion, never a client field.

// ---- Stage clients: each is a single-capability provider factory. ----------

client DeepgramSTT() {
  Deepgram { api_key: env.DEEPGRAM_API_KEY, model: "nova-3", language: "en" }
}

client WhisperSTT() {                              // batch-only STT — same client shape
  Whisper { api_key: env.OPENAI_API_KEY, model: "whisper-1" }
}

client ElevenTTS() {
  ElevenLabs {
    api_key: env.ELEVEN_API_KEY,
    voice_id: "rachel",
    model_id: "eleven_turbo_v2_5",
    stability: 0.5,
    similarity_boost: 0.75,
  }
}

client CartesiaTTS() {                             // divergent TTS — speed is a string, not a float
  Cartesia { api_key: env.CARTESIA_API_KEY, voice_id: "a0e...", model_id: "sonic-2", speed: "normal" }
}

client BrainLLM() {                                // the chat stage is just ANY HttpProvider
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

// ---- The cascaded voice agent: wire three stages into one Realtime. --------
//   Swapping a stage = swapping one argument. Deepgram + GPT-4o + ElevenLabs.

client PhoneAgent() {
  CascadedVoice {
    stt: DeepgramSTT(),
    llm: BrainLLM(),
    tts: ElevenTTS(),
    vad: silero_vad(),
    speaker: "rachel",
  }
}

// Same agent, three stages swapped, ZERO downstream changes: Whisper STT
// (batch — `listen` will buffer), a cheaper LLM, Cartesia TTS (string speed).
//   This proves portability: the combinator only sees the capability interfaces.
client PhoneAgentBudget() {
  CascadedVoice {
    stt: WhisperSTT(),
    llm: OpenAI { model: "gpt-4o-mini", api_key: env.OPENAI_API_KEY },
    tts: CartesiaTTS(),
    vad: silero_vad(),
    speaker: "a0e...",
  }
}

// The architecture fork — single STS instead of the cascade. One line changes;
// both `PhoneAgent` and this return a Realtime Provider, so `VoiceChat.live`
// below is unchanged. Latency drops; per-stage control is gone.
client PhoneAgentLowLatency() {
  NovaSonic { region: "us-east-1", voice: "matthew" }
}

// ---- The function that drives a live session. ------------------------------
//   `.live(args, io)` is the Realtime companion: it `match`es the client's
//   provider for Realtime and calls `run`. Both cascaded and STS clients work.

function VoiceChat(system: string) -> VoiceTranscript {
  client: PhoneAgent()                            // swap to PhoneAgentLowLatency() to go STS
  prompt #"{{ system }}"#
}

// At the call site, the transport hands in the live audio channel:
//   let io = twilio.channel(call_sid);             // G.711 phone transport
//   VoiceChat.live("You are a terse support agent.", io)
//        ->  CascadedVoice.run(prompt, io)          // (or NovaSonic.run for STS)

// ---- Unified-verb usage: CompositeVoice mix-and-match (no LLM). ------------
//   Deepgram STT in, ElevenLabs TTS out, behind one object. Handed to an agent
//   that supplies its own brain, or used verb-by-verb.

client SupportVoice() {
  CompositeVoice { input: DeepgramSTT(), output: ElevenTTS() }
}

// speak(): text -> audio. The `.speak` companion matches TtsProvider.
function Announce(text: string) -> AudioStream {
  client: SupportVoice()
  // .speak is the unified TTS verb; no prompt rendering needed for raw TTS.
  prompt #"{{ text }}"#
}
//   Announce.speak("Your call is being transferred.", null)
//        ->  CompositeVoice.speak(text, null)  ->  ElevenLabs.speak(...)

// listen(): audio -> transcript. The `.listen` companion matches SttProvider.
function Hear(clip: AudioStream) -> string {
  client: SupportVoice()
  prompt #""#
}
//   Hear.listen(mic_stream)
//        ->  CompositeVoice.listen(audio)  ->  Deepgram.listen(...)  (final string)

// ---- Capability hole, made visible. ----------------------------------------
//   SupportVoice() returns a `CompositeVoice`, which does NOT implement
//   `HttpProvider` — it has no `call` at all (no fake, no stub). The plain
//   `Announce(text)` companion routes to the basic-call path: it `match`es the
//   provider for `HttpProvider`, finds none, and its `_` arm THROWS
//   `baml.errors.Unsupported`. The hole is now in the COMPANION match, not a
//   forged provider method. This is the runtime capability promise of `client`
//   sugar (proposal §5): the type says `Provider` (the marker), the absence of
//   `call` is discovered at call time, on the CallError channel `Announce` declares.
//
//   Announce.speak("hi")  // works:  routes to the TTS verb
//
//   The app catches the hole by the concrete `Unsupported` type (most-specific
//   first), then the CallError interface, then the UnknownError escape hatch:
//
//   Announce("hi") catch (e) {
//     let u: baml.errors.Unsupported => fallbackToBeep();           // the capability hole
//     let c: baml.errors.CallError   => { if (c.is_network_error()) { retry(); } else { fail(); } }
//     let u: baml.UnknownError       => report(u.message);          // anything foreign
//   }

// ---- Resilience reads as a combinator chain (inherited Provider methods). ---
//   A flaky TTS can fall back; the cascade still presents one Realtime face.

client ResilientPhoneAgent() {
  CascadedVoice {
    stt: DeepgramSTT(),
    llm: BrainLLM().with_retry(2),                // retry just the brain stage (Retry implements HttpProvider)
    tts: ElevenTTS(),                             // (TTS fallback would wrap at the TtsProvider layer)
    vad: silero_vad(),
    speaker: "rachel",
  }
}
