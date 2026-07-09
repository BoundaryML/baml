// scenario 06 — non-text output: ../../../../../llm-provider/ideas/scenarios/06-non-text-output/implement.baml
// ============================================================================
// 06 · Non-text output (image / speech / transcription) — LIBRARY code
// ============================================================================
//
// Builds on the real `baml.ai` spine (Provider marker, HttpProvider with the
// `type Body` seam, ResponseMeta, CallResult). Nothing here redefines those — we
// only ADD, exactly as the design's implement.baml did:
//
//   • a `s06_Media` result type (mime + data | url)      ← net-new value type
//   • a two-meter `s06_Usage` cost record                ← net-new value type
//   • an external `s06_MediaCost` ResponseMeta dimension ← the two meters as a
//     projected sidecar (see divergence #3 in the header block below)
//   • an external `s06_RevisesPrompt` ResponseMeta dimension (image endpoints
//     rewrite your prompt and echo it back; chat/tts/stt do not)
//   • two provider SHAPES for the same capability:
//       (a) dedicated-endpoint, output-specialized (GptImage / GptTts / GptStt)
//       (b) in-conversation built-in tool (Gpt55 returns mixed modality)
//
// The teaching point survives intact: the SAME `HttpProvider` codec absorbs an
// output that is NOT a string and NOT SAP-parsed — the divergence lives entirely
// in `parse` + the associated `type Body` (design §2 / OQ4). The PRODUCT sidecars
// (the two cost meters, the revised prompt) ride `HttpProvider.call_with` +
// `ResponseMeta` — NOT a bespoke `Metered`/`last_usage` companion — so they
// survive a Fallback/Retry instead of degrading to zero (the P1 win).
//
// ── Divergences from the frozen design → the real surface ───────────────────
//   1. Media.data: uint8array?  →  string? (base64). JSON/SAP carry strings and
//      there is no exposed base64→bytes host on the *value* path; the byte-typed
//      slot has no home in a SAP-parseable class.
//   2. baml.media.as<T> downcast  →  s06_downcast<S,T>: serialize the value with
//      baml.json.to_string then baml.sap.parse<T>. SAP is the only *generic*
//      value→T mechanism on the real surface; it PRESERVES the design's (c)#2
//      teaching point (a Draw()->Resume misuse still blows up at runtime).
//   3. The two-meter Usage was projected as `m => m.usage()`. On the real surface
//      ResponseMeta.usage() returns the TOKEN-ONLY `baml.ai.Usage`, so the media
//      cost meters ride an EXTERNAL dimension `s06_MediaCost` reached by a runtime
//      `match` — which is exactly the design's OWN external-dimension pattern
//      (RevisesPrompt), now applied to cost as well.
//   4. Supported<T> = T | Unsupported  →  T | baml.ai.Unavailable.
//   5. tuple (T, U)  →  baml.ai.CallResult<T, V>.
//   6. call/call_with on the bare Provider marker → on HttpProvider; the primitive
//      is call_messages_with and the exchange type is native ChatMessage[].
//   7. meta_of  →  parse_meta.
//   8. type Body = baml.http.Response → s06_RawResp {status, ok, text}, captured
//      by `send` in ONE read (Response.text() is not idempotent — a double read in
//      both parse and parse_meta would fail) and carrying the status code.
//   9. TTS audio-byte decode: no Audio.from_bytes / bytes→base64 host exists.
//      BLOCKED: P8 media-byte output surface (see s06_GptTts.parse).
//  10. Partial-image SSE streaming: the media-frame stream constructor is net-new.
//      BLOCKED: P8 (see the Streaming impls).
//  11. SaveTo.persist_if_media: no media-persist host. BLOCKED: P8 (the combinator
//      still FORWARDS call_with, which is the actual P1 point being demonstrated).

// ----------------------------------------------------------------------------
// Net-new value types
// ----------------------------------------------------------------------------

