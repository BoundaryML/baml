// scenario 06 — non-text output: ../../../../../llm-provider/ideas/scenarios/06-non-text-output/usage.baml
// ============================================================================
// 06 · Non-text output — APPLICATION code (what an end user writes)
// ============================================================================
//
// The library (implementation.baml) did the heavy lifting. Here we show the
// ergonomics: functions name a return type, and the dedicated-vs-in-conversation
// divergence shows up as a different `-> T` (s06_Media vs s06_MixedReply). The two
// cost meters and the revised prompt ride the `$with` sidecar companion (the
// design's `.call_with`), uniform across both paths.
//
// Design clients (Painter / Narrator / Scribe / VisualChat / SavedPainter) were
// `client Name() { <custom provider> }` sugar. On the real surface a `client`
// cannot hold an inline custom provider, so — exactly as scenario 02 does — the
// LLM functions carry a dead default client and the tests / live testset inject the
// real provider (or an offline fake) via the `client =` override.

// The swappable default client. Offline tests override it with s06_MediaEcho; the
// live testset overrides it with the real s06_GptImage / s06_Gpt55 providers.
client<llm> s06_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// ----------------------------------------------------------------------------
// Functions — the `-> T` is where the modality lives.
// ----------------------------------------------------------------------------

// Dedicated path: T = s06_Media. The app gets mime + payload back, not a string.
function s06_DrawFox(subject: string) -> s06_Media {
    client s06_Dead
    prompt #"a watercolor {{ subject }}"#
}

// TTS: input text -> s06_Media (audio). (Live byte-decode is BLOCKED P8; the fake
// produces the audio Media offline so the modality still round-trips in tests.)
function s06_Speak(line: string) -> s06_Media {
    client s06_Dead
    prompt #"{{ line }}"#
}

// STT INVERTS modality: the media is an INPUT (an audio attachment in the prompt),
// the output is a plain string. Same HttpProvider path — SAP is back on this one.
function s06_Transcribe(clip: audio) -> string {
    client s06_Dead
    prompt #"{{ clip }}"#
}

// In-conversation path: T = s06_MixedReply. One call returns interleaved text +
// image; BOTH cost meters ride the sidecar, not a field on the reply.
function s06_IllustrateAnswer(question: string) -> s06_MixedReply {
    client s06_Dead
    prompt #"Answer and illustrate: {{ question }}"#
}

// Partial-preview image streaming. `.stream` matches the provider against Streaming;
// the real GptImage/Gpt55 CLAIM it, but driving it is BLOCKED P8 (media-frame stream
// constructor). A non-Streaming provider is a typed Unsupported (tested below).
function s06_PreviewFox(subject: string) -> baml.llm.Stream<s06_Media, s06_Media> {
    client s06_Dead
    prompt #"a watercolor {{ subject }}"#
}

// COMPILE-TIME-PRECISE escape hatch (design §3): drop the client sugar so the
// return type is the concrete provider, making "this only yields images" STATIC —
// the only fix for the OQ4 output-constraint leak, at the cost of the sugar.
// (The real from_model reads env.OPENAI_API_KEY; a literal keeps this offline-safe.)
function s06_DallePrecise() -> s06_GptImage {
    s06_GptImage { model: "gpt-image-1", api_key: "unused", size: "1024x1024", partial_images: 0 }
}

