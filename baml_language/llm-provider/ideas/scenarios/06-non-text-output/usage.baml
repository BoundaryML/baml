// ============================================================================
// 06 · Non-text output — APPLICATION code (what an end user writes)
// ============================================================================
//
// The library (implement.baml) did the heavy lifting. Here we show the
// ergonomics: clients are config-only sugar, functions name a return type, and
// the media-vs-mixed divergence shows up as a different `-> T`.

// ----------------------------------------------------------------------------
// Clients — pure sugar for `function Name() -> Provider { body }`.
// ----------------------------------------------------------------------------

// Dedicated image endpoint.
client Painter() {
  GptImage { model: "gpt-image-2", api_key: env.OPENAI_API_KEY, size: "1024x1024", partial_images: 2 }
}

// Dedicated TTS + STT.
client Narrator() {
  GptTts { model: "gpt-4o-mini-tts", api_key: env.OPENAI_API_KEY, voice: "alloy" }
}
client Scribe() {
  GptStt { model: "gpt-4o-transcribe", api_key: env.OPENAI_API_KEY }
}

// In-conversation image generation on a mainline model.
client VisualChat() {
  Gpt55 { model: "gpt-5.5", api_key: env.OPENAI_API_KEY, image_tool: true }
}

// Combinator chain: dedicated image, persisted to disk, retried on the ~2-min
// latency / moderation hiccups. Reads exactly like a text-provider chain.
client SavedPainter() {
  SaveTo { inner: GptImage { model: "gpt-image-2", api_key: env.OPENAI_API_KEY, size: "1024x1024", partial_images: 0 }, dir: "./out" }
    .with_retry(2)
}

// COMPILE-TIME-PRECISE escape hatch (proposal §3): drop the sugar so the return
// type is the concrete provider, making "this only yields images" static.
function DallePrecise() -> GptImage {
  GptImage.from_model("gpt-image-2")
}

// ----------------------------------------------------------------------------
// Functions — the `-> T` is where the modality lives.
// ----------------------------------------------------------------------------

// Dedicated path: T = Media. The app gets bytes + mime back, not a string.
function DrawFox(subject: string) -> Media {
  client: Painter()
  prompt #"a watercolor {{ subject }}"#
}

// TTS: input text -> Media (audio bytes).
function Speak(line: string) -> Media {
  client: Narrator()
  prompt #"{{ line }}"#
}

// STT INVERTS modality: the media is an INPUT (an audio attachment in the
// prompt), the output is a plain string. Same Provider.call interface.
function Transcribe(clip: audio) -> string {
  client: Scribe()
  prompt #"{{ clip }}"#
}

// In-conversation path: T = MixedReply. One call returns interleaved text +
// image, and the usage carries BOTH cost meters.
function IllustrateAnswer(question: string) -> MixedReply {
  client: VisualChat()
  prompt #"Answer and illustrate: {{ question }}"#
}

// ----------------------------------------------------------------------------
// Companions — same call, richer delivery.
// ----------------------------------------------------------------------------

// Partial-preview image streaming. `.stream` matches the provider against
// Streaming; both Painter (dedicated) and VisualChat (in-conversation) support
// it, so the SAME call site is portable across the two paths.
function PreviewFox(subject: string) -> baml.llm.Stream<Media, Media> {
  client: Painter()
  prompt #"a watercolor {{ subject }}"#
}
// call site:  for (let frame in PreviewFox.stream("fox")) { show(frame); }

// ----------------------------------------------------------------------------
// Reading the two cost meters at the app layer.
// ----------------------------------------------------------------------------
function CostAwareIllustrate(question: string) -> string {
  let p = VisualChat();
  let reply: MixedReply = p.call<MixedReply>(baml.llm.render("Answer and illustrate: " + question));
  // The app can branch on which path billed what — the dedicated path would
  // report mainline_tokens == 0, the tool path reports both.
  let u: Usage = reply.usage;
  "billed " + u.mainline_tokens + " mainline tokens + " + u.generation_units + " " + u.generation_kind
}

// ----------------------------------------------------------------------------
// Portability check across the two divergent providers, side by side.
// Painter (GptImage, b64 bytes, dedicated) and VisualChat (Gpt55, output-item,
// in-conversation) both reach `.call` — but note the return type MUST differ
// (Media vs MixedReply), which is where the abstraction leaks (see evaluation).
// ----------------------------------------------------------------------------
function TwoPaths(subject: string) -> Media {
  let dedicated: Media = DrawFox(subject);                       // GptImage -> Media
  let mixed: MixedReply = IllustrateAnswer("draw a " + subject); // Gpt55     -> MixedReply
  // To treat them uniformly the app must reach INTO MixedReply for the media:
  if (mixed.media.len() > 0) { mixed.media[0] } else { dedicated }
}