// The text-shaped abstraction does not fit non-text output (background §"What's
// hard": "Output isn't a string"). A media result carries a mime type plus the
// payload, which is EITHER inline bytes OR a hosted URL — providers differ on
// which (OpenAI dedicated = b64 bytes; hosted images = URL; Gemini = inline part).
// One type papers over both. (`data` is the base64 payload — divergence #1.)
class s06_Media {
    mime: string,               // "image/png", "audio/mp3", ...
    data: string?,              // inline payload, base64-encoded, if present
    url: string?,               // hosted asset, if the provider returned a link
    // NOTE: `revised_prompt` USED to live here as a value field. It is a PRODUCT
    // SIDECAR ("the value, plus what the provider changed about your request"), so
    // it rides call_with + s06_RevisesPrompt.revised_prompt() now. Media is JUST
    // the asset again.
}

// Two cost meters (background §"What's hard": "Two paths, two cost meters"). A
// dedicated endpoint bills ONLY generation. An in-conversation tool bills mainline
// tokens PLUS generation. Keeping both fields lets the app see which path it took.
// This is the `V` a caller projects out of the sidecar via call_with — NOT a field
// stuffed into the answer `T`.
class s06_Usage {
    mainline_tokens: int,       // 0 for a dedicated endpoint
    generation_units: int,      // images, audio-seconds, or transcription-seconds
    generation_kind: string,    // "image" | "audio" | "transcription"
}

// A response that may interleave modalities. The in-conversation path (Gpt55)
// returns this; a dedicated endpoint never does. `T` in the call is specialized to
// s06_Media for dedicated models and to s06_MixedReply for the tool path.
class s06_MixedReply {
    text: string?,              // the otherwise-textual reply, if any
    media: s06_Media[],         // generated assets, in output order
    // NOTE: `usage` USED to be a field here — the in-conversation path folded the
    // two meters into `T` because `call<T> -> T` had no other slot. That was the
    // (c)#3 awkwardness: the dedicated path (`-> Media`) had nowhere to put the
    // same figure. With call_with, usage is a uniform SIDECAR on BOTH paths, so
    // MixedReply is just the interleaved value again.
}

// ----------------------------------------------------------------------------
// Concrete typed errors this scenario raises on purpose. Each implements the
// mandatory `Failure` decision axis PLUS the capability interface(s) it can
// surface on, so the deliberate throw is legal on the method's channel and the
// app branches on it by runtime match.
// ----------------------------------------------------------------------------

// Image-gen prompt moderation: an HTTP-level decline where the provider refuses to
// draw the prompt. Canonical Refused shape — a decline is neither the answer nor a
// transport failure; it is its own typed outcome the app catches by name.
class s06_Refused {
    category: string,
    message: string,
}
implements baml.errors.Failure for s06_Refused {
    function is_retryable(self) -> bool { false }       // a decline is authoritative
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { true }    // THE bit apps/combinators triage on
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
}
implements baml.errors.CallError for s06_Refused {
    function is_network_error(self) -> bool { false }
    function is_rate_limit(self) -> bool { false }
    function is_parse_error(self) -> bool { false }      // a refusal is NOT a parse failure
}
// The same decline can surface mid-preview-stream, so it rides StreamError too.
implements baml.errors.StreamError for s06_Refused {
    function is_network_error(self) -> bool { false }
}

// A non-2xx from a media endpoint (image/tts/stt/responses). Carries the status so
// the app can triage; classifies as neither network nor rate-limit nor parse.
class s06_OpenAiMediaHttpError {
    status: int,
    detail: string,
}
implements baml.errors.Failure for s06_OpenAiMediaHttpError {
    function is_retryable(self) -> bool { self.status >= 500 }
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { false }
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
}
implements baml.errors.CallError for s06_OpenAiMediaHttpError {
    function is_network_error(self) -> bool { false }
    function is_rate_limit(self) -> bool { self.status == 429 }
    function is_parse_error(self) -> bool { false }
}
implements baml.errors.StreamError for s06_OpenAiMediaHttpError {
    function is_network_error(self) -> bool { false }
}

