// ============================================================================
// implement.baml — Constrained decoding beyond JSON Schema
// Library / std-lib-author code.
//
// The breadth this scenario exercises:
//   - Hosted providers (OpenAI/Anthropic/Gemini) can only guarantee *JSON that
//     validates a schema*. They go through the ordinary HttpProvider codec and SAP.
//   - Open-weight backends you run yourself (vLLM, llama.cpp, Outlines) expose
//     token-level constrained decoding: guided_choice / guided_regex /
//     guided_grammar. These can guarantee NON-JSON shapes (a bare enum token, a
//     phone-number string, a SQL statement) that no JSON schema can express.
//
// So "what can be guaranteed" and "where it runs" are properties of the provider
// CLASS, not of the call site. We model that with a new capability interface,
// `Constrained`, that `requires Provider`. Hosted providers simply do not
// implement it; self-hosted ones do. Capability negotiation (§5) then becomes a
// runtime `match` exactly like Streaming/Realtime.
// ============================================================================

// ---------------------------------------------------------------------------
// 0. The constraint vocabulary (NET-NEW)
//
// A `Constraint` is the shape we want the decoder to guarantee. It is a closed
// union over the three token-level primitives every constrained-decoding stack
// exposes, plus the JSON-schema case so that one type can describe BOTH tiers.
// `T` (the function's output type) is still threaded for parsing; `Constraint`
// only describes the wire-level decode mask.
// ---------------------------------------------------------------------------

// One of a fixed set of literal strings. Output is guaranteed to be exactly one
// of `options` — typically decoded back into a `T` that is an enum.
class Choice  { options: string[] }

// The output is guaranteed to match this regular expression (RE2-ish syntax,
// the common denominator vLLM/Outlines accept). e.g. r"\(\d{3}\) \d{3}-\d{4}".
class Regex   { pattern: string }

// A context-free grammar. `dialect` selects the surface syntax the backend wants
// ("gbnf" for llama.cpp, "lark"/"ebnf" for Outlines/XGrammar). The library does
// NOT translate between dialects — an over-clever transpiler is exactly the kind
// of subtle-wrong grammar bug the background warns about — so the dialect rides
// along and the provider rejects what it can't speak.
class Grammar { source: string, dialect: string }

// The JSON-schema tier, reified so hosted providers and self-hosted providers can
// share one `Constrained` surface. `schema_of: type` uses the first-class `type`
// primitive; the provider lowers it to a JSON Schema with reflect.
class JsonShape { schema_of: type }

// The closed union. (BAML unions are structural; the `match` in build_request
// dispatches on membership.)
type Constraint = Choice | Regex | Grammar | JsonShape

// ---------------------------------------------------------------------------
// 1. The capability interface (NET-NEW)
//
// `Constrained requires Provider`. A provider that implements it promises: "give
// me a Constraint and I will GUARANTEE the output matches it by construction,
// not by post-hoc validation." Note there is no default body — unlike Streaming
// (which can degrade to a buffered .call), a constraint guarantee CANNOT be faked
// on top of an unconstrained .call. A provider either has token-level masking or
// it does not. That asymmetry is the whole point of the two-tier portability.
// ---------------------------------------------------------------------------

// Constrained decoding is a capability BEYOND call/stream/realtime/tools, so it
// gets its OWN capability-error interface (NET-NEW, flagged in evaluation.md).
// Its classifier is constraint-specific: was the decode guarantee impossible to
// honor on this backend? Network/parse failures still come through CallError-shaped
// concrete errors that ALSO implement this interface, so a `decode` caller can
// triage everything off one channel.
interface baml.errors.ConstraintError {
  function is_unsatisfiable(self) -> bool
  function is_network_error(self) -> bool
}

interface Constrained requires Provider {
  // Decode a `T` whose wire form is guaranteed to satisfy `constraint`.
  // Throws Unsatisfiable if the provider cannot honor this particular
  // constraint class (e.g. a JSON-only hosted backend handed a Grammar, or a
  // backend handed a grammar dialect it doesn't speak).
  function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint)
      -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError>
}

