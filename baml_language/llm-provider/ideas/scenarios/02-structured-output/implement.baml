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
  // Parse failures here are a call-path concern → the CallError channel.
  function repair_parse<T>(raw: string) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> { $rust_io_function }
}


// ----------------------------------------------------------------------------
// §1  A refusal is NOT malformed output — it is a typed failure.
//     Modeled as a concrete CallError so the parse layer can branch on it
//     BEFORE attempting repair (running repair on a refusal fabricates an
//     object the model never produced — the background's core warning).
// ----------------------------------------------------------------------------

// A refusal: HTTP 200, no usable T, the decline lands in a side field
// (Anthropic stop_reason "refusal" / OpenAI message.refusal / Gemini
// finishReason "SAFETY"). It is neither the answer nor a transport failure —
// a distinct typed CallError the app branches on. (Canonical `Refused` shape.)
class Refused {
  category: string
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError { /* same error can surface mid-stream */ }
}

// An empty/unusable structured payload that is NOT a refusal: a genuine parse
// failure on the call path (truncation, missing slot). Concrete so the app can
// classify it as a parse error rather than transport or decline.
class EmptyStructuredError {
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return true; }
  }
}

// A non-2xx HTTP response from a structured-output provider. Concrete CallError
// so the app can branch on status; classifies 429 as a rate limit.
class StructuredHttpError {
  provider: string
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// ----------------------------------------------------------------------------
// Response navigation over the REAL `baml.json` surface (NOT an invented
// `Response.json_path`). Each provider reads a handful of string fields out of
// its response at four diverging paths; that divergence is pure path *data*.
// We parse the body once (`baml.json.parse`) and walk the resulting `json`
// union (`map<string, json>` arms via `baml.json.field`, `json[]` arms via the
// Array `.at` method). `JsonNav.str(body, ["a", "0", "b"])` returns the string
// at that path, or "" when any segment is missing / non-string. A numeric
// segment indexes the array arm; everything else is a map key. Errors are
// swallowed to "" here so the four `parse<T>` bodies stay on the CallError
// channel (a malformed body becomes an empty slot → EmptyStructuredError).
class JsonNav {
  // Walk one segment deeper into a `json` value: a numeric segment indexes the
  // `json[]` arm, any other segment is a `map<string, json>` key.
  function step(node: json, segment: string) -> json throws never {
    match (node) {
      let arr: json[] => {
        // numeric segment → array index. int.from_string throws on non-numeric,
        // so guard with is_ascii_numeric and fall through to null otherwise.
        if (segment.is_ascii_numeric() && segment != "") {
          let idx = int.from_string(segment) catch (e) { _ => { return null; } };
          match (arr.at(idx)) { null => null, let v: json => v }
        } else { null }
      },
      let obj: map<string, json> => baml.json.field(node, segment),
      _ => null,
    }
  }

  // Resolve a dotted path (as segments) to the `json` value at that leaf, or
  // `null` if any hop is absent.
  function at(body: string, path: string[]) -> json throws never {
    let cur: json = baml.json.parse(body) catch (e) { _ => { return null; } };
    for (let seg in path) { cur = JsonNav.step(cur, seg); }
    cur
  }

  // Resolve a dotted path to the string at that leaf, or "" if any hop is
  // absent or the leaf is not a string.
  function str(body: string, path: string[]) -> string throws never {
    match (JsonNav.at(body, path)) {
      let s: string => s,
      _ => "",
    }
  }

