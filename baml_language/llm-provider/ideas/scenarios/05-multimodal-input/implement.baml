// =============================================================================
// 05 · Multimodal input — implement.baml  (library / std-lib-author code)
//
// Sending image / audio / PDF / video *alongside* text. The whole scenario lives
// inside ONE method of the existing spine: `HttpProvider.build_request`. The base
// `Provider` and `HttpProvider` interfaces from _conventions.md are reused VERBATIM
// — multimodality is not a new capability, it is the per-provider encoding of media
// parts that already ride inside `baml.llm.PromptAst`.
//
// What this file adds (all flagged in evaluation.md §b):
//   - `baml.media.*` host surface: the typed media value + its three readable
//     encodings + a host downloader/uploader.   (net-new host primitives)
//   - `Media` / `MediaPart` / `MediaKind` view types that `build_request` walks.
//   - `MediaIngest` capability interface: a provider declares, per media kind, which
//     *transports* (url / inline-base64 / uploaded-handle) it accepts — the
//     supportedUrls-style negotiation map — plus the pre-fetch / pre-upload resolver
//     that the conventions' `build_request` would otherwise have to inline by hand.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. Host surface for media (NET-NEW; sits next to baml.http / baml.llm).
//
// `render_prompt` already produces `baml.llm.PromptAst` (opaque). For multimodal we
// need to *read* the media that the AST carries, so the AST grows one accessor and
// we add a small `baml.media` namespace. None of this is provider-specific — it is
// the raw material every provider's build_request consumes.
// -----------------------------------------------------------------------------

// The kinds we distinguish for capability negotiation. Audio/video are the ones with
// provider gaps (audio: not Anthropic; video: Gemini-only).
enum MediaKind { image, audio, pdf, video }

// How a given media value is *currently* held. A value can be cheaply re-encoded
// between Bytes <-> DataUrl (host does the base64), but Url -> Bytes needs a network
// fetch and Handle is provider-scoped (an upload receipt), hence its `provider` tag.
class MediaRef {
  kind: MediaKind
  // exactly one of the following is populated; `which()` tells you which.
  url: string?                 // remote URL the model may fetch itself
  bytes: uint8array?           // raw bytes (host re-encodes to base64 on demand)
  mime: string?                // required whenever bytes/data_url are used
  data_url: string?            // "data:<mime>;base64,...."  (OpenAI's framing)
  handle: string?              // an uploaded-file URI/id (Gemini Files API, gs://, OpenAI file id)
  handle_provider: string?     // which provider minted `handle` (handles are NOT portable)

  // host-computed discriminant so build_request can `match` without null-juggling.
  function which(self) -> MediaTransport { $rust_io_function }
}

enum MediaTransport { url, bytes, data_url, handle }

// A flat view of the prompt: the rendered text plus every media part, in order.
class MediaPart  { ref: MediaRef, index: int }     // index = position in the message
class PromptView { text: string, media: MediaPart[] }

// baml.media (ns_media/media.baml) — NET-NEW host functions.
//   baml.media.view(prompt)                 -> PromptView         // walk the AST's parts
//   baml.media.to_base64(ref)               -> string  throws Io  // bytes/data_url -> raw b64
//   baml.media.to_data_url(ref)             -> string  throws Io  // -> "data:<mime>;base64,..."
//   baml.media.download(ref)                -> MediaRef throws Io  // Url -> Bytes (pre-fetch)
//   baml.media.guess_mime(ref)              -> string             // sniff when caller omitted it
//   baml.media.image_from_base64(b64)       -> image              // (already cited in proposal §2)


// -----------------------------------------------------------------------------
// 1. The capability: MediaIngest  (NET-NEW interface, `requires Provider`)
//
// This is the supportedUrls-style negotiation, made explicit. A provider that can
// take media declares, per (kind, transport), whether it accepts the wire form
// DIRECTLY. The default `resolve` then turns any incoming MediaRef into a form the
// provider accepts — forwarding a URL, pre-fetching bytes, or pre-uploading to get a
// handle — OR throws Unsupported if the provider cannot ingest that kind at all.
//
// `build_request` calls `self.resolve(ref)` per part; the per-provider serializer
// then only ever sees a transport it already swore it accepts.
// -----------------------------------------------------------------------------
interface MediaIngest requires Provider {