// A typed failure so the negotiation `match` can distinguish "this provider
// can't do constraints at all" (membership miss) from "this provider does
// constraints but not THIS one" (Unsatisfiable). It is a concrete error on the
// ConstraintError channel — a domain failure the provider raises on purpose.
class Unsatisfiable {
  message: string
  constraint: Constraint
  implements baml.errors.ConstraintError {
    function is_unsatisfiable(self)  -> bool { return true; }
    function is_network_error(self)  -> bool { return false; }
  }
}

// ---------------------------------------------------------------------------
// 2. Host boundary helpers
//
// Three genuine host primitives. The first two are pure (no IO): they lower a
// BAML `type` to a JSON-schema string, and decode a guaranteed-shaped string
// back into a typed `T` without SAP's coercion (the string is ALREADY exact, so
// we want a strict cast, not schema-aligned repair). The third is the SQL CFG a
// usage example reuses; kept here so usage.baml stays declarative.
// ---------------------------------------------------------------------------

// type -> JSON Schema (string). Already exists internally for response_format.
function json_schema_of(t: type) -> string { $rust_io_function }

// Strict decode of an already-guaranteed string into T. For Choice this maps the
// literal token onto an enum variant; for Regex/Grammar T is usually `string`
// and this is the identity cast. NOT SAP: there is nothing to repair. On the
// constrained decode path → the ConstraintError channel.
function cast_exact<T>(s: string) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> { $rust_io_function }