  // Resolve a dotted path to a sub-object and re-serialize it to JSON text (for
  // a provider whose structured answer is a nested JSON object, e.g. an
  // Anthropic tool_use `input`). Returns "" if the leaf is absent.
  function obj_text(body: string, path: string[]) -> string throws never {
    match (JsonNav.at(body, path)) {
      null => "",
      let v: json => baml.json.stringify(v),
    }
  }
}

// Shared decode helper: given the raw structured-answer string AND a possible
// refusal string pulled from the provider's refusal slot, do the right thing
// in the right order. This is library code so all four providers share the
// "check refusal -> repair -> SAP" discipline instead of re-implementing it.
// On the call path → the CallError channel.
class StructuredDecode {
  // refusal: the text found in the provider's refusal channel, or "" if none.
  // raw:     the structured-answer payload (may be near-JSON, may be empty).
  function decode<T>(refusal: string, raw: string) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    if (refusal != "") {
      // refusal arrives in a DIFFERENT place than the answer; never repair it.
      throw Refused { category: "refusal", message: refusal };
    }
    if (raw == "") {
      throw EmptyStructuredError { message: "empty structured response" };
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
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

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw StructuredHttpError { provider: "openai", status: from.status_code }; }
      let body = from.text() catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai read body failed");
      };
      // refusal lands in choices[0].message.refusal — a DIFFERENT slot than content.
      let refusal = JsonNav.str(body, ["choices", "0", "message", "refusal"]);
      let content = JsonNav.str(body, ["choices", "0", "message", "content"]);
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
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

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw StructuredHttpError { provider: "openai", status: from.status_code }; }
      let body = from.text() catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai read body failed");
      };
      // Responses puts refusal in a content PART of type "refusal".
      let refusal = JsonNav.str(body, ["output", "0", "content", "0", "refusal"]);
      let text = JsonNav.str(body, ["output", "0", "content", "0", "text"]);
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
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

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw StructuredHttpError { provider: "anthropic", status: from.status_code }; }
      let body = from.text() catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic read body failed");
      };
      // refusal: stop_reason == "refusal", no tool input. Check it FIRST.
      let stop = JsonNav.str(body, ["stop_reason"]);
      if (stop == "refusal") {
        throw Refused { category: "refusal", message: "model refused (stop_reason refusal)" };
      }
      // the answer is the FIRST tool_use block's `input` — a nested JSON object;
      // re-serialize it to text so repair/SAP can decode it like any other body.
      let input = JsonNav.obj_text(body, ["content", "0", "input"]);
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
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

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini send failed");
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw StructuredHttpError { provider: "gemini", status: from.status_code }; }
      let body = from.text() catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini read body failed");
      };
      // Gemini has no separate refusal slot; a block can carry finishReason
      // SAFETY/RECITATION. Treat those as refusal-shaped.
      let finish = JsonNav.str(body, ["candidates", "0", "finishReason"]);
      if (finish == "SAFETY" || finish == "RECITATION") {
        throw Refused { category: "safety", message: "gemini finishReason " + finish };
      }
      let text = JsonNav.str(body, ["candidates", "0", "content", "parts", "0", "text"]);
      StructuredDecode.decode<T>("", text)
    }
  }
}


// ----------------------------------------------------------------------------
// §6  Combinator: StrictFallback — the standard schema-feature fallback chain.
//     "try strict -> if 400/rejected, retry non-strict -> always repair."
//     The strict/non-strict choice is INSIDE build_request per provider, so
//     this combinator can only retry the whole call. It wraps a Provider and,
//     on a non-refusal CallError (or an UnknownError), retries against a second
//     provider configured for the looser mode. The combinator declares the SAME
//     CallError channel as everything else — no narrow/widen/forward. This is
//     orchestration, so it lives here.
// ----------------------------------------------------------------------------

class StrictFallback {
  // primary: typically a strict-capable provider; loose: the non-strict retry.
  primary: Provider
  loose: Provider

  // StrictFallback forwards the request/response capability, so it implements
  // HttpProvider (call/call_with own this shape now — see ../provider-as-marker.md).
  // The combinator factories (with_retry/fallback_to/cached) are inherited via
  // `requires Provider`; we do NOT redefine them.
  implements HttpProvider {
    type Body = unknown

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // try the primary; a Refused is authoritative and must NOT be retried.
      match (self.primary) {
        let h: HttpProvider => h.call<T>(prompt) catch (e) {
          let r: Refused => { throw r; },   // honest decline; propagate.
          _ => { return self.loose_call<T>(prompt); },  // schema rejected / 400 / transport: fall to loose.
        },
        _ => self.loose_call<T>(prompt),    // primary can't do request/response: go loose.
      }
    }

    // Value + sidecar forwarding (the P1 fix): route the projection to whichever
    // member actually serves the call, so usage/cost is NOT dropped. A Refused on
    // the primary stays authoritative; anything else degrades to the loose member.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.primary) {
        let h: HttpProvider => h.call_with<T, U>(prompt, project) catch (e) {
          let r: Refused => { throw r; },
          _ => { return self.loose_call_with<T, U>(prompt, project); },
        },
        _ => self.loose_call_with<T, U>(prompt, project),
      }
    }
  }

  // Internal routers to the loose member — also a capability match, so a
  // non-http loose member fails honestly rather than fabricating a call.
  function loose_call<T>(self, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    match (self.loose) {
      let h: HttpProvider => h.call<T>(prompt),
      _ => throw baml.errors.Unsupported { message: "StrictFallback.loose is not request/response" },
    }
  }

  function loose_call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    match (self.loose) {
      let h: HttpProvider => h.call_with<T, U>(prompt, project),
      _ => throw baml.errors.Unsupported { message: "StrictFallback.loose is not request/response" },
    }
  }
}
