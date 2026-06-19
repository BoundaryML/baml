// ============================================================================
// implement.baml  —  Structured / typed output: one type, four wire encodings
// ----------------------------------------------------------------------------
// STD-LIB / library-author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, baml.http, baml.sap, baml.llm).
//
// The teaching point is the path of the T generic:
//
//     function Extract(...) -> Resume
//                              ^^^^^^  one type T
//
//   T  ──►  build_request<T>   (decides WHERE the schema goes on the wire)
//   T  ──►  parse<T>           (decides HOW the bytes become a T)
//
// Four providers below put the SAME T on the wire four different ways, and
// parse it back four different ways, behind the single HttpProvider seam.
// ============================================================================


// ----------------------------------------------------------------------------
// §0  Net-new host primitives this scenario needs (genuine host boundaries).
//     These are the only additions; every interface above them is canonical.
// ----------------------------------------------------------------------------

// Schema lowering. A first-class `type` value (T, captured via reflect) has to
// become provider-specific schema JSON. The dialects diverge enough that this
// is a host concern, not pure BAML string-building.
namespace baml.schema {
  // Standard JSON Schema. `strict` toggles `additionalProperties:false` +
  // `required: <all>` (OpenAI/Responses strict mode). Returns the schema text.
  function json_schema(t: type, strict: bool) -> string { $rust_io_function }

  // Gemini's OpenAPI-3.0 subset: UPPERCASE type names, no $ref, no
  // additionalProperties. A *different dialect*, so a separate lowering.
  function gemini_schema(t: type) -> string { $rust_io_function }

  // Does T use a feature strict mode rejects? ($ref cycles, oneOf/anyOf in
  // disallowed positions, patternProperties, if/then/else, pattern/format).
  // Drives strict-vs-nonstrict negotiation BEFORE the call (see §1).
  function strict_supports(t: type) -> bool { $rust_io_function }
}

// Repair / coercion. The universal safety net under every wire format:
// strip code fences, close brackets, coerce "3"->3, fill defaults, re-validate.
// Distinct from SAP: SAP aligns a (possibly-prose) string to a schema; repair
// fixes near-JSON. We run repair, then SAP, as the parse-layer fallback.
namespace baml.sap {
  // repair the raw string into canonical JSON, then SAP-parse into T.
  function repair_parse<T>(raw: string) -> T throws baml.errors.LlmClient { $rust_io_function }
}


// ----------------------------------------------------------------------------
// §1  A refusal is NOT malformed output — it is a typed failure.
//     Modeled as a member of the Error channel so the parse layer can branch
//     on it BEFORE attempting repair (running repair on a refusal fabricates
//     an object the model never produced — the background's core warning).
// ----------------------------------------------------------------------------

namespace baml.errors {
  // already exists: class LlmClient { message: string }
  // net-new: a distinct refusal so callers can `catch` it specifically.
  class Refusal { message: string }
}

// Shared decode helper: given the raw structured-answer string AND a possible
// refusal string pulled from the provider's refusal slot, do the right thing
// in the right order. This is library code so all four providers share the
// "check refusal -> repair -> SAP" discipline instead of re-implementing it.
class StructuredDecode {
  // refusal: the text found in the provider's refusal channel, or "" if none.
  // raw:     the structured-answer payload (may be near-JSON, may be empty).
  function decode<T>(refusal: string, raw: string) -> T throws baml.errors.LlmClient {
    if (refusal != "") {
      // refusal arrives in a DIFFERENT place than the answer; never repair it.
      throw baml.errors.Refusal { message: refusal };
    }
    if (raw == "") {
      throw baml.errors.LlmClient { message: "empty structured response" };
    }
    // coercion/repair is mandatory even under "guaranteed" modes (truncation,
    // rejected schema). repair_parse strips fences/closes brackets, then SAP.
    baml.sap.repair_parse<T>(raw)
  }
}


// ----------------------------------------------------------------------------
// §2  Encoding #1 — OpenAI Chat Completions: response_format + strict toggle.
//     T flows into build_request (schema -> response_format.json_schema) and
//     into parse (content string -> repair/SAP, refusal slot checked first).
// ----------------------------------------------------------------------------

class OpenAIChat {
  model: string
  api_key: string?
  max_tokens: int?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // strict-vs-nonstrict negotiation happens HERE, before the call.
      let strict = baml.schema.strict_supports(reflect.type_of<T>());
      let schema = baml.schema.json_schema(reflect.type_of<T>(), strict);
      let rf = #"
        { "type": "json_schema",
          "json_schema": { "name": "Output", "schema": {{ schema }}, "strict": {{ strict }} } }
      "#;
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
      if (!from.ok()) { throw baml.errors.LlmClient { message: "openai status {{ from.status_code }}" }; }
      // refusal lands in choices[0].message.refusal — a DIFFERENT slot than content.
      let refusal = from.json_path("choices.0.message.refusal") catch (e) { _ => { "" } };
      let content = from.json_path("choices.0.message.content") catch (e) { _ => { "" } };
      StructuredDecode.decode<T>(refusal, content)
    }
  }

  function from_model(model: string) -> OpenAIChat {
    OpenAIChat { model: model, api_key: env.OPENAI_API_KEY, max_tokens: null }
  }
}


