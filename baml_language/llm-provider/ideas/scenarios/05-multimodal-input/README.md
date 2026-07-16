# 05 · Multimodal input (image / audio / PDF / video)

Sending media alongside text. The interesting part is never the call — it is that one logical image has three+ wire encodings (remote URL, `data:` data-URL, raw base64 + explicit mime, uploaded file handle), each provider accepts a different subset, and the subset varies *by media kind* (audio isn't accepted by Anthropic; video is Gemini-only). This scenario keeps the spine's `Provider`/`HttpProvider` untouched and pushes everything into one method, `build_request`, behind a new `MediaIngest` capability whose default `resolve` does the supportedUrls-style forward / pre-fetch / pre-upload negotiation. App authors (`usage.baml`) just put `{{ img }}` in a prompt and pick a client; gaps surface as runtime `Unsupported` throws, or are degraded by a `TranscribeFirst` combinator. See `evaluation.md` for why the audio/video gap cannot be made a compile-time guarantee.

Background: background/01-single-turn.md → ## ★ Multimodal input
