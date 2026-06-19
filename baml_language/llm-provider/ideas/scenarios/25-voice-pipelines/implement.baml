// ============================================================================
// 25 — Cascaded voice pipelines + unified voice abstraction (LIBRARY CODE)
// ============================================================================
//
// We build the voice stack on the canonical spine (`_conventions.md`):
//   - `Provider`  — the irreducible base (call<T>)
//   - `Realtime`  — `run(prompt, io: Channel) -> Transcript`
//   - `Channel`   — the pass-in duplex the caller hands to a realtime provider
//
// Three NET-NEW single-capability interfaces (`SttProvider`, `TtsProvider`)
// plus two combinators (`CascadedVoice`, `CompositeVoice`) that themselves
// implement `Realtime`. STT/LLM/TTS are three composable providers; a
// combinator wires them and presents a single `Realtime` face. Swap any
// stage by swapping a field.
//
// Everything below is std-lib / library-author code.

// ----------------------------------------------------------------------------
// Net-new media + frame primitives (host boundary).
//   These are the audio analogues of `baml.llm.PromptAst` / `baml.http.Body`:
//   opaque host values the providers move around but never introspect in BAML.
// ----------------------------------------------------------------------------

// A chunk of PCM/Opus/µ-law audio. Opaque; carries its own sample format.
class AudioChunk { _data: $rust_type }            // .sample_rate() -> int ; .bytes() -> uint8array

// A growable live audio stream (one direction). Mirrors baml.http.SseStream.
class AudioStream { _handle: $rust_type }
//   .next() -> AudioChunk? throws baml.errors.Io      // null = end of speech
//   .push(chunk: AudioChunk) -> null throws baml.errors.Io
//   .close() -> null

// A word/segment of recognized speech with timing, so transcripts can be
// stitched back onto the audio clock (the "aligning transcripts" hard part).
class Transcription { text: string, is_final: bool, start_ms: int, end_ms: int }

// The unified turn record a voice session returns.
class VoiceTranscript { turns: VoiceTurn[] }
class VoiceTurn { role: string, text: string, start_ms: int, end_ms: int }

// ----------------------------------------------------------------------------
// NET-NEW capability interfaces: the unified voice verbs.
//   `listen` = STT, `speak` = TTS. Each `requires Provider`, so an STT box is
//   *also* a Provider whose `call<T>` is "transcribe one clip to T" (T=string
//   in practice). This is the Open-Question-1 "best-effort single turn" stance.
// ----------------------------------------------------------------------------

// STT: audio stream in -> transcription stream out.  Mastra's `listen()`.
interface SttProvider requires Provider {
  // streaming form: partial + final transcriptions as audio arrives.
  function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string> throws Error
}

// TTS: text in -> audio stream out.  Mastra's `speak()`.
interface TtsProvider requires Provider {
  // `voice` overrides the provider's default speaker per call (Mastra's `speaker:`).
  function speak(self, text: string, voice: string?) -> AudioStream throws Error
}

// ----------------------------------------------------------------------------
// STAGE PROVIDER 1 — Deepgram STT (an HttpProvider that ALSO does SttProvider).
//   Divergence shown: Deepgram is a websocket-streaming recognizer; its `call`
//   (one-shot transcribe) is a plain HTTP POST, but `listen` is the live path.
// ----------------------------------------------------------------------------
class Deepgram {
  api_key: string
  model: string                                   // "nova-2", "nova-3", ...
  language: string?

  implements HttpProvider {                        // gives `call<T>` = transcribe one clip
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // one-shot: the "prompt" carries a single AudioChunk to transcribe.
      $rust_io_function                            // POST audio/* to api.deepgram.com/v1/listen
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "deepgram http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // Deepgram wraps the transcript at results.channels.0.alternatives.0.transcript
      baml.sap.parse<T>(from.json_path("results.channels.0.alternatives.0.transcript"))
    }
  }

  implements SttProvider {
    function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string> throws baml.errors.LlmClient {
      // opens a Deepgram streaming websocket, pushes audio chunks, yields
      // interim+final Transcription frames, finalizes to the full string.
      $rust_io_function
    }
  }
}

// ----------------------------------------------------------------------------
// STAGE PROVIDER 2 — Whisper STT (a DIFFERENT STT, batch-only).
//   Divergence shown: Whisper has NO streaming recognizer. `listen` must
//   buffer the whole AudioStream, transcribe once, and emit a single final
//   frame. This is the STT analogue of "streaming degrades to call".
// ----------------------------------------------------------------------------
class Whisper {
  api_key: string
  model: string                                   // "whisper-1"

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function                            // multipart POST to /v1/audio/transcriptions
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "whisper http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("text"))
    }
  }

  implements SttProvider {
    function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string> throws baml.errors.LlmClient {
      // Whisper can't stream: drain the whole clip, POST once, emit ONE final
      // frame. The interface is identical to Deepgram's; the behavior diverges.
      $rust_io_function
    }
  }
}