  // Does this provider accept (kind, transport) on the wire, with no rewriting?
  // e.g. Anthropic: accepts(image, data_url)=false but accepts(image, bytes)=true,
  //      accepts(audio, *)=false ;  Gemini: accepts(video, handle)=true.
  function accepts(self, kind: MediaKind, t: MediaTransport) -> bool

  // Upload bytes to the provider's Files API and return a handle. Only meaningful for
  // providers that mint handles (Gemini); others may leave the default that throws.
  function upload(self, ref: MediaRef) -> MediaRef throws baml.errors.LlmClient {
    throw baml.errors.Unsupported { message: "provider has no Files API to upload to" }
  }

  // The negotiation engine. Default method — every MediaIngest provider inherits it,
  // and almost none override it. This is the logic the Vercel SDK hand-rolls per call.
  function resolve(self, ref: MediaRef) -> MediaRef throws baml.errors.LlmClient {
    // Hard gate first: a provider that accepts NO transport for this kind can't ingest
    // it. (audio -> Anthropic ; video -> non-Gemini.) Caught, never silently dropped.
    if (!self.accepts(ref.kind, url) &&
        !self.accepts(ref.kind, bytes) &&
        !self.accepts(ref.kind, data_url) &&
        !self.accepts(ref.kind, handle)) {
      throw baml.errors.Unsupported {
        message: "provider does not accept media kind: " + reflect.show(ref.kind)
      };
    }

    match (ref.which()) {
      url => {
        // Forward the URL untouched if the provider fetches it itself...
        if (self.accepts(ref.kind, url)) { return ref; }
        // ...otherwise pre-fetch the bytes and re-resolve (may then pre-upload).
        return self.resolve(baml.media.download(ref));
      },
      bytes => {
        if (self.accepts(ref.kind, bytes)) { return ref; }
        if (self.accepts(ref.kind, data_url)) {
          return MediaRef { kind: ref.kind, mime: ref.mime,
                            data_url: baml.media.to_data_url(ref) };
        }
        if (self.accepts(ref.kind, handle)) { return self.upload(ref); }
        throw baml.errors.LlmClient { message: "no acceptable transport for inline bytes" };
      },
      data_url => {
        if (self.accepts(ref.kind, data_url)) { return ref; }
        // Anthropic wants raw base64 + media_type, not a data: URL — re-frame it.
        if (self.accepts(ref.kind, bytes)) {
          return MediaRef { kind: ref.kind, mime: baml.media.guess_mime(ref),
                            bytes: baml.media.to_base64_bytes(ref) };
        }
        if (self.accepts(ref.kind, handle)) { return self.upload(ref); }
        throw baml.errors.LlmClient { message: "no acceptable transport for data URL" };
      },
      handle => {
        // A handle is only valid for the provider that minted it.
        match (self) {
          let mi: MediaIngest => {
            if (self.accepts(ref.kind, handle) && ref.handle_provider == self.provider_id()) {
              return ref;
            }
          },
          _ => {},
        }
        throw baml.errors.LlmClient {
          message: "uploaded handle is not portable to this provider; re-upload the bytes"
        };
      },
    }
  }

  // Identity used to validate handle portability.
  function provider_id(self) -> string
}

// baml.media.to_base64_bytes(ref) -> uint8array : decode a data_url back to raw bytes.
// (Listed with the other baml.media host fns above; kept here to keep `resolve` honest.)


// -----------------------------------------------------------------------------
// 2. OpenAI Chat Completions — image (URL or data-URL) + inline audio, NO video.
//
//   image -> `image_url` accepting either a remote URL or a `data:` URL.
//   audio -> `input_audio` { data: <raw b64>, format }   (inline only)
//   video -> not accepted -> resolve() throws Unsupported.
// -----------------------------------------------------------------------------
class OpenAI {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements MediaIngest {
    function provider_id(self) -> string { "openai" }

