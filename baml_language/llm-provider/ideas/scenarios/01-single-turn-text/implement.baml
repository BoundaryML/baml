// ============================================================================
// Scenario 01 — Plain text in / text out (the base `Provider.call`)
//   implement.baml — STD-LIB / library-author code.
//
// Builds ONLY on the canonical primitives from ../_conventions.md:
//   interface Provider, interface HttpProvider, interface ChatModelOptions,
//   baml.llm.PromptAst, baml.http.{Request,Response,send}, baml.sap.parse.
//
// What this scenario must make the provider bodies absorb:
//   - Provider.call as the irreducible base.
//   - System-prompt placement diverges 3 ways: an in-array message (OpenAI),
//     a top-level `system` string (Anthropic), a top-level `system_instruction`
//     (Gemini).
//   - Role names diverge: assistant vs "model"; the tool/function role.
//   - Sampling params + max_tokens are PLAIN provider fields. No options blob.
//     Usage is NOT a field — it is a PRODUCT sidecar, projected from the
//     normalized ResponseMeta via the base `call_with` (../value-sidecar-model.md).
//   - A refusal is NOT the answer — it lands in a separate channel
//     (stop_reason / refusal part), so `parse` must look there before SAP.
//   - There is NO shared message type — each provider re-shapes the SAME
//     PromptAst into its own wire envelope inside build_request.
// ============================================================================


// ---------------------------------------------------------------------------
// NET-NEW host surface this scenario needs (flagged in evaluation.md §b).
// PromptAst is opaque (`class PromptAst { _data: $rust_type }`); to re-shape it
// into three different envelopes we need to *read* it. Today nothing exposes
// its structure to BAML, so we add a tiny read-interface over the AST.
// ---------------------------------------------------------------------------

// One normalized turn, as the renderer already groups them internally.
// `role` is the NORMALIZED role: "system" | "user" | "assistant". Each provider
// maps it to its own wire name in build_request.
class PromptTurn {
  role: string
  text: string
}

// Read-side view over the opaque PromptAst. $rust_io_function bodies because
// the AST lives in the host (it is `_data: $rust_type`).
namespace baml.llm {
  // Returns the turns with the system turn(s) STILL in the array, normalized.
  function turns(prompt: baml.llm.PromptAst) -> PromptTurn[] { $rust_io_function }
  // Convenience: the concatenated system text (empty string if none), for the
  // providers that hoist it out of the array.
  function system_text(prompt: baml.llm.PromptAst) -> string { $rust_io_function }
  // The turns WITHOUT any system turn — for the hoisting providers.
  function non_system_turns(prompt: baml.llm.PromptAst) -> PromptTurn[] { $rust_io_function }
}

// JSON assembly uses the REAL stdlib: `baml.json` already gives us the `json`
// value type (`null | bool | int | float | string | json[] | map<string, json>`)
// plus `baml.json.stringify(j) -> string`. We build the request body as a NATIVE
// `map<string, json>` literal (nested arrays are `json[]`, nested objects are
// `map<string, json>` — no escaping by hand) and serialize once at the seam. For
// reading responses we use `baml.json.from_json<T>` into a typed class (preferred)
// or `baml.json.parse` + navigation. No invented `obj`/`arr`/`raw`/`json_path`.


// ---------------------------------------------------------------------------
// Usage = a PRODUCT sidecar, carried by `call_with` + `ResponseMeta`.
//
// `Provider.call<T>` returns T — the *answer*. Token usage is a side-band fact
// about the same call, not part of the answer, so it does not fit `call`'s
// return. The earlier draft bolted on a parallel `Metered.call_metered<T> ->
// Metered_Result<T>` companion, which duplicated send/parse plumbing AND got
// silently dropped when routed through an inherited `.call` (a Fallback that
// forwards `Metered` degrades to zero-usage). The value+sidecar model
// (../value-sidecar-model.md) replaces that bespoke wrapper with the BASE
// method `call_with<T,U>(prompt, project: (ResponseMeta) -> U) -> (T, U)`:
// every combinator forwards it, so usage survives Fallback/Retry/Cache.
//
// `ResponseMeta` is an INTERFACE (the SAME interface/concrete/external-impl/
// `Supported` shape as the error model). Each provider's `meta_of` wraps the
// wire Body into a concrete response type that normalizes it LAZILY — projecting
// `m => m.usage()` never touches logprobs/citations.
// ---------------------------------------------------------------------------

class Usage {
  input_tokens: int
  output_tokens: int
  cache_read_tokens: int     // Anthropic/OpenAI report this; Gemini may be 0
  cache_write_tokens: int
}