// ----------------------------------------------------------------------------
// STAGE PROVIDER 3 — ElevenLabs TTS.
//   Divergence shown: ElevenLabs streams audio over a websocket and exposes
//   per-call voice + a `voice_settings` blob (stability/similarity) that lives
//   as provider FIELDS, not an options bag.
// ----------------------------------------------------------------------------
class ElevenLabs {
  api_key: string
  voice_id: string                                // default speaker
  model_id: string                                // "eleven_turbo_v2_5"
  stability: float?                               // bespoke voice_settings -> fields
  similarity_boost: float?

  implements HttpProvider {                        // `call<T>` here is rarely used directly
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "11labs http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // T is audio: decode the returned bytes into an AudioChunk-bearing value.
      baml.sap.parse<T>(from.json_path("audio_base64"))
    }
  }

  implements TtsProvider {
    function speak(self, text: string, voice: string?) -> AudioStream throws baml.errors.LlmClient {
      // voice ?? self.voice_id selects the speaker; stability/similarity_boost
      // ride along in the websocket open frame. Streams audio chunks back.
      $rust_io_function
    }
  }
}

// ----------------------------------------------------------------------------
// STAGE PROVIDER 4 — Cartesia TTS (a DIFFERENT TTS).
//   Divergence shown: Cartesia uses a `voice` *object id* and exposes `speed`
//   as an enum-ish string ("slow"|"normal"|"fast"), not a float. The unified
//   `speak(text, voice)` verb papers over this; the per-provider knobs stay
//   as fields. Swapping ElevenLabs -> Cartesia is a one-field change upstream.
// ----------------------------------------------------------------------------
class Cartesia {
  api_key: string
  voice_id: string                                // Cartesia voice UUID
  model_id: string                                // "sonic-2"
  speed: string?                                  // "slow" | "normal" | "fast" (NOT a float)

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "cartesia http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("audio"))
    }
  }

  implements TtsProvider {
    function speak(self, text: string, voice: string?) -> AudioStream throws baml.errors.LlmClient {
      $rust_io_function                            // POST /tts/bytes, stream sonic audio frames
    }
  }
}

// ----------------------------------------------------------------------------
// NET-NEW host primitive: turn detection (VAD).  The "interruption / barge-in"
// hard part. A VadDetector watches an inbound AudioStream and reports
// speech-start / speech-end so the combinator can cancel in-flight TTS.
// ----------------------------------------------------------------------------
class VadDetector { _handle: $rust_type }
//   .on_speech_start(handler: () -> void) -> null
//   .on_speech_end(handler: () -> void) -> null          // end-of-user-turn
//   .feed(chunk: AudioChunk) -> null

function silero_vad() -> VadDetector { $rust_io_function }

