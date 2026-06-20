// ============================================================================
// 25 — Cascaded voice pipelines + unified voice abstraction (LIBRARY CODE)
// ============================================================================
//
// We build the voice stack on the canonical spine (`_conventions.md`):
//   - `Provider`  — the irreducible MARKER (no interaction method; see ../provider-as-marker.md)
//   - `Realtime`  — `run(prompt, io: Channel) -> Transcript`
//   - `Channel`   — the pass-in duplex the caller hands to a realtime provider
//
// Three NET-NEW single-capability interfaces (`SttProvider`, `TtsProvider`)
// plus two combinators (`CascadedVoice`, `CompositeVoice`) that themselves
// implement `Realtime`. STT/LLM/TTS are three composable providers; a
// combinator wires them and presents a single `Realtime` face. Swap any
// stage by swapping a field.
//
// NOTE (provider-as-marker revision): `Provider` is a bare marker now — it has
// NO `call`. A realtime combinator/provider exposes ONLY its real interaction
// (`Realtime.run`); a pure voice-I/O adapter exposes ONLY `listen`/`speak`.
// None of them fakes a degenerate `call` to satisfy the base (OQ1 resolved).
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
//   `listen` = STT, `speak` = TTS. Each `requires Provider` (the marker), so an
//   STT/TTS box IS a Provider in the compositional sense — but it owns its OWN
//   interaction (`listen`/`speak`), not a forced `call`. A box that also wants a
//   one-shot HTTP path adds `implements HttpProvider` separately (Deepgram does).
// ----------------------------------------------------------------------------

// NET-NEW capability error channels for the voice verbs. STT and TTS are
// capabilities beyond call/stream/realtime/tools, so they get their OWN error
// interfaces (same shape as the core ones: classifiers callers triage on
// WITHOUT knowing the concrete class). Flagged net-new in evaluation §(b).
interface baml.errors.SttError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
}
interface baml.errors.TtsError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
}

// STT: audio stream in -> transcription stream out.  Mastra's `listen()`.
interface SttProvider requires Provider {
  // streaming form: partial + final transcriptions as audio arrives.
  function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string>
      throws baml.ExtendUnknownError<baml.errors.SttError>
}

// TTS: text in -> audio stream out.  Mastra's `speak()`.
interface TtsProvider requires Provider {
  // `voice` overrides the provider's default speaker per call (Mastra's `speaker:`).
  function speak(self, text: string, voice: string?) -> AudioStream
      throws baml.ExtendUnknownError<baml.errors.TtsError>
}

// ----------------------------------------------------------------------------
// STAGE PROVIDER 1 — Deepgram STT (an HttpProvider that ALSO does SttProvider).
//   Divergence shown: Deepgram is a websocket-streaming recognizer; its `call`
//   (one-shot transcribe) is a plain HTTP POST, but `listen` is the live path.
// ----------------------------------------------------------------------------
// A classified Deepgram transport failure (non-2xx). It can land on the one-shot
// call path AND on the streaming `listen` path, so it implements BOTH CallError
// and the net-new SttError.
class DeepgramHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.SttError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class Deepgram {
  api_key: string
  model: string                                   // "nova-2", "nova-3", ...
  language: string?