// ----------------------------------------------------------------------------
// External ResponseMeta dimensions (the design's §7 "novel dimension added later"
// pattern). Both are reached by a runtime `match (m) { let d: <Dim> => ... }` over
// the projected ResponseMeta — so a provider that cannot supply the dimension is a
// typed match-miss, never a silent null.
// ----------------------------------------------------------------------------

// The two-meter cost record as an external sidecar dimension (divergence #3): the
// base ResponseMeta.usage() is token-only, so media cost rides here. It
// `requires baml.ai.ResponseMeta` so a projected `ResponseMeta` value narrows to it
// via a runtime `match` (the same sub-interface downcast as Provider → HttpProvider).
interface s06_MediaCost requires baml.ai.ResponseMeta {
    function media_cost(self) -> s06_Usage throws never
}

// Image endpoints REWRITE your prompt and echo the rewrite back; chat/tts/stt do
// not. `string | Unavailable` keeps "this provider can't revise" distinct from
// "no rewrite this call". Also a ResponseMeta sub-interface (see above).
interface s06_RevisesPrompt requires baml.ai.ResponseMeta {
    function revised_prompt(self) -> string | baml.ai.Unavailable throws never
}

// ----------------------------------------------------------------------------
// Shared helpers.
// ----------------------------------------------------------------------------

// Capture the wire response in ONE read (divergence #8): Response.text() is not
// idempotent, so a Body that both `parse` and `parse_meta` re-read would fail. This
// s06_RawResp is the associated `type Body` — status + ok + the body text.
class s06_RawResp {
    status: int,
    ok: bool,
    text: string,
}
// Flatten the native ChatMessage[] to raw instruction text — a dedicated endpoint
// (image/tts) has no role structure. (The stdlib's _messages_text is private, so
// this scenario carries its own; only exercised on the live path.)
function s06_messages_text(messages: baml.ai.ChatMessage[]) -> string throws never {
    let out = "";
    for (let m in messages) {
        for (let p in m.parts) {
            out = out + (p.text ?? "");
        }
    }
    out
}

function s06_send_raw(request: baml.http.Request) -> s06_RawResp
    throws baml.errors.CallError | baml.errors.UnknownError {
    let resp = baml.http.send(request) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s06 http send failed"] },
    };
    let t = resp.text() catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s06 body read failed"] },
    };
    s06_RawResp { status: resp.status_code, ok: resp.ok(), text: t }
}

// The `baml.media.as<T>` downcast analog (divergence #2): a provider only ever
// produces a concrete value (s06_Media / s06_MixedReply), but parse<T> must return
// T. Serialize the value and SAP it into T. When T matches the concrete shape it
// round-trips; when an app mis-declares the return type (T = Resume) this blows up
// at runtime — the design's (c)#2 leak, preserved. `S` is the concrete source shape.
function s06_downcast<S, T>(v: S) -> T throws baml.errors.UnknownError {
    let s = baml.json.to_string<S>(v) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s06 value serialize failed"] },
    };
    baml.sap.parse<T>(s) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s06 value downcast failed"] },
    }
}

// Wire shapes read off the JSON body via baml.json.from_json (the real openai.baml
// idiom — there is no Response.json_path).
class s06_ImageDatumWire { b64_json: string?, revised_prompt: string? }
class s06_ImageWire { data: s06_ImageDatumWire[] }

class s06_RespContentWire { text: string? }
class s06_RespItemWire {
    type: string,
    result: string?,            // b64 image for an image_generation_call item
    revised_prompt: string?,
    content: s06_RespContentWire[]?,
}
class s06_RespUsageWire { total_tokens: int? }
class s06_RespWire { output: s06_RespItemWire[], usage: s06_RespUsageWire? }

// ============================================================================
// (a) DEDICATED ENDPOINTS — output-specialized providers.
//     Body is captured wire text; `parse` decodes payload, never SAP (except STT,
//     which inverts modality back to text). `T` is pinned to s06_Media for these —
//     they cannot produce an arbitrary schema (the OQ4 output-constraint leak).
// ============================================================================

class s06_GptImage {
    model: string,              // "gpt-image-1"
    api_key: string,
    size: string,               // "1024x1024" — a provider FIELD, not an options blob
    partial_images: int,        // 0..3 preview frames (drives Streaming below)

