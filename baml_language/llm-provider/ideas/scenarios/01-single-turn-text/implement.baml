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
//   - Sampling params + max_tokens + usage are PLAIN provider fields. No
//     options blob.
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

// JSON assembly at the host boundary. A pure-BAML provider could string-concat,
// but real providers build a structured body; this keeps build_request honest
// without making us hand-roll JSON escaping in BAML.
namespace baml.json {
  function obj(fields: map<string, unknown>) -> string { $rust_io_function }
  function arr(items: unknown[]) -> string { $rust_io_function }
  // Raw, already-serialized fragment so we can nest obj/arr results.
  function raw(serialized: string) -> unknown { $rust_io_function }
}


// ---------------------------------------------------------------------------
// NET-NEW capability: Usage reporting.
//
// `Provider.call<T>` returns T — the *answer*. Token usage is a side-band fact
// about the call, not part of the answer, so it does not fit `call`'s return.
// We model it as a capability interface (requires Provider) so a provider that
// can report usage opts in, and a harness/combinator can `match` for it.
// This is the same shape as Streaming/Inspectable: a refinement on top of call.
// ---------------------------------------------------------------------------

class Usage {
  input_tokens: int
  output_tokens: int
  cache_read_tokens: int     // Anthropic/OpenAI report this; Gemini may be 0
  cache_write_tokens: int
}

// A call that ALSO hands back usage. `call` throws away usage; this doesn't.
interface Metered requires Provider {
  function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered_Result<T> throws Error
}

class Metered_Result<T> {
  value: T
  usage: Usage
}


// ---------------------------------------------------------------------------
// NET-NEW failure shape: a refusal is a typed channel, not a thrown error and
// not the answer. The model returned 200 OK, produced no usable T, and told us
// why in a separate field. We surface it on the typed `throws` channel as a
// distinct error class so callers can branch on it (vs a transport failure).
// ---------------------------------------------------------------------------

class Refused {
  // Provider-specific category string, normalized best-effort:
  //   Anthropic stop_reason "refusal" (+ stop_details.category on newer models),
  //   OpenAI choices[0].message.refusal (Chat) / a refusal content part (Responses).
  category: string
  message: string
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Re-shape the SAME PromptAst into Anthropic's envelope.
      // (1) system hoisted out of the message array:
      let system = baml.llm.system_text(prompt);

      // (2) user/assistant messages — Anthropic's role names happen to match
      //     our normalized names, but we still map explicitly so the divergence
      //     is visible at the seam.
      let msgs: unknown[] = [];
      for (let turn in baml.llm.non_system_turns(prompt)) {
        let role = match (turn.role) {
          "assistant" => "assistant",
          _           => "user",            // Anthropic has no other inbound role
        };
        msgs = msgs.append(baml.json.raw(baml.json.obj({
          "role": role,
          "content": turn.text,
        })));
      }

      // (3) body fields — bespoke provider fields go straight in, no blob.
      let fields: map<string, unknown> = {
        "model": self.model,
        "max_tokens": self.max_tokens,         // REQUIRED here
        "messages": baml.json.raw(baml.json.arr(msgs)),
      };
      if (system != "") { fields = fields.insert("system", system); }
      match (self.temperature) { let t: float => { fields = fields.insert("temperature", t); }, _ => {} }
      match (self.top_p)       { let p: float => { fields = fields.insert("top_p", p); }, _ => {} }
      match (self.top_k)       { let k: int   => { fields = fields.insert("top_k", k); }, _ => {} }
      match (self.stop_sequences) { let s: string[] => { fields = fields.insert("stop_sequences", baml.json.raw(baml.json.arr(s))); }, _ => {} }