// The normalized sidecar interface. Common dimensions live here (everyone
// implements them, `Supported` where a provider CAN'T); genuinely-novel
// dimensions arrive later as external-impl interfaces (../value-sidecar-model.md §7).
// Only `usage` / `finish_reason` are load-bearing for THIS scenario; the rest are
// shown so the three providers demonstrate "can't" (`Unsupported`) vs "empty".
interface ResponseMeta {
  function usage(self)         -> Usage
  function finish_reason(self) -> string
  function logprobs(self)      -> Supported<Logprob[]>     // "can't" is typed, not null
}

// "this provider CAN'T" vs "empty this call" — keeps per-field absence honest
// instead of overloading null (../value-sidecar-model.md §4).
type Supported<T> = T | Unsupported
class Unsupported { reason: string }

class Logprob { token: string, logprob: float }

// ---------------------------------------------------------------------------
// Typed WIRE classes — the shape we read OUT of each provider's JSON response.
// Instead of the invented `Response.json_path("a.b")` (Response has NO such
// method), we decode the body ONCE into a typed class with `baml.json.from_json<T>`
// (the real stdlib coercion) and read named fields. Optional fields cover the
// keys that are absent on some models (e.g. Anthropic cache_* / stop_details).
// ---------------------------------------------------------------------------

class AnthropicUsageWire {
  input_tokens: int
  output_tokens: int
  cache_read_input_tokens: int?
  cache_creation_input_tokens: int?
}
class AnthropicContentBlock { text: string? }
class AnthropicStopDetails { category: string? }
class AnthropicBodyWire {
  stop_reason: string?
  stop_details: AnthropicStopDetails?
  content: AnthropicContentBlock[]?
  usage: AnthropicUsageWire
}

class OpenAiPromptTokensDetails { cached_tokens: int? }
class OpenAiUsageWire {
  prompt_tokens: int
  completion_tokens: int
  prompt_tokens_details: OpenAiPromptTokensDetails?
}
class OpenAiMessageWire { content: string?, refusal: string? }
class OpenAiChoiceWire { message: OpenAiMessageWire, finish_reason: string? }
class OpenAiBodyWire {
  choices: OpenAiChoiceWire[]
  usage: OpenAiUsageWire
}

class GeminiUsageWire {
  promptTokenCount: int
  candidatesTokenCount: int
  cachedContentTokenCount: int?
}
class GeminiPart { text: string? }
class GeminiContentWire { parts: GeminiPart[]? }
class GeminiCandidateWire { content: GeminiContentWire?, finishReason: string? }
class GeminiBodyWire {
  candidates: GeminiCandidateWire[]
  usageMetadata: GeminiUsageWire
}

// Small helper: unwrap an `int?` (absent cache field) to 0.
function int_or_zero(v: int?) -> int {
  match (v) { let n: int => n, _ => 0 }
}

// Concrete provider response types implement ResponseMeta over the raw wire
// Body, normalizing LAZILY. `meta_of` (HttpProvider) wraps the Body into these.
// Each decodes the raw response text into its typed wire class on demand.
class AnthropicResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let b = baml.json.from_json<AnthropicBodyWire>(baml.json.parse(self.raw.text()));
      Usage {
        input_tokens:       b.usage.input_tokens,
        output_tokens:      b.usage.output_tokens,
        cache_read_tokens:  int_or_zero(b.usage.cache_read_input_tokens),
        cache_write_tokens: int_or_zero(b.usage.cache_creation_input_tokens),
      }
    }
    function finish_reason(self) -> string {
      let b = baml.json.from_json<AnthropicBodyWire>(baml.json.parse(self.raw.text()));
      match (b.stop_reason) { let s: string => s, _ => "" }
    }
    function logprobs(self) -> Supported<Logprob[]> { Unsupported { reason: "anthropic: no logprobs" } }
  }
}

class OpenAIResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let b = baml.json.from_json<OpenAiBodyWire>(baml.json.parse(self.raw.text()));
      // OpenAI cache hits live under prompt_tokens_details; absent -> 0.
      let cached = match (b.usage.prompt_tokens_details) {
        let d: OpenAiPromptTokensDetails => int_or_zero(d.cached_tokens),
        _ => 0,
      };
      Usage {
        input_tokens:       b.usage.prompt_tokens,
        output_tokens:      b.usage.completion_tokens,
        cache_read_tokens:  cached,
        cache_write_tokens: 0,
      }
    }
    function finish_reason(self) -> string {
      let b = baml.json.from_json<OpenAiBodyWire>(baml.json.parse(self.raw.text()));
      match (b.choices.at(0)) {
        let c: OpenAiChoiceWire => match (c.finish_reason) { let r: string => r, _ => "" },
        _ => "",
      }
    }
    function logprobs(self) -> Supported<Logprob[]> { Unsupported { reason: "openai chat: logprobs not requested" } }
  }
}

class GeminiResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let b = baml.json.from_json<GeminiBodyWire>(baml.json.parse(self.raw.text()));
      Usage {
        input_tokens:       b.usageMetadata.promptTokenCount,
        output_tokens:      b.usageMetadata.candidatesTokenCount,
        cache_read_tokens:  int_or_zero(b.usageMetadata.cachedContentTokenCount),
        cache_write_tokens: 0,
      }
    }
    function finish_reason(self) -> string {
      let b = baml.json.from_json<GeminiBodyWire>(baml.json.parse(self.raw.text()));
      match (b.candidates.at(0)) {
        let c: GeminiCandidateWire => match (c.finishReason) { let r: string => r, _ => "" },
        _ => "",
      }
    }
    function logprobs(self) -> Supported<Logprob[]> { Unsupported { reason: "gemini: no logprobs" } }
  }
}


// ---------------------------------------------------------------------------
// NET-NEW failure shape: a refusal is a typed channel, not a thrown error and
// not the answer. The model returned 200 OK, produced no usable T, and told us
// why in a separate field. We surface it on the `baml.errors.CallError` channel
// as a CONCRETE error class so callers can branch on it (vs a transport
// failure). It implements CallError directly (not a transport/rate-limit/parse
// failure — all three classifiers are false), so it is legal on the call path.
// ---------------------------------------------------------------------------

class Refused {
  // Provider-specific category string, normalized best-effort:
  //   Anthropic stop_reason "refusal" (+ stop_details.category on newer models),
  //   OpenAI choices[0].message.refusal (Chat) / a refusal content part (Responses).
  category: string
  message: string

  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}


// ---------------------------------------------------------------------------
// NET-NEW domain errors: an HTTP non-2xx is a DELIBERATE throw the provider
// raises on purpose (the model said no at the transport level). Per-provider
// concrete classes implementing `baml.errors.CallError`, with a `status` so the
// app can branch. 429 is the one classification that matters (is_rate_limit);
// everything else is a generic call failure. We carry `status` so the consumer
// can refine further without us minting a class per HTTP code.
// ---------------------------------------------------------------------------

class AnthropicHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class OpenAiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class GeminiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}