// Pull `choices[0].message.content` out of an OpenAI-shaped response body. Reads
// the response text, parses it to a native `json` value, and navigates the
// map/json[] union by hand (Response has no `.json_path`). Foreign Io / parse
// failures are normalized to the caller's channel by its trailing catch.
function openai_content(resp: baml.http.Response) -> string throws baml.errors.Io | baml.json.JsonParseError {
  let root = baml.json.parse(resp.text());
  match (root) {
    let obj: map<string, json> => match (obj.get("choices")) {
      let choices: json[] => match (choices.at(0)) {
        let first: map<string, json> => match (first.get("message")) {
          let msg: map<string, json> => match (msg.get("content")) {
            let s: string => s,
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

// llama.cpp's /completion returns the text under a top-level `content` field.
function llamacpp_content(resp: baml.http.Response) -> string throws baml.errors.Io | baml.json.JsonParseError {
  match (baml.json.parse(resp.text())) {
    let obj: map<string, json> => match (obj.get("content")) {
      let s: string => s,
      _ => "",
    },
    _ => "",
  }
}

// ============================================================================
// 3. HOSTED tier — schema-JSON only. These do NOT implement `Constrained`.
//
// OpenAIHosted shows the JSON-schema encoding inside build_request (response_format
// with strict:true). The teaching point: it is structurally unable to honor a
// Regex/Choice/Grammar, so it does not claim `Constrained`. A constraint companion
// `match`ing it will miss and the app must fall back or error (see usage.baml).
// ============================================================================

class OpenAIHosted {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The hosted strict-JSON encoding: schema goes in response_format.
      // (One of the four wire encodings of a schema; the others — Gemini
      // responseSchema, Anthropic forced-tool, prompt-text+SAP — live in their
      // own providers. Here we show exactly this one.)
      let schema = json_schema_of(reflect.type_of<T>());
      // Build the request body as a native `map<string, json>` literal (nested
      // values are native json: strings / sub-maps / bools) and serialize it.
      let body: map<string, json> = {
        "model": self.model,
        "messages": baml.llm.as_openai_messages(prompt),
        "response_format": { "type": "json_schema", "json_schema": { "name": "out", "strict": true, "schema": schema } },
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http failed"); }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(openai_content(from)) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed"); }
    }
  }

  // NOTE: no `implements Constrained`. That absence IS the model.
}

// ============================================================================
// 4. SELF-HOSTED tier — token-level constrained decoding.
//
// vLLM, llama.cpp, and Outlines are three separate Provider classes. They share
// the ordinary chat codec (HttpProvider / Provider) AND implement `Constrained`.
// The real divergence lives inside `build_request`/`decode`: each backend names
// the four constraint kinds differently and accepts different grammar dialects.
// ============================================================================

// ---- 4a. vLLM OpenAI-compatible server: guided_* extra_body --------------
class VLLM {
  model: string
  base_url: string          // e.g. "http://localhost:8000/v1" — `where it runs` is a field
  api_key: string?          // usually a throwaway token

  implements ChatModelOptions { model as model, api_key as api_key }

  // Ordinary unconstrained chat still works (HttpProvider.call via the codec).
  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> {
      // null constraint => no guided_* body => the dialect-throw branch is dead;
      // normalize anyway so the ConstraintError-typed helper fits the CallError channel.
      self.request_with(prompt, null) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "vllm build_request failed"); }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "vllm http failed"); }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(openai_content(from)) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "vllm parse failed"); }
    }
  }

  // The guaranteed-shape capability.
  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
      let req = self.request_with(prompt, constraint);
      let resp = baml.http.send(req);
      let raw = openai_content(resp);
      // The output is GUARANTEED to match by construction, so cast exactly.
      // (JsonShape is the exception: it's still JSON, so SAP-parse it.)
      match (constraint) {
        let _j: JsonShape => baml.sap.parse<T>(raw),
        _                 => cast_exact<T>(raw),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ConstraintError>(e, "vllm decode failed");
    }
  }

  // The divergence, isolated: how vLLM names each constraint in its extra_body.
  // On the constrained construction path → the ConstraintError channel; the
  // deliberate dialect-mismatch throw is an `Unsatisfiable`.
  function request_with(self, prompt: baml.llm.PromptAst, constraint: Constraint?) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
    // vLLM dispatches guided_choice / guided_regex / guided_grammar / guided_json
    // to its backend (XGrammar / Guidance). Each constraint kind => one extra key
    // in a native `map<string, json>` (the guided_choice value is itself a json[]).
    let guided: map<string, json> = match (constraint) {
      null              => {},
      let c: Choice     => { "guided_choice": c.options },
      let r: Regex      => { "guided_regex": r.pattern },
      let g: Grammar    => {
        // vLLM's guided_grammar wants a GBNF / Lark-ish CFG. Reject dialects we
        // know it can't parse rather than emit a silently-wrong mask.
        if (g.dialect != "gbnf" && g.dialect != "lark") {
          throw Unsatisfiable { message: "vLLM guided_grammar supports gbnf|lark, got " + g.dialect, constraint: g };
        }
        { "guided_grammar": g.source }
      },
      let j: JsonShape  => { "guided_json": json_schema_of(j.schema_of) },
    };
    // Build the body as one native json map (base fields + the nested guided block).
    let body: map<string, json> = {
      "model": self.model,
      "messages": baml.llm.as_openai_messages(prompt),
      "extra_body": guided,
    };
    baml.http.Request {
      method: "POST",
      url: self.base_url + "/chat/completions",
      headers: { "authorization": "Bearer " + (self.api_key ?? "x"), "content-type": "application/json" },
      body: baml.json.stringify(body),
    }
  }
}

// ---- 4b. llama.cpp server: GBNF only ('grammar' field) -------------------
// Divergence: llama.cpp has NO native choice/regex knobs. It speaks ONE thing —
// a GBNF grammar in its `grammar` request field. So it LOWERS choice and regex
// INTO GBNF before sending. This is where a real stack earns its keep: the
// portable `Constraint` union is preserved, but the provider does the lowering.
class LlamaCpp {
  base_url: string          // e.g. "http://localhost:8080"
  model: string?            // llama.cpp serves one model; field is decorative

