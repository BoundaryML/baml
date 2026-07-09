// scenario 03 — constrained decoding: ../../../../../llm-provider/ideas/scenarios/03-constrained-decoding/implement.baml
// ============================================================================
// implementation.baml — Constrained decoding beyond JSON Schema.
// Library / std-lib-author code (the design's implement.baml, on the real surface).
//
// The breadth this scenario exercises (carried over from the design):
//   - Hosted providers (OpenAI/Anthropic/Gemini) can only guarantee *JSON that
//     validates a schema*. They go through the ordinary HttpProvider codec + SAP.
//   - Open-weight backends you run yourself (vLLM, llama.cpp, Outlines) expose
//     token-level constrained decoding: guided_choice / guided_regex /
//     guided_grammar. These can guarantee NON-JSON shapes (a bare enum token, a
//     phone-number string, a SQL statement) that no JSON schema can express.
//
// So "what can be guaranteed" and "where it runs" are properties of the provider
// CLASS, not of the call site. We model that with a capability interface,
// `s03_Constrained`, that `requires baml.ai.Provider`. Hosted providers simply do
// not implement it; self-hosted ones do. Capability negotiation is then a runtime
// `match` exactly like Streaming/Realtime.
//
// REAL-SURFACE NOTE: the stdlib ships a THINNER `baml.ai.Constrained` whose
// `decode(prompt: string, pattern: string)` carries a single pattern string (see
// the anchor test in usage.baml). The design's richer capability — a closed
// `Constraint` union + a typed `Unsatisfiable` channel + per-provider divergence —
// is rendered here scenario-local as `s03_Constrained`, because (a) the stdlib
// capability is thinner and (b) `baml.errors.*` is read-only so a net-new
// `ConstraintError` interface cannot live under it. See DIVERGENCES in the report.
// ============================================================================

// ---------------------------------------------------------------------------
// 0. The constraint vocabulary (NET-NEW)
//
// A `Constraint` is the shape we want the decoder to guarantee. It is a closed
// union over the three token-level primitives every constrained-decoding stack
// exposes, plus the JSON-schema case so one type describes BOTH tiers. `T` (the
// function's output type) is still threaded for parsing; the constraint only
// describes the wire-level decode mask.
// ---------------------------------------------------------------------------

// One of a fixed set of literal strings. Output is guaranteed to be exactly one
// of `options` — typically decoded back into a `T` that is an enum.
class s03_Choice { options: string[] }

// The output is guaranteed to match this regular expression (RE2-ish syntax, the
// common denominator vLLM/Outlines accept). e.g. r"\(\d{3}\) \d{3}-\d{4}".
class s03_Regex { pattern: string }

// A context-free grammar. `dialect` selects the surface syntax the backend wants
// ("gbnf" for llama.cpp, "lark"/"ebnf" for Outlines/XGrammar). The library does
// NOT translate between dialects — an over-clever transpiler is exactly the kind
// of subtle-wrong grammar bug the background warns about — so the dialect rides
// along and the provider rejects what it can't speak.
class s03_Grammar { source: string, dialect: string }

// The JSON-schema tier, reified so hosted and self-hosted providers share one
// `s03_Constrained` surface. `schema_of: type` uses the first-class `type`
// primitive; the provider lowers it to a JSON Schema with `reflect`.
class s03_JsonShape { schema_of: type }

// The closed union. (BAML unions are structural; the `match` in request_with
// dispatches on membership.)
type s03_Constraint = s03_Choice | s03_Regex | s03_Grammar | s03_JsonShape

// ---------------------------------------------------------------------------
// 1. The capability's error channel (NET-NEW) + the capability interface.
//
// Constrained decoding is a capability BEYOND call/stream/realtime/tools, so it
// gets its OWN capability-error interface. Its load-bearing classifier is
// constraint-specific: was the decode guarantee impossible to honor on this
// backend? (`baml.errors.ConstraintError` would be the natural home, but the
// `baml.errors` namespace is read-only, so it lives scenario-local here.)
// ---------------------------------------------------------------------------

interface s03_ConstraintError {
  function is_unsatisfiable(self) -> bool
  function is_network_error(self) -> bool
}

