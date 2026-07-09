// scenario 05 — multimodal input: ../../../../../llm-provider/ideas/scenarios/05-multimodal-input/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design's implement.baml hand-rolled an entire net-new stack to make media
// work: a `baml.media.*` host surface (view / download / to_data_url / to_base64),
// value-level view types (`MediaRef` / `MediaTransport` / `PromptView`), and — the
// centrepiece — a `MediaIngest` capability interface whose default `resolve` engine
// ran the supportedUrls-style forward / pre-fetch / pre-upload negotiation, plus a
// concrete `MediaTransportError`. On the REAL surface ALL of that is already stdlib,
// but factored differently than the design guessed (see REALIZED.md §05):
//
//   - Media values are `baml.media.Image/Audio/Pdf/Video` (the `image`/`audio`/`pdf`/
//     `video` primitive types) with `.url()`, `.base64()`, `.mime_type()`, `.file()` —
//     the design's three readable encodings, already on the value.
//   - A prompt's media rides `baml.ai.MessagePart { text?, image?, audio?, pdf?, video? }`
//     inside `baml.ai.ChatMessage[]` — the design's `PromptView`/`MediaPart`, but a
//     product type (one field set) so it crosses the host boundary (gotchas).
//   - The `MediaIngest.resolve` negotiation is NOT a user-facing capability. It is
//     REALIZED per-provider, inline in each stdlib `HttpProvider.build_request`
//     (`OpenAi`/`OpenAiStrict`/`Anthropic`/`Gemini` each walk the parts and emit their
//     own wire form — `image_url` vs `source.base64` vs `inline_data`). The audio/video
//     gaps (`audio ≠ Anthropic`, `video` inline nowhere) fall out as a runtime `throw`
//     from that same method — the design's "detected, not typed" verdict, made real.
//
// So the ONLY genuinely net-new surface THIS scenario adds on the real model is the
// DEGRADE COMBINATOR (design §5): `TranscribeFirst`, a Provider-wrapping combinator
// that rewrites the messages — folding audio the inner model can't ingest into a
// transcript produced by a second (STT) provider — then forwards to `inner`. Plus two
// offline fakes so usage.baml can exercise the degrade path and the audio gap with no
// network and no keys.


// ---------------------------------------------------------------------------
// NET-NEW: the "transcribe first" degrade combinator (design §5).
//
// "Send audio to a provider that doesn't take audio" can either ERROR (the default —
// the inner provider's build_request throws the audio gap) or DEGRADE: transcribe the
// audio with a second provider and inline the transcript as text. Exactly like the
// spine's `Fallback`/`Retry`, this is a plain class that wraps providers and overrides
// the primitive `call_messages_with` (from which `call` derives) to do the real
// routing; the codec methods (build_request/send/parse/parse_meta) have no single
// honest meaning for a combinator, so they throw. The inherited factories
// (`with_retry`/`fallback_to`/`traced`) come free via `implements Provider`.
//
// Faithful to the design's marker revision: it rewrites the messages then FORWARDS to
// `inner`'s `call_messages_with`, so the inner chat completion's `ResponseMeta`
// (usage/cost) reaches the caller's projection instead of being dropped (the P1 fix).
// ---------------------------------------------------------------------------
class s05_TranscribeFirst {
    inner: baml.ai.Provider,      // the target chat model (e.g. Anthropic — no audio)
    stt: baml.ai.Provider,        // a speech-to-text provider used to turn audio into text

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["transcribe-first has no single request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["transcribe-first does not send directly"] }
        }
        function parse<T>(
            self,
            from: unknown,
        ) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["transcribe-first does not parse directly"] }
        }
        function parse_meta(
            self,
            from: unknown,
        ) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["transcribe-first has no meta"] }
        }

        // The real routing: fold every audio part into "[audio transcript: …]" text via
        // `stt`, then forward the rewritten messages to `inner`'s call_messages_with so
        // its ResponseMeta reaches `project` (not thrown away).
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let rewritten: baml.ai.ChatMessage[] = [];
            for (let m in messages) {
                let new_parts: baml.ai.MessagePart[] = [];
                for (let p in m.parts) {
                    // House style: null-eliminate the media field (a `_` arm after a media
                    // binding breaks the runtime type-test — see _plan/baml_gotchas.md).
                    match (p.audio) {
                        null => {
                            new_parts.push(p);
                        },
                        let a: audio => {
                            // Transcribe this one clip with the STT provider, then inline it.
                            let clip_msgs: baml.ai.ChatMessage[] = [
                                baml.ai.ChatMessage { role: "user", parts: [baml.ai.MessagePart { audio: a }] },
                            ];
                            let transcript: string = match (self.stt) {
                                let h: baml.ai.HttpProvider => h.call_messages<string>(clip_msgs),
                                _ => {
                                    throw baml.errors.Unsupported { message: "stt provider cannot transcribe (no request/response call)" };
                                },
                            };
                            new_parts.push(baml.ai.MessagePart.from_text("[audio transcript: " + transcript + "]"));
                        },
                    }
                }
                rewritten.push(baml.ai.ChatMessage { role: m.role, parts: new_parts });
            }
            // Forward to inner's request/response call_with so its usage/cost reaches `project`.
            match (self.inner) {
                let h: baml.ai.HttpProvider => {
                    return h.call_messages_with<T, V, E2>(rewritten, project);
                },
                _ => {
                    throw baml.errors.Unsupported { message: "transcribe-first inner is not request/response" };
                },
            }
        }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #1 — a provider WITH THE AUDIO GAP, network-free.
//
// The design wraps real Anthropic (which rejects audio) with TranscribeFirst. Offline
// that can't complete (Anthropic's `send` needs the network even once the audio is
// folded away). So this fake models the SAME shape as Anthropic — "I reject audio
// parts, but answer plain text" — with no network, letting the degrade path be proven
// deterministically. It runs the codec-bypassing `call_messages_with` (like the stdlib
// combinators): scan for an audio part → throw `Unsupported` (the gap); otherwise echo.
// ---------------------------------------------------------------------------
class s05_AudioRejectingEcho {
    reply: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["audio-rejecting echo has no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["audio-rejecting echo does not send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["audio-rejecting echo parse failed"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            // The audio gap, in data: this provider cannot ingest an audio part.
            for (let m in messages) {
                for (let p in m.parts) {
                    match (p.audio) {
                        null => {},
                        let a: audio => {
                            throw baml.errors.Unsupported { message: "provider does not accept media kind: audio" };
                        },
                    }
                }
            }
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["audio-rejecting echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #2 — a scripted STT provider. Returns a fixed transcript for any
// audio clip so the TranscribeFirst degrade path is deterministic offline; stands in
// for the design's Gemini-as-STT (`stt: Flash()`).
// ---------------------------------------------------------------------------
class s05_SttEcho {
    transcript: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["stt echo has no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["stt echo does not send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["stt echo parse failed"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.transcript) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["stt echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}
