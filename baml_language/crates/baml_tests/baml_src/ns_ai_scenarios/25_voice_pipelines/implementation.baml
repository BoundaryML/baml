// scenario 25 — voice pipelines: ../../../../../llm-provider/ideas/scenarios/25-voice-pipelines/README.md
//
// LIBRARY / std-lib-author code — the scenario-level capability interfaces the
// design nominates for stdlib promotion (`SttProvider`/`TtsProvider`, the unified
// `listen`/`speak` verbs) plus the scripted fakes the offline tier drives.
//
// The design (implement.baml) builds a Pipecat-style CASCADE: three independent
// single-capability providers — STT (Deepgram/Whisper), an LLM (any Provider), and
// TTS (ElevenLabs/Cartesia) — wired by a `CascadedVoice` combinator that
// `implements Realtime` and owns the VAD/barge-in/transcript-stitching glue; plus a
// Mastra-style `CompositeVoice` (STT in + TTS out, no LLM) exposing the unified
// verbs. The tradeoff the scenario dramatizes is STRUCTURAL: the cascade (more moving
// parts, per-stage control, higher latency) vs. the integrated single-STS realtime
// path (scenario 22 — one hop, lower latency, no per-stage control).
//
// What we KEEP on the real surface (this file):
//   - `s25_Stt` / `s25_Tts` — the two net-new single-capability interfaces (the
//     `listen`/`speak` verbs), scenario-scoped candidates for stdlib promotion.
//   - Scripted `s25_FakeStt` / `s25_FakeTts` (+ an alternate TTS to prove stage swap
//     is a FIELD SWAP) — the whole cascade runs on these offline.
//   - `s25_CompositeVoice` — the pure voice-I/O adapter (STT in + TTS out, NO LLM),
//     forwarding the unified verbs. It is a `Provider` (marker) but carries NO
//     `HttpProvider` `call` — the capability hole the usage tests make visible.
//
// BLOCKED: P8 audio host surface. The design's `AudioChunk`/`AudioStream`/
//   `Transcription`/`VadDetector`/`silero_vad()` and the audio realtime event frames
//   (`InEventAudio`/`OutEventAudio`/`OutEventFlush`) are net-new `$rust_type`-backed
//   host primitives that do NOT exist. So STT/TTS are SCRIPTED here (audio edges
//   faked, `transcribe(audio_url) -> string`, `synthesize(text) -> string`) — the
//   PIPELINE STRUCTURE (STT -> LLM -> TTS) is real even when the audio edges are not,
//   which is exactly what the live tier proves with a real LLM middle stage.
// BLOCKED: P8 barge-in / VAD / latency budget. The design's `CascadedVoice.run` owns
//   turn detection, in-flight-TTS cancel, and speaker-queue flush inside one Realtime
//   body — all riding the audio host surface above. Scenario 23 (`LiveControl`) holds
//   the barge-in capability shape; here the cascade is the ordered three-stage flow.


// ----------------------------------------------------------------------------
// NET-NEW capability interfaces: the unified voice verbs (design §(b)).
//   `listen` = STT (audio in -> text out), `speak` = TTS (text in -> audio out).
//   The design makes each `requires Provider` with its own net-new error channel
//   (`baml.errors.SttError`/`TtsError`). On the real surface we keep the verbs as
//   scenario-scoped interfaces; the fakes are total (`throws never`) since the audio
//   host surface — where the real network errors would arise — is BLOCKED (P8).
// ----------------------------------------------------------------------------

/// STT: recognize speech. Scripted here (audio host blocked P8) — `audio_url` names
/// the clip, `transcribe` returns the recognized text. Mastra's `listen()`.
interface s25_Stt {
    function transcribe(self, audio_url: string) -> string throws never
}

/// TTS: synthesize speech. Scripted here — text in, an opaque "audio" token out
/// (the audio bytes are host-surface blocked). Mastra's `speak()`.
interface s25_Tts {
    function synthesize(self, text: string) -> string throws never
}


// ----------------------------------------------------------------------------
// Scripted stage fakes. The audio EDGES are faked; the pipeline STRUCTURE is real.
// ----------------------------------------------------------------------------

/// A scripted recognizer: returns its seeded `transcript` regardless of the clip
/// (the real recognizer is host-surface blocked P8). Seeding the transcript is how
/// the LIVE tier feeds a real utterance to the real LLM middle stage.
class s25_FakeStt {
    transcript: string,

    implements s25_Stt {
        function transcribe(self, audio_url: string) -> string throws never {
            // Scripted: the "recognized" text is the seed; `audio_url` is recorded
            // only to prove the clip flowed into the stage.
            self.transcript
        }
    }
}

/// A scripted synthesizer: wraps the reply text in an opaque `audio<...>` token
/// standing in for the synthesized bytes (host-surface blocked P8).
class s25_FakeTts {
    implements s25_Tts {
        function synthesize(self, text: string) -> string throws never {
            "audio<" + text + ">"
        }
    }
}

/// A SECOND, divergent TTS (SSML-wrapping) — proves "swap a stage = swap a field":
/// the cascade body never changes, only the `tts` argument does. This is the design's
/// ElevenLabs -> Cartesia one-field swap, minus the host audio surface.
class s25_SsmlTts {
    implements s25_Tts {
        function synthesize(self, text: string) -> string throws never {
            "<speak>" + text + "</speak>"
        }
    }
}


// ----------------------------------------------------------------------------
// COMPOSITE VOICE — the Mastra unified-verb adapter: mix an STT input and a TTS
//   output behind ONE object exposing `listen`/`speak`. NO LLM stage — it is a pure
//   voice-I/O adapter. Under the provider-as-marker revision it IS a `Provider`
//   (the marker), but it does NOT implement `HttpProvider`, so it has NO `call` to
//   fake or stub. The capability hole is HONEST: driving an LLM function (the `.call`
//   path) on a `CompositeVoice` throws a typed `Unsupported` at negotiation — proven
//   in usage.baml. (Design §(c): OQ1 resolved — no degenerate `call`.)
// ----------------------------------------------------------------------------
class s25_CompositeVoice {
    input: s25_Stt,
    output: s25_Tts,

    // The marker only — NO HttpProvider. The absence of `call` is the point.
    implements baml.ai.Provider {}

    // Forward the unified verbs to the chosen members (Mastra's CompositeVoice).
    implements s25_Stt {
        function transcribe(self, audio_url: string) -> string throws never {
            self.input.transcribe(audio_url)
        }
    }
    implements s25_Tts {
        function synthesize(self, text: string) -> string throws never {
            self.output.synthesize(text)
        }
    }
}