// A typed failure so the negotiation `match` can distinguish "this provider can't
// do constraints at all" (membership miss) from "this provider does constraints
// but not THIS one" (Unsatisfiable). A domain error the provider raises on
// purpose — the per-provider deliberate throws (vLLM dialect, llama.cpp regex,
// Outlines dialect) all collapse onto this one type.
class s03_Unsatisfiable {
  message: string
  constraint: s03_Constraint
  implements s03_ConstraintError {
    function is_unsatisfiable(self) -> bool { true }
    function is_network_error(self) -> bool { false }
  }
}

// `s03_Constrained requires baml.ai.Provider`. A provider that implements it
// promises: "give me a Constraint and I will GUARANTEE the output matches it by
// construction, not by post-hoc validation." Note there is NO default body —
// unlike Streaming (which degrades to a buffered .call), a constraint guarantee
// CANNOT be faked on top of an unconstrained .call. A provider either has
// token-level masking or it does not. That asymmetry is the whole point of the
// two-tier portability (and why the stdlib's `baml.ai.Constrained` also has no
// default method).
interface s03_Constrained requires baml.ai.Provider {
  // Decode a `T` whose wire form is guaranteed to satisfy `constraint`. Throws
  // `s03_Unsatisfiable` if the provider cannot honor this particular constraint
  // class (a JSON-only backend handed a Grammar, or a dialect it doesn't speak).
  function decode<T>(self, prompt: baml.llm.PromptAst, constraint: s03_Constraint) -> T
      throws s03_ConstraintError | baml.errors.UnknownError
}

// ---------------------------------------------------------------------------
// 2. Host boundary helpers.
//
// The design used two host primitives: `json_schema_of(type) -> string` (lower a
// BAML `type` to a JSON-schema string) and `cast_exact<T>(string) -> T` (STRICT
// decode of an already-guaranteed string — NOT SAP, since there is nothing to
// repair). On the real surface `json_schema_of` IS `baml.schema.json_schema`;
// `cast_exact` has no separate primitive, so we route it through `baml.sap.parse`
// (a superset — it also accepts an already-exact string). See DIVERGENCES.
// ---------------------------------------------------------------------------

// type -> JSON Schema (string). The design's `json_schema_of`.
function s03_json_schema_of(t: type) -> string throws baml.errors.UnknownError {
  baml.schema.json_schema(t, false) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["schema lowering failed"] },
  }
}

// Strict decode of an already-guaranteed string into T. For Choice this maps the
// literal token onto an enum variant; for Regex/Grammar T is usually `string` and
// this is the identity cast. (The design's `cast_exact`; realized via SAP.)
function s03_cast_exact<T>(s: string) -> T throws baml.errors.UnknownError {
  baml.sap.parse<T>(s) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["cast_exact failed"] },
  }
}

// GBNF lowering: choice -> alternation of quoted string literals. In the design
// this was a `$rust_io_function`; the choice case is simple, correct string work,
// so it is pure BAML here (and offline-testable). e.g. ["a","b"] -> root ::= "a" | "b".
function s03_gbnf_from_choice(options: string[]) -> string throws never {
  let alts: string[] = [];
  for (let o in options) {
    alts.push("\"" + o + "\"");
  }
  "root ::= " + alts.join(" | ")
}

// Pull `choices[0].message.content` out of an OpenAI-shaped response body: parse
// to a native `json` value and navigate the map/json[] union by hand (Response
// has no `.json_path`).
function s03_openai_content(resp: baml.http.Response) -> string throws baml.errors.UnknownError {
  let text = resp.text() catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["read body"] } };
  let root = baml.json.parse(text) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["parse body"] } };
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
function s03_llamacpp_content(resp: baml.http.Response) -> string throws baml.errors.UnknownError {
  let text = resp.text() catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["read body"] } };
  match (baml.json.parse(text) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["parse body"] } }) {
    let obj: map<string, json> => match (obj.get("content")) {
      let s: string => s,
      _ => "",
    },
    _ => "",
  }
}

// ============================================================================
// 3. HOSTED tier — schema-JSON only. Does NOT implement `s03_Constrained`.
//
// The teaching point: it is structurally unable to honor a Regex/Choice/Grammar,
// so it does not claim the capability. A constraint negotiation `match`ing it will
// miss and the app must fall back or error (see usage.baml). It DOES implement the
// ordinary request/response capability (`baml.ai.HttpProvider`, strict response_format).
// ============================================================================