    function accepts(self, kind: MediaKind, t: MediaTransport) -> bool {
      match (kind) {
        image => { t == url || t == data_url },     // both wire forms ok
        audio => { t == bytes },                    // input_audio is inline raw-b64 only
        pdf   => { t == handle },                   // Responses input_file (file id) only
        video => { false },                         // OpenAI chat: no video
      }
    }
    // OpenAI mints file ids for PDFs; upload() would POST /v1/files. Elided host body:
    function upload(self, ref: MediaRef) -> MediaRef throws baml.errors.LlmClient {
      $rust_io_function
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      let view = baml.media.view(prompt);
      // Build the content array. Text first, then each media part in its accepted form.
      let parts: string[] = [json.object({ "type": "text", "text": view.text })];
      for (let part in view.media) {
        let r = self.resolve(part.ref);             // <-- negotiation happens HERE
        match (r.kind) {
          image => {
            // image_url takes a remote URL or a data: URL — both land in the same field.
            let u = match (r.which()) { url => r.url!, _ => baml.media.to_data_url(r) };
            parts = parts + [json.object({
              "type": "image_url",
              "image_url": json.object({ "url": u, "detail": "auto" }),
            })];
          },
          audio => {
            parts = parts + [json.object({
              "type": "input_audio",
              "input_audio": json.object({
                "data": baml.media.to_base64(r),
                "format": baml.media.audio_format(r),     // "wav" | "mp3"
              }),
            })];
          },
          pdf => {
            parts = parts + [json.object({ "type": "input_file", "file_id": r.handle! })];
          },
          video => { baml.sys.panic("unreachable: resolve() rejects video for OpenAI"); },
        }
      }
      let body = json.object({
        "model": self.model,
        "messages": json.array([ json.object({ "role": "user", "content": json.array(parts) }) ]),
      });
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
    }
  }
}


// -----------------------------------------------------------------------------
// 3. Anthropic — image + PDF as raw-base64 + explicit media_type (or URL). NO audio.
//
//   image    -> `image`    { source: base64 + media_type | url }
//   document -> `document` { source: base64 + media_type | url }     (PDF)
//   audio    -> NOT accepted -> resolve() throws Unsupported (the audio gap).
//   video    -> NOT accepted.
//
// Note Anthropic refuses the `data:` data-URL framing: it wants raw base64 + an
// explicit media_type. `resolve` re-frames a data_url into bytes for us (§1).
// -----------------------------------------------------------------------------
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  implements ChatModelOptions { model as model, api_key as api_key }

  implements MediaIngest {
    function provider_id(self) -> string { "anthropic" }

    function accepts(self, kind: MediaKind, t: MediaTransport) -> bool {
      match (kind) {
        image => { t == url || t == bytes },        // url, OR raw-b64 + media_type
        pdf   => { t == url || t == bytes },        // `document` part, same two forms
        audio => { false },                          // <-- the audio gap, declared in data
        video => { false },
      }
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      let view = baml.media.view(prompt);
      let parts: string[] = [json.object({ "type": "text", "text": view.text })];
      for (let part in view.media) {
        let r = self.resolve(part.ref);             // re-frames data_url -> bytes; rejects audio/video
        // `image` and `document` differ only in the top-level `type` tag; the `source`
        // sub-object is identical (base64+media_type or url). Factor it.
        let source = match (r.which()) {
          url   => json.object({ "type": "url", "url": r.url! }),
          _     => json.object({ "type": "base64",
                                 "media_type": r.mime!,        // resolve() guarantees mime is set
                                 "data": baml.media.to_base64(r) }),
        };
        let tag = match (r.kind) { pdf => "document", _ => "image" };
        parts = parts + [json.object({ "type": tag, "source": source })];
      }
      let body = json.object({
        "model": self.model,
        "max_tokens": self.max_tokens,
        "messages": json.array([ json.object({ "role": "user", "content": json.array(parts) }) ]),
      });
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": self.api_key catch (e) { _ => env.ANTHROPIC_API_KEY },
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("content.0.text"))
    }
  }
}


// -----------------------------------------------------------------------------
// 4. Gemini — inline bytes OR Files-API handle; the ONLY provider that takes video.
//
//   image/audio/pdf -> inline_data (raw b64 + mime) OR file_data (uri + mime)
//   video           -> file_data (handle) — and big inline media should pre-upload.
//
// Gemini is where `upload()` is real and where `resolve` chooses pre-upload: we mark
// `accepts(video, handle)=true` and `accepts(video, bytes)=false`, so a caller who
// hands raw video bytes gets them auto-uploaded to the Files API by `resolve`.
// -----------------------------------------------------------------------------
class Gemini {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements MediaIngest {
    function provider_id(self) -> string { "gemini" }

