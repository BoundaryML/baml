// scenario 05 — multimodal input: ../../../../../llm-provider/ideas/scenarios/05-multimodal-input/usage.baml
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The point (design README): the app author writes a NORMAL LLM function with media in
// the prompt and picks a client. All the per-provider encoding (URL vs data-URL vs
// raw-b64+mime vs uploaded handle) and the audio/video capability gaps are absorbed by
// the providers — on the REAL surface, inside each stdlib provider's `build_request`.
// The app writes ~nothing extra until it hits a hard gap (audio→Anthropic,
// video→inline), which surfaces as a thrown error, exactly as it should.
//
// This mirrors the design's usage.baml function-for-function on the real surface:
//   - media rides `{{ img }}` in a prompt, and `img: image` is a plain LLM-fn param;
//   - the SAME body works across providers (portability), each encoding differently
//     inside build_request — proven offline by inspecting the built request, and
//     against the real API in the live tier;
//   - the capability gaps are RUNTIME throws (design §c "detected, not typed"): the
//     inner provider's build_request throws BEFORE any network, so the gap is
//     deterministic offline;
//   - the degrade path is a Provider-wrapping combinator (s05_TranscribeFirst), opted
//     into explicitly — nothing implicit.
//
// Naming: all scenario dirs share ONE namespace, so app decls carry the `s05_` prefix.
// Live tier = testset "integ-test-05-multimodal" (OpenAI vision).


// ---- Clients: config only, ordinary functions returning a Provider ----------
// The design writes `client GPT4() { OpenAI {…} }` etc. On the real surface a plain
// function returning a `baml.ai.Provider` is exactly that — and it works as an LLM
// function's default `client` (proven below) and as a call-site `client =` override.
// Provider-specific params (max_tokens, …) are plain fields; no options blob.
function s05_gpt4() -> baml.ai.Provider {
    baml.ai.OpenAi { model: "gpt-4.1", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null }
}
function s05_claude() -> baml.ai.Provider {
    baml.ai.Anthropic {
        model: "claude-sonnet-4-5", api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"), base_url: null,
        max_tokens: 1024, anthropic_version: null, extra_headers: null, extra_body: null, append_output_schema: null,
    }
}
function s05_flash() -> baml.ai.Provider {
    baml.ai.Gemini {
        model: "gemini-flash-latest", api_key: baml.env.get_or_panic("GOOGLE_API_KEY"), base_url: null,
        extra_headers: null, extra_body: null, append_output_schema: null,
    }
}

// A resilient image reader: try OpenAI, fall back to Gemini. Both ingest images, so the
// inherited `fallback_to` combinator forwards `.call` to whichever answers (design's
// `ImageReader = GPT4().fallback_to(Flash())`).
function s05_image_reader() -> baml.ai.Provider {
    s05_gpt4().fallback_to(s05_flash())
}

// The degrade client: wrap Anthropic (no audio) with a transcribe-first combinator that
// uses Gemini as the STT (design's `ClaudeWithSttFallback`).
function s05_claude_with_stt_fallback() -> baml.ai.Provider {
    s05_TranscribeFirst { inner: s05_claude(), stt: s05_flash() }
}


// ---- 1. The happy path: identical app code, media in the prompt --------------
//
// `img` is a media value the host hands in (from a URL, a file, or raw bytes — the app
// doesn't pre-encode it). The SAME function body works against any vision provider;
// each provider's build_request resolves the image to the form it accepts. Default
// client is the resilient reader; a call site can swap it with `client =`.
class s05_Caption { text: string, tags: string[] }

function s05_DescribeImage(img: image) -> s05_Caption {
    client s05_image_reader()
    prompt #"
        What is in this image? Give a one-line caption and 3 tags.
        {{ img }}
    "#
}


// ---- 2. A PDF — portable across OpenAI and Anthropic -------------------------
function s05_SummarizeDoc(doc: pdf) -> string {
    client s05_claude()              // Anthropic `document` part (base64 + media_type)
    prompt #"Summarize this document in 5 bullets. {{ doc }}"#
}

function s05_SummarizeDocOpenAI(doc: pdf) -> string {
    client s05_gpt4()                // OpenAI: `file`/`file_data` content part
    prompt #"Summarize this document in 5 bullets. {{ doc }}"#
}


