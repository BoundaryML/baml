// ============================================================================
// 06 · Non-text output (image / speech / transcription) — LIBRARY code
// ============================================================================
//
// Builds on the canonical spine in ../_conventions.md (Provider, HttpProvider,
// Streaming, call_with/ResponseMeta). Nothing here redefines those — we only ADD:
//
//   • a `Media` result type (mime + data | url)        ← net-new value type
//   • a `Usage` cost record with TWO meters             ← net-new value type
//   • `baml.media.*` host decoders                      ← net-new host boundary
//   • provider-specific `ResponseMeta` impls that normalize the cost meters
//     and the provider-revised prompt off the wire response LAZILY
//   • two provider shapes for the same capability:
//       (a) dedicated-endpoint, output-specialized (GptImage / GptTts / GptStt)
//       (b) in-conversation built-in tool (Gpt55 returns mixed modality)
//
// The teaching point: the SAME `HttpProvider.call<T>` codec absorbs an output
// that is NOT a string and NOT SAP-parsed — the divergence lives entirely in
// `parse` + the associated `type Body`, exactly as proposal §2 claims. (`call`/
// `call_with` belong to `HttpProvider`, not to the bare `Provider` marker — see
// ../provider-as-marker.md.) The PRODUCT sidecars (token/generation usage, the
// provider-revised prompt) ride `HttpProvider.call_with` + `ResponseMeta`
// (../value-sidecar-model.md), NOT a bespoke `Metered`/`last_usage` companion —
// so usage survives a Fallback/Retry instead of degrading to zero.

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
  // NOTE: `revised_prompt` USED to live here, as a value field. It is a PRODUCT
  // SIDECAR ("the value, plus what the provider changed about your request"), so
  // it now rides `call_with` + `ResponseMeta.revised_prompt()` instead — see the
  // ResponseMeta impls below. The Media value is JUST the asset again.
}

// Two cost meters (background §"What's hard": "Two paths, two cost meters").
// A dedicated endpoint bills ONLY generation. An in-conversation tool bills
// mainline tokens PLUS generation. Keeping both fields lets the app see which
// path it actually took. This is the `U` a caller projects out of ResponseMeta
// via `call_with` — NOT a field stuffed into the answer `T`.
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
  // NOTE: `usage` USED to be a field here — the in-conversation path folded the
  // two cost meters into the answer `T` because `call<T> -> T` had no other slot.
  // That was the (c)#3 awkwardness: the dedicated path (`-> Media`) had nowhere
  // to put the same figure. With `call_with`, usage is a uniform SIDECAR on BOTH
  // paths (m => m.usage()), so MixedReply is just the interleaved value again.
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
// Concrete typed errors this scenario raises on purpose. Each `implements` the
// capability interface(s) it can surface on, so the deliberate throw is legal on
// the method's channel and the app can branch on it by runtime match.
// ----------------------------------------------------------------------------