    function accepts(self, kind: MediaKind, t: MediaTransport) -> bool {
      match (kind) {
        // small media: inline bytes or a prior handle. (No remote-URL fetch in v1beta.)
        image => { t == bytes || t == handle },
        audio => { t == bytes || t == handle },     // <-- audio supported (unlike Anthropic)
        pdf   => { t == bytes || t == handle },
        video => { t == handle },                   // <-- video, handle ONLY -> forces upload
      }
    }

    // The real Files API upload. resolve() routes raw video bytes (and large media)
    // here automatically, then references the returned handle.
    function upload(self, ref: MediaRef) -> MediaRef throws baml.errors.LlmClient {
      $rust_io_function                              // POST .../upload/v1beta/files, poll ACTIVE
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      let view = baml.media.view(prompt);
      let parts: string[] = [json.object({ "text": view.text })];
      for (let part in view.media) {
        let r = self.resolve(part.ref);             // video-bytes -> upload() -> handle, here
        let p = match (r.which()) {
          handle => json.object({
            "file_data": json.object({ "mime_type": baml.media.guess_mime(r), "file_uri": r.handle! }),
          }),
          _ => json.object({
            "inline_data": json.object({ "mime_type": r.mime!, "data": baml.media.to_base64(r) }),
          }),
        };
        parts = parts + [p];
      }
      let body = json.object({
        "contents": json.array([ json.object({ "role": "user", "parts": json.array(parts) }) ]),
      });
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent",
        headers: { "x-goog-api-key": self.api_key, "content-type": "application/json" },
        body: json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("candidates.0.content.parts.0.text"))
    }
  }
}


// -----------------------------------------------------------------------------
// 5. Optional degrade combinator: TranscribeFirst.
//
// "Send audio to a provider that doesn't take audio" can either ERROR (the default,
// via resolve's Unsupported) or DEGRADE: transcribe the audio to text with a second
// provider and inline the transcript. This is the "transcribe first" path the
// background calls out — expressed as a Provider-wrapping combinator, NOT baked into
// any model. It rewrites the PromptAst before delegating.
// -----------------------------------------------------------------------------
class TranscribeFirst {
  inner: Provider          // the target chat model (e.g. Anthropic — no audio)
  stt: Provider            // a speech-to-text provider used to turn audio into text

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let view = baml.media.view(prompt);
      // If inner can already ingest every media kind present, do nothing.
      let rewritten = match (self.inner) {
        let mi: MediaIngest => {
          let needs_stt = false;
          for (let part in view.media) {
            if (part.ref.kind == audio &&
                !mi.accepts(audio, url) && !mi.accepts(audio, bytes) &&
                !mi.accepts(audio, data_url) && !mi.accepts(audio, handle)) {
              needs_stt = true;
            }
          }
          if (needs_stt) { self.fold_audio_to_text(prompt) } else { prompt }
        },
        _ => prompt,        // inner isn't MediaIngest -> can't reason; pass through
      };
      self.inner.call<T>(rewritten)
    }
  }

  // Replace every audio part with "[audio transcript: ...]" text, transcribing via stt.
  function fold_audio_to_text(self, prompt: baml.llm.PromptAst) -> baml.llm.PromptAst throws baml.errors.LlmClient {
    let view = baml.media.view(prompt);
    let extra = "";
    for (let part in view.media) {
      if (part.ref.kind == audio) {
        let text: string = self.stt.call<string>(baml.media.audio_only_prompt(part.ref));
        extra = extra + "\n[audio transcript: " + text + "]";
      }
    }
    baml.media.with_text_and_drop_kind(prompt, view.text + extra, audio)
  }
}

// baml.media.audio_only_prompt(ref)               -> PromptAst   // a STT-shaped prompt
// baml.media.with_text_and_drop_kind(p, text, k)  -> PromptAst   // new AST: text + media minus kind k
