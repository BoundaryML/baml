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

interface Constrained requires Provider {
  // Decode a `T` whose wire form is guaranteed to satisfy `constraint`.
  // Throws Unsatisfiable if the provider cannot honor this particular
  // constraint class (e.g. a JSON-only hosted backend handed a Grammar, or a
  // backend handed a grammar dialect it doesn't speak).
  function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint)
      -> T throws Error
}

// A typed failure so the negotiation `match` can distinguish "this provider
// can't do constraints at all" (membership miss) from "this provider does
// constraints but not THIS one" (Unsatisfiable).
class Unsatisfiable { message: string, constraint: Constraint }

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
// and this is the identity cast. NOT SAP: there is nothing to repair.
function cast_exact<T>(s: string) -> T throws baml.errors.LlmClient { $rust_io_function }

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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // The hosted strict-JSON encoding: schema goes in response_format.
      // (One of the four wire encodings of a schema; the others — Gemini
      // responseSchema, Anthropic forced-tool, prompt-text+SAP — live in their
      // own providers. Here we show exactly this one.)
      let schema = json_schema_of(reflect.type_of<T>());
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.encode({
          "model": self.model,
          "messages": baml.llm.as_openai_messages(prompt),
          "response_format": { "type": "json_schema", "json_schema": { "name": "out", "strict": true, "schema": schema } },
        }),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
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

  // Ordinary unconstrained chat still works (Provider.call via the codec).
  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      self.request_with(prompt, null)     // no guided_* body
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "vllm http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
    }
  }

  // The guaranteed-shape capability.
  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.errors.LlmClient {
      let req = self.request_with(prompt, constraint);
      let resp = baml.http.send(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "vllm http failed" }; } };
      let raw = resp.json_path("choices.0.message.content");
      // The output is GUARANTEED to match by construction, so cast exactly.
      // (JsonShape is the exception: it's still JSON, so SAP-parse it.)
      match (constraint) {
        let _j: JsonShape => baml.sap.parse<T>(raw),
        _                 => cast_exact<T>(raw),
      }
    }
  }

  // The divergence, isolated: how vLLM names each constraint in its extra_body.
  function request_with(self, prompt: baml.llm.PromptAst, constraint: Constraint?) -> baml.http.Request throws baml.errors.LlmClient {
    let base = {
      "model": self.model,
      "messages": baml.llm.as_openai_messages(prompt),
    };
    // vLLM dispatches guided_choice / guided_regex / guided_grammar / guided_json
    // to its backend (XGrammar / Guidance). Each constraint kind => one extra key.
    let guided = match (constraint) {
      null              => {},
      let c: Choice     => { "guided_choice": c.options },
      let r: Regex      => { "guided_regex": r.pattern },
      let g: Grammar    => {
        // vLLM's guided_grammar wants a GBNF / Lark-ish CFG. Reject dialects we
        // know it can't parse rather than emit a silently-wrong mask.
        if (g.dialect != "gbnf" && g.dialect != "lark") {
          throw baml.errors.LlmClient { message: "vLLM guided_grammar supports gbnf|lark, got " + g.dialect };
        }
        { "guided_grammar": g.source }
      },
      let j: JsonShape  => { "guided_json": json_schema_of(j.schema_of) },
    };
    baml.http.Request {
      method: "POST",
      url: self.base_url + "/chat/completions",
      headers: { "authorization": "Bearer " + (self.api_key ?? "x"), "content-type": "application/json" },
      body: baml.json.encode(baml.json.merge(base, { "extra_body": guided })),
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      self.request_with(prompt, null)
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "llama.cpp http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("content"))
    }
  }

  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.errors.LlmClient {
      let req = self.request_with(prompt, constraint);
      let resp = baml.http.send(req) catch (e) { _ => { throw baml.errors.LlmClient { message: "llama.cpp http failed" }; } };
      let raw = resp.json_path("content");
      match (constraint) {
        let _j: JsonShape => baml.sap.parse<T>(raw),
        _                 => cast_exact<T>(raw),
      }
    }
  }

  // Everything becomes GBNF. This is the provider-internal lowering the model
  // is supposed to absorb: choice -> alternation of string literals; regex ->
  // (llama.cpp accepts a regex INSIDE GBNF via the `=` char-class rules, but the
  // robust path is to refuse raw regex and require the caller pass a Grammar).
  function request_with(self, prompt: baml.llm.PromptAst, constraint: Constraint?) -> baml.http.Request throws baml.errors.LlmClient {
    let gbnf = match (constraint) {
      null              => null,
      let c: Choice     => gbnf_from_choice(c.options),       // "root ::= \"a\" | \"b\""
      let g: Grammar    => {
        if (g.dialect != "gbnf") {
          throw baml.errors.LlmClient { message: "llama.cpp only speaks gbnf, got " + g.dialect };
        }
        g.source
      },
      let _j: JsonShape => gbnf_from_json_schema(json_schema_of(_j.schema_of)),  // llama.cpp ships json-schema->gbnf
      let _r: Regex     => {
        // Honest failure: llama.cpp's grammar engine does not take a bare regex.
        // Translating regex->GBNF is the subtle-wrong trap; refuse it.
        throw baml.errors.LlmClient { message: "llama.cpp: pass a Grammar, not a Regex (no regex->gbnf)" };
      },
    };
    let body = match (gbnf) {
      null            => { "prompt": baml.llm.as_completion_prompt(prompt) },
      let src: string => { "prompt": baml.llm.as_completion_prompt(prompt), "grammar": src },
    };
    baml.http.Request {
      method: "POST",
      url: self.base_url + "/completion",
      headers: { "content-type": "application/json" },
      body: baml.json.encode(body),
    }
  }
}

