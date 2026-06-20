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
  // On the call path (build_request -> resolve -> upload) -> the CallError channel.
  function upload(self, ref: MediaRef) -> MediaRef
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    throw baml.errors.Unsupported { message: "provider has no Files API to upload to" }
  }

  // The negotiation engine. Default method — every MediaIngest provider inherits it,
  // and almost none override it. This is the logic the Vercel SDK hand-rolls per call.
  function resolve(self, ref: MediaRef) -> MediaRef
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    // Hard gate first: a provider that accepts NO transport for this kind can't ingest
    // it. (audio -> Anthropic ; video -> non-Gemini.) Caught, never silently dropped.
    if (!self.accepts(ref.kind, url) &&
        !self.accepts(ref.kind, bytes) &&
        !self.accepts(ref.kind, data_url) &&
        !self.accepts(ref.kind, handle)) {
      throw baml.errors.Unsupported {
        message: "provider does not accept media kind: " + ref.kind.to_string()
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
        throw MediaTransportError { message: "no acceptable transport for inline bytes" };
      },
      data_url => {
        if (self.accepts(ref.kind, data_url)) { return ref; }
        // Anthropic wants raw base64 + media_type, not a data: URL — re-frame it.
        if (self.accepts(ref.kind, bytes)) {
          return MediaRef { kind: ref.kind, mime: baml.media.guess_mime(ref),
                            bytes: baml.media.to_base64_bytes(ref) };
        }
        if (self.accepts(ref.kind, handle)) { return self.upload(ref); }
        throw MediaTransportError { message: "no acceptable transport for data URL" };
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
        throw MediaTransportError {
          message: "uploaded handle is not portable to this provider; re-upload the bytes"
        };
      },
    }
  } catch (e) {
    // Foreign Io/Timeout from baml.media.{download,to_data_url,to_base64_bytes,which}
    // (host pre-fetch / re-encode) normalize here; deliberate Unsupported /
    // MediaTransportError are known CallErrors and pass through with_message unchanged.
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "media resolve failed");
  }

  // Identity used to validate handle portability.
  function provider_id(self) -> string
}

// baml.media.to_base64_bytes(ref) -> uint8array : decode a data_url back to raw bytes.
// (Listed with the other baml.media host fns above; kept here to keep `resolve` honest.)


// -----------------------------------------------------------------------------
// 1b. Media-resolution domain error (NET-NEW concrete error).
//
// `resolve` runs ENTIRELY on the call path (build_request calls it per part), so its
// deliberate throws live on the CallError channel. A transport-negotiation failure
// ("no acceptable wire form for these bytes", "this handle isn't portable") is a real,
// branchable domain error — distinct from a transport/network failure and from a
// model refusal — so it gets a concrete class implementing baml.errors.CallError.
// It is NOT a network/rate-limit/parse failure, hence all three classifiers are false.
// -----------------------------------------------------------------------------
class MediaTransportError {
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}


// -----------------------------------------------------------------------------
// 1c. Typed response shapes (just the fields `parse` reads). Each provider's
// `parse` decodes its response with `baml.json.from_json<…>` into one of these and
// reads the inner content string, then SAP-parses that into the caller's `T`.
// (Replaces the invented `Response.json_path("a.b.c")` reader.)
// -----------------------------------------------------------------------------
class OpenAIChatMessage  { content: string }
class OpenAIChatChoice   { message: OpenAIChatMessage }
class OpenAIChatResponse { choices: OpenAIChatChoice[] }

class AnthropicBlock     { text: string }
class AnthropicResponse  { content: AnthropicBlock[] }