  implements HttpProvider {                        // gives `call<T>` = transcribe one clip
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // one-shot: the "prompt" carries a single AudioChunk to transcribe.
      $rust_io_function                            // POST audio/* to api.deepgram.com/v1/listen
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp = baml.http.send(request);
      if (!resp.ok()) { throw DeepgramHttpError { status: resp.status_code }; }
      resp
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "deepgram http failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Deepgram wraps the transcript at results.channels.0.alternatives.0.transcript.
      // Navigate the parsed `json` value (map -> array index -> map) down to the string.
      let root: baml.json.json = baml.json.parse(from.text());
      let channels = baml.json.field(baml.json.field(root, "results"), "channels");
      let alts = match (channels) {
        let arr: baml.json.json[] => match (arr.at(0)) { let c: baml.json.json => baml.json.field(c, "alternatives"), _ => null },
        _ => null,
      };
      let transcript = match (alts) {
        let arr: baml.json.json[] => match (arr.at(0)) { let a: baml.json.json => baml.json.field(a, "transcript"), _ => null },
        _ => null,
      };
      match (transcript) {
        let s: string => baml.sap.parse<T>(s),
        _ => baml.sap.parse<T>(""),
      }
    }
  }

  implements SttProvider {
    function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string>
        throws baml.ExtendUnknownError<baml.errors.SttError> {
      // opens a Deepgram streaming websocket, pushes audio chunks, yields
      // interim+final Transcription frames, finalizes to the full string.
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SttError>(e, "deepgram listen failed");
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function                            // multipart POST to /v1/audio/transcriptions
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "whisper http failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Whisper returns { "text": "..." } at the top level.
      match (baml.json.field(baml.json.parse(from.text()), "text")) {
        let s: string => baml.sap.parse<T>(s),
        _ => baml.sap.parse<T>(""),
      }
    }
  }

  implements SttProvider {
    function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string>
        throws baml.ExtendUnknownError<baml.errors.SttError> {
      // Whisper can't stream: drain the whole clip, POST once, emit ONE final
      // frame. The interface is identical to Deepgram's; the behavior diverges.
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SttError>(e, "whisper listen failed");
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "11labs http failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // T is audio: decode the returned base64 bytes into an AudioChunk-bearing value.
      match (baml.json.field(baml.json.parse(from.text()), "audio_base64")) {
        let s: string => baml.sap.parse<T>(s),
        _ => baml.sap.parse<T>(""),
      }
    }
  }

  implements TtsProvider {
    function speak(self, text: string, voice: string?) -> AudioStream
        throws baml.ExtendUnknownError<baml.errors.TtsError> {
      // voice ?? self.voice_id selects the speaker; stability/similarity_boost
      // ride along in the websocket open frame. Streams audio chunks back.
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.TtsError>(e, "11labs speak failed");
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "cartesia http failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Cartesia returns { "audio": "<base64>" }.
      match (baml.json.field(baml.json.parse(from.text()), "audio")) {
        let s: string => baml.sap.parse<T>(s),
        _ => baml.sap.parse<T>(""),
      }
    }
  }

  implements TtsProvider {
    function speak(self, text: string, voice: string?) -> AudioStream
        throws baml.ExtendUnknownError<baml.errors.TtsError> {
      $rust_io_function                            // POST /tts/bytes, stream sonic audio frames
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.TtsError>(e, "cartesia speak failed");
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
  llm: HttpProvider                               // swap: ANY chat provider (the brain stage needs `call`)
  tts: TtsProvider                                // swap: ElevenLabs | Cartesia | ...
  vad: VadDetector
  speaker: string?                                // default TTS voice for this pipeline

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> VoiceTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
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
        turns.push(VoiceTurn { role: "user", text: heard, start_ms: 0, end_ms: 0 });
        context = context.append_user(heard);

        // LLM: any HttpProvider works — only `.call` is required. The chat stage
        // is a plain HttpProvider, so Whisper-STT × Llama-LLM × Cartesia-TTS all fit.
        let reply: string = self.llm.call<string>(context.to_prompt())
          catch (e) { _ => { return; } };
        turns.push(VoiceTurn { role: "assistant", text: reply, start_ms: 0, end_ms: 0 });
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

  // No fake `call`. `Realtime requires Provider` (the marker), so CascadedVoice
  // already IS a Provider without declaring `implements Provider`. There is no
  // honest "one synchronous voice turn" without a live channel, so we no longer
  // forge a degenerate `call` that silently drops STT/TTS — the cascade exposes
  // ONLY its real interaction, `Realtime.run`. (OQ1 resolved: no degenerate call.)
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

  // It IS a Provider (the marker, inherited via SttProvider/TtsProvider's
  // `requires Provider`), but composite voice has no model of its own. Under the
  // marker revision it simply does NOT implement `HttpProvider`, so there is no
  // `call` to fake or to throw from — the capability is absent, not stubbed. The
  // `.call`-routing companion (`Announce(text)`) discovers this absence at the
  // companion `match` and throws `Unsupported` there (see usage.baml / evaluation §c).

  // Forward the unified verbs to the chosen members.
  implements SttProvider {
    function listen(self, audio: AudioStream) -> baml.llm.Stream<Transcription, string>
        throws baml.ExtendUnknownError<baml.errors.SttError> {
      self.input.listen(audio)
    }
  }
  implements TtsProvider {
    function speak(self, text: string, voice: string?) -> AudioStream
        throws baml.ExtendUnknownError<baml.errors.TtsError> {
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
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> VoiceTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      // one network hop: audio in -> audio out, prosody preserved, low latency,
      // no VAD/barge-in bookkeeping in BAML (the model owns turn-taking).
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "nova sonic run failed");
    }
  }
  // No fake `call`. A single-STS model is a duplex realtime session; "one HTTP
  // turn over the socket" was a degenerate best-effort. `Realtime requires
  // Provider` makes NovaSonic a Provider (marker) already; it exposes ONLY `run`.
}

// ----------------------------------------------------------------------------
// Channel event shapes used above. These extend the canonical `Channel`'s
// In/OutEvent surface with the audio + flush frames a voice transport carries.
// (Net-new: the realtime InEvent/OutEvent unions need audio members.)
// ----------------------------------------------------------------------------
class InEventAudio  { chunk: AudioChunk }          // mic frame from the user
class OutEventAudio { chunk: AudioChunk }          // speaker frame to the user
class OutEventFlush {}                             // drop the speaker queue (barge-in)
