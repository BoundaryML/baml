// ============================================================================
// implement.baml  —  Capability negotiation: knowing what a model can do
// ----------------------------------------------------------------------------
// STD-LIB / library-author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, Streaming, baml.http, baml.sap).
//
// The background lists FOUR discovery mechanisms that diverge across providers:
//
//   1. runtime 400          — everyone: send it, get rejected, adapt
//   2. declarative flags     — Vercel supportedUrls / capability fields (data)
//   3. static metadata table — LiteLLM supports_vision / max_input_tokens
//   4. live query            — Claude Agent SDK q.supportedModels()
//
// The proposal answers capability negotiation with a RUNTIME `match` on the
// existential Provider (§5). That covers mechanism #1 and the *interaction-shape*
// gate (does this provider implement Streaming/Realtime/Tools at all). It does
// NOT cover the GRADIENT inside one capability — "supports JSON schema but only
// shallow", "supports images but not at this resolution", "supports tools but
// not in parallel". A `match` is binary; support is not.
//
// So this file shows the proposal's native answer (the `match`), and then the
// net-new surface required to express the three mechanisms a `match` can't:
//   - a declarative `Capabilities` read-interface (mechanism #2/#3),
//   - a `warnings` sidecar for silent downgrades — which is a PRODUCT sidecar
//     (value AND metadata), so it rides the canonical `call_with` + `ResponseMeta`
//     path (../value-sidecar-model.md, _conventions.md "Value + sidecar model")
//     instead of a bespoke `Warned<T>` companion,
//   - a `negotiate` escape hatch that a probe can call when the static answer
//     is "maybe" (the non-binary gradient -> the runtime probe is the truth).
// ============================================================================


// ----------------------------------------------------------------------------
// §0  Net-new host primitives. The only additions below the interface layer.
// ----------------------------------------------------------------------------

namespace baml.schema {
  // Standard JSON Schema lowering (carried from scenario 02).
  function json_schema(t: type, strict: bool) -> string { $rust_io_function }
  function gemini_schema(t: type) -> string { $rust_io_function }

  // GRADIENT probes — "supported" is not binary, so these answer the shape of
  // the schema, not a yes/no for the provider. They let a declarative
  // capability decide *for this T* whether the static answer holds.
  //   depth: max nesting level of T (Gemini chokes past a point; OpenAI strict
  //          rejects deep $ref cycles).
  function depth(t: type) -> int { $rust_io_function }
  //   uses_unions: T contains oneOf/anyOf (Gemini OpenAPI subset forbids them).
  function uses_unions(t: type) -> bool { $rust_io_function }
  //   uses_refs: T contains $ref cycles (OpenAI strict mode forbids).
  function uses_refs(t: type) -> bool { $rust_io_function }
}

namespace baml.media {
  // Pixel dimensions of an attached image, so a resolution gate is checkable
  // BEFORE the call (the "image-but-not-at-this-resolution" gradient).
  function image_dims(img: image) -> Dims { $rust_io_function }
  class Dims { width: int, height: int }
  // Downscale to a max edge — the SILENT-DOWNGRADE path made explicit.
  function downscale(img: image, max_edge: int) -> image { $rust_io_function }
}


// ----------------------------------------------------------------------------
// §1  NET-NEW: a declarative capability read-interface.
//     This is mechanism #2/#3 (Vercel supportedUrls / LiteLLM static table)
//     ported into the proposal's own idiom: a capability is an interface that
//     `requires Provider`, but this one carries NO behavior — it exposes DATA a
//     caller reads before sending. It is the proposal's missing third leg:
//     §5 gives us "match on the *behavioral* capability"; this gives us
//     "ask the provider, as data, about a refinement INSIDE that capability".
//
//     Crucially every field returns a Support, not a bool — because the
//     background's core finding is "support is not binary". Support has a
//     `Maybe` arm that says "the static table can't decide; probe at runtime".
// ----------------------------------------------------------------------------

// The non-binary support lattice. This is the type a flag SHOULD have been.
enum Support {
  Yes        // statically known to work
  No         // statically known to fail (don't bother sending)
  Maybe      // depends on the specific payload -> the runtime probe is the truth
}

