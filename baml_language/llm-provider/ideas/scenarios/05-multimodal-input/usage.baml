// =============================================================================
// 05 · Multimodal input — usage.baml  (application-author code)
//
// The point: the app author writes a NORMAL function with media in the prompt and
// picks a client. All the per-provider encoding (URL vs data-URL vs raw-b64+mime vs
// uploaded handle), the pre-fetch / pre-upload, and the audio/video capability gaps
// are absorbed by the providers in implement.baml. The app author writes ~nothing
// extra — until they hit a hard gap (audio->Anthropic, video->non-Gemini), which
// surfaces as a thrown error, exactly as it should.
// =============================================================================


// ---- Clients: config only, ordinary functions returning a Provider ----------
client GPT4()   { OpenAI    { model: "gpt-4.1",         api_key: env.OPENAI_API_KEY } }
client Claude() { Anthropic { model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 } }
client Flash()  { Gemini    { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY } }

// A resilient image reader: try OpenAI, fall back to Gemini. Both ingest images, so
// the Fallback combinator (from the spine) forwards .call to whichever answers.
client ImageReader() { GPT4().fallback_to(Flash()) }


// ---- 1. The happy path: identical app code, two providers, one image ---------
//
// `img` is a media value the host hands in (from a URL, a file, or raw bytes — the
// app doesn't pre-encode it). The SAME function body works against GPT4 or Claude;
// each provider's build_request resolves the image to the form it accepts.
class Caption { text: string, tags: string[] }

function DescribeImage(img: image) -> Caption {
  client: ImageReader()
  prompt #"
    What is in this image? Give a one-line caption and 3 tags.
    {{ img }}
  "#
}
// DescribeImage(my_image)  ->  Caption
//   via GPT4   : image_url { url: "https://..." }      (URL forwarded untouched)
//   via Claude : image { source: base64 + media_type } (URL pre-fetched, re-framed)
//   via Flash  : inline_data { mime_type, data }        (URL pre-fetched -> bytes)


// ---- 2. A PDF — portable across OpenAI (file id) and Anthropic (document part) --
function SummarizeDoc(doc: pdf) -> string {
  client: Claude()                 // Anthropic `document` part; data-URL re-framed to b64+mime
  prompt #"Summarize this document in 5 bullets. {{ doc }}"#
}

function SummarizeDocOpenAI(doc: pdf) -> string {
  client: GPT4()                   // OpenAI: resolve() auto-uploads -> input_file { file_id }
  prompt #"Summarize this document in 5 bullets. {{ doc }}"#
}


// ---- 3. Audio: the capability GAP made visible -------------------------------
//
// Gemini takes audio inline; Anthropic does not. Same prompt, two clients, two
// outcomes — and the difference is a *runtime throw*, not a silent drop.
function TranscribeNotes(clip: audio) -> string {
  client: Flash()                  // OK: Gemini inline_data audio
  prompt #"Transcribe this voice memo, then summarize it. {{ clip }}"#
}

function TranscribeNotesClaude(clip: audio) -> string {
  client: Claude()                 // resolve() throws Unsupported("audio") at build_request time
  prompt #"Transcribe this voice memo. {{ clip }}"#
}

// ...or DEGRADE instead of erroring: wrap Anthropic with a transcribe-first combinator
// that uses Gemini as the STT. The app opts in explicitly; nothing is implicit.
client ClaudeWithSttFallback() {
  TranscribeFirst { inner: Claude(), stt: Flash() }
}
function ReadNotesDegrading(clip: audio) -> string {
  client: ClaudeWithSttFallback()  // audio -> Gemini STT -> "[audio transcript: ...]" -> Claude
  prompt #"Here is a voice memo; answer questions about it. {{ clip }}"#
}


// ---- 4. Video: Gemini-only, and it forces a pre-upload -----------------------
//
// Only Gemini accepts video, and only as a Files-API handle. The app hands in raw
// video bytes (or a path the host loads); resolve() auto-uploads and references the
// handle. No app-side upload dance.
function DescribeVideo(clip: video) -> string {
  client: Flash()
  prompt #"Describe what happens in this video, beat by beat. {{ clip }}"#
}

// If someone aims a video at a non-video provider, it throws Unsupported — caught here
// so the app can fall back to "extract a keyframe and send the image" instead.
function DescribeVideoSafe(clip: video, keyframe: image) -> string {
  client: GPT4()
  prompt #"Describe this clip. {{ clip }}"#
}
// catch at the call site:
//   DescribeVideoSafe(v, frame) catch (e) {
//     baml.errors.Unsupported => { DescribeImage(frame); }   // degrade to the keyframe image
//   }


// ---- 5. Mixed media in one prompt + a combinator chain -----------------------
//
// Text + image + PDF + audio together, against Gemini (the only one that takes all
// four), cached and retried. Reads as a fluent chain because clients are functions.
client AnalystCached(store: KV) { Flash().with_retry(2).cached(store) }

function AnalyzeBundle(chart: image, report: pdf, briefing: audio, store: KV) -> Caption {
  client: AnalystCached(store)
  prompt #"
    Cross-reference the chart, the report, and the spoken briefing. Caption the result.
    {{ chart }}
    {{ report }}
    {{ briefing }}
  "#
}