// ----------------------------------------------------------------------------
// Reading the two cost meters at the app layer — the PRODUCT SIDECAR path via the
// `$with` companion (the design's `.call_with`). It returns a CallResult<T, V>: the
// answer AND whatever the `project` closure pulled off the ResponseMeta. The
// two-meter cost is an EXTERNAL dimension (s06_MediaCost), reached by a runtime
// `match` — the SAME projection on the dedicated (-> Media) and in-conversation
// (-> MixedReply) paths, so a Fallback no longer degrades usage to zero.
// ----------------------------------------------------------------------------
function s06_CostAwareIllustrate(question: string, client: baml.ai.Provider) -> string {
    let r = s06_IllustrateAnswer$with(
        question,
        (m: baml.ai.ResponseMeta) -> s06_Usage {
            match (m) {
                let mc: s06_MediaCost => mc.media_cost(),
                _ => s06_Usage { mainline_tokens: -1, generation_units: -1, generation_kind: "unknown" },
            }
        },
        client = client,
    ) catch (e) {
        _ => baml.ai.CallResult<s06_MixedReply, s06_Usage> {
            value: s06_MixedReply { text: null, media: [] },
            meta: s06_Usage { mainline_tokens: -1, generation_units: -1, generation_kind: "err" },
        },
    };
    // The app branches on which path billed what — dedicated reports mainline == 0.
    "billed " + r.meta.mainline_tokens.to_string() + " mainline + " + r.meta.generation_units.to_string() + " " + r.meta.generation_kind
}

// The dedicated path projects the SAME `media_cost()` — the seam usage USED to fall
// through (Media had no usage slot, MixedReply did) is gone: one projection, both paths.
function s06_CostAwareDraw(subject: string, client: baml.ai.Provider) -> string {
    let r = s06_DrawFox$with(
        subject,
        (m: baml.ai.ResponseMeta) -> s06_Usage {
            match (m) {
                let mc: s06_MediaCost => mc.media_cost(),
                _ => s06_Usage { mainline_tokens: -1, generation_units: -1, generation_kind: "unknown" },
            }
        },
        client = client,
    ) catch (e) {
        _ => baml.ai.CallResult<s06_Media, s06_Usage> {
            value: s06_Media { mime: "err", data: null, url: null },
            meta: s06_Usage { mainline_tokens: -1, generation_units: -1, generation_kind: "err" },
        },
    };
    "drew " + r.value.mime + " — billed " + r.meta.generation_units.to_string() + " " + r.meta.generation_kind
}

// The provider-revised prompt is ALSO a sidecar (it left the Media value). It is an
// EXTERNAL dimension (s06_RevisesPrompt), reached by a runtime `match` through the
// ResponseMeta — the §7 escape-hatch pattern. A provider that can't revise (tts/stt)
// is an `Unavailable` match-miss, not a silent null.
function s06_DrawAndShowRewrite(subject: string, client: baml.ai.Provider) -> string {
    let r = s06_DrawFox$with(
        subject,
        (m: baml.ai.ResponseMeta) -> string | baml.ai.Unavailable {
            match (m) {
                let rp: s06_RevisesPrompt => rp.revised_prompt(),
                _ => baml.ai.Unavailable { reason: "provider does not revise prompts" },
            }
        },
        client = client,
    ) catch (e) {
        _ => baml.ai.CallResult<s06_Media, string | baml.ai.Unavailable> {
            value: s06_Media { mime: "err", data: null, url: null },
            meta: baml.ai.Unavailable { reason: "err" },
        },
    };
    match (r.meta) {
        let rev: string => "drew " + r.value.mime + " (rewrote to: " + rev + ")",
        let un: baml.ai.Unavailable => "drew " + r.value.mime + " (prompt used as written)",
    }
}

// ----------------------------------------------------------------------------
// Portability check across the two divergent providers, side by side. Both reach
// the value via HttpProvider — but the return type MUST differ (Media vs
// MixedReply), which is where the abstraction leaks (design (c)#1): an app cannot
// write one function `-> Media` and point it at either provider.
// ----------------------------------------------------------------------------
function s06_TwoPaths(subject: string, img_client: baml.ai.Provider, mix_client: baml.ai.Provider) -> s06_Media {
    let dedicated: s06_Media = s06_DrawFox(subject, client = img_client);            // GptImage -> Media
    let mixed: s06_MixedReply = s06_IllustrateAnswer("draw a " + subject, client = mix_client); // Gpt55 -> MixedReply
    // To treat them uniformly the app must reach INTO MixedReply for the media.
    match (mixed.media.at(0)) { let first: s06_Media => first, null => dedicated }
}