// ----------------------------------------------------------------------------
// §3  Encoding #2 — OpenAI Responses API: SAME schema, nested under text.format
//     instead of response_format. The ONLY divergence from OpenAIChat is the
//     envelope + where refusal/content live. Proves the seam absorbs nesting.
// ----------------------------------------------------------------------------

class OpenAIResponses {
  model: string
  api_key: string?
  max_tokens: int?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      let strict = baml.schema.strict_supports(reflect.type_of<T>());
      let schema = baml.schema.json_schema(reflect.type_of<T>(), strict);
      // same payload, different nesting: text.format instead of response_format.
      let tf = #"
        { "format": { "type": "json_schema", "name": "Output",
                      "schema": {{ schema }}, "strict": {{ strict }} } }
      "#;
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/responses",
        headers: { "authorization": "Bearer {{ self.api_key }}", "content-type": "application/json" },
        body: baml.llm.openai_responses_body(prompt, self.model, self.max_tokens, tf),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "openai status {{ from.status_code }}" }; }
      // Responses puts refusal in a content PART of type "refusal".
      let refusal = from.json_path("output.0.content.refusal.0.refusal") catch (e) { _ => { "" } };
      let text = from.json_path("output.0.content.text.0.text") catch (e) { _ => { "" } };
      StructuredDecode.decode<T>(refusal, text)
    }
  }
}


// ----------------------------------------------------------------------------
// §4  Encoding #3 — Anthropic: NO response_format. Forced-tool injection.
//     A synthetic tool whose input_schema IS T; tool_choice forces the call;
//     the structured answer is the tool_use.input. Refusal = stop_reason
//     "refusal" + no parseable tool input. This is the one with side effects.
// ----------------------------------------------------------------------------

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Anthropic accepts JSON-Schema as a tool input_schema; no strict toggle.
      // We always lower non-strict (the tool input is not constrained-decoded),
      // and lean on parse-layer repair for validity.
      let schema = baml.schema.json_schema(reflect.type_of<T>(), false);
      // synthetic return tool: input_schema == the desired type.
      let tools = #"[ { "name": "record_output",
                        "description": "Return the structured result.",
                        "input_schema": {{ schema }} } ]"#;
      let tool_choice = #"{ "type": "tool", "name": "record_output" }"#;
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": "{{ self.api_key }}",
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: baml.llm.anthropic_body(prompt, self.model, self.max_tokens, tools, tool_choice),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "anthropic status {{ from.status_code }}" }; }
      // refusal: stop_reason == "refusal", no tool input. Check it FIRST.
      let stop = from.json_path("stop_reason") catch (e) { _ => { "" } };
      if (stop == "refusal") {
        throw baml.errors.Refusal { message: "model refused (stop_reason refusal)" };
      }
      // the answer is the FIRST tool_use block's `input`, already JSON.
      let input = from.json_path("content.tool_use.0.input") catch (e) { _ => { "" } };
      // no refusal slot here, so pass "" and let decode repair/SAP the input.
      StructuredDecode.decode<T>("", input)
    }
  }

  function from_model(model: string) -> Anthropic {
    Anthropic { model: model, api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
  }
}


// ----------------------------------------------------------------------------
// §5  Encoding #4 — Gemini: responseSchema + responseMimeType on
//     generationConfig, in Gemini's OWN dialect (UPPERCASE, OpenAPI subset).
//     Always constrained; no strict/non-strict toggle. parse reads resp.text.
// ----------------------------------------------------------------------------

class Gemini {
  model: string
  api_key: string?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // DIFFERENT DIALECT: gemini_schema, not json_schema. No strict toggle.
      let schema = baml.schema.gemini_schema(reflect.type_of<T>());
      let gen_cfg = #"
        { "responseMimeType": "application/json", "responseSchema": {{ schema }} }
      "#;
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
      if (!from.ok()) { throw baml.errors.LlmClient { message: "gemini status {{ from.status_code }}" }; }
      // Gemini has no separate refusal slot; a block can carry finishReason
      // SAFETY/RECITATION. Treat those as refusal-shaped.
      let finish = from.json_path("candidates.0.finishReason") catch (e) { _ => { "STOP" } };
      if (finish == "SAFETY" || finish == "RECITATION") {
        throw baml.errors.Refusal { message: "gemini finishReason {{ finish }}" };
      }
      let text = from.json_path("candidates.0.content.parts.0.text") catch (e) { _ => { "" } };
      StructuredDecode.decode<T>("", text)
    }
  }
}


// ----------------------------------------------------------------------------
// §6  Combinator: StrictFallback — the standard schema-feature fallback chain.
//     "try strict -> if 400/rejected, retry non-strict -> always repair."
//     The strict/non-strict choice is INSIDE build_request per provider, so
//     this combinator can only retry the whole call. It wraps a Provider and,
//     on an LlmClient error (not a Refusal), retries against a second provider
//     configured for the looser mode. This is orchestration, so it lives here.
// ----------------------------------------------------------------------------

class StrictFallback {
  // primary: typically a strict-capable provider; loose: the non-strict retry.
  primary: Provider
  loose: Provider

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // try the primary; a Refusal is authoritative and must NOT be retried.
      self.primary.call<T>(prompt) catch (e) {
        let r: baml.errors.Refusal => { throw r; },   // honest decline; propagate.
        _ => {
          // schema rejected / 400 / transport: fall through to the looser mode.
          self.loose.call<T>(prompt)
        },
      }
    }
  }
}