// A declarative, behavior-free capability. `requires Provider` so it is-a
// Provider and rides on the same existential, but every method is a pure read.
interface Capabilities requires Provider {
  // structured output for THIS type T (depth/unions/refs gradient lives here).
  function structured_output<T>(self) -> Support
  // image input at THIS resolution (the resolution gradient).
  function image_input(self, dims: baml.media.Dims) -> Support
  // can it run tools in PARALLEL (a refinement of the Tools capability)?
  function parallel_tools(self) -> Support
  // context window — a scalar, not a Support; lets a caller pre-flight length.
  function max_input_tokens(self) -> int
}


// ----------------------------------------------------------------------------
// §2  Warnings for SILENT downgrades — a PRODUCT sidecar on `call_with`.
//     The background: "Silent downgrades are worse than errors." A provider
//     that ignores `seed`, or downscales an image, succeeds with wrong-ish
//     output. The AI SDK returns a `warnings` array from doGenerate.
//
//     `warnings` is value AND metadata (you got a `T`, AND a note about how it
//     was produced) — the exact PRODUCT shape `call_with` + `ResponseMeta` is
//     for (../value-sidecar-model.md §5). So we do NOT invent a bespoke
//     `Warned<T>` companion; we add ONE `warnings()` dimension to `ResponseMeta`
//     and let callers project it: `call_with(p, m => m.warnings())`. Because
//     `call_with` is the HttpProvider value+sidecar method that combinators
//     forward (Negotiated routes it to a member's `call_with`), a downgrade
//     warning emitted behind a Fallback/Retry survives, where a `Warned<T>`
//     returned from a parallel companion would have been silently dropped.
// ----------------------------------------------------------------------------

class Warning { code: string, message: string }

// `warnings` extends the canonical ResponseMeta interface (_conventions.md
// "Value + sidecar model") as one more provider dimension. A downgrade-aware
// codec records the downgrade INTO the meta it returns; the projection reads it
// back out. No new carrier shape, no out-of-band sink — same path as usage.
interface Warnings requires ResponseMeta {
  function warnings(self) -> Warning[]
}


// ----------------------------------------------------------------------------
// §3  A provider that declares its capabilities statically AND honors the
//     gradient. OpenAI: structured output Yes unless T is too deep / has $ref
//     cycles (strict mode rejects), images up to 2000px, parallel tools Yes.
//
//     Note the four-way return: Yes / No / Maybe. `Maybe` is what makes this
//     more than a flag — for a borderline schema it punts to the runtime probe.
// ----------------------------------------------------------------------------

class OpenAIChat {
  model: string
  api_key: string?
  max_tokens: int?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Capabilities {
    function structured_output<T>(self) -> Support {
      let t = reflect.type_of<T>();
      // OpenAI strict mode rejects $ref cycles outright -> statically No.
      if (baml.schema.uses_refs(t)) { return No; }
      // very deep schemas sometimes 400 (strict has limits) -> can't decide.
      if (baml.schema.depth(t) > 5) { return Maybe; }
      Yes
    }
    function image_input(self, dims: baml.media.Dims) -> Support {
      // OpenAI accepts up to ~2000px edge before it auto-tiles/downscales.
      if (dims.width > 2000 || dims.height > 2000) { return Maybe; }
      Yes
    }
    function parallel_tools(self) -> Support { Yes }
    function max_input_tokens(self) -> int { 128000 }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let strict = baml.schema.uses_refs(reflect.type_of<T>()) == false;
      let schema = baml.schema.json_schema(reflect.type_of<T>(), strict);
      let rf = #"{ "type": "json_schema",
                   "json_schema": { "name": "Output", "schema": {{ schema }}, "strict": {{ strict }} } }"#;
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer {{ self.api_key }}", "content-type": "application/json" },
        body: baml.llm.openai_chat_body(prompt, self.model, self.max_tokens, rf),
      }
    } catch (e) {
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
      if (!from.ok()) {
        // THE RUNTIME 400 — mechanism #1. A structured-output rejection looks
        // like this; the body distinguishes it so a fallback can react. Parse the
        // error envelope into a typed class and read its `code` field.
        let err = baml.json.from_json<OpenAiErrorBody>(baml.json.parse(from.text()))
            catch (e) { _ => { OpenAiErrorBody { error: OpenAiErrorDetail { code: "" } } } };
        if (from.status_code == 400 && err.error.code == "json_schema_unsupported") {
          throw SchemaRejected { message: "openai rejected the schema" };
        }
        throw OpenAiHttpError { status: from.status_code };
      }
      // Decode the chat-completions envelope, then SAP-parse the message content.
      let body = baml.json.from_json<OpenAiChatBody>(baml.json.parse(from.text()));
      baml.sap.parse<T>(content_of(body))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> { $rust_io_function }
  }

  function from_model(model: string) -> OpenAIChat {
    OpenAIChat { model: model, api_key: env.OPENAI_API_KEY, max_tokens: null }
  }
}