class s03_OpenAIHosted {
  model: string
  api_key: string

  implements baml.ai.Provider {}

  implements baml.ai.HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
        throws baml.errors.CallError | baml.errors.UnknownError {
      // The hosted strict-JSON encoding: schema goes in response_format. (One of
      // the four wire encodings of a schema; the others — Gemini responseSchema,
      // Anthropic forced-tool, prompt-text+SAP — live in their own providers.)
      let schema = s03_json_schema_of(reflect.type_of<T>());
      let fmt: map<string, json> = {};
      fmt.set("type", "json_schema");
      let js: map<string, json> = {};
      js.set("name", "out");
      js.set("strict", true);
      js.set("schema", baml.json.parse(schema) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["schema parse"] } });
      fmt.set("json_schema", js);
      let body: map<string, json> = {};
      body.set("model", self.model);
      let msgs: json[] = [];
      for (let m in messages) {
        let mm: map<string, json> = {};
        mm.set("role", m.role);
        mm.set("content", s03_text_of(m));
        msgs.push(mm);
      }
      body.set("messages", msgs);
      body.set("response_format", fmt);
      let headers: map<string, string> = {};
      headers.set("authorization", "Bearer " + self.api_key);
      headers.set("content-type", "application/json");
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: headers,
        body: baml.json.stringify(body),
      }
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.http.send(request) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["openai http failed"] } }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.sap.parse<T>(s03_openai_content(from)) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["openai parse failed"] } }
    }

    function parse_meta(self, from: baml.http.Response) -> baml.ai.ResponseMeta throws never {
      baml.ai.LegacyMeta {}
    }
  }

  // NOTE: no `implements s03_Constrained`. That absence IS the model.
}

// Flatten a ChatMessage's text parts into one string (helper for the wire encoders).
function s03_text_of(m: baml.ai.ChatMessage) -> string throws never {
  let out: string[] = [];
  for (let p in m.parts) {
    match (p.text) {
      let t: string => { out.push(t); },
      _ => {},
    }
  }
  out.join("\n")
}

// ============================================================================
// 4. SELF-HOSTED tier — token-level constrained decoding.
//
// vLLM, llama.cpp, and Outlines are three separate Provider classes. They share
// the ordinary chat codec AND implement `s03_Constrained`. The real divergence
// lives inside `request_with`/`decode`: each backend names the constraint kinds
// differently and accepts different grammar dialects.
// ============================================================================

// ---- 4a. vLLM OpenAI-compatible server: guided_* extra_body ----------------
// "Where it runs" is a field (`base_url`). Implements BOTH the ordinary
// request/response capability (unconstrained chat) AND `s03_Constrained` — the
// evaluation's point that these are two independent capability rows.
class s03_VLLM {
  model: string
  base_url: string          // e.g. "http://localhost:8000/v1"
  api_key: string?          // usually a throwaway token

  implements baml.ai.Provider {}
  implements s03_ConstrainedCombinators {}