// ----------------------------------------------------------------------------
// Consumer error triage — image-gen moderation is a TYPED domain decline. Runtime
// match, most-specific first: concrete s06_Refused -> s06_OpenAiMediaHttpError ->
// the CallError / StreamError interfaces -> UnknownError (the escape hatch).
// ----------------------------------------------------------------------------
function s06_DrawOrExplain(subject: string, client: baml.ai.Provider) -> string {
    // The catch rides the call EXPRESSION (each arm returns); most-specific first:
    // concrete s06_Refused -> s06_OpenAiMediaHttpError -> the CallError / StreamError
    // interfaces -> UnknownError (the escape hatch).
    let m: s06_Media = s06_DrawFox(subject, client = client) catch (e) {
        let r: s06_Refused => return "[declined: " + r.category + " — " + r.message + "]",
        let h: s06_OpenAiMediaHttpError => return "[http " + h.status.to_string() + ": " + h.detail + "]",
        let c: baml.errors.CallError => return if (c.is_network_error()) { "[transient, retry]" } else { "[failed]" },
        let s: baml.errors.StreamError => return "[stream failed]",
        let u: baml.errors.UnknownError => return "[unexpected: " + u.message.join("; ") + "]",
    };
    "drew " + m.mime
}

// ============================================================================
// Offline fixtures — a single meta-bearing echo serves every modality. It SAP-
// parses `reply_json` into the caller's T (Media / MixedReply / string) and returns
// the supplied `meta` (an s06_ImageMeta / s06_AudioMeta / s06_ResponsesMeta), so the
// $with projections and the external-dimension `match`es exercise the real shapes.
// ============================================================================
class s06_MediaEcho {
    reply_json: string,
    meta: baml.ai.ResponseMeta,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["media echo has no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["media echo does not send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["media echo parse failed"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            self.meta
        }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["media echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(self.meta) }
        }
    }
}

// A provider that declines to draw — the offline moderation fixture.
class s06_RefusingImage {
    implements baml.ai.Provider {}
    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no parse"] }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            throw s06_Refused { category: "image_moderation", message: "the prompt was declined" }
        }
    }
}

// Canonical fixtures reused across tests.
function s06_image_reply() -> string throws never { "{\"mime\": \"image/png\", \"data\": \"aGk=\", \"url\": null}" }
function s06_audio_reply() -> string throws never { "{\"mime\": \"audio/mp3\", \"data\": \"YXVkaW8=\", \"url\": null}" }
function s06_mixed_reply() -> string throws never {
    "{\"text\": \"Here is your fox\", \"media\": [{\"mime\": \"image/png\", \"data\": \"aGk=\", \"url\": null}]}"
}
function s06_image_meta_raw() -> string throws never {
    "{\"data\": [{\"b64_json\": \"aGk=\", \"revised_prompt\": \"a watercolor red fox, high detail\"}]}"
}
function s06_responses_meta_raw() -> string throws never {
    "{\"output\": [{\"type\": \"image_generation_call\", \"result\": \"aGk=\", \"revised_prompt\": \"a diagram of the water cycle\"}, {\"type\": \"message\", \"content\": [{\"text\": \"Here it is\"}]}], \"usage\": {\"total_tokens\": 42}}"
}

// ============================================================================
// OFFLINE TESTS (deterministic, no network / no keys)
// ============================================================================

test "06: dedicated path returns Media (non-text output), not a string" {
    let m = s06_DrawFox("fox", client = s06_MediaEcho {
        reply_json: s06_image_reply(),
        meta: s06_ImageMeta { raw: s06_image_meta_raw() },
    }) catch (e) { _ => s06_Media { mime: "failed", data: null, url: null } };
    assert.equal(m.mime, "image/png")
    assert.equal(m.data ?? "nil", "aGk=")
}

test "06: TTS produces an audio Media over the SAME HttpProvider path" {
    let m = s06_Speak("hello there", client = s06_MediaEcho {
        reply_json: s06_audio_reply(),
        meta: s06_AudioMeta { kind: "audio" },
    }) catch (e) { _ => s06_Media { mime: "failed", data: null, url: null } };
    assert.equal(m.mime, "audio/mp3")
}