// ===========================================================================
// PROVIDER 1 — Anthropic (Messages API)
//   - system prompt is HOISTED to a top-level `system` string.
//   - roles: user / assistant only; first turn must be user.
//   - max_tokens is REQUIRED.
//   - temperature range 0..1; exposes top_k; NO frequency/presence penalties.
//   - refusal: stop_reason == "refusal" (separate channel, empty content).
//   - usage: usage.input_tokens / output_tokens / cache_*_input_tokens.
// ===========================================================================

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int                 // bespoke + REQUIRED on this provider
  temperature: float?             // 0..1 on Anthropic
  top_p: float?
  top_k: int?                     // Anthropic exposes this; OpenAI Chat does not
  stop_sequences: string[]?       // note: stop_sequences, not `stop`

  // Standard params satisfy the shared read-interface (no options blob).
  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Re-shape the SAME PromptAst into Anthropic's envelope.
      // (1) system hoisted out of the message array:
      let system = baml.llm.system_text(prompt);

      // (2) user/assistant messages — Anthropic's role names happen to match
      //     our normalized names, but we still map explicitly so the divergence
      //     is visible at the seam. `msgs` is a native `json[]` of message objects.
      let msgs: json[] = [];
      for (let turn in baml.llm.non_system_turns(prompt)) {
        let role = match (turn.role) {
          "assistant" => "assistant",
          _           => "user",            // Anthropic has no other inbound role
        };
        // Native `map<string, json>` literal — no hand-rolled JSON.
        let msg: map<string, json> = { "role": role, "content": turn.text };
        msgs.push(msg);
      }

      // (3) body fields — bespoke provider fields go straight in, no blob.
      let fields: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,         // REQUIRED here
        "messages": msgs,
      };
      if (system != "") { fields.set("system", system); }
      match (self.temperature) { let t: float => { fields.set("temperature", t); }, _ => {} }
      match (self.top_p)       { let p: float => { fields.set("top_p", p); }, _ => {} }
      match (self.top_k)       { let k: int   => { fields.set("top_k", k); }, _ => {} }
      match (self.stop_sequences) { let s: string[] => { fields.set("stop_sequences", s); }, _ => {} }

      let key = match (self.api_key) { let k: string => k, _ => env.ANTHROPIC_API_KEY };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "content-type": "application/json",
          "x-api-key": key,
          "anthropic-version": "2023-06-01",
        },
        body: baml.json.stringify(fields),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Foreign transport errors (baml.errors.Io / Timeout) are not classified
      // here — normalize them into UnknownError with context.
      baml.http.send(request) catch (e) {
        _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http failed"); }
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw AnthropicHttpError { status: from.status_code }; }
      // Decode the body ONCE into the typed wire class (no invented json_path).
      let b = baml.json.from_json<AnthropicBodyWire>(baml.json.parse(from.text()));
      // REFUSAL CHANNEL FIRST — a refusal is not the answer. The text you want
      // is empty and the decline lands in stop_reason, never in content.
      let stop = match (b.stop_reason) { let s: string => s, _ => "" };
      if (stop == "refusal") {
        let category = match (b.stop_details) {
          let d: AnthropicStopDetails => match (d.category) { let c: string => c, _ => "" },
          _ => "",                                          // "" on older models
        };
        throw Refused { category: category, message: "anthropic refused" };
      }
      // Only now extract the answer text and SAP-parse it into T.
      // content is a list of typed blocks; content[0].text is the first text.
      let text = match (b.content) {
        let blocks: AnthropicContentBlock[] => match (blocks.at(0)) {
          let blk: AnthropicContentBlock => match (blk.text) { let t: string => t, _ => "" },
          _ => "",
        },
        _ => "",
      };
      baml.sap.parse<T>(text)
    }

    // The sidecar half: wrap the SAME Body into a concrete ResponseMeta that
    // normalizes usage/finish_reason LAZILY. `call_with` (inherited from
    // HttpProvider) pairs `parse<T>` with `project(meta_of(body))` in ONE codec
    // pass — no duplicated send, and combinators forward it (no zero-usage drop).
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      AnthropicResponse { raw: from }
    }
  }
}


// ===========================================================================
// PROVIDER 2 — OpenAI (Chat Completions)
//   - system prompt is an IN-ARRAY message with role "system".
//   - roles: system / user / assistant / tool.
//   - max_tokens optional.
//   - temperature range 0..2; exposes frequency_penalty/presence_penalty;
//     NO top_k on Chat Completions.
//   - refusal: choices[0].message.refusal (separate field), text may be empty.
//   - usage: usage.prompt_tokens / completion_tokens.
// ===========================================================================

class OpenAI {
  model: string
  api_key: string?
  max_tokens: int?
  temperature: float?             // 0..2 on OpenAI
  top_p: float?
  frequency_penalty: float?       // OpenAI has these; Anthropic does not
  presence_penalty: float?
  stop: string[]?                 // note: `stop`, not stop_sequences

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Re-shape the SAME PromptAst into OpenAI's FLAT message array, with the
      // system prompt KEPT IN the array as a role:"system" message. This is the
      // opposite placement decision from Anthropic — same PromptAst, different
      // envelope, and there is no shared message type to lean on.
      let msgs: json[] = [];
      for (let turn in baml.llm.turns(prompt)) {       // includes the system turn
        // OpenAI role names match our normalized ones here, but we map
        // explicitly so the (lack of) divergence is auditable.
        let role = match (turn.role) {
          "system"    => "system",
          "assistant" => "assistant",
          _           => "user",
        };
        let msg: map<string, json> = { "role": role, "content": turn.text };
        msgs.push(msg);
      }

      let fields: map<string, json> = {
        "model": self.model,
        "messages": msgs,
      };
      match (self.max_tokens)        { let m: int   => { fields.set("max_tokens", m); }, _ => {} }
      match (self.temperature)       { let t: float => { fields.set("temperature", t); }, _ => {} }
      match (self.top_p)             { let p: float => { fields.set("top_p", p); }, _ => {} }
      match (self.frequency_penalty) { let f: float => { fields.set("frequency_penalty", f); }, _ => {} }
      match (self.presence_penalty)  { let p: float => { fields.set("presence_penalty", p); }, _ => {} }
      match (self.stop)              { let s: string[] => { fields.set("stop", s); }, _ => {} }