  implements ChatModelOptions { model as model, api_key as null }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.request_with(prompt, null) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "llama.cpp build_request failed"); }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "llama.cpp http failed"); }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(llamacpp_content(from)) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "llama.cpp parse failed"); }
    }
  }

  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
      let req = self.request_with(prompt, constraint);
      let resp = baml.http.send(req);
      let raw = llamacpp_content(resp);
      match (constraint) {
        let _j: JsonShape => baml.sap.parse<T>(raw),
        _                 => cast_exact<T>(raw),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ConstraintError>(e, "llama.cpp decode failed");
    }
  }

  // Everything becomes GBNF. This is the provider-internal lowering the model
  // is supposed to absorb: choice -> alternation of string literals; regex ->
  // (llama.cpp accepts a regex INSIDE GBNF via the `=` char-class rules, but the
  // robust path is to refuse raw regex and require the caller pass a Grammar).
  function request_with(self, prompt: baml.llm.PromptAst, constraint: Constraint?) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
    let gbnf = match (constraint) {
      null              => null,
      let c: Choice     => gbnf_from_choice(c.options),       // "root ::= \"a\" | \"b\""
      let g: Grammar    => {
        if (g.dialect != "gbnf") {
          throw Unsatisfiable { message: "llama.cpp only speaks gbnf, got " + g.dialect, constraint: g };
        }
        g.source
      },
      let _j: JsonShape => gbnf_from_json_schema(json_schema_of(_j.schema_of)),  // llama.cpp ships json-schema->gbnf
      let _r: Regex     => {
        // Honest failure: llama.cpp's grammar engine does not take a bare regex.
        // Translating regex->GBNF is the subtle-wrong trap; refuse it.
        throw Unsatisfiable { message: "llama.cpp: pass a Grammar, not a Regex (no regex->gbnf)", constraint: _r };
      },
    };
    let body: map<string, json> = match (gbnf) {
      null            => { "prompt": baml.llm.as_completion_prompt(prompt) },
      let src: string => { "prompt": baml.llm.as_completion_prompt(prompt), "grammar": src },
    };
    baml.http.Request {
      method: "POST",
      url: self.base_url + "/completion",
      headers: { "content-type": "application/json" },
      body: baml.json.stringify(body),
    }
  }
}

// GBNF lowering helpers (host boundary — string templating that's easy to get
// subtly wrong, so it lives behind a tested primitive, not inline jinja).
function gbnf_from_choice(options: string[]) -> string { $rust_io_function }
function gbnf_from_json_schema(schema: string) -> string { $rust_io_function }

// ---- 4c. Outlines (in-process request/response, NO HTTP) -----------------
// Divergence of a different axis: Outlines isn't an HTTP server at all. It runs
// the model in-process. Its `call` is nonetheless a genuine request/response
// (drive the local transformers model, get one value back) — so it owns the
// request/response capability `HttpProvider` (which is "request/response", not
// "must speak HTTP"). It supplies `call`/`call_with` directly over a
// $rust_io_function instead of going through build_request/send/parse — the
// codec defaults are overridden, no actual HTTP underneath. This proves "where
// it runs" is a provider-class property: same `Constrained` surface, no HTTP.
class Outlines {
  model_id: string          // HF id, e.g. "meta-llama/Llama-3.1-8B-Instruct"
  device: string?           // "cuda" | "cpu"

  implements HttpProvider {
    type Body = unknown      // no wire body; call_with is supplied directly
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function       // unconstrained in-process generate + SAP, with meta
    }
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
      // Outlines has first-class generate.choice / generate.regex / generate.cfg.
      // It has NO json-schema knob distinct from regex (it compiles schema->regex
      // internally), so JsonShape is lowered to its schema generator here.
      match (constraint) {
        let _c: Choice    => self.run_choice<T>(prompt, _c.options),
        let _r: Regex     => self.run_regex<T>(prompt, _r.pattern),
        let _g: Grammar   => {
          if (_g.dialect != "lark" && _g.dialect != "ebnf") {
            throw Unsatisfiable { message: "Outlines CFG wants lark|ebnf, got " + _g.dialect, constraint: _g };
          }
          self.run_cfg<T>(prompt, _g.source)
        },
        let _j: JsonShape => self.run_json<T>(prompt, json_schema_of(_j.schema_of)),
      }
    }
  }

  // Each binds a distinct Outlines generator. Host bodies: these drive the
  // in-process compiled FSM/mask — the genuine boundary → ConstraintError channel.
  function run_choice<T>(self, prompt: baml.llm.PromptAst, options: string[]) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> { $rust_io_function }
  function run_regex<T>(self, prompt: baml.llm.PromptAst, pattern: string) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> { $rust_io_function }
  function run_cfg<T>(self, prompt: baml.llm.PromptAst, source: string) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> { $rust_io_function }
  function run_json<T>(self, prompt: baml.llm.PromptAst, schema: string) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> { $rust_io_function }
}