// HTTP non-2xx from a media endpoint (tts/stt/responses). Carries the status so
// the app can triage; classifies as neither network nor rate-limit nor parse.
class OpenAiMediaHttpError {
  status: int
  detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  // The same error can surface mid-stream (partial-image / partial-text SSE).
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// Image-gen prompt moderation: an HTTP 200-ish decline where the provider
// refuses to draw the prompt. Canonical Refused shape from _conventions.md.
class Refused {
  category: string
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// ----------------------------------------------------------------------------
// The cost/usage meter is a PRODUCT SIDECAR, not a bespoke capability. It used
// to be an interface `Metered { last_usage(self) -> Usage }` with its own
// `baml.errors.MeterError` channel — a parallel companion that (a) was stateful
// on an immutable provider value ("the LAST call"), (b) raced under concurrency,
// and (c) was SILENTLY DROPPED through a Fallback that forwarded `.call`. All
// three are exactly the P1 failure the value+sidecar model removes.
//
// The replacement: usage and the provider-revised prompt are normalized LAZILY
// off the wire response by each provider's `ResponseMeta` impl, and the app
// reaches them through `HttpProvider.call_with` (which combinators forward — usage
// survives Fallback/Retry instead of degrading to zero). `MeterError` is gone:
// reading usage is no longer a separate fallible method, it is a pure projection
// over a response the codec already parsed, so it rides the `CallError` channel
// of the `call_with` that produced it.
//
// `ResponseMeta` is an interface (../_conventions.md "Value + sidecar model").
// The base shape carries `usage`/`finish_reason`/`logprobs`/`citations`/
// `reasoning`; this scenario adds ONE media-specific dimension as an
// external-impl interface (the §7 "novel dimension added later" pattern), so a
// provider that can't revise a prompt is `Unsupported`, not silently null.
// ----------------------------------------------------------------------------
interface RevisesPrompt {
  // image endpoints REWRITE your prompt and echo the rewrite back; chat/tts/stt
  // do not. `Supported<T>` keeps "this provider can't" distinct from "empty".
  function revised_prompt(self) -> Supported<string>
}

// Shared media-usage projection helpers, used by the ResponseMeta impls below so
// the parse path and the projection path read the SAME fields off the wire.
function dedicated_usage(kind: string) -> Usage {
  // a dedicated endpoint: NO mainline tokens, exactly one generation unit.
  Usage { mainline_tokens: 0, generation_units: 1, generation_kind: kind }
}

// ----------------------------------------------------------------------------
// JSON-read navigation helpers. The wire responses are dynamic (walk an `output`
// array by index, branch on each item's `type`), so we parse the body with the
// REAL `baml.json.parse` into the `json` union and navigate the `map`/`json[]`
// arms — there is no `Response.json_path` (that was invented). Each helper match-
// narrows the `json` union and returns a sensible default on a shape miss, so the
// parse path stays total (the trailing `catch` still boxes genuine failures).
// ----------------------------------------------------------------------------

// A field off a json object, or `null` if `j` is not an object / the key is absent.
function json_field(j: baml.json.json, key: string) -> baml.json.json {
  match (j) {
    let m: map<string, baml.json.json> => match (m.get(key)) { null => null, let v: baml.json.json => v },
    _                                  => null,
  }
}

// An element off a json array, or `null` if `j` is not an array / index is out of range.
function json_at(j: baml.json.json, i: int) -> baml.json.json {
  match (j) {
    let xs: baml.json.json[] => match (xs.at(i)) { null => null, let v: baml.json.json => v },
    _                        => null,
  }
}

// Coerce a json value to a string, "" on a non-string (matches the old json_path
// string reads, which yielded the empty string on a miss/absent rewrite).
function json_str(j: baml.json.json) -> string {
  match (j) { let s: string => s, _ => "" }
}

// Coerce a json value to an int, 0 on a non-int.
function json_int(j: baml.json.json) -> int {
  match (j) { let n: int => n, _ => 0 }
}

// The length of a json array, 0 on a non-array.
function json_array_len(j: baml.json.json) -> int {
  match (j) { let xs: baml.json.json[] => xs.length(), _ => 0 }
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Build a native `map<string, json>` body, then serialize with the REAL
      // `baml.json.stringify` (there is no `baml.json.encode`). Every value here is
      // already a `json` (strings), so the literal IS a `map<string, json>`.
      let body: map<string, baml.json.json> = {
        "model": self.model,
        "prompt": baml.llm.prompt_text(prompt),    // image endpoint takes raw text, no roles
        "size": self.size,
        "response_format": "b64_json",
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/images/generations",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Image gen can take ~2 min (background §"Generation-specific failure modes").
      // Foreign Io/Timeout are normalized into UnknownError by the trailing catch.
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai image http failed");
    }

    // No SAP. Decode b64 -> Media. T is asserted to be Media at the host
    // boundary; if a caller writes call<Resume> against GptImage it fails here
    // (and the compile-time escape hatch — `function … -> GptImage` returning a
    // provider whose only honest output is Media — is the only static guard).
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let doc: baml.json.json = baml.json.parse(from.text());
      if (!from.ok()) {
        // prompt moderation is a deliberate domain decline — a typed Refused the
        // app branches on, distinct from transport failure.
        throw Refused { category: "image_moderation", message: json_str(json_field(json_field(doc, "error"), "message")) };
      }
      let m: Media = baml.media.image_from_base64(json_str(json_field(json_at(json_field(doc, "data"), 0), "b64_json")));
      // NOTE: revised_prompt is NOT read here anymore — it is a sidecar, read
      // lazily by OpenAiImageResponse.revised_prompt() only if the app projects it.
      baml.media.as<T>(Media { mime: m.mime, data: m.data, url: null })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai image parse failed");
    }