  // Ordinary unconstrained chat still works (HttpProvider.call via the codec).
  implements baml.ai.HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.request_with(messages, null) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["vllm build_request failed"] } }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.http.send(request) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["vllm http failed"] } }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.sap.parse<T>(s03_openai_content(from)) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["vllm parse failed"] } }
    }
    function parse_meta(self, from: baml.http.Response) -> baml.ai.ResponseMeta throws never {
      baml.ai.LegacyMeta {}
    }
  }

  // The guaranteed-shape capability.
  implements s03_Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: s03_Constraint) -> T
        throws s03_ConstraintError | baml.errors.UnknownError {
      let req = self.request_with(baml.ai.prompt_to_messages(prompt), constraint);
      let resp = baml.http.send(req) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["vllm decode http failed"] } };
      let raw = s03_openai_content(resp);
      // The output is GUARANTEED to match by construction, so cast exactly.
      // (JsonShape is the exception: it's still JSON, so SAP-parse it.)
      match (constraint) {
        let _j: s03_JsonShape => baml.sap.parse<T>(raw) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["vllm json parse"] } },
        _                     => s03_cast_exact<T>(raw),
      }
    }
  }

  // The divergence, isolated: how vLLM names each constraint in its extra_body.
  // vLLM dispatches guided_choice / guided_regex / guided_grammar / guided_json to
  // its backend (XGrammar / Guidance). Each kind => one extra key.
  function request_with(self, messages: baml.ai.ChatMessage[], constraint: s03_Constraint?) -> baml.http.Request
      throws s03_ConstraintError | baml.errors.UnknownError {
    let guided: map<string, json> = {};
    match (constraint) {
      null => {},
      let c: s03_Choice => {
        let opts: json[] = [];
        for (let o in c.options) { opts.push(o); }
        guided.set("guided_choice", opts);
      },
      let r: s03_Regex => { guided.set("guided_regex", r.pattern); },
      let g: s03_Grammar => {
        // vLLM's guided_grammar wants a GBNF / Lark-ish CFG. Reject dialects we
        // know it can't parse rather than emit a silently-wrong mask.
        if (g.dialect != "gbnf" && g.dialect != "lark") {
          throw s03_Unsatisfiable { message: "vLLM guided_grammar supports gbnf|lark, got " + g.dialect, constraint: g };
        }
        guided.set("guided_grammar", g.source);
      },
      let j: s03_JsonShape => { guided.set("guided_json", baml.json.parse(s03_json_schema_of(j.schema_of)) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["guided_json schema"] } }); },
    }
    let body: map<string, json> = {};
    body.set("model", self.model);
    let msgs: json[] = [];
    for (let m in messages) {
      let mm: map<string, json> = {};
      mm.set("role", m.role);
      mm.set("content", s03_text_of(m));
      msgs.push(mm);
    }
    body.set("messages", msgs);
    body.set("extra_body", guided);
    let headers: map<string, string> = {};
    headers.set("authorization", "Bearer " + (self.api_key ?? "x"));
    headers.set("content-type", "application/json");
    baml.http.Request {
      method: "POST",
      url: self.base_url + "/chat/completions",
      headers: headers,
      body: baml.json.stringify(body),
    }
  }
}

// ---- 4b. llama.cpp server: GBNF only ('grammar' field) ---------------------
// Divergence: llama.cpp has NO native choice/regex knobs. It speaks ONE thing —
// a GBNF grammar in its `grammar` request field. So it LOWERS choice INTO GBNF
// before sending, and REFUSES a bare regex (regex->GBNF is the subtle-wrong trap).
class s03_LlamaCpp {
  base_url: string          // e.g. "http://localhost:8080"
  model: string?            // llama.cpp serves one model; field is decorative

  implements baml.ai.Provider {}
  implements s03_ConstrainedCombinators {}

  implements baml.ai.HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.request_with(messages, null) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["llama.cpp build_request failed"] } }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.http.send(request) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["llama.cpp http failed"] } }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      baml.sap.parse<T>(s03_llamacpp_content(from)) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["llama.cpp parse failed"] } }
    }
    function parse_meta(self, from: baml.http.Response) -> baml.ai.ResponseMeta throws never {
      baml.ai.LegacyMeta {}
    }
  }

  implements s03_Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: s03_Constraint) -> T
        throws s03_ConstraintError | baml.errors.UnknownError {
      let req = self.request_with(baml.ai.prompt_to_messages(prompt), constraint);
      let resp = baml.http.send(req) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["llama.cpp decode http failed"] } };
      let raw = s03_llamacpp_content(resp);
      match (constraint) {
        let _j: s03_JsonShape => baml.sap.parse<T>(raw) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["llama.cpp json parse"] } },
        _                     => s03_cast_exact<T>(raw),
      }
    }
  }

  // Everything becomes GBNF. This is the provider-internal lowering the model is
  // supposed to absorb: choice -> alternation of string literals; a Grammar must
  // already be gbnf; a bare Regex is REFUSED (no honest regex->gbnf); JsonShape
  // would use llama.cpp's json-schema->gbnf (BLOCKED below).
  function request_with(self, messages: baml.ai.ChatMessage[], constraint: s03_Constraint?) -> baml.http.Request
      throws s03_ConstraintError | baml.errors.UnknownError {
    let body: map<string, json> = {};
    body.set("prompt", s03_completion_prompt(messages));
    match (constraint) {
      null => {},
      let c: s03_Choice => { body.set("grammar", s03_gbnf_from_choice(c.options)); },
      let g: s03_Grammar => {
        if (g.dialect != "gbnf") {
          throw s03_Unsatisfiable { message: "llama.cpp only speaks gbnf, got " + g.dialect, constraint: g };
        }
        body.set("grammar", g.source);
      },
      let r: s03_Regex => {
        // Honest failure: llama.cpp's grammar engine does not take a bare regex.
        // Translating regex->GBNF is the subtle-wrong trap; refuse it.
        throw s03_Unsatisfiable { message: "llama.cpp: pass a Grammar, not a Regex (no regex->gbnf)", constraint: r };
      },
      let j: s03_JsonShape => {
        // llama.cpp ships json-schema->gbnf, but the lowering is a nontrivial host
        // helper (the design's `gbnf_from_json_schema` $rust_io_function).
        // BLOCKED: P8 json-schema -> GBNF lowering host primitive not available.
        throw s03_Unsatisfiable { message: "llama.cpp json-schema->gbnf lowering not available (P8)", constraint: j };
      },
    }
    let headers: map<string, string> = {};
    headers.set("content-type", "application/json");
    baml.http.Request {
      method: "POST",
      url: self.base_url + "/completion",
      headers: headers,
      body: baml.json.stringify(body),
    }
  }
}