    // Static factory — the design's from_model / the escape-hatch constructor.
    // (get_or_panic reads the env at call time; its lookup rides Io | ParseError.)
    function from_model(model: string) -> s06_GptImage throws baml.errors.Io | baml.errors.ParseError {
        s06_GptImage { model: model, api_key: baml.env.get_or_panic("OPENAI_API_KEY"), size: "1024x1024", partial_images: 0 }
    }

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = s06_RawResp

        // The schema does NOT go on the wire as a response_format — the endpoint
        // only emits images. `T` is here for the interface but build_request ignores
        // it (the divergence from a chat model that branches on where the schema lands).
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.CallError | baml.errors.UnknownError {
            let body: map<string, baml.json.json> = {
                "model": self.model,
                "prompt": s06_messages_text(messages),   // image endpoint takes raw text, no roles
                "size": self.size,
                // NB: no `response_format` — gpt-image-1 rejects the param
                // (DALL-E-only knob) and always returns b64_json anyway.
            };
            baml.http.Request {
                method: "POST",
                url: "https://api.openai.com/v1/images/generations",
                headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
                body: baml.json.stringify(body),
            }
        }

        function send(self, request: baml.http.Request) -> s06_RawResp
            throws baml.errors.CallError | baml.errors.UnknownError {
            s06_send_raw(request)
        }

        // No SAP. Decode b64 → Media via the host image decoder, then downcast to T.
        function parse<T>(self, from: s06_RawResp) -> T
            throws baml.errors.CallError | baml.errors.UnknownError {
            if (!from.ok) {
                // prompt moderation is a deliberate domain decline — a typed Refused,
                // distinct from transport failure.
                throw s06_Refused { category: "image_moderation", message: from.text };
            }
            let wire: s06_ImageWire = baml.json.from_json<s06_ImageWire>(baml.json.parse(from.text)) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["openai image parse failed"] },
            };
            match (wire.data.at(0)) {
                let d: s06_ImageDatumWire => {
                    let img = baml.media.Image.from_base64(d.b64_json ?? "", "image/png");
                    // revised_prompt is a SIDECAR, read lazily by s06_ImageMeta — not here.
                    s06_downcast<s06_Media, T>(s06_Media { mime: img.mime_type() ?? "image/png", data: img.base64(), url: null })
                },
                _ => throw baml.errors.UnknownError { data: null, message: ["openai image: no data"] },
            }
        }

        // The sidecar normalizer — wraps the captured body so usage / the revised
        // prompt read LAZILY (a projection of `m.media_cost()` never touches the
        // revised_prompt path).
        function parse_meta(self, from: s06_RawResp) -> baml.ai.ResponseMeta throws never {
            s06_ImageMeta { raw: from.text }
        }
    }

    // Image gen can stream PARTIAL preview images (0..3). Same call, richer delivery
    // → a Streaming refinement, not a different shape.
    implements baml.ai.Streaming {
        function stream_messages<TStream, TFinal>(self, messages: baml.ai.ChatMessage[])
            -> baml.llm.Stream<TStream, TFinal>
            throws baml.errors.StreamError | baml.errors.UnknownError {
            // BLOCKED: P8 — partial-image SSE needs a media-frame stream constructor
            // (the design's baml.llm.stream_from_sse over Media frames), which is
            // net-new host surface. The capability is CLAIMED (negotiation sees it);
            // driving it is not yet possible.
            throw baml.errors.Unsupported { message: "s06 partial-image streaming: BLOCKED P8 (media-frame stream constructor)" }
        }
    }
}