// ============================================================================
// 5. A combinator: ConstrainedFallback
//
// The two-tier story needs an orchestration primitive: "try to GUARANTEE the
// shape on a self-hosted backend; if no member can, degrade to best-effort
// schema-JSON + post-hoc validation on a hosted backend." This is the constrained
// analog of `Fallback`, and it shows the negotiation `match` at the combinator
// layer. Like every combinator it's a plain non-generic class that forwards by
// runtime delegation and STATICALLY claims Constrained (degrading at call time).
// ============================================================================

// Combinator-exhaustion error: every member of a fallback chain failed. A domain
// failure on the unconstrained call path → CallError. It is not itself a network/
// rate-limit/parse failure (those were the per-member causes, already discarded).
class AllMembersFailed {
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class ConstrainedFallback {
  strategy: Provider[]      // first capable member wins; mixed hosted/self-hosted

  implements HttpProvider {
    type Body = unknown      // pure forwarder; call/call_with route to a member
    // Forward value+sidecar to the first http-capable member, so usage/cost
    // (the P1 fix) is not dropped on the unconstrained degrade path.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      for (let p in self.strategy) {
        match (p) {
          let h: HttpProvider => {
            let r: (T, U) = h.call_with<T, U>(prompt, project) catch (e) { _ => { continue; } };
            return r;
          },
          _ => { continue; },
        }
      }
      throw AllMembersFailed { message: "all providers failed" }
    }
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
      // Prefer a member that can actually guarantee the shape.
      for (let p in self.strategy) {
        match (p) {
          let c: Constrained => {
            let r: T = c.decode<T>(prompt, constraint) catch (e) {
              // Unsatisfiable (e.g. dialect mismatch) => try the next member,
              // don't give up on the whole chain.
              _ => { continue; }
            };
            return r;
          },
          _ => { continue; },
        }
      }
      // No member could guarantee it. Honest degradation is ONLY possible for the
      // JsonShape tier (validate-after-the-fact). For Choice/Regex/Grammar there
      // is no honest fallback to an unconstrained provider — surface that.
      match (constraint) {
        let _j: JsonShape => self.call<T>(prompt),     // best-effort schema-JSON + SAP
        _                 => throw Unsatisfiable {
          message: "no member supports token-level constraints for this shape",
          constraint: constraint,
        },
      }
    } catch (e) {
      // The JsonShape degrade routes through `self.call`, whose CallError-typed
      // failures (AllMembersFailed, …) don't belong on the ConstraintError channel;
      // normalize them so this method stays on its declared channel. Known
      // ConstraintErrors (Unsatisfiable) pass through with_message untouched.
      _ => throw baml.UnknownError.with_message<baml.errors.ConstraintError>(e, "constrained fallback decode failed");
    }
  }

  function add(self, p: Provider) -> ConstrainedFallback {
    ConstrainedFallback { strategy: self.strategy.concat([p]) }
  }
}

// Sugar on Provider so app authors can write `vllm().or_else_hosted(openai())`.
interface ConstrainedCombinators requires Provider {
  function or_else_hosted(self, hosted: Provider) -> ConstrainedFallback {
    ConstrainedFallback { strategy: [self, hosted] }
  }
}