// Flatten messages into a single completion-style prompt (llama.cpp /completion).
function s03_completion_prompt(messages: baml.ai.ChatMessage[]) -> string throws never {
  let out: string[] = [];
  for (let m in messages) {
    out.push(s03_text_of(m));
  }
  out.join("\n")
}

// ---- 4c. Outlines (in-process request/response, NO HTTP) -------------------
// Divergence of a different axis: Outlines isn't an HTTP server at all — it runs
// the model in-process. It has first-class generate.choice / generate.regex /
// generate.cfg, and folds json-schema into its regex generator. "Where it runs"
// (in-process cuda) is a provider-class property: same `s03_Constrained` surface,
// no HTTP. The actual in-process generation is a host boundary.
// BLOCKED: P8 in-process transformers/Outlines runtime not available offline; the
// dispatch + dialect gate below ARE exercised, the generation steps throw.
class s03_Outlines {
  model_id: string          // HF id, e.g. "meta-llama/Llama-3.1-8B-Instruct"
  device: string?           // "cuda" | "cpu"

  implements baml.ai.Provider {}
  implements s03_ConstrainedCombinators {}

  implements s03_Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: s03_Constraint) -> T
        throws s03_ConstraintError | baml.errors.UnknownError {
      // Outlines binds a distinct generator per constraint kind. The dispatch +
      // the CFG dialect gate are pure; the generation steps are the host boundary.
      match (constraint) {
        let c: s03_Choice => self.run_choice<T>(prompt, c.options),
        let r: s03_Regex  => self.run_regex<T>(prompt, r.pattern),
        let g: s03_Grammar => {
          if (g.dialect != "lark" && g.dialect != "ebnf") {
            throw s03_Unsatisfiable { message: "Outlines CFG wants lark|ebnf, got " + g.dialect, constraint: g };
          }
          self.run_cfg<T>(prompt, g.source)
        },
        let j: s03_JsonShape => self.run_json<T>(prompt, s03_json_schema_of(j.schema_of)),
      }
    }
  }

  // Each binds a distinct Outlines generator (in-process compiled FSM/mask).
  // BLOCKED: P8 — no in-process model runtime; the shape + the dispatch compile.
  function run_choice<T>(self, prompt: baml.llm.PromptAst, options: string[]) -> T throws baml.errors.UnknownError {
    throw baml.errors.UnknownError { data: null, message: ["BLOCKED: P8 Outlines in-process generate.choice"] }
  }
  function run_regex<T>(self, prompt: baml.llm.PromptAst, pattern: string) -> T throws baml.errors.UnknownError {
    throw baml.errors.UnknownError { data: null, message: ["BLOCKED: P8 Outlines in-process generate.regex"] }
  }
  function run_cfg<T>(self, prompt: baml.llm.PromptAst, source: string) -> T throws baml.errors.UnknownError {
    throw baml.errors.UnknownError { data: null, message: ["BLOCKED: P8 Outlines in-process generate.cfg"] }
  }
  function run_json<T>(self, prompt: baml.llm.PromptAst, schema: string) -> T throws baml.errors.UnknownError {
    throw baml.errors.UnknownError { data: null, message: ["BLOCKED: P8 Outlines in-process generate.json"] }
  }
}