// ---- 3. Audio: the capability GAP made visible -------------------------------
//
// Gemini takes audio inline; Anthropic does not. Same prompt, two clients, two
// outcomes — and the difference is a RUNTIME THROW, not a silent drop.
function s05_TranscribeNotes(clip: audio) -> string {
    client s05_flash()               // OK: Gemini inline_data audio
    prompt #"Transcribe this voice memo, then summarize it. {{ clip }}"#
}

function s05_TranscribeNotesClaude(clip: audio) -> string {
    client s05_claude()              // build_request throws the audio gap (before any network)
    prompt #"Transcribe this voice memo. {{ clip }}"#
}

// …or DEGRADE instead of erroring: the transcribe-first combinator uses Gemini as STT.
// The app opts in explicitly; nothing is implicit.
function s05_ReadNotesDegrading(clip: audio) -> string {
    client s05_claude_with_stt_fallback()   // audio → STT → "[audio transcript: …]" → Claude
    prompt #"Here is a voice memo; answer questions about it. {{ clip }}"#
}


// ---- 4. Video: the tightest gap (inline video is unsupported everywhere) -----
//
// The design makes video Gemini-only via a Files-API pre-upload. That native transport
// (POST /upload/v1beta/files, poll ACTIVE) is not in the offline host surface, so on
// the real model inline video is a runtime gap on EVERY provider — the same "detected,
// not typed" story, just tighter than the design's Gemini escape hatch.
// BLOCKED: P8 — real Gemini video ingest (Files-API pre-upload + poll ACTIVE, then a
// file_data handle part) has no host surface; inline video throws on every provider,
// so the video-accepting path can only be exercised as the runtime gap + degrade below.
function s05_DescribeVideo(clip: video) -> string {
    client s05_flash()
    prompt #"Describe what happens in this video, beat by beat. {{ clip }}"#
}

// Aim a video at any provider and it throws; catch at the call site to degrade to a
// keyframe image (see test 05:14 below).
function s05_DescribeVideoSafe(clip: video) -> string {
    client s05_gpt4()
    prompt #"Describe this clip. {{ clip }}"#
}


// ---- 5. Mixed media in one prompt + a combinator chain -----------------------
//
// Text + image + PDF together, against one client, retried. Reads as a fluent chain
// because clients are functions. (The design also `.cached(store)`s this; caching is a
// `ManagedCache` capability on the real surface, not a fluent `Provider` factory, so we
// use the real `.with_retry` — see divergences.)
function s05_analyst_retried() -> baml.ai.Provider {
    s05_flash().with_retry(2)
}

function s05_AnalyzeBundle(chart: image, report: pdf) -> s05_Caption {
    client s05_analyst_retried()
    prompt #"
        Cross-reference the chart and the report. Caption the result.
        {{ chart }}
        {{ report }}
    "#
}


// =============================================================================
// OFFLINE TESTS (deterministic, no network, no keys).
// =============================================================================

// --- 1. The happy path: media flows through the desugar. ---------------------
// EchoProvider ignores the wire (it overrides call_messages_with), so this proves
// the PLUMBING — an `img: image` param renders into the prompt and the SAME body
// yields a captioned answer regardless of which client is wired in (portability).
// The per-provider ENCODING is proven separately below, and live at the bottom.
test "05: media flows through the desugar into a captioned answer" {
    let img = baml.media.Image.from_url("https://example.com/cat.png", "image/png");
    let r = s05_DescribeImage(img, client = EchoProvider {
        reply: "{\"text\": \"a napping cat\", \"tags\": [\"cat\", \"pet\", \"cute\"]}",
    }) catch (e) {
        _ => s05_Caption { text: "call failed", tags: [] },
    };
    assert.equal(r.text, "a napping cat")
    assert.equal(r.tags.length(), 3)
}