// ----------------------------------------------------------------------------
// COMBINATOR 1 — CascadedVoice: STT -> LLM -> TTS wired into one Realtime.
//   The three stages are plain fields, each a single-capability Provider.
//   `requires`-typed fields make "swap any stage" a field swap. The combinator
//   owns the latency-budget glue: VAD turn detection, barge-in cancel, and
//   stitching STT/TTS text back into the LLM context.
//
//   This is the "cascaded fork": more moving parts, full per-stage control.
// ----------------------------------------------------------------------------
class CascadedVoice {
  stt: SttProvider                                // swap: Deepgram | Whisper | ...
  llm: Provider                                   // swap: ANY chat provider (just needs .call)
  tts: TtsProvider                                // swap: ElevenLabs | Cartesia | ...
  vad: VadDetector
  speaker: string?                                // default TTS voice for this pipeline

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> VoiceTranscript throws baml.errors.LlmClient {
      // `prompt` carries the system instruction; `io` is the duplex to the user
      // (mic in via InEvent::audio, speaker out via OutEvent::audio).
      let context = baml.llm.transcript_from(prompt);   // accumulated LLM context
      let turns: VoiceTurn[] = [];
      let mic = baml.voice.audio_stream();              // inbound user audio buffer
      let speaking = baml.voice.cancel_token();         // tracks in-flight TTS for barge-in

      // 1. user audio -> VAD + STT
      io.on(event => {
        match (event) {
          let a: InEventAudio => {
            self.vad.feed(a.chunk);
            mic.push(a.chunk) catch (e) { _ => {} };
          },
          _ => {},
        };
      });

      // 2. barge-in: when the user starts talking, kill any in-flight TTS and
      //    flush the speaker queue. This is the bookkeeping the STS path hides.
      self.vad.on_speech_start(() => { speaking.cancel(); io.send(OutEventFlush {}) catch (e) { _ => {} }; });

      // 3. on end-of-user-turn: STT finalize -> LLM -> TTS, one turn.
      self.vad.on_speech_end(() => {
        // STT: drain the live recognizer to a final transcript.
        let heard = baml.llm.stream_final(self.stt.listen(mic))
          catch (e) { _ => { return; } };
        turns = turns.append(VoiceTurn { role: "user", text: heard, start_ms: 0, end_ms: 0 });
        context = context.append_user(heard);

        // LLM: any Provider works — only `.call` is required. The chat stage
        // is a plain Provider, so Whisper-STT × Llama-LLM × Cartesia-TTS all fit.
        let reply: string = self.llm.call<string>(context.to_prompt())
          catch (e) { _ => { return; } };
        turns = turns.append(VoiceTurn { role: "assistant", text: reply, start_ms: 0, end_ms: 0 });
        context = context.append_assistant(reply);

        // TTS: stream synthesized audio back down `io`, honoring barge-in.
        let voiced = self.tts.speak(reply, self.speaker)
          catch (e) { _ => { return; } };
        for (let chunk in baml.voice.iter(voiced)) {
          if (speaking.cancelled()) { break; }    // user barged in -> stop mid-utterance
          io.send(OutEventAudio { chunk: chunk }) catch (e) { _ => { break; } };
        }
        speaking.reset();
      });

      baml.voice.block_until_closed(io);           // run the session until the channel closes
      VoiceTranscript { turns: turns }
    }
  }

  // Realtime requires Provider, so we owe a best-effort `call<T>`: run ONE
  // synchronous STT->LLM->TTS turn over a clip and return the LLM's text.
  // (OQ1: "best-effort single turn over the connection".)
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // No live audio in a one-shot call -> the cascade degenerates to the LLM
      // stage. This is the honest floor: text in, text out.
      self.llm.call<T>(prompt)
    }
  }
}

// ----------------------------------------------------------------------------
// COMBINATOR 2 — CompositeVoice: mix an input (STT) provider and an output
//   (TTS) provider behind one object, exposing the unified verbs. Mastra's
//   `CompositeVoice({ input, output })`. No LLM stage — it's a pure voice I/O
//   adapter (e.g. Deepgram in + ElevenLabs out) you hand to an agent.
// ----------------------------------------------------------------------------
class CompositeVoice {
  input: SttProvider
  output: TtsProvider

  // It IS a Provider (base requirement of every capability), but composite
  // voice has no model of its own, so `call` is meaningless -> it errors.
  // This is a capability HOLE made explicit (see evaluation §c).
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      throw baml.errors.LlmClient { message: "CompositeVoice has no LLM; use .listen/.speak or wrap it in CascadedVoice" }
    }
  }

  // Forward the unified verbs to the chosen members.
  implements SttProvider {
    function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string> throws baml.errors.LlmClient {
      self.input.listen(audio)
    }
  }
  implements TtsProvider {
    function speak(self, text: string, voice: string?) -> AudioStream throws baml.errors.LlmClient {
      self.output.speak(text, voice)
    }
  }
}

// ----------------------------------------------------------------------------
// The single-STS path (for contrast in usage.baml): one provider that does the
// whole duplex itself. Already a `Realtime` provider per the proposal §3 — we
// just declare a representative one here so the swap "cascaded <-> STS" is a
// one-line change at the client site (both return a Realtime Provider).
// ----------------------------------------------------------------------------
class NovaSonic {                                  // AWS Nova Sonic: single speech-to-speech
  region: string
  voice: string

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> VoiceTranscript throws baml.errors.LlmClient {
      // one network hop: audio in -> audio out, prosody preserved, low latency,
      // no VAD/barge-in bookkeeping in BAML (the model owns turn-taking).
      $rust_io_function
    }
  }
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient { $rust_io_function }
  }
}

// ----------------------------------------------------------------------------
// Channel event shapes used above. These extend the canonical `Channel`'s
// In/OutEvent surface with the audio + flush frames a voice transport carries.
// (Net-new: the realtime InEvent/OutEvent unions need audio members.)
// ----------------------------------------------------------------------------
class InEventAudio  { chunk: AudioChunk }          // mic frame from the user
class OutEventAudio { chunk: AudioChunk }          // speaker frame to the user
class OutEventFlush {}                             // drop the speaker queue (barge-in)