    // The sidecar normalizer. `call_with`'s default body calls this; it wraps the
    // already-sent body in a provider response type that reads usage / the
    // revised prompt LAZILY (m => m.usage() never touches the revised_prompt path).
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAiImageResponse { raw: from }
    }
  }

  // Image gen can stream PARTIAL preview images (0..3) on BOTH paths. Same call,
  // richer delivery -> it is a Streaming refinement, not a different shape.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      // TStream = Media (partial frame), TFinal = Media (final image).
      let req = self.build_request<TFinal>(prompt);
      let sse = baml.http.fetch_sse(req);
      baml.llm.stream_from_sse<TStream, TFinal>(sse, line => {
        // each "...partial_image" event -> a Media frame; the final -> TFinal.
        // Parse the SSE line with the REAL `baml.json.parse`, then pull the b64 field.
        baml.media.image_from_base64(json_str(json_field(baml.json.parse(line), "partial_image_b64")))
      })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.StreamError>(e, "openai partial-image sse failed");
    }
  }
}

// The provider response type for image generation. Holds the wire response and
// normalizes the sidecars LAZILY (../value-sidecar-model.md §2). Implements the
// base `ResponseMeta` PLUS the external `RevisesPrompt` dimension — image
// endpoints rewrite the prompt; chat/tts/stt do not.
class OpenAiImageResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage                    { dedicated_usage("image") }
    function finish_reason(self) -> string           { "stop" }
    function logprobs(self) -> Supported<Logprob[]>  { Unsupported { reason: "image endpoint" } }
    function citations(self) -> Supported<Citation[]> { Unsupported { reason: "image endpoint" } }
    function reasoning(self) -> Supported<string>     { Unsupported { reason: "image endpoint" } }
  }
  implements RevisesPrompt {
    function revised_prompt(self) -> Supported<string> {
      let doc: baml.json.json = baml.json.parse(self.raw.text()) catch (e) { _ => { null } };
      let r: string = json_str(json_field(json_at(json_field(doc, "data"), 0), "revised_prompt"));
      if (r == "") { Unsupported { reason: "no rewrite this call" } } else { r }
    }
  }
}

class GptTts {
  model: string                // "gpt-4o-mini-tts"
  api_key: string
  voice: string                // "alloy"

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let body: map<string, baml.json.json> = {
        "model": self.model, "voice": self.voice,
        "input": baml.llm.prompt_text(prompt), "format": "mp3",
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/audio/speech",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai tts http failed");
    }

    // Audio comes back as a raw byte body, not JSON. `parse` reads bytes(),
    // not text() — a different Body decode than chat or image.
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw OpenAiMediaHttpError { status: from.status_code, detail: "tts failed" }; }
      let m: Media = baml.media.audio_from_bytes(from.bytes(), "audio/mp3");
      baml.media.as<T>(m)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai tts parse failed");
    }

    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAiAudioResponse { raw: from, kind: "audio" }
    }
  }
}

// Provider response type for TTS/STT (audio generation / transcription). Same
// dedicated-usage shape, different `generation_kind`. Does NOT implement
// RevisesPrompt — projecting revised_prompt over this response is an
// `Unsupported` match-miss, which is the honest answer.
class OpenAiAudioResponse {
  raw: baml.http.Response
  kind: string                                       // "audio" | "transcription"
  implements ResponseMeta {
    function usage(self) -> Usage                    { dedicated_usage(self.kind) }
    function finish_reason(self) -> string           { "stop" }
    function logprobs(self) -> Supported<Logprob[]>  { Unsupported { reason: "audio endpoint" } }
    function citations(self) -> Supported<Citation[]> { Unsupported { reason: "audio endpoint" } }
    function reasoning(self) -> Supported<string>     { Unsupported { reason: "audio endpoint" } }
  }
}