// Image response type: normalizes the sidecars LAZILY off the captured body.
// Implements the base ResponseMeta PLUS s06_MediaCost AND the external
// s06_RevisesPrompt — image endpoints rewrite prompts; chat/tts/stt do not.
class s06_ImageMeta {
    raw: string,
    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never { baml.ai.Usage { input_tokens: 0, output_tokens: 0 } }
        function reasoning(self) -> string | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "image endpoint" } }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "image endpoint" } }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "image endpoint" } }
    }
    implements s06_MediaCost {
        // a dedicated endpoint: NO mainline tokens, exactly one generation unit.
        function media_cost(self) -> s06_Usage throws never {
            s06_Usage { mainline_tokens: 0, generation_units: 1, generation_kind: "image" }
        }
    }
    implements s06_RevisesPrompt {
        function revised_prompt(self) -> string | baml.ai.Unavailable throws never {
            let wire: s06_ImageWire = baml.json.from_json<s06_ImageWire>(baml.json.parse(self.raw)) catch (e) {
                _ => s06_ImageWire { data: [] },
            };
            match (wire.data.at(0)) {
                let d: s06_ImageDatumWire => match (d.revised_prompt) {
                    let r: string => if (r == "") { baml.ai.Unavailable { reason: "no rewrite this call" } } else { r },
                    null => baml.ai.Unavailable { reason: "no rewrite this call" },
                },
                _ => baml.ai.Unavailable { reason: "no rewrite this call" },
            }
        }
    }
}

class s06_GptTts {
    model: string,              // "gpt-4o-mini-tts"
    api_key: string,
    voice: string,              // "alloy"

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = s06_RawResp

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.CallError | baml.errors.UnknownError {
            let body: map<string, baml.json.json> = {
                "model": self.model,
                "voice": self.voice,
                "input": s06_messages_text(messages),
                "format": "mp3",
            };
            baml.http.Request {
                method: "POST",
                url: "https://api.openai.com/v1/audio/speech",
                headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
                body: baml.json.stringify(body),
            }
        }

        function send(self, request: baml.http.Request) -> s06_RawResp
            throws baml.errors.CallError | baml.errors.UnknownError {
            s06_send_raw(request)
        }

        // Audio comes back as a raw byte body — a DIFFERENT Body decode than image
        // (JSON) or chat (text). BLOCKED: P8 — there is no Audio.from_bytes /
        // bytes→base64 host on the real surface, so the byte payload cannot be
        // lifted into s06_Media.data. We keep the shape (non-2xx → typed error) and
        // stub the decode.
        function parse<T>(self, from: s06_RawResp) -> T
            throws baml.errors.CallError | baml.errors.UnknownError {
            if (!from.ok) { throw s06_OpenAiMediaHttpError { status: from.status, detail: "tts failed" }; }
            throw baml.errors.Unsupported { message: "s06 tts byte→Media decode: BLOCKED P8 (no Audio.from_bytes host)" }
        }

        function parse_meta(self, from: s06_RawResp) -> baml.ai.ResponseMeta throws never {
            s06_AudioMeta { kind: "audio" }
        }
    }
}

// Response type for TTS/STT (audio generation / transcription). Same dedicated-cost
// shape, different generation_kind. Does NOT implement s06_RevisesPrompt: projecting
// revised_prompt over this is an Unavailable match-miss — the honest answer.
class s06_AudioMeta {
    kind: string,               // "audio" | "transcription"
    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never { baml.ai.Usage { input_tokens: 0, output_tokens: 0 } }
        function reasoning(self) -> string | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "audio endpoint" } }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "audio endpoint" } }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "audio endpoint" } }
    }
    implements s06_MediaCost {
        function media_cost(self) -> s06_Usage throws never {
            s06_Usage { mainline_tokens: 0, generation_units: 1, generation_kind: self.kind }
        }
    }
}

class s06_GptStt {
    model: string,              // "gpt-4o-transcribe"
    api_key: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = s06_RawResp

        // STT INVERTS the modality: input is media, output is text. The audio rides
        // the prompt as a media attachment; a real build_request would lift it into
        // multipart (s06_messages_text carries the textual instruction here).
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.CallError | baml.errors.UnknownError {
            baml.http.Request {
                method: "POST",
                url: "https://api.openai.com/v1/audio/transcriptions",
                headers: { "authorization": "Bearer " + self.api_key, "content-type": "multipart/form-data" },
                body: s06_messages_text(messages),
            }
        }

