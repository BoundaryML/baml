// ============================================================================
// 06 · Non-text output (image / speech / transcription) — LIBRARY code
// ============================================================================
//
// Builds on the canonical spine in ../_conventions.md (Provider, HttpProvider,
// Streaming). Nothing here redefines those — we only ADD:
//
//   • a `Media` result type (mime + data | url)        ← net-new value type
//   • a `Usage` cost record with TWO meters             ← net-new value type
//   • `baml.media.*` host decoders                      ← net-new host boundary
//   • two provider shapes for the same capability:
//       (a) dedicated-endpoint, output-specialized (GptImage / GptTts / GptStt)
//       (b) in-conversation built-in tool (Gpt55 returns mixed modality)
//
// The teaching point: the SAME `Provider.call<T>` interface absorbs an output
// that is NOT a string and NOT SAP-parsed — the divergence lives entirely in
// `parse` + the associated `type Body`, exactly as proposal §2 claims.

// ----------------------------------------------------------------------------
// Net-new value types
// ----------------------------------------------------------------------------

// The text-shaped abstraction does not fit non-text output (background §"What's
// hard": "Output isn't a string"). A media result carries a mime type plus the
// payload, which is EITHER inline bytes OR a hosted URL — providers differ on
// which (OpenAI dedicated = b64 bytes; OpenAI hosted images = URL; Gemini =
// inline part). One type papers over both.
class Media {
  mime: string                 // "image/png", "audio/mp3", ...
  data: uint8array?            // inline bytes (base64-decoded), if present
  url: string?                 // hosted asset, if the provider returned a link
  revised_prompt: string?      // provider may REWRITE your prompt; surface it
}

// Two cost meters (background §"What's hard": "Two paths, two cost meters").
// A dedicated endpoint bills ONLY generation. An in-conversation tool bills
// mainline tokens PLUS generation. Keeping both fields lets the app see which
// path it actually took.
class Usage {
  mainline_tokens: int         // 0 for a dedicated endpoint
  generation_units: int        // images, audio-seconds, or transcription-seconds
  generation_kind: string      // "image" | "audio" | "transcription"
}

// A response that may interleave modalities. The in-conversation path (Gpt55)
// returns this; a dedicated endpoint never does. `T` in `call<T>` is specialized
// to `Media` for dedicated models and to `MixedReply` for the tool path.
class MixedReply {
  text: string?                // the otherwise-textual reply, if any
  media: Media[]               // generated assets, in output order
  usage: Usage
}

// ----------------------------------------------------------------------------
// Net-new host boundary: media decoders. These wrap the raw codec details that
// cannot be written in pure BAML (base64 -> bytes, sniffing mime, etc).
// ----------------------------------------------------------------------------

// (declared, not implemented here — these are the host primitives this scenario
//  requires the runtime to expose; flagged net-new in evaluation.md)
//   baml.media.image_from_base64(b64: string) -> Media          // mime image/png
//   baml.media.audio_from_bytes(bytes: uint8array, mime: string) -> Media
//   baml.media.from_url(url: string, mime: string) -> Media

// ----------------------------------------------------------------------------
// A capability for the cost/usage meter. Not every provider exposes usage, so
// it is its OWN interface (requires Provider) rather than a field on the base.
// ----------------------------------------------------------------------------
interface Metered requires Provider {
  // The usage of the LAST call on this provider value. Host-backed because the
  // figure is read off the wire response the codec already consumed.
  function last_usage(self) -> Usage throws Error
}

// ============================================================================
// (a) DEDICATED ENDPOINTS — output-specialized providers.
//     Body is an HTTP Response; `parse` decodes BYTES, never SAP. `T` is pinned
//     to `Media` for these — they cannot produce an arbitrary schema.
// ============================================================================

class GptImage {
  model: string                // "gpt-image-2"
  api_key: string
  size: string                 // "1024x1024" — a provider field, not an options blob
  partial_images: int          // 0..3 preview frames (drives Streaming below)