class GptStt {
  model: string                // "gpt-4o-transcribe"
  api_key: string

  implements HttpProvider {
    type Body = baml.http.Response

    // STT INVERTS the modality: input is media, output is text. The audio rides
    // the prompt as a media attachment; build_request lifts it into multipart.
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/audio/transcriptions",
        headers: { "authorization": "Bearer " + self.api_key,
                   "content-type": "multipart/form-data" },
        body: baml.llm.multipart_audio(prompt, self.model),   // host: pulls the audio part out of the AST
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai stt http failed");
    }

    // Output IS a string here — so SAP is back. STT is the one non-text-output
    // provider whose result is text; `T` is a normal schema (e.g. string, or a
    // struct with word timestamps).
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw OpenAiMediaHttpError { status: from.status_code, detail: "stt failed" }; }
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai stt parse failed");
    }

    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAiAudioResponse { raw: from, kind: "transcription" }
    }
  }

  implements Streaming {
    // STT streams PARTIAL TEXT — degrade-to-call works (buffer the final).
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      let sse = baml.http.fetch_sse(self.build_request<TFinal>(prompt));
      baml.llm.stream_text<TStream, TFinal>(sse)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.StreamError>(e, "openai stt sse failed");
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The Responses endpoint — the divergence from the dedicated path is that
      // we name a MAINLINE model and attach a built-in tool; the model picks the
      // sub-model and bills mainline tokens PLUS generation.
      // `tools` is a native `json[]` (each tool a `map<string, json>`); the body is
      // a `map<string, json>` literal serialized with the REAL `baml.json.stringify`.
      let tools: baml.json.json[] = if (self.image_tool) { [{ "type": "image_generation" }] } else { [] };
      let body: map<string, baml.json.json> = {
        "model": self.model,
        "input": baml.llm.prompt_text(prompt),
        "tools": tools,
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/responses",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai responses http failed");
    }

    // MIXED-MODALITY parse (background §"What's hard": "Mixed-modality
    // responses"). One response interleaves text, image_generation_call items,
    // and reasoning — we must walk the output array and bucket each item by type.
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let doc: baml.json.json = baml.json.parse(from.text());
      if (!from.ok()) { throw OpenAiMediaHttpError { status: from.status_code, detail: json_str(json_field(json_field(doc, "error"), "message")) }; }
      // Walk the `output` array off the parsed `json` (no `Response.json_path`).
      let output: baml.json.json = json_field(doc, "output");
      let n: int = json_array_len(output);
      let media: Media[] = [];
      let text: string = "";
      let i: int = 0;
      while (i < n) {
        let item: baml.json.json = json_at(output, i);
        let kind: string = json_str(json_field(item, "type"));
        match (kind) {
          "image_generation_call" => {
            let m: Media = baml.media.image_from_base64(json_str(json_field(item, "result")));
            // revised_prompt is a sidecar, not a Media field — read lazily by
            // OpenAiResponsesResponse.revised_prompt() if the app projects it.
            media.push(Media { mime: m.mime, data: m.data, url: null });
          },
          "message" => { text = text + json_str(json_field(json_at(json_field(item, "content"), 0), "text")); },
          _ => { /* reasoning / tool plumbing items: skip */ },
        }
        i = i + 1;
      }
      // The two-meter Usage is no longer folded into the answer — it is read off
      // the same body by the ResponseMeta impl below and projected via call_with.
      baml.media.as<T>(MixedReply {
        text: if (text == "") { null } else { text },
        media: media,
      })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai responses parse failed");
    }

    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAiResponsesResponse { raw: from }
    }
  }

  // Partial-image streaming also exists on the Responses path.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      let sse = baml.http.fetch_sse(self.build_request<TFinal>(prompt));
      baml.llm.stream_from_sse<TStream, TFinal>(sse, line => {
        // parse the SSE line (REAL `baml.json.parse`), then pull the b64 string field.
        baml.media.image_from_base64(json_str(json_field(baml.json.parse(line), "partial_image_b64")))
      })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.StreamError>(e, "openai responses sse failed");
    }
  }
}