// GBNF lowering helpers (host boundary — string templating that's easy to get
// subtly wrong, so it lives behind a tested primitive, not inline jinja).
function gbnf_from_choice(options: string[]) -> string { $rust_io_function }
function gbnf_from_json_schema(schema: string) -> string { $rust_io_function }

// ---- 4c. Outlines (in-process, NOT HttpProvider) -------------------------
// Divergence of a different axis: Outlines isn't an HTTP server at all. It runs
// the model in-process (like the harness in §8 of the proposal). So it does NOT
// implement HttpProvider — it implements `Provider.call` and `Constrained.decode`
// over a $rust_io_function that drives the local transformers model. This proves
// "where it runs" is a provider-class property: same `Constrained` surface, no
// HTTP underneath at all.
class Outlines {
  model_id: string          // HF id, e.g. "meta-llama/Llama-3.1-8B-Instruct"
  device: string?           // "cuda" | "cpu"

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function       // unconstrained in-process generate + SAP
    }
  }

  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.errors.LlmClient {
      // Outlines has first-class generate.choice / generate.regex / generate.cfg.
      // It has NO json-schema knob distinct from regex (it compiles schema->regex
      // internally), so JsonShape is lowered to its schema generator here.
      match (constraint) {
        let _c: Choice    => self.run_choice<T>(prompt, _c.options),
        let _r: Regex     => self.run_regex<T>(prompt, _r.pattern),
        let _g: Grammar   => {
          if (_g.dialect != "lark" && _g.dialect != "ebnf") {
            throw baml.errors.LlmClient { message: "Outlines CFG wants lark|ebnf, got " + _g.dialect };
          }
          self.run_cfg<T>(prompt, _g.source)
        },
        let _j: JsonShape => self.run_json<T>(prompt, json_schema_of(_j.schema_of)),
      }
    }
  }

  // Each binds a distinct Outlines generator. Host bodies: these drive the
  // in-process compiled FSM/mask — the genuine boundary.
  function run_choice<T>(self, prompt: baml.llm.PromptAst, options: string[]) -> T throws baml.errors.LlmClient { $rust_io_function }
  function run_regex<T>(self, prompt: baml.llm.PromptAst, pattern: string) -> T throws baml.errors.LlmClient { $rust_io_function }
  function run_cfg<T>(self, prompt: baml.llm.PromptAst, source: string) -> T throws baml.errors.LlmClient { $rust_io_function }
  function run_json<T>(self, prompt: baml.llm.PromptAst, schema: string) -> T throws baml.errors.LlmClient { $rust_io_function }
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

class ConstrainedFallback {
  strategy: Provider[]      // first capable member wins; mixed hosted/self-hosted

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        let r: T = p.call<T>(prompt) catch (e) { _ => { continue; } };
        return r;
      }
      throw baml.errors.LlmClient { message: "all providers failed" }
    }
  }

  implements Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: Constraint) -> T throws baml.errors.LlmClient {
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
    }
  }

  function add(self, p: Provider) -> ConstrainedFallback {
    ConstrainedFallback { strategy: self.strategy.append(p) }
  }
}

// Sugar on Provider so app authors can write `vllm().or_else_hosted(openai())`.
interface ConstrainedCombinators requires Provider {
  function or_else_hosted(self, hosted: Provider) -> ConstrainedFallback {
    ConstrainedFallback { strategy: [self, hosted] }
  }
}