// ----------------------------------------------------------------------------
// §4  Gemini diverges on EVERY axis the background calls out:
//     - structured output is ALWAYS constrained, no fallback mode, and the
//       OpenAPI subset REJECTS unions/$ref -> statically No for those T (not
//       Maybe: Gemini won't even accept the schema).
//     - schema dialect mismatch surfaces as a hard 400 on the converted schema.
//     This proves the declarative answer is provider-specific: same T, OpenAI
//     says Yes/Maybe, Gemini says No.
// ----------------------------------------------------------------------------

class Gemini {
  model: string
  api_key: string?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Capabilities {
    function structured_output<T>(self) -> Support {
      let t = reflect.type_of<T>();
      // Gemini's OpenAPI subset forbids oneOf/anyOf and $ref outright.
      if (baml.schema.uses_unions(t)) { return No; }
      if (baml.schema.uses_refs(t))   { return No; }
      // even within the subset, very deep schemas get rejected -> probe.
      if (baml.schema.depth(t) > 4) { return Maybe; }
      Yes
    }
    function image_input(self, dims: baml.media.Dims) -> Support {
      // Gemini tiles aggressively; resolution is rarely the blocker.
      Yes
    }
    // Gemini does NOT support parallel tool calls -> statically No.
    function parallel_tools(self) -> Support { No }
    function max_input_tokens(self) -> int { 1000000 }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // DIFFERENT DIALECT. If the caller ignored the Capabilities `No` and
      // sent a union anyway, the schema lowering still runs and Gemini 400s.
      let schema = baml.schema.gemini_schema(reflect.type_of<T>());
      let gen_cfg = #"{ "responseMimeType": "application/json", "responseSchema": {{ schema }} }"#;
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/{{ self.model }}:generateContent",
        headers: { "x-goog-api-key": "{{ self.api_key }}", "content-type": "application/json" },
        body: baml.llm.gemini_body(prompt, gen_cfg),
      }
    } catch (e) {
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
      if (!from.ok()) {
        let err = baml.json.from_json<GeminiErrorBody>(baml.json.parse(from.text()))
            catch (e) { _ => { GeminiErrorBody { error: GeminiErrorDetail { status: "" } } } };
        if (from.status_code == 400 && err.error.status == "INVALID_ARGUMENT") {
          // the dialect mismatch surfaces ONLY here — declared `No` would have
          // saved the round-trip, but a stale table makes this the real truth.
          throw SchemaRejected { message: "gemini rejected the schema dialect" };
        }
        throw GeminiHttpError { status: from.status_code };
      }
      // Decode the generateContent envelope, then SAP-parse the first part's text.
      let body = baml.json.from_json<GeminiGenerateBody>(baml.json.parse(from.text()));
      baml.sap.parse<T>(text_of(body))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini parse failed");
    }
  }

  // NOTE: no `implements Streaming`. Capability of a DIFFERENT kind (behavioral)
  // is still answered by the §5 `match`, not by Capabilities. The two layers
  // coexist: `match (p) { let s: Streaming => ... }` for "can it stream at all",
  // Capabilities for "can it stream THIS / at THIS fidelity".
}


// ----------------------------------------------------------------------------
// §5  THE PROPOSAL'S NATIVE NEGOTIATION — companions that `match` (proposal §5).
//     This is what the proposal already gives us, reused unchanged. A streaming
//     companion degrades to `.call`; a tools companion errors if absent. The
//     `match` answers the BINARY, behavioral question: is this provider a
//     Streaming / Tools at all. It cannot answer the gradient — that needs §6.
// ----------------------------------------------------------------------------