test "06: STT inverts modality — audio input, plain-string output (SAP is back)" {
    let clip = baml.media.Audio.from_base64("YXVkaW8=", "audio/mp3");
    let text = s06_Transcribe(clip, client = s06_MediaEcho {
        reply_json: "the quick brown fox",
        meta: s06_AudioMeta { kind: "transcription" },
    }) catch (e) { _ => "failed" };
    assert.equal(text, "the quick brown fox")
}

test "06: in-conversation path returns a MixedReply — interleaved text + media" {
    let r = s06_IllustrateAnswer("what is a fox?", client = s06_MediaEcho {
        reply_json: s06_mixed_reply(),
        meta: s06_ResponsesMeta { raw: s06_responses_meta_raw() },
    }) catch (e) { _ => s06_MixedReply { text: "failed", media: [] } };
    assert.equal(r.text ?? "nil", "Here is your fox")
    assert.equal(r.media.length(), 1)
    match (r.media.at(0)) {
        let first: s06_Media => assert.equal(first.mime, "image/png"),
        null => assert.is_true(false),
    }
}

test "06: the two cost meters ride the $with sidecar — dedicated bills mainline == 0" {
    let out = s06_CostAwareDraw("fox", s06_MediaEcho {
        reply_json: s06_image_reply(),
        meta: s06_ImageMeta { raw: s06_image_meta_raw() },
    });
    // dedicated endpoint: 0 mainline tokens, exactly 1 generation unit, kind image.
    assert.equal(out, "drew image/png — billed 1 image")
}

test "06: the in-conversation path bills BOTH meters through the SAME projection" {
    let out = s06_CostAwareIllustrate("what is the water cycle?", s06_MediaEcho {
        reply_json: s06_mixed_reply(),
        meta: s06_ResponsesMeta { raw: s06_responses_meta_raw() },
    });
    // responses path: 42 mainline tokens PLUS 1 generated image.
    assert.equal(out, "billed 42 mainline + 1 image")
}

test "06: revised_prompt is a sidecar dimension — the image endpoint echoes it back" {
    let out = s06_DrawAndShowRewrite("fox", s06_MediaEcho {
        reply_json: s06_image_reply(),
        meta: s06_ImageMeta { raw: s06_image_meta_raw() },
    });
    assert.equal(out, "drew image/png (rewrote to: a watercolor red fox, high detail)")
}

test "06: a provider that can't revise is an Unavailable match-miss, not a silent null" {
    // s06_AudioMeta does NOT implement s06_RevisesPrompt — the `match` misses and the
    // projection returns the typed "does not revise" branch.
    let out = s06_DrawAndShowRewrite("a spoken line", s06_MediaEcho {
        reply_json: s06_image_reply(),
        meta: s06_AudioMeta { kind: "audio" },
    });
    assert.equal(out, "drew image/png (prompt used as written)")
}

test "06: image-gen moderation is a typed Refused the app catches by name" {
    let out = s06_DrawOrExplain("something disallowed", s06_RefusingImage {});
    assert.equal(out, "[declined: image_moderation — the prompt was declined]")
}

test "06: a non-refusal failure triages through the CallError interface" {
    let out = s06_DrawOrExplain("fox", AlwaysFailProvider {});
    // AlwaysFailProvider throws a bare UnknownError → the escape-hatch arm.
    assert.is_true(out.starts_with("[unexpected"))
}

test "06: the dedicated/in-conversation split is NOT portable — T diverges (design (c)#1)" {
    // TwoPaths must reach INTO MixedReply for the media because there is no single T
    // both providers produce. Here the in-conversation media[0] wins.
    let m = s06_TwoPaths(
        "fox",
        s06_MediaEcho { reply_json: s06_image_reply(), meta: s06_ImageMeta { raw: s06_image_meta_raw() } },
        s06_MediaEcho { reply_json: s06_mixed_reply(), meta: s06_ResponsesMeta { raw: s06_responses_meta_raw() } },
    ) catch (e) { _ => s06_Media { mime: "failed", data: null, url: null } };
    assert.equal(m.mime, "image/png")
}