  function from_model(model: string) -> GptImage {            // static factory
    GptImage { model: model, api_key: env.OPENAI_API_KEY, size: "1024x1024", partial_images: 0 }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    // The schema does NOT go on the wire as a response_format — the endpoint
    // only emits images. `T` is here for the interface, but build_request
    // ignores it. (This is the divergence from a chat model's build_request,
    // which would branch four ways on where the schema lands.)
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/images/generations",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.encode({
          "model": self.model,
          "prompt": baml.llm.prompt_text(prompt),    // image endpoint takes raw text, no roles
          "size": self.size,
          "response_format": "b64_json",
        }),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      // Image gen can take ~2 min (background §"Generation-specific failure modes").
      baml.http.send(request) catch (e) {
        let t: baml.errors.Timeout => { throw baml.errors.LlmClient { message: "image gen timed out" }; },
        _ => { throw baml.errors.LlmClient { message: "image http failed" }; },
      }
    }

    // No SAP. Decode b64 -> Media. T is asserted to be Media at the host
    // boundary; if a caller writes call<Resume> against GptImage it fails here
    // (and the compile-time escape hatch — `function … -> GptImage` returning a
    // provider whose only honest output is Media — is the only static guard).
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) {
        // prompt moderation has its own error shape — surface it distinctly
        throw baml.errors.LlmClient { message: "image gen rejected: " + from.json_path("error.message") };
      }
      let m: Media = baml.media.image_from_base64(from.json_path("data.0.b64_json"));
      let revised: string = from.json_path("data.0.revised_prompt") catch (e) { _ => { "" } };
      baml.media.as<T>(Media { mime: m.mime, data: m.data, url: null,
                               revised_prompt: if (revised == "") { null } else { revised } })
    }
  }

  // Image gen can stream PARTIAL preview images (0..3) on BOTH paths. Same call,
  // richer delivery -> it is a Streaming refinement, not a different shape.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      // TStream = Media (partial frame), TFinal = Media (final image).
      let req = self.build_request<TFinal>(prompt);
      let sse = baml.http.fetch_sse(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "partial-image sse failed" }; } };
      baml.llm.stream_from_sse<TStream, TFinal>(sse, line => {
        // each "...partial_image" event -> a Media frame; the final -> TFinal
        baml.media.image_from_base64(baml.json.field(line, "partial_image_b64"))
      })
    }
  }

  implements Metered {
    function last_usage(self) -> Usage throws baml.errors.LlmClient {
      // dedicated endpoint: NO mainline tokens, exactly one image.
      Usage { mainline_tokens: 0, generation_units: 1, generation_kind: "image" }
    }
  }
}

class GptTts {
  model: string                // "gpt-4o-mini-tts"
  api_key: string
  voice: string                // "alloy"

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/audio/speech",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.encode({ "model": self.model, "voice": self.voice,
                                 "input": baml.llm.prompt_text(prompt), "format": "mp3" }),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "tts http failed" }; } }
    }

    // Audio comes back as a raw byte body, not JSON. `parse` reads bytes(),
    // not text() — a different Body decode than chat or image.
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "tts failed: " + from.status_code }; }
      let m: Media = baml.media.audio_from_bytes(from.bytes(), "audio/mp3");
      baml.media.as<T>(m)
    }
  }

  implements Metered {
    function last_usage(self) -> Usage throws baml.errors.LlmClient {
      Usage { mainline_tokens: 0, generation_units: 1, generation_kind: "audio" }
    }
  }
}

class GptStt {
  model: string                // "gpt-4o-transcribe"
  api_key: string

  implements HttpProvider {
    type Body = baml.http.Response

    // STT INVERTS the modality: input is media, output is text. The audio rides
    // the prompt as a media attachment; build_request lifts it into multipart.
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/audio/transcriptions",
        headers: { "authorization": "Bearer " + self.api_key,
                   "content-type": "multipart/form-data" },
        body: baml.llm.multipart_audio(prompt, self.model),   // host: pulls the audio part out of the AST
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "stt http failed" }; } }
    }

    // Output IS a string here — so SAP is back. STT is the one non-text-output
    // provider whose result is text; `T` is a normal schema (e.g. string, or a
    // struct with word timestamps).
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "stt failed: " + from.status_code }; }
      baml.sap.parse<T>(from.text())
    }
  }

  implements Streaming {
    // STT streams PARTIAL TEXT — degrade-to-call works (buffer the final).
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      let sse = baml.http.fetch_sse(self.build_request<TFinal>(prompt))
        catch (e) { _ => { throw baml.errors.LlmClient { message: "stt sse failed" }; } };
      baml.llm.stream_text<TStream, TFinal>(sse)
    }
  }

  implements Metered {
    function last_usage(self) -> Usage throws baml.errors.LlmClient {
      Usage { mainline_tokens: 0, generation_units: 1, generation_kind: "transcription" }
    }
  }
}