// Buffer a one-shot answer as a degenerate single-chunk stream (the §5 fallback).
function buffer_as_stream<TStream, TFinal>(value: TFinal)
    -> baml.llm.Stream<TStream, TFinal> { $rust_io_function }


// ----------------------------------------------------------------------------
// §6  NET-NEW combinator: Negotiated — the bridge between the declarative
//     answer (§1) and the runtime probe (§5 / mechanism #1). This is the heart
//     of "support is not binary": on Yes it sends; on No it refuses up front
//     (no wasted 400); on Maybe it PROBES — sends, and on a SchemaRejected it
//     falls back to a degraded mode (prompt-based JSON), recording a WARNING for
//     the silent-downgrade record.
//
//     The downgrade note is a PRODUCT sidecar: the combinator wraps the inner
//     provider's ResponseMeta in a DowngradeMeta that forwards every provider
//     dimension and ANSWERS `warnings()` itself — the same delegation pattern
//     as FrameworkMeta (../value-sidecar-model.md §3). Because it forwards
//     `call_with` (the HttpProvider value+sidecar method) to a member, a
//     projection over the result still sees usage, timings AND the downgrade
//     warning — nothing is dropped under Negotiated.
//
//     It wraps a single Provider and `implements HttpProvider` by routing
//     `call_with` to a member (the bare Provider marker has no call_with, so it
//     matches the member to HttpProvider first); `call` derives from call_with.
//     This is the Fallback/Retry template (§6 of provider-as-marker.md).
// ----------------------------------------------------------------------------

// A meta layer that records this combinator's downgrade warnings while
// forwarding every provider dimension to the inner meta (the §3 delegation).
class DowngradeMeta {
  inner: ResponseMeta
  emitted: Warning[]
  implements ResponseMeta {
    function usage(self)         -> Usage                 { self.inner.usage() }          // forward
    function finish_reason(self) -> string                { self.inner.finish_reason() }
    function logprobs(self)      -> Supported<Logprob[]>  { self.inner.logprobs() }
    function citations(self)     -> Supported<Citation[]> { self.inner.citations() }
    function reasoning(self)     -> Supported<string>     { self.inner.reasoning() }
  }
  implements Warnings {
    // this combinator's downgrade notes, plus any the inner meta already carries.
    function warnings(self) -> Warning[] {
      match (self.inner) {
        let w: Warnings => self.emitted.concat(w.warnings()),
        _               => self.emitted,
      }
    }
  }
}

class Negotiated {
  inner: Provider
  // the degraded fallback: a SECOND provider configured for prompt-based JSON
  // (no response_format) — used when structured output is No or a probe fails.
  degraded: Provider