// Provider response type for the in-conversation Responses path. BOTH cost
// meters tick here, so usage() reads the real mainline + generation figures off
// the wire (the dedicated impls hard-code mainline=0). Implements RevisesPrompt:
// the built-in image tool can echo a revised prompt on its output item.
class OpenAiResponsesResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      // walk the output array once: mainline tokens off `usage`, generation_units
      // = the count of image_generation_call items (the SAME walk parse does).
      let doc: baml.json.json = baml.json.parse(self.raw.text()) catch (e) { _ => { null } };
      let output: baml.json.json = json_field(doc, "output");
      let n: int = json_array_len(output);
      let gen: int = 0;
      let i: int = 0;
      while (i < n) {
        if (json_str(json_field(json_at(output, i), "type")) == "image_generation_call") { gen = gen + 1; }
        i = i + 1;
      }
      Usage {
        mainline_tokens: json_int(json_field(json_field(doc, "usage"), "total_tokens")),   // mainline tokens billed
        generation_units: gen,                                                             // PLUS generation cost
        generation_kind: "image",
      }
    }
    function finish_reason(self) -> string {
      let doc: baml.json.json = baml.json.parse(self.raw.text()) catch (e) { _ => { null } };
      let s: string = json_str(json_field(json_at(json_field(doc, "output"), 0), "status"));
      if (s == "") { "stop" } else { s }
    }
    function logprobs(self) -> Supported<Logprob[]>  { Unsupported { reason: "responses path" } }
    function citations(self) -> Supported<Citation[]> { Unsupported { reason: "responses path" } }
    function reasoning(self) -> Supported<string>     { Unsupported { reason: "not surfaced here" } }
  }
  implements RevisesPrompt {
    function revised_prompt(self) -> Supported<string> {
      // walk the output array for the first image_generation_call's rewrite.
      let doc: baml.json.json = baml.json.parse(self.raw.text()) catch (e) { _ => { null } };
      let output: baml.json.json = json_field(doc, "output");
      let n: int = json_array_len(output);
      let i: int = 0;
      while (i < n) {
        let item: baml.json.json = json_at(output, i);
        if (json_str(json_field(item, "type")) == "image_generation_call") {
          let r: string = json_str(json_field(item, "revised_prompt"));
          if (r != "") { return r; }
        }
        i = i + 1;
      }
      Unsupported { reason: "no rewrite this call" }
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

  // SaveTo is a COMBINATOR over the request/response path, so its interaction lives
  // on `HttpProvider` now (Provider is a bare marker — ../provider-as-marker.md).
  // `call_with` is the primitive: it forwards `project` to the inner provider's
  // own `call_with` so the usage/revised-prompt SIDECAR is NOT dropped (the P1 fix),
  // persists the Media value as a side effect, and returns the same (value, sidecar).
  // The combinator factories (with_retry/fallback_to/cached) are inherited from the
  // `Provider` marker via `requires Provider` — not redefined here.
  implements HttpProvider {
    type Body = unknown

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Route to the inner provider's call_with so the projected sidecar survives —
      // a combinator declares the SAME channel, no narrow/widen/forward.
      match (self.inner) {
        let h: HttpProvider => {
          let (v, u): (T, U) = h.call_with<T, U>(prompt, project);
          // best-effort persist; only acts when T is Media-bearing.
          baml.media.persist_if_media<T>(v, self.dir) catch (e) { _ => { /* non-media T: no-op */ } };
          return (v, u);
        },
        _ => {},
      }
      throw baml.errors.Unsupported { message: "SaveTo inner provider cannot call" };
    }

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }
}