      let key = match (self.api_key) { let k: string => k, _ => env.ANTHROPIC_API_KEY };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "content-type": "application/json",
          "x-api-key": key,
          "anthropic-version": "2023-06-01",
        },
        body: baml.json.obj(fields),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "anthropic http failed" }; }
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "anthropic http " }; }
      // REFUSAL CHANNEL FIRST — a refusal is not the answer. The text you want
      // is empty and the decline lands in stop_reason, never in content.
      let stop = from.json_path("stop_reason");
      if (stop == "refusal") {
        throw Refused {
          category: from.json_path("stop_details.category"),  // "" on older models
          message: "anthropic refused",
        };
      }
      // Only now extract the answer text and SAP-parse it into T.
      // content is a list of typed blocks; content[0].text is the first text.
      baml.sap.parse<T>(from.json_path("content.0.text"))
    }
  }

  // Usage capability: same wire, but lift the usage object too.
  implements Metered {
    function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered_Result<T> throws baml.errors.LlmClient {
      let resp = self.send(self.build_request<T>(prompt));
      let value: T = self.parse<T>(resp);
      Metered_Result<T> {
        value: value,
        usage: Usage {
          input_tokens:       baml.sap.parse<int>(resp.json_path("usage.input_tokens")),
          output_tokens:      baml.sap.parse<int>(resp.json_path("usage.output_tokens")),
          cache_read_tokens:  baml.sap.parse<int>(resp.json_path("usage.cache_read_input_tokens")),
          cache_write_tokens: baml.sap.parse<int>(resp.json_path("usage.cache_creation_input_tokens")),
        },
      }
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Re-shape the SAME PromptAst into OpenAI's FLAT message array, with the
      // system prompt KEPT IN the array as a role:"system" message. This is the
      // opposite placement decision from Anthropic — same PromptAst, different
      // envelope, and there is no shared message type to lean on.
      let msgs: unknown[] = [];
      for (let turn in baml.llm.turns(prompt)) {       // includes the system turn
        // OpenAI role names match our normalized ones here, but we map
        // explicitly so the (lack of) divergence is auditable.
        let role = match (turn.role) {
          "system"    => "system",
          "assistant" => "assistant",
          _           => "user",
        };
        msgs = msgs.append(baml.json.raw(baml.json.obj({
          "role": role,
          "content": turn.text,
        })));
      }

      let fields: map<string, unknown> = {
        "model": self.model,
        "messages": baml.json.raw(baml.json.arr(msgs)),
      };
      match (self.max_tokens)        { let m: int   => { fields = fields.insert("max_tokens", m); }, _ => {} }
      match (self.temperature)       { let t: float => { fields = fields.insert("temperature", t); }, _ => {} }
      match (self.top_p)             { let p: float => { fields = fields.insert("top_p", p); }, _ => {} }
      match (self.frequency_penalty) { let f: float => { fields = fields.insert("frequency_penalty", f); }, _ => {} }
      match (self.presence_penalty)  { let p: float => { fields = fields.insert("presence_penalty", p); }, _ => {} }
      match (self.stop)              { let s: string[] => { fields = fields.insert("stop", baml.json.raw(baml.json.arr(s))); }, _ => {} }

      let key = match (self.api_key) { let k: string => k, _ => env.OPENAI_API_KEY };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: {
          "content-type": "application/json",
          "authorization": "Bearer " + key,
        },
        body: baml.json.obj(fields),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "openai http failed" }; }
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "openai http " }; }
      // REFUSAL CHANNEL FIRST — OpenAI lands the decline in a sibling field
      // `refusal`, and the content you'd read is empty. Blindly reading the
      // text would surface "" and misclassify a refusal as a malformed answer.
      let refusal = from.json_path("choices.0.message.refusal");
      if (refusal != "" && refusal != "null") {
        throw Refused { category: "policy", message: refusal };
      }
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
    }
  }

  implements Metered {
    function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered_Result<T> throws baml.errors.LlmClient {
      let resp = self.send(self.build_request<T>(prompt));
      let value: T = self.parse<T>(resp);
      Metered_Result<T> {
        value: value,
        usage: Usage {
          input_tokens:       baml.sap.parse<int>(resp.json_path("usage.prompt_tokens")),
          output_tokens:      baml.sap.parse<int>(resp.json_path("usage.completion_tokens")),
          // OpenAI cache hits live under prompt_tokens_details; absent -> 0.
          cache_read_tokens:  baml.sap.parse<int>(resp.json_path("usage.prompt_tokens_details.cached_tokens")),
          cache_write_tokens: 0,
        },
      }
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Re-shape the SAME PromptAst a THIRD way: contents[] with role "model"
      // for assistant turns, system hoisted into a system_instruction OBJECT.
      let contents: unknown[] = [];
      for (let turn in baml.llm.non_system_turns(prompt)) {
        let role = match (turn.role) {
          "assistant" => "model",          // <-- the assistant/"model" divergence
          _           => "user",
        };
        contents = contents.append(baml.json.raw(baml.json.obj({
          "role": role,
          "parts": baml.json.raw(baml.json.arr([
            baml.json.raw(baml.json.obj({ "text": turn.text })),
          ])),
        })));
      }

      let gen: map<string, unknown> = {};
      match (self.max_output_tokens) { let m: int   => { gen = gen.insert("maxOutputTokens", m); }, _ => {} }
      match (self.temperature)       { let t: float => { gen = gen.insert("temperature", t); }, _ => {} }
      match (self.top_p)             { let p: float => { gen = gen.insert("topP", p); }, _ => {} }
      match (self.top_k)             { let k: int   => { gen = gen.insert("topK", k); }, _ => {} }
      match (self.stop_sequences)    { let s: string[] => { gen = gen.insert("stopSequences", baml.json.raw(baml.json.arr(s))); }, _ => {} }

      let fields: map<string, unknown> = {
        "contents": baml.json.raw(baml.json.arr(contents)),
        "generationConfig": baml.json.raw(baml.json.obj(gen)),
      };
      let system = baml.llm.system_text(prompt);
      if (system != "") {
        // system_instruction is an OBJECT with parts — a third placement shape.
        fields = fields.insert("system_instruction", baml.json.raw(baml.json.obj({
          "parts": baml.json.raw(baml.json.arr([
            baml.json.raw(baml.json.obj({ "text": system })),
          ])),
        })));
      }

      let key = match (self.api_key) { let k: string => k, _ => env.GEMINI_API_KEY };
      baml.http.Request {
        method: "POST",
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + self.model + ":generateContent?key=" + key,
        headers: { "content-type": "application/json" },
        body: baml.json.obj(fields),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "gemini http failed" }; }
      }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      if (!from.ok()) { throw baml.errors.LlmClient { message: "gemini http " }; }
      // Gemini's refusal channel is finishReason == "SAFETY" with no parts.
      let finish = from.json_path("candidates.0.finishReason");
      if (finish == "SAFETY" || finish == "PROHIBITED_CONTENT") {
        throw Refused { category: finish, message: "gemini blocked" };
      }
      baml.sap.parse<T>(from.json_path("candidates.0.content.parts.0.text"))
    }
  }

  implements Metered {
    function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered_Result<T> throws baml.errors.LlmClient {
      let resp = self.send(self.build_request<T>(prompt));
      let value: T = self.parse<T>(resp);
      Metered_Result<T> {
        value: value,
        usage: Usage {
          input_tokens:       baml.sap.parse<int>(resp.json_path("usageMetadata.promptTokenCount")),
          output_tokens:      baml.sap.parse<int>(resp.json_path("usageMetadata.candidatesTokenCount")),
          cache_read_tokens:  baml.sap.parse<int>(resp.json_path("usageMetadata.cachedContentTokenCount")),
          cache_write_tokens: 0,
        },
      }
    }
  }
}