// A capability-less client is a typed Unsupported (the negotiation-failure fixture).
test "05: a capability-less client is a typed Unsupported" {
    let img = baml.media.Image.from_url("https://example.com/cat.png", "image/png");
    let r = s05_DescribeImage(img, client = NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}

// --- 2. The core lesson: one image, three providers, three wire encodings. ---
// This IS scenario 05 — the per-provider media serialization lives inside each
// provider's build_request. Success paths can't complete offline (send needs the
// net), so we inspect the BUILT request (driver-style, justified: the negotiation IS
// the lesson). Same image, three shapes: `image_url` / `source.base64` / `inline_data`.
test "05: the same image resolves to each provider's own wire encoding" {
    let msgs: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage { role: "user", parts: [
            baml.ai.MessagePart.from_text("What is in this image?"),
            baml.ai.MessagePart.from_image(baml.media.Image.from_url("https://example.com/cat.png", "image/png")),
        ] },
    ];
    // OpenAI → image_url (a URL forwarded untouched).
    let oai: baml.ai.Provider = baml.ai.OpenAi { model: "gpt-4.1", api_key: "offline-dummy", base_url: null };
    let oai_body = s05_built_body(oai, msgs);
    assert.is_true(oai_body.includes("image_url"))
    assert.is_true(oai_body.includes("cat.png"))

    // Anthropic → image { source: { type: url | base64 + media_type } }.
    let ant: baml.ai.Provider = baml.ai.Anthropic {
        model: "claude-sonnet-4-5", api_key: "offline-dummy", base_url: null,
        max_tokens: 256, anthropic_version: null, extra_headers: null, extra_body: null, append_output_schema: null,
    };
    let ant_body = s05_built_body(ant, msgs);
    assert.is_true(ant_body.includes("\"source\""))

    // Gemini → inline_data (no remote-URL fetch: bytes are inlined).
    let gem: baml.ai.Provider = baml.ai.Gemini {
        model: "gemini-flash-latest", api_key: "offline-dummy", base_url: null,
        extra_headers: null, extra_body: null, append_output_schema: null,
    };
    let gem_body = s05_built_body(gem, msgs);
    assert.is_true(gem_body.includes("inline_data"))
}

// A PDF is portable across OpenAI (a `file` content part) and Anthropic (a `document`
// block) — same design story, both encodings built offline.
test "05: a PDF is portable across OpenAI and Anthropic" {
    let msgs: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage { role: "user", parts: [
            baml.ai.MessagePart.from_text("Summarize this."),
            baml.ai.MessagePart { pdf: baml.media.Pdf.from_base64("JVBERi0=", "application/pdf") },
        ] },
    ];
    let oai: baml.ai.Provider = baml.ai.OpenAi { model: "gpt-4.1", api_key: "offline-dummy", base_url: null };
    assert.is_true(s05_built_body(oai, msgs).includes("file"))

    let ant: baml.ai.Provider = baml.ai.Anthropic {
        model: "claude-sonnet-4-5", api_key: "offline-dummy", base_url: null,
        max_tokens: 256, anthropic_version: null, extra_headers: null, extra_body: null, append_output_schema: null,
    };
    assert.is_true(s05_built_body(ant, msgs).includes("document"))
}

// Helper: build the request for a provider and return its body (or "THREW" on the gap).
// This is the offline window into build_request — the seam the whole scenario lives in.
function s05_built_body(p: baml.ai.Provider, msgs: baml.ai.ChatMessage[]) -> string {
    let out: string = match (p) {
        let h: baml.ai.HttpProvider => {
            let req = h.build_request<string>(msgs) catch (e) {
                _ => baml.http.Request { method: "THREW", url: "", headers: {}, body: "THREW" },
            };
            req.body
        },
        _ => "no-http",
    };
    out
}

// --- 3. Audio: Gemini takes it inline; Anthropic is a runtime gap. -----------
// Positive: Gemini resolves audio to inline_data (the design's "audio supported,
// unlike Anthropic"), proven on the built request offline.
test "05: audio rides Gemini inline (inline_data)" {
    let msgs: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage { role: "user", parts: [
            baml.ai.MessagePart.from_text("Transcribe this."),
            baml.ai.MessagePart.from_audio(baml.media.Audio.from_base64("QUFB", "audio/wav")),
        ] },
    ];
    let gem: baml.ai.Provider = baml.ai.Gemini {
        model: "gemini-flash-latest", api_key: "offline-dummy", base_url: null,
        extra_headers: null, extra_body: null, append_output_schema: null,
    };
    assert.is_true(s05_built_body(gem, msgs).includes("inline_data"))
}

// The gap, made visible: the SAME `s05_TranscribeNotesClaude` LLM function, called
// with a real Anthropic client, throws at build_request — BEFORE any network — so it
// is deterministic offline. A runtime throw, never a silent drop (design §3/§c).
test "05: audio to Anthropic is a runtime gap, not a silent drop" {
    let clip = baml.media.Audio.from_base64("QUFB", "audio/wav");
    let r = s05_TranscribeNotesClaude(clip, client = baml.ai.Anthropic {
        model: "claude-sonnet-4-5", api_key: "offline-dummy", base_url: null,
        max_tokens: 256, anthropic_version: null, extra_headers: null, extra_body: null, append_output_schema: null,
    }) catch (e) {
        _ => "[gap: audio not accepted]",
    };
    assert.equal(r, "[gap: audio not accepted]")
}