test "06: the escape hatch returns a CONCRETE provider — 'yields only images' is static" {
    let p = s06_DallePrecise();
    // p's static type is s06_GptImage (not the existential Provider), so the output
    // constraint is compile-time — the OQ4 fix, at the cost of the client sugar.
    assert.equal(p.model, "gpt-image-1")
    assert.equal(p.size, "1024x1024")
}

test "06: .stream on a non-streaming provider is a typed Unsupported (negotiation)" {
    // The offline echo does not implement Streaming; $stream degrades to a typed
    // Unsupported. (The real GptImage/Gpt55 CLAIM Streaming but driving it is BLOCKED P8.)
    let outcome = s06_PreviewFox$stream("fox", client = s06_MediaEcho {
        reply_json: s06_image_reply(),
        meta: s06_ImageMeta { raw: s06_image_meta_raw() },
    }) catch (e) {
        // drive_stream throws a typed Unsupported (a StreamError whose Failure axis
        // answers is_unsupported()); triage it off the interface, no concrete arm.
        let s: baml.errors.StreamError => if (s.is_unsupported()) { "unsupported" } else { "stream-error" },
        _ => "other",
    };
    assert.equal(outcome, "unsupported")
}

test "06: SaveTo forwards the sidecar — usage survives the combinator (P1)" {
    // The combinator wraps the inner provider and forwards `project` to its
    // call_messages_with, so the projected cost is NOT dropped (the media-persist
    // side effect is BLOCKED P8; forwarding is the point).
    let saver: baml.ai.Provider = s06_SaveTo {
        inner: s06_MediaEcho { reply_json: s06_image_reply(), meta: s06_ImageMeta { raw: s06_image_meta_raw() } },
        dir: "./out",
    };
    let r = match (saver) {
        let h: baml.ai.HttpProvider => h.call_with<s06_Media, s06_Usage, never>(
            "a watercolor fox",
            (m: baml.ai.ResponseMeta) -> s06_Usage {
                match (m) {
                    let mc: s06_MediaCost => mc.media_cost(),
                    _ => s06_Usage { mainline_tokens: -1, generation_units: -1, generation_kind: "unknown" },
                }
            },
        ) catch (e) {
            _ => baml.ai.CallResult<s06_Media, s06_Usage> {
                value: s06_Media { mime: "err", data: null, url: null },
                meta: s06_Usage { mainline_tokens: -9, generation_units: -9, generation_kind: "err" },
            },
        },
        _ => baml.ai.CallResult<s06_Media, s06_Usage> {
            value: s06_Media { mime: "not-http", data: null, url: null },
            meta: s06_Usage { mainline_tokens: -1, generation_units: -1, generation_kind: "err" },
        },
    };
    assert.equal(r.value.mime, "image/png")
    assert.equal(r.meta.generation_units, 1)          // the sidecar survived SaveTo
    assert.equal(r.meta.generation_kind, "image")
}

// ============================================================================
// LIVE TIER — the dedicated image endpoint (excluded by default; run with keys).
// Non-text OUTPUT is media-gated (P8): image gen returns b64 which the host decoder
// lifts, but tts byte-decode / partial-image streaming are BLOCKED. So the live
// tier here covers only the JSON-bodied image-generation path.
// ============================================================================
testset "integ-test-06-non-text-output" {
    // BLOCKED (partial): live media output rides P8 host surface. This shape drives
    // the real dedicated image endpoint; the b64→Image decode is real, but the model
    // name / response_format are unverified against a live key.
    test "06 live: dedicated image generation returns a decoded Media" {
        let m = s06_DrawFox("a red fox in watercolor", client = s06_GptImage {
            model: "gpt-image-1",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            size: "1024x1024",
            partial_images: 0,
        }) catch (e) {
            _ => {
                log.warn(e);
                s06_Media { mime: "live failed", data: null, url: null }
            },
        };
        log.info("06 image mime: " + m.mime);
        assert.is_true(m.mime.starts_with("image/"))
    }
}