        function send(self, request: baml.http.Request) -> s06_RawResp
            throws baml.errors.CallError | baml.errors.UnknownError {
            s06_send_raw(request)
        }

        // Output IS a string here — so SAP is back. STT is the one non-text-OUTPUT
        // provider whose result is text; `T` is a normal schema (string, or a struct
        // with word timestamps). This is OQ6 inverted: SAP sits right next to
        // GptImage/GptTts which must NOT use it.
        function parse<T>(self, from: s06_RawResp) -> T
            throws baml.errors.CallError | baml.errors.UnknownError {
            if (!from.ok) { throw s06_OpenAiMediaHttpError { status: from.status, detail: "stt failed" }; }
            baml.sap.parse<T>(from.text) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["openai stt parse failed"] },
            }
        }

        function parse_meta(self, from: s06_RawResp) -> baml.ai.ResponseMeta throws never {
            s06_AudioMeta { kind: "transcription" }
        }
    }
}

// ============================================================================
// (b) IN-CONVERSATION PATH — image generation as a built-in TOOL on a mainline
//     model. SAME endpoint as a chat model; the image arrives as one output ITEM
//     among text/reasoning. `T` is specialized to s06_MixedReply here.
// ============================================================================

class s06_Gpt55 {
    model: string,              // "gpt-5.5" — a MAINLINE model
    api_key: string,
    image_tool: bool,           // attach the built-in image_generation tool?

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = s06_RawResp

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.CallError | baml.errors.UnknownError {
            // The Responses endpoint — the divergence from the dedicated path is that
            // we name a MAINLINE model and attach a built-in tool; the model picks the
            // sub-model and bills mainline tokens PLUS generation.
            let tools: baml.json.json[] = if (self.image_tool) { [{ "type": "image_generation" }] } else { [] };
            let body: map<string, baml.json.json> = {
                "model": self.model,
                "input": s06_messages_text(messages),
                "tools": tools,
            };
            baml.http.Request {
                method: "POST",
                url: "https://api.openai.com/v1/responses",
                headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
                body: baml.json.stringify(body),
            }
        }

        function send(self, request: baml.http.Request) -> s06_RawResp
            throws baml.errors.CallError | baml.errors.UnknownError {
            s06_send_raw(request)
        }

        // MIXED-MODALITY parse (background §"What's hard": "Mixed-modality responses").
        // One response interleaves text, image_generation_call items, and reasoning —
        // walk the output array and bucket each item by type.
        function parse<T>(self, from: s06_RawResp) -> T
            throws baml.errors.CallError | baml.errors.UnknownError {
            if (!from.ok) { throw s06_OpenAiMediaHttpError { status: from.status, detail: from.text }; }
            let wire: s06_RespWire = baml.json.from_json<s06_RespWire>(baml.json.parse(from.text)) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["openai responses parse failed"] },
            };
            let media: s06_Media[] = [];
            let text: string = "";
            for (let item in wire.output) {
                match (item.type) {
                    "image_generation_call" => {
                        let img = baml.media.Image.from_base64(item.result ?? "", "image/png");
                        media.push(s06_Media { mime: img.mime_type() ?? "image/png", data: img.base64(), url: null });
                    },
                    "message" => {
                        match (item.content) {
                            let cs: s06_RespContentWire[] => match (cs.at(0)) {
                                let c: s06_RespContentWire => { text = text + (c.text ?? ""); },
                                _ => {},
                            },
                            null => {},
                        }
                    },
                    _ => {},   // reasoning / tool plumbing: skip
                }
            }
            // The two-meter cost is no longer folded into T — s06_ResponsesMeta reads
            // it off the same body and it is projected via call_with.
            s06_downcast<s06_MixedReply, T>(s06_MixedReply { text: if (text == "") { null } else { text }, media: media })
        }

        function parse_meta(self, from: s06_RawResp) -> baml.ai.ResponseMeta throws never {
            s06_ResponsesMeta { raw: from.text }
        }
    }

    implements baml.ai.Streaming {
        function stream_messages<TStream, TFinal>(self, messages: baml.ai.ChatMessage[])
            -> baml.llm.Stream<TStream, TFinal>
            throws baml.errors.StreamError | baml.errors.UnknownError {
            // BLOCKED: P8 — same media-frame stream constructor gap as GptImage.
            throw baml.errors.Unsupported { message: "s06 responses partial-image streaming: BLOCKED P8" }
        }
    }
}