  implements HttpProvider {
    // call_with is the primitive: it carries the downgrade warning out as a
    // product sidecar. `call` derives from it by projecting nothing (inherited
    // from HttpProvider). Negotiated owns NO codec of its own — build_request/
    // send/parse/meta_of are never reached because call_with is overridden to
    // route to a member's call_with; so this `implements HttpProvider` is a
    // pure forwarder, not a request/response codec.
    type Body = unknown
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Step 1: consult the DECLARATIVE answer, if the provider exposes one.
      // If it doesn't implement Capabilities, we have no static knowledge and
      // must fall straight through to the runtime probe (the honest default).
      match (self.inner) {
        let cap: Capabilities => {
          match (cap.structured_output<T>()) {
            No => {
              // statically unsupported: skip the doomed call entirely, record
              // the downgrade, and run the degraded prompt-based path. NO 400.
              let w = Warning {
                code: "structured_output.unsupported",
                message: "provider cannot constrain this schema; using prompt-based JSON",
              };
              return self.degraded_call_with<T, U>(prompt,
                m => project(DowngradeMeta { inner: m, emitted: [w] }));
            },
            Yes => { return self.inner_call_with<T, U>(prompt, project); },  // confident.
            Maybe => {
              // THE GRADIENT: the table can't decide. Probe at runtime, and on
              // a schema rejection (mechanism #1) fall back + record a warning.
              return self.inner_call_with<T, U>(prompt, project) catch (e) {
                let r: SchemaRejected => {
                  let w = Warning {
                    code: "structured_output.downgraded",
                    message: "schema probe rejected at runtime; fell back to prompt JSON",
                  };
                  self.degraded_call_with<T, U>(prompt,
                    m => project(DowngradeMeta { inner: m, emitted: [w] }))
                },
                _ => { throw e; },   // transport/other errors are not a downgrade.
              };
            },
          }
        },
        // no Capabilities surface at all -> pure runtime-400 negotiation.
        _ => {
          return self.inner_call_with<T, U>(prompt, project) catch (e) {
            let r: SchemaRejected => {
              let w = Warning {
                code: "structured_output.downgraded",
                message: "schema rejected at runtime (no static capability); fell back",
              };
              self.degraded_call_with<T, U>(prompt,
                m => project(DowngradeMeta { inner: m, emitted: [w] }))
            },
            _ => { throw e; },
          };
        },
      }
    }
  }

  // The members are typed `Provider` (the bare marker — no `call_with`), so the
  // forward must MATCH each member to HttpProvider first. These two helpers do
  // the §6-of-provider-as-marker routing: the projection is forwarded intact, so
  // the downgrade/usage sidecar is never dropped (the P1 fix).
  function inner_call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    match (self.inner) {
      let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
      _ => { throw baml.errors.Unsupported { message: "negotiated inner provider has no request/response call" }; },
    }
  }
  function degraded_call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    match (self.degraded) {
      let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
      _ => { throw baml.errors.Unsupported { message: "negotiated degraded provider has no request/response call" }; },
    }
  }

  // Negotiated only forwards the one-shot path. It deliberately does NOT claim
  // Streaming: its whole job is the structured-output gradient on `call`.
}


// ----------------------------------------------------------------------------
// §7  NET-NEW host primitive surfacing the IMAGE-RESOLUTION gradient as a
//     pre-flight gate + silent-downscale, mirroring §6 for a different axis.
//     Shows the gradient is not specific to schemas — "supported" is graded on
//     resolution too, and the same Yes/Maybe/No lattice applies.
//
//     The downscale note is the SAME product sidecar as §6's downgrade warning,
//     but it concerns the REQUEST, not the response — `guard` runs before the
//     call, so there is no ResponseMeta yet to attach it to. It therefore hands
//     the warning back next to the (possibly downscaled) image as a TUPLE
//     `(image, Warning[])`; the caller threads it into the call's projection
//     (usage.baml §4) so it lands in the SAME `m.warnings()` channel as a
//     response-time downgrade. Same carrier shape (tuple), one warnings sink.
// ----------------------------------------------------------------------------

class ResolutionGate {
  inner: Provider
  max_edge: int          // downscale target when the model can't take full-res

  function guard(self, img: image) -> (image, Warning[])
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let dims = baml.media.image_dims(img);
    match (self.inner) {
      let cap: Capabilities => {
        match (cap.image_input(dims)) {
          Yes => { (img, []) },                 // full resolution accepted.
          No  => {
            throw ImageInputUnsupported { message: "model rejects images entirely" };
          },
          Maybe => {
            // graded support: downscale and hand back a WARNING — a silent
            // quality downgrade made explicit so it isn't discovered as a
            // degraded answer later.
            (baml.media.downscale(img, self.max_edge), [Warning {
              code: "image.downscaled",
              message: "image exceeds known-safe resolution; downscaled to {{ self.max_edge }}px",
            }])
          },
        }
      },
      _ => { (img, []) },   // no capability info -> send as-is, let the 400 decide.
    }
  }
}

// A ResponseMeta layer that injects pre-flight (request-time) warnings into the
// response's `warnings()` channel — the same delegation shape as DowngradeMeta,
// so a request-side downscale and a response-side downgrade read out together.
class PreflightMeta {
  inner: ResponseMeta
  emitted: Warning[]
  implements ResponseMeta {
    function usage(self)         -> Usage                 { self.inner.usage() }
    function finish_reason(self) -> string                { self.inner.finish_reason() }
    function logprobs(self)      -> Supported<Logprob[]>  { self.inner.logprobs() }
    function citations(self)     -> Supported<Citation[]> { self.inner.citations() }
    function reasoning(self)     -> Supported<string>     { self.inner.reasoning() }
  }
  implements Warnings {
    function warnings(self) -> Warning[] {
      match (self.inner) {
        let w: Warnings => self.emitted.concat(w.warnings()),
        _               => self.emitted,
      }
    }
  }
}


