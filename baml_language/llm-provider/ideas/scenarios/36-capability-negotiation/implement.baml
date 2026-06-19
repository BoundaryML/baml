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
//   - a `warnings` channel for silent downgrades,
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
// §2  NET-NEW: a warnings channel for SILENT downgrades.
//     The background: "Silent downgrades are worse than errors." A provider
//     that ignores `seed`, or downscales an image, succeeds with wrong-ish
//     output. The AI SDK returns a `warnings` array from doGenerate.
//
//     The proposal's `call<T>(prompt) -> T` has NO room for a warnings array —
//     the return is just T. So this is genuinely net-new surface: either widen
//     the result, or thread warnings out of band. We choose out-of-band via a
//     `Warned<T>` wrapper a capability-aware codec can return, plus a Warnings
//     sink on ExecutionContext for the plain `.call` path that can't change T.
// ----------------------------------------------------------------------------

class Warning { code: string, message: string }

// Result-with-warnings. A provider that downgrades returns this from a
// dedicated companion; the plain `call<T>` path drops the warnings (the cost
// of keeping `call` returning a bare T — see evaluation (c)).
class Warned<T> { value: T, warnings: Warning[] }

// A provider that wants to report silent downgrades on the PLAIN path threads
// them through here (extends the proposal's ExecutionContext, §7 net-new note).
namespace baml.sys {
  // append a warning to the ambient call's sink; observability reads it later.
  function warn(w: Warning) -> null { $rust_io_function }
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
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
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) {
        // THE RUNTIME 400 — mechanism #1. A structured-output rejection looks
        // like this; the body distinguishes it so a fallback can react.
        let kind = from.json_path("error.code") catch (e) { _ => { "" } };
        if (from.status_code == 400 && kind == "json_schema_unsupported") {
          throw baml.errors.SchemaRejected { message: "openai rejected the schema" };
        }
        throw baml.errors.LlmClient { message: "openai status {{ from.status_code }}" };
      }
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient { $rust_io_function }
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
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
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) {
        let status = from.json_path("error.status") catch (e) { _ => { "" } };
        if (from.status_code == 400 && status == "INVALID_ARGUMENT") {
          // the dialect mismatch surfaces ONLY here — declared `No` would have
          // saved the round-trip, but a stale table makes this the real truth.
          throw baml.errors.SchemaRejected { message: "gemini rejected the schema dialect" };
        }
        throw baml.errors.LlmClient { message: "gemini status {{ from.status_code }}" };
      }
      baml.sap.parse<T>(from.json_path("candidates.0.content.parts.0.text"))
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
//     falls back to a degraded mode (prompt-based JSON), emitting a WARNING for
//     the silent-downgrade record.
//
//     It wraps a single Provider and only forwards `.call`. It is plain and
//     non-generic, exactly the Fallback/Retry template (§6 of the proposal).
// ----------------------------------------------------------------------------

class Negotiated {
  inner: Provider
  // the degraded fallback: a SECOND provider configured for prompt-based JSON
  // (no response_format) — used when structured output is No or a probe fails.
  degraded: Provider

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // Step 1: consult the DECLARATIVE answer, if the provider exposes one.
      // If it doesn't implement Capabilities, we have no static knowledge and
      // must fall straight through to the runtime probe (the honest default).
      match (self.inner) {
        let cap: Capabilities => {
          match (cap.structured_output<T>()) {
            No => {
              // statically unsupported: skip the doomed call entirely, warn,
              // and run the degraded prompt-based path. NO wasted 400.
              baml.sys.warn(Warning {
                code: "structured_output.unsupported",
                message: "provider cannot constrain this schema; using prompt-based JSON",
              });
              return self.degraded.call<T>(prompt);
            },
            Yes => { return self.inner.call<T>(prompt); },  // confident: just send.
            Maybe => {
              // THE GRADIENT: the table can't decide. Probe at runtime, and on
              // a schema rejection (mechanism #1) fall back + warn.
              return self.inner.call<T>(prompt) catch (e) {
                let r: baml.errors.SchemaRejected => {
                  baml.sys.warn(Warning {
                    code: "structured_output.downgraded",
                    message: "schema probe rejected at runtime; fell back to prompt JSON",
                  });
                  self.degraded.call<T>(prompt)
                },
                _ => { throw e; },   // transport/other errors are not a downgrade.
              };
            },
          }
        },
        // no Capabilities surface at all -> pure runtime-400 negotiation.
        _ => {
          return self.inner.call<T>(prompt) catch (e) {
            let r: baml.errors.SchemaRejected => {
              baml.sys.warn(Warning {
                code: "structured_output.downgraded",
                message: "schema rejected at runtime (no static capability); fell back",
              });
              self.degraded.call<T>(prompt)
            },
            _ => { throw e; },
          };
        },
      }
    }
  }

  // Negotiated only forwards `.call`. It deliberately does NOT claim Streaming:
  // its whole job is the structured-output gradient on the one-shot path.
}


// ----------------------------------------------------------------------------
// §7  NET-NEW host primitive surfacing the IMAGE-RESOLUTION gradient as a
//     pre-flight gate + silent-downscale, mirroring §6 for a different axis.
//     Shows the gradient is not specific to schemas — "supported" is graded on
//     resolution too, and the same Yes/Maybe/No lattice + warn applies.
// ----------------------------------------------------------------------------

class ResolutionGate {
  inner: Provider
  max_edge: int          // downscale target when the model can't take full-res

  function guard(self, img: image) -> image throws baml.errors.LlmClient {
    let dims = baml.media.image_dims(img);
    match (self.inner) {
      let cap: Capabilities => {
        match (cap.image_input(dims)) {
          Yes => { img },                       // full resolution accepted.
          No  => {
            throw baml.errors.LlmClient { message: "model rejects images entirely" };
          },
          Maybe => {
            // graded support: downscale and WARN — a silent quality downgrade
            // made explicit so it isn't discovered as a degraded answer later.
            baml.sys.warn(Warning {
              code: "image.downscaled",
              message: "image exceeds known-safe resolution; downscaled to {{ self.max_edge }}px",
            });
            baml.media.downscale(img, self.max_edge)
          },
        }
      },
      _ => { img },   // no capability info -> send as-is, let the 400 decide.
    }
  }
}


// ----------------------------------------------------------------------------
// §8  Errors used above. SchemaRejected is the typed face of the runtime 400
//     for structured output — distinct from a transport LlmClient so a
//     combinator can branch on "the schema was the problem" vs "the network".
// ----------------------------------------------------------------------------

namespace baml.errors {
  // already exists: class LlmClient { message: string }
  class SchemaRejected { message: string }   // net-new: a 400 about the SCHEMA.
}