// Response type for the in-conversation path. BOTH cost meters tick here, so
// media_cost() reads the real mainline + generation figures off the wire (the
// dedicated impls hard-code mainline=0). Implements s06_RevisesPrompt: the built-in
// image tool can echo a revised prompt on its output item.
class s06_ResponsesMeta {
    raw: string,
    function _wire(self) -> s06_RespWire throws never {
        baml.json.from_json<s06_RespWire>(baml.json.parse(self.raw)) catch (e) { _ => s06_RespWire { output: [], usage: null } }
    }
    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            let u = self._wire().usage;
            match (u) {
                let uu: s06_RespUsageWire => baml.ai.Usage { input_tokens: uu.total_tokens ?? 0, output_tokens: 0 },
                null => baml.ai.Usage { input_tokens: 0, output_tokens: 0 },
            }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "not surfaced here" } }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "responses path" } }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never { baml.ai.Unavailable { reason: "responses path" } }
    }
    implements s06_MediaCost {
        function media_cost(self) -> s06_Usage throws never {
            let w = self._wire();
            let gen = 0;
            for (let item in w.output) {
                if (item.type == "image_generation_call") { gen = gen + 1; }
            }
            let mainline = match (w.usage) {
                let uu: s06_RespUsageWire => uu.total_tokens ?? 0,
                null => 0,
            };
            s06_Usage { mainline_tokens: mainline, generation_units: gen, generation_kind: "image" }
        }
    }
    implements s06_RevisesPrompt {
        function revised_prompt(self) -> string | baml.ai.Unavailable throws never {
            for (let item in self._wire().output) {
                if (item.type == "image_generation_call") {
                    match (item.revised_prompt) {
                        let r: string => if (r != "") { return r; },
                        null => {},
                    }
                }
            }
            baml.ai.Unavailable { reason: "no rewrite this call" }
        }
    }
}

// ============================================================================
// A combinator specific to this scenario: s06_SaveTo wraps any image/audio provider
// and persists the Media payload as a side effect, returning the SAME value. It
// forwards the request/response capability, so it implements HttpProvider (Provider
// is a bare marker). Its primitive is call_messages_with, which forwards `project`
// to the inner provider's own call_messages_with — so the usage/revised-prompt
// SIDECAR is NOT dropped (the actual P1 win). It declares the SAME CallError channel
// as its members (no narrow/widen/forward — error-model rule 4). The fluent
// factories (with_retry/fallback_to/traced) are inherited via Provider.
// ============================================================================
class s06_SaveTo {
    inner: baml.ai.Provider,
    dir: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = unknown

        // Router: no single honest request/response, so the codec methods throw; the
        // real work is in call_messages_with (mirrors the stdlib Fallback).
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["SaveTo has no single request"] }
        }
        function send(self, request: baml.http.Request) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["SaveTo does not send directly"] }
        }
        function parse<T>(self, from: unknown) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["SaveTo does not parse directly"] }
        }
        function parse_meta(self, from: unknown) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["SaveTo has no meta"] }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            match (self.inner) {
                let h: baml.ai.HttpProvider => {
                    // Forward `project` to the inner call so the sidecar survives — a
                    // combinator declares the SAME channel, no gymnastics.
                    let r: baml.ai.CallResult<T, V> = h.call_messages_with<T, V, E2>(messages, project);
                    // BLOCKED: P8 — best-effort media persistence (design's
                    // baml.media.persist_if_media) has no host on the real surface.
                    // The FORWARDING above is the P1 point being demonstrated; the
                    // persist is a side effect we stub.
                    r
                },
                _ => throw baml.errors.Unsupported { message: "SaveTo inner provider cannot call" },
            }
        }
    }
}