      let key = match (self.api_key) { let k: string => k, _ => env.OPENAI_API_KEY };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: {
          "content-type": "application/json",
          "authorization": "Bearer " + key,
        },
        body: baml.json.stringify(fields),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http failed"); }
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw OpenAiHttpError { status: from.status_code }; }
      // Decode the body ONCE into the typed wire class (no invented json_path).
      let b = baml.json.from_json<OpenAiBodyWire>(baml.json.parse(from.text()));
      let msg = match (b.choices.at(0)) {
        let c: OpenAiChoiceWire => c.message,
        _ => OpenAiMessageWire { content: null, refusal: null },
      };
      // REFUSAL CHANNEL FIRST — OpenAI lands the decline in a sibling field
      // `refusal`, and the content you'd read is empty. Blindly reading the
      // text would surface "" and misclassify a refusal as a malformed answer.
      match (msg.refusal) {
        let r: string => { throw Refused { category: "policy", message: r }; },
        _ => {},
      }
      let content = match (msg.content) { let t: string => t, _ => "" };
      baml.sap.parse<T>(content)
    }

    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAIResponse { raw: from }
    }
  }
}


// ===========================================================================
// PROVIDER 3 — Google Gemini (generateContent)
//   - system prompt is a TOP-LEVEL `system_instruction` (a different hoist
//     than Anthropic's bare string — it's an object with `parts`).
//   - container is `contents[]`, not `messages[]`.
//   - assistant role is named "model"; the tool role is "function".
//   - usage: usageMetadata.{promptTokenCount, candidatesTokenCount}.
//   - response text path: candidates[0].content.parts[0].text.
// ===========================================================================

class Gemini {
  model: string
  api_key: string?
  max_output_tokens: int?         // Gemini's name for max_tokens
  temperature: float?             // 0..2 on Gemini
  top_p: float?
  top_k: int?                     // Gemini exposes top_k
  stop_sequences: string[]?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Re-shape the SAME PromptAst a THIRD way: contents[] with role "model"
      // for assistant turns, system hoisted into a system_instruction OBJECT.
      let contents: json[] = [];
      for (let turn in baml.llm.non_system_turns(prompt)) {
        let role = match (turn.role) {
          "assistant" => "model",          // <-- the assistant/"model" divergence
          _           => "user",
        };
        let part: map<string, json> = { "text": turn.text };
        let entry: map<string, json> = { "role": role, "parts": [part] };
        contents.push(entry);
      }

      let gen: map<string, json> = {};
      match (self.max_output_tokens) { let m: int   => { gen.set("maxOutputTokens", m); }, _ => {} }
      match (self.temperature)       { let t: float => { gen.set("temperature", t); }, _ => {} }
      match (self.top_p)             { let p: float => { gen.set("topP", p); }, _ => {} }
      match (self.top_k)             { let k: int   => { gen.set("topK", k); }, _ => {} }
      match (self.stop_sequences)    { let s: string[] => { gen.set("stopSequences", s); }, _ => {} }

      let fields: map<string, json> = {
        "contents": contents,
        "generationConfig": gen,
      };
      let system = baml.llm.system_text(prompt);
      if (system != "") {
        // system_instruction is an OBJECT with parts — a third placement shape.
        let sys_part: map<string, json> = { "text": system };
        let sys_instr: map<string, json> = { "parts": [sys_part] };
        fields.set("system_instruction", sys_instr);
      }

      let key = match (self.api_key) { let k: string => k, _ => env.GEMINI_API_KEY };
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent?key=" + key,
        headers: { "content-type": "application/json" },
        body: baml.json.stringify(fields),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini http failed"); }
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw GeminiHttpError { status: from.status_code }; }
      // Decode the body ONCE into the typed wire class (no invented json_path).
      let b = baml.json.from_json<GeminiBodyWire>(baml.json.parse(from.text()));
      let cand = match (b.candidates.at(0)) {
        let c: GeminiCandidateWire => c,
        _ => GeminiCandidateWire { content: null, finishReason: null },
      };
      // Gemini's refusal channel is finishReason == "SAFETY" with no parts.
      let finish = match (cand.finishReason) { let f: string => f, _ => "" };
      if (finish == "SAFETY" || finish == "PROHIBITED_CONTENT") {
        throw Refused { category: finish, message: "gemini blocked" };
      }
      let text = match (cand.content) {
        let co: GeminiContentWire => match (co.parts) {
          let ps: GeminiPart[] => match (ps.at(0)) {
            let p: GeminiPart => match (p.text) { let t: string => t, _ => "" },
            _ => "",
          },
          _ => "",
        },
        _ => "",
      };
      baml.sap.parse<T>(text)
    }

    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      GeminiResponse { raw: from }
    }
  }
}