// ============================================================================
// 5. A combinator: s03_ConstrainedFallback
//
// The two-tier story needs an orchestration primitive: "try to GUARANTEE the
// shape on a self-hosted backend; if no member can, degrade to best-effort
// schema-JSON + post-hoc validation on a hosted backend." The constrained analog
// of `baml.ai.Fallback`: a plain non-generic class that forwards by runtime
// delegation and STATICALLY claims `s03_Constrained` (degrading at call time).
// ============================================================================

class s03_ConstrainedFallback {
  strategy: baml.ai.Provider[]      // first capable member wins; mixed hosted/self-hosted

  implements baml.ai.Provider {}

  // Pure forwarder for the unconstrained degrade path: route call to the first
  // http-capable member (so the JsonShape best-effort tier works).
  implements baml.ai.HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
        throws baml.errors.CallError | baml.errors.UnknownError {
      throw baml.errors.UnknownError { data: null, message: ["fallback forwards call, not build_request"] }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.errors.CallError | baml.errors.UnknownError {
      throw baml.errors.UnknownError { data: null, message: ["fallback forwards call, not send"] }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      throw baml.errors.UnknownError { data: null, message: ["fallback forwards call, not parse"] }
    }
    function parse_meta(self, from: baml.http.Response) -> baml.ai.ResponseMeta throws never {
      baml.ai.LegacyMeta {}
    }
    // Forward the whole call to the first http-capable member.
    function call_messages<T>(self, messages: baml.ai.ChatMessage[]) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      for (let p in self.strategy) {
        match (p) {
          let h: baml.ai.HttpProvider => {
            let r: T = h.call_messages<T>(messages) catch (e) { _ => { continue; } };
            return r;
          },
          _ => { continue; },
        }
      }
      // Combinator-exhaustion: no member could serve the unconstrained call.
      throw baml.errors.UnknownError { data: null, message: ["all providers failed"] }
    }
  }

  implements s03_Constrained {
    function decode<T>(self, prompt: baml.llm.PromptAst, constraint: s03_Constraint) -> T
        throws s03_ConstraintError | baml.errors.UnknownError {
      // Prefer a member that can actually guarantee the shape.
      for (let p in self.strategy) {
        match (p) {
          let c: s03_Constrained => {
            // Unsatisfiable (e.g. dialect mismatch) => try the next member,
            // don't give up on the whole chain.
            let r: T = c.decode<T>(prompt, constraint) catch (e) { _ => { continue; } };
            return r;
          },
          _ => { continue; },
        }
      }
      // No member could guarantee it. Honest degradation is ONLY possible for the
      // JsonShape tier (validate-after-the-fact). For Choice/Regex/Grammar there is
      // no honest fallback to an unconstrained provider — surface that (the leak).
      match (constraint) {
        let _j: s03_JsonShape => self.call_messages<T>(baml.ai.prompt_to_messages(prompt)) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["constrained fallback json degrade failed"] } },
        _                     => throw s03_Unsatisfiable {
          message: "no member supports token-level constraints for this shape",
          constraint: constraint,
        },
      }
    }
  }

  function add(self, p: baml.ai.Provider) -> s03_ConstrainedFallback throws never {
    s03_ConstrainedFallback { strategy: self.strategy.concat([p]) }
  }
}

// Sugar so app authors can write `vllm.or_else_hosted(hosted)` on a CONCRETE
// self-hosted provider value. (The design put this on the `Provider` marker so a
// `client` result could chain it; the real `baml.ai.Provider` existential has no
// such method, so it lives on a scenario-local interface a concrete provider
// implements — see DIVERGENCES.)
interface s03_ConstrainedCombinators requires baml.ai.Provider {
  function or_else_hosted(self, hosted: baml.ai.Provider) -> s03_ConstrainedFallback throws never {
    s03_ConstrainedFallback { strategy: [self, hosted] }
  }
}