// ----------------------------------------------------------------------------
// §8  Errors used above. Each is a CONCRETE class implementing the capability
//     error interface(s) it can arise on. SchemaRejected is the typed face of
//     the runtime 400 for structured output — distinct from a transport error
//     so a combinator can branch on "the schema was the problem" (is_parse_error
//     -> true, downgrade-able) vs "the network was the problem" (don't downgrade).
//     It implements StreamError too, because a streamed structured output can be
//     rejected mid-stream. The per-provider HTTP errors are the non-2xx transport
//     face (a 5xx / unexpected 4xx that is NOT a schema rejection).
// ----------------------------------------------------------------------------

// The structured-output runtime-400, as a typed, classified error. is_parse_error
// is what lets Negotiated decide "downgrade" instead of treating it as a blip.
class SchemaRejected {
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return true; }
  }
  implements baml.errors.StreamError {
    // same rejection can land mid-stream on a streamed structured output.
    function is_network_error(self) -> bool { return false; }
  }
}

// OpenAI non-2xx transport failure that is NOT a schema rejection.
class OpenAiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return false; }
  }
}

// Gemini non-2xx transport failure that is NOT a schema rejection.
class GeminiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return false; }
  }
}

// ResolutionGate's pre-flight rejection: the model cannot take images at all.
// Call-path only (the gate runs before .call), so it implements CallError only.
class ImageInputUnsupported {
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}


// ----------------------------------------------------------------------------
// §9  Wire-response decode classes. The codecs above read the provider envelope
//     by coercing `from.text()` into one of these typed classes via
//     `baml.json.from_json<T>` (then reading fields) — the canonical, typed way
//     to read a JSON response. Optional fields default to absent so a partial or
//     error body still decodes; the `*_of` helpers walk the nested shape with
//     ordinary Array `.at` / null handling instead of a string path.
// ----------------------------------------------------------------------------

// --- OpenAI chat-completions success envelope (choices.0.message.content). ---
class OpenAiMessage { content: string? }
class OpenAiChoice  { message: OpenAiMessage? }
class OpenAiChatBody { choices: OpenAiChoice[]? }

// --- OpenAI error envelope (error.code). ---
class OpenAiErrorDetail { code: string? }
class OpenAiErrorBody   { error: OpenAiErrorDetail }

// --- Gemini generateContent success envelope (candidates.0.content.parts.0.text). ---
class GeminiPart      { text: string? }
class GeminiContent   { parts: GeminiPart[]? }
class GeminiCandidate { content: GeminiContent? }
class GeminiGenerateBody { candidates: GeminiCandidate[]? }

// --- Gemini error envelope (error.status). ---
class GeminiErrorDetail { status: string? }
class GeminiErrorBody   { error: GeminiErrorDetail }

// Navigate the decoded OpenAI envelope to the message content (or empty string).
function content_of(body: OpenAiChatBody) -> string {
  match (body.choices) {
    let cs: OpenAiChoice[] => match (cs.at(0)) {
      let c: OpenAiChoice => match (c.message) {
        let m: OpenAiMessage => match (m.content) { let s: string => s, _ => "" },
        _ => "",
      },
      _ => "",
    },
    _ => "",
  }
}

// Navigate the decoded Gemini envelope to the first part's text (or empty string).
function text_of(body: GeminiGenerateBody) -> string {
  match (body.candidates) {
    let cs: GeminiCandidate[] => match (cs.at(0)) {
      let c: GeminiCandidate => match (c.content) {
        let ct: GeminiContent => match (ct.parts) {
          let ps: GeminiPart[] => match (ps.at(0)) {
            let p: GeminiPart => match (p.text) { let s: string => s, _ => "" },
            _ => "",
          },
          _ => "",
        },
        _ => "",
      },
      _ => "",
    },
    _ => "",
  }
}