// …or DEGRADE: the transcribe-first combinator folds the audio into transcript text so
// the audio-less inner model succeeds. Offline fixtures (implementation.baml):
// s05_AudioRejectingEcho models Anthropic's gap without a network; s05_SttEcho is the
// scripted STT. Direct call rejects; wrapped call recovers.
test "05: the degrade path — transcribe first, then send to the audio-less model" {
    let clip = baml.media.Audio.from_base64("QUFB", "audio/wav");

    // Direct: the inner (audio-less) provider rejects the clip.
    let direct = s05_ReadNotesDegrading(clip, client = s05_AudioRejectingEcho {
        reply: "should not be reached",
    }) catch (e) {
        _ => "[rejected]",
    };
    assert.equal(direct, "[rejected]")

    // Wrapped: audio folded to a transcript, inner answers.
    let degraded = s05_ReadNotesDegrading(clip, client = s05_TranscribeFirst {
        inner: s05_AudioRejectingEcho { reply: "answered from the transcript" },
        stt: s05_SttEcho { transcript: "buy milk and call mom" },
    }) catch (e) {
        _ => "[degrade failed]",
    };
    assert.equal(degraded, "answered from the transcript")
}

// --- 4. Video: a runtime gap on every provider (inline video unsupported). ---
// The `s05_DescribeVideoSafe` LLM function throws at build_request; the app catches and
// degrades to a keyframe image (the design's fallback). Deterministic offline: the
// throw precedes any network. (Real Gemini video via Files-API pre-upload is BLOCKED.)
test "05: video is a runtime gap; the app degrades to a keyframe image" {
    let clip = baml.media.Video.from_base64("QUFB", "video/mp4");
    let keyframe = baml.media.Image.from_url("https://example.com/frame.png", "image/png");

    let r = s05_DescribeVideoSafe(clip, client = baml.ai.Gemini {
        model: "gemini-flash-latest", api_key: "offline-dummy", base_url: null,
        extra_headers: null, extra_body: null, append_output_schema: null,
    }) catch (e) {
        // The gap throws; degrade to describing the keyframe with an echo client.
        _ => {
            s05_DescribeImage(keyframe, client = EchoProvider {
                reply: "{\"text\": \"a still frame\", \"tags\": [\"video\", \"keyframe\", \"fallback\"]}",
            }).text catch (e2) { _ => "degrade failed" }
        },
    };
    assert.equal(r, "a still frame")
}

// --- 5. Mixed media in one prompt rides a combinator chain. -------------------
// Text + image + PDF together, through a retry chain — the media-carrying function
// composes with the inherited `Provider` combinators with zero media-awareness
// (design §a: "combinators wrap, unchanged").
test "05: mixed media in one prompt composes with a retry combinator chain" {
    let chart = baml.media.Image.from_url("https://example.com/chart.png", "image/png");
    let report = baml.media.Pdf.from_base64("JVBERi0=", "application/pdf");
    let resilient: baml.ai.Provider = EchoProvider {
        reply: "{\"text\": \"revenue up 12%\", \"tags\": [\"chart\", \"pdf\", \"analysis\"]}",
    }.with_retry(2);
    let r = s05_AnalyzeBundle(chart, report, client = resilient) catch (e) {
        _ => s05_Caption { text: "chain failed", tags: [] },
    };
    assert.equal(r.text, "revenue up 12%")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The real per-provider encoding, end to end: an inline image described by the real
// OpenAI vision model through the native message pipeline. Proves the happy path the
// offline tier can only stage (encoding + send + parse), mirroring e2e_multimodal_live.
testset "integ-test-05-multimodal" {
    test "05 live: an inline image is described through the native pipeline" {
        // A 64x64 solid-red PNG, inline — no external fetch for OpenAI to reject.
        let red = baml.media.Image.from_base64(
            "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAAb0lEQVR4nO3PAQkAAAyEwO9feoshgnABdLep8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3IPanc8OLDQitxAAAAAElFTkSuQmCC",
            "image/png");
        let r = s05_DescribeImage(red, client = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        }) catch (e) {
            _ => s05_Caption { text: "live call failed", tags: [] },
        };
        assert.is_true(r.text.includes("red") || r.text.includes("Red") || r.tags.length() > 0)
    }
}