// ============================================================================
// (b) IN-CONVERSATION PATH — image generation as a built-in TOOL on a mainline
//     model. SAME endpoint as a chat model; the image arrives as one output
//     ITEM among text/reasoning. `T` is specialized to `MixedReply` here.
// ============================================================================

class Gpt55 {
  model: string                // "gpt-5.5" — a MAINLINE model
  api_key: string
  image_tool: bool             // attach the built-in image_generation tool?

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // The Responses endpoint — the divergence from the dedicated path is that
      // we name a MAINLINE model and attach a built-in tool; the model picks the
      // sub-model and bills mainline tokens PLUS generation.
      let tools = if (self.image_tool) { [{ "type": "image_generation" }] } else { [] };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/responses",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.encode({
          "model": self.model,
          "input": baml.llm.prompt_text(prompt),
          "tools": tools,
        }),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "responses http failed" }; } }
    }

    // MIXED-MODALITY parse (background §"What's hard": "Mixed-modality
    // responses"). One response interleaves text, image_generation_call items,
    // and reasoning — we must walk the output array and bucket each item by type.
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "responses failed: " + from.json_path("error.message") }; }
      let n: int = baml.json.len(from.text(), "output");
      let media: Media[] = [];
      let text: string = "";
      for (let i in baml.range(0, n)) {
        let kind: string = from.json_path("output." + i + ".type");
        match (kind) {
          "image_generation_call" => {
            let m: Media = baml.media.image_from_base64(from.json_path("output." + i + ".result"));
            let revised: string = from.json_path("output." + i + ".revised_prompt") catch (e) { _ => { "" } };
            media = media.append(Media { mime: m.mime, data: m.data, url: null,
                                         revised_prompt: if (revised == "") { null } else { revised } });
          },
          "message" => { text = text + from.json_path("output." + i + ".content.0.text"); },
          _ => { /* reasoning / tool plumbing items: skip */ },
        }
      }
      baml.media.as<T>(MixedReply {
        text: if (text == "") { null } else { text },
        media: media,
        usage: Usage {
          mainline_tokens: from.json_path("usage.total_tokens"),     // mainline tokens billed
          generation_units: media.len(),                            // PLUS generation cost
          generation_kind: "image",
        },
      })
    }
  }

  // Partial-image streaming also exists on the Responses path.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      let sse = baml.http.fetch_sse(self.build_request<TFinal>(prompt))
        catch (e) { _ => { throw baml.errors.LlmClient { message: "responses sse failed" }; } };
      baml.llm.stream_from_sse<TStream, TFinal>(sse, line => {
        baml.media.image_from_base64(baml.json.field(line, "partial_image_b64"))
      })
    }
  }

  implements Metered {
    function last_usage(self) -> Usage throws baml.errors.LlmClient {
      // both meters tick on this path.
      Usage { mainline_tokens: 0, generation_units: 0, generation_kind: "image" }   // real figure filled by parse
    }
  }
}

// ============================================================================
// A combinator specific to this scenario: SaveTo wraps any image/audio provider
// and persists the Media payload as a side effect, returning the same value.
// Demonstrates that the Media-shaped output composes through the combinator
// layer exactly like a text value does.
// ============================================================================
class SaveTo {
  inner: Provider
  dir: string

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let v: T = self.inner.call<T>(prompt);
      // best-effort persist; only acts when T is Media-bearing.
      baml.media.persist_if_media<T>(v, self.dir) catch (e) { _ => { /* non-media T: no-op */ } };
      v
    }
  }
}