class GeminiPart         { text: string }
class GeminiContent      { parts: GeminiPart[] }
class GeminiCandidate    { content: GeminiContent }
class GeminiResponse     { candidates: GeminiCandidate[] }


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
    function upload(self, ref: MediaRef) -> MediaRef
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai file upload failed");
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let view = baml.media.view(prompt);
      // Build the content array as native json. Text first, then each media part in
      // its accepted form. Each part is a `map<string, json>`; `parts` is a `json[]`.
      let parts: json[] = [{ "type": "text", "text": view.text }];
      for (let part in view.media) {
        let r = self.resolve(part.ref);             // <-- negotiation happens HERE
        match (r.kind) {
          image => {
            // image_url takes a remote URL or a data: URL — both land in the same field.
            let u = match (r.which()) { url => r.url!, _ => baml.media.to_data_url(r) };
            parts.push({
              "type": "image_url",
              "image_url": { "url": u, "detail": "auto" },
            });
          },
          audio => {
            parts.push({
              "type": "input_audio",
              "input_audio": {
                "data": baml.media.to_base64(r),
                "format": baml.media.audio_format(r),     // "wav" | "mp3"
              },
            });
          },
          pdf => {
            parts.push({ "type": "input_file", "file_id": r.handle! });
          },
          video => { baml.sys.panic("unreachable: resolve() rejects video for OpenAI"); },
        }
      }
      let body: map<string, json> = {
        "model": self.model,
        "messages": [ { "role": "user", "content": parts } ],
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    } catch (e) {
      // Foreign Io from baml.media.{view,to_data_url,to_base64,audio_format} normalize here;
      // deliberate Unsupported/MediaTransportError from resolve are known CallErrors and pass through.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Parse the response into a typed class, then SAP the inner content into T.
      let resp: OpenAIChatResponse = baml.json.from_json<OpenAIChatResponse>(
        baml.json.parse(from.text()));
      let choice = resp.choices.at(0)!;
      baml.sap.parse<T>(choice.message.content)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let view = baml.media.view(prompt);
      let parts: json[] = [{ "type": "text", "text": view.text }];
      for (let part in view.media) {
        let r = self.resolve(part.ref);             // re-frames data_url -> bytes; rejects audio/video
        // `image` and `document` differ only in the top-level `type` tag; the `source`
        // sub-object is identical (base64+media_type or url). Factor it.
        let source: map<string, json> = match (r.which()) {
          url   => { "type": "url", "url": r.url! },
          _     => { "type": "base64",
                     "media_type": r.mime!,        // resolve() guarantees mime is set
                     "data": baml.media.to_base64(r) },
        };
        let tag = match (r.kind) { pdf => "document", _ => "image" };
        parts.push({ "type": tag, "source": source });
      }
      let body: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,
        "messages": [ { "role": "user", "content": parts } ],
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": self.api_key catch (e) { _ => env.ANTHROPIC_API_KEY },
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: baml.json.stringify(body),
      }
    } catch (e) {
      // Foreign Io from baml.media.{view,to_base64,guess_mime} normalize here; deliberate
      // Unsupported/MediaTransportError from resolve are known CallErrors and pass through.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp: AnthropicResponse = baml.json.from_json<AnthropicResponse>(
        baml.json.parse(from.text()));
      baml.sap.parse<T>(resp.content.at(0)!.text)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
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
    function upload(self, ref: MediaRef) -> MediaRef
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function                              // POST .../upload/v1beta/files, poll ACTIVE
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini files upload failed");
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let view = baml.media.view(prompt);
      let parts: json[] = [{ "text": view.text }];
      for (let part in view.media) {
        let r = self.resolve(part.ref);             // video-bytes -> upload() -> handle, here
        let p: map<string, json> = match (r.which()) {
          handle => {
            "file_data": { "mime_type": baml.media.guess_mime(r), "file_uri": r.handle! },
          },
          _ => {
            "inline_data": { "mime_type": r.mime!, "data": baml.media.to_base64(r) },
          },
        };
        parts.push(p);
      }
      let body: map<string, json> = {
        "contents": [ { "role": "user", "parts": parts } ],
      };
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent",
        headers: { "x-goog-api-key": self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    } catch (e) {
      // Foreign Io from baml.media.{view,guess_mime,to_base64} and from resolve's pre-upload
      // normalize here; deliberate Unsupported/MediaTransportError are known CallErrors.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp: GeminiResponse = baml.json.from_json<GeminiResponse>(
        baml.json.parse(from.text()));
      baml.sap.parse<T>(resp.candidates.at(0)!.content.parts.at(0)!.text)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini parse failed");
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

  // TranscribeFirst is a request/response combinator: it rewrites the PromptAst (folding
  // audio it can't ingest into transcript text) and forwards to `inner`. So its call path
  // is an `HttpProvider` capability now (provider-as-marker.md): it OWNS call + call_with,
  // and `call_with` FORWARDS the projection to `inner` so the inner provider's sidecar
  // (usage/cost) is NOT dropped (the P1 fix). The combinator factories (with_retry/
  // fallback_to/cached) are inherited via `requires Provider` — not redefined here.
  implements HttpProvider {
    type Body = unknown

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
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
      // Forward to inner's request/response call_with so its ResponseMeta (usage/cost of
      // the actual chat completion) reaches `project` instead of being thrown away.
      match (self.inner) {
        let h: HttpProvider => { return h.call_with<T, U>(rewritten, project); },
        _ => { throw baml.errors.Unsupported { message: "transcribe-first inner is not request/response" }; },
      }
    } catch (e) {
      // Foreign Io from baml.media.view normalizes here; known CallErrors from
      // fold_audio_to_text (STT) and inner.call_with pass through with_message unchanged.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "transcribe-first call failed");
    }

    // `call` derives from call_with — keep it (the basic single-value path).
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  // Replace every audio part with "[audio transcript: ...]" text, transcribing via stt.
  function fold_audio_to_text(self, prompt: baml.llm.PromptAst) -> baml.llm.PromptAst
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let view = baml.media.view(prompt);
    let extra = "";
    for (let part in view.media) {
      if (part.ref.kind == audio) {
        // stt is a Provider (marker); `call` is an HttpProvider capability now — match for it.
        let text: string = match (self.stt) {
          let h: HttpProvider => h.call<string>(baml.media.audio_only_prompt(part.ref)),
          _ => { throw baml.errors.Unsupported { message: "stt provider cannot transcribe (no request/response call)" }; },
        };
        extra = extra + "\n[audio transcript: " + text + "]";
      }
    }
    baml.media.with_text_and_drop_kind(prompt, view.text + extra, audio)
  } catch (e) {
    // Foreign Io from baml.media.{view,audio_only_prompt,with_text_and_drop_kind} normalizes
    // here; the STT sub-call's CallErrors are known and pass through with_message unchanged.
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "stt transcribe failed");
  }
}

// baml.media.audio_only_prompt(ref)               -> PromptAst   // a STT-shaped prompt
// baml.media.with_text_and_drop_kind(p, text, k)  -> PromptAst   // new AST: text + media minus kind k
