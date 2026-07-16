// ============================================================================
// implement.baml  —  Provider diversity & gateways
// ----------------------------------------------------------------------------
// STD-LIB / library-author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, ChatModelOptions, baml.http,
// baml.sap, baml.llm).
//
// The teaching points of THIS scenario:
//
//   1. `base_url` + `headers` are ordinary provider fields. An "OpenAI-compatible
//      proxy" is the SAME OpenAI provider class with a different base_url and a
//      different auth field — no new class, no new wire codec.
//
//   2. Auth shape diverges four ways (Bearer / x-api-key / ?key= / cloud IAM).
//      Each divergence is a provider FIELD plus a tiny `auth_headers` helper —
//      not a transport-envelope blob, not a framework concern.
//
//   3. Prefix-routing ("anthropic/claude-...", "internal/gpt-4.1") and a
//      provider REGISTRY are ordinary BAML — a function returning a Provider,
//      i.e. a `client`. "Clients are functions that return a Provider" is the
//      WHOLE STORY here; LiteLLM/Vercel-registry collapse into `match` on a
//      prefix string.
//
//   4. The lowest-common-denominator trap and its escape hatch. A normalizing
//      bridge can only forward what every backend shares; the escape hatch is
//      to NOT wrap — to hold the concrete provider and reach its bespoke fields.
// ============================================================================


// ----------------------------------------------------------------------------
// §0  Net-new host primitives this scenario needs (genuine host boundaries).
//     Everything above these is canonical or pure-BAML.
// ----------------------------------------------------------------------------

// Cloud IAM signing. Bedrock/Vertex don't carry a bearer key; they sign the
// request with SigV4 / OAuth from ambient credentials. This is irreducibly a
// host concern (it reads the credential chain, computes an HMAC over the canon-
// ical request, and may refresh a token), so it mutates a Request in place.
namespace baml.cloud {
  // AWS SigV4: signs `request` for `service` in `region` using the ambient
  // credential chain (env / profile / instance role). Returns a signed copy.
  function sigv4_sign(request: baml.http.Request, service: string, region: string)
      -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> { $rust_io_function }

  // GCP OAuth: mints/refreshes an access token from ADC for the given scope.
  function gcp_access_token(scope: string) -> string throws baml.ExtendUnknownError<baml.errors.CallError> { $rust_io_function }
}


// ----------------------------------------------------------------------------
// §1  Auth shape is a provider concern, expressed as a small interface +
//     per-provider field. The four shapes from the background:
//       Bearer            -> Authorization: Bearer <key>
//       x-api-key         -> x-api-key: <key> (+ a version header)
//       ?key=             -> query param on the URL
//       cloud IAM         -> sign the whole request (no static header)
//
//     `Auth` is the read-interface a provider's fields satisfy (the same
//     pattern as ChatModelOptions). It is NOT the transport envelope — it is
//     part of the provider's identity. Each variant is a tiny class so a
//     provider just holds an `auth: Auth` field and applies it in build_request.
// ----------------------------------------------------------------------------

interface Auth {
  // apply `self` to a freshly-built request: add headers, rewrite the URL, or
  // sign. Returns the request ready to send. On the call path -> CallError.
  function apply(self, request: baml.http.Request) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError>
}

// Bearer (OpenAI, most OpenAI-compatible proxies, Together/Groq/Fireworks...).
class BearerAuth {
  key: string
  implements Auth {
    function apply(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let headers = request.headers;
      headers.set("authorization", "Bearer {{ self.key }}");
      baml.http.Request {
        method: request.method,
        url: request.url,
        headers: headers,
        body: request.body,
      }
    }
  }
}

// x-api-key + a fixed version header (Anthropic).
class ApiKeyHeaderAuth {
  key: string
  version: string         // e.g. "2023-06-01" -> anthropic-version
  implements Auth {
    function apply(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let headers = request.headers;
      headers.set("x-api-key", "{{ self.key }}");
      headers.set("anthropic-version", "{{ self.version }}");
      baml.http.Request {
        method: request.method,
        url: request.url,
        headers: headers,
        body: request.body,
      }
    }
  }
}

// ?key=<key> query param (Gemini generativelanguage endpoint).
class QueryKeyAuth {
  key: string
  param: string           // "key"
  implements Auth {
    function apply(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // url already has no query; append. (A real impl would merge existing q.)
      baml.http.Request {
        method: request.method,
        url: "{{ request.url }}?{{ self.param }}={{ self.key }}",
        headers: request.headers,
        body: request.body,
      }
    }
  }
}

// AWS SigV4 (Bedrock). No static credential header; sign the whole request.
class SigV4Auth {
  service: string         // "bedrock"
  region: string          // "us-east-1"
  implements Auth {
    function apply(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.cloud.sigv4_sign(request, self.service, self.region)
    }
  }
}

// GCP OAuth bearer (Vertex). A bearer header, but the token is minted, not held.
class GcpOAuthAuth {
  scope: string           // "https://www.googleapis.com/auth/cloud-platform"
  implements Auth {
    function apply(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let token = baml.cloud.gcp_access_token(self.scope);
      let headers = request.headers;
      headers.set("authorization", "Bearer {{ token }}");
      baml.http.Request {
        method: request.method,
        url: request.url,
        headers: headers,
        body: request.body,
      }
    }
  }
}


// ----------------------------------------------------------------------------
// §2  The OpenAI Chat Completions provider — the de-facto wire standard.
//     base_url + headers + auth are FIELDS. A proxy is THIS class with a
//     different base_url and a different auth value (see usage.baml). One codec,
//     N endpoints — the single most common interop strategy, expressed as
//     construction, not subclassing.
// ----------------------------------------------------------------------------

// A non-2xx response from an OpenAI-compatible endpoint (openai itself, a proxy,
// a gateway). Classified: it is not a parse error and not (necessarily) a
// network error, but the status lets the app branch (429 -> rate limit).
class OpenAiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError { /* same status error can surface mid-stream */ }
}

// Typed view of the OpenAI Chat Completions response envelope — just the path we
// read (`choices[0].message.content`). `baml.json.from_json<OpenAiChatResponse>`
// coerces the parsed body into this class; absent fields are tolerated as `?`.
class OpenAiMessage  { content: string }
class OpenAiChoice   { message: OpenAiMessage }
class OpenAiChatResponse { choices: OpenAiChoice[] }

class OpenAIChat {
  model: string
  // base_url is the entire portability story for OpenAI-compatible proxies.
  // Default points at openai; a proxy overrides it. Nothing else changes.
  base_url: string
  auth: Auth
  // gateways often key off arbitrary request headers (x-tenant, x-team, ...).
  // This is the per-provider header bag — distinct from the combinator-layer
  // transport envelope, because for a GATEWAY the header IS part of routing,
  // i.e. part of provider identity, not orchestration.
  headers: map<string, string>
  max_tokens: int?

  implements ChatModelOptions { model as model, api_key as null }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let schema = baml.schema.json_schema(reflect.type_of<T>(), false);
      let rf = #"{ "type": "json_schema",
                   "json_schema": { "name": "Output", "schema": {{ schema }} } }"#;
      // gateway headers first, then content-type; auth applied last (below).
      let headers: map<string, string> = {};
      for (let k in self.headers.keys()) {
        match (self.headers.get(k)) { null => {}, let v: string => { headers.set(k, v); } }
      }
      headers.set("content-type", "application/json");
      let base = baml.http.Request {
        method: "POST",
        // base_url + the fixed Chat Completions path. ONE line is all that
        // separates api.openai.com from groq / ollama / together / a gateway.
        url: "{{ self.base_url }}/chat/completions",
        headers: headers,
        body: baml.llm.openai_chat_body(prompt, self.model, self.max_tokens, rf),
      };
      // auth shape is a field -> apply it. Bearer for openai, Bearer-with-a-
      // different-key for a proxy, SigV4 if someone points this at a signing
      // gateway. The codec doesn't know or care.
      self.auth.apply(base)
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai-compatible send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) {
        throw OpenAiHttpError { status: from.status_code };
      }
      // Decode the wire envelope into a typed class and read `choices[0].message.content`.
      let env = baml.json.from_json<OpenAiChatResponse>(baml.json.parse(from.text()));
      let content = match (env.choices.at(0)) {
        null => "",
        let c: OpenAiChoice => c.message.content,
      };
      baml.sap.parse<T>(content)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai-compatible parse failed");
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      $rust_io_function   // SSE drain over the same base_url; elided (scenario 04)
    }
  }

  // Named constructors that encode the common endpoints. A proxy is a one-liner.
  function openai(model: string) -> OpenAIChat {
    OpenAIChat {
      model: model, base_url: "https://api.openai.com/v1",
      auth: BearerAuth { key: env.OPENAI_API_KEY }, headers: {}, max_tokens: null,
    }
  }
  // SAME class, different base_url + key. This is the whole "proxy" mechanism.
  function compatible(model: string, base_url: string, key: string, headers: map<string, string>) -> OpenAIChat {
    OpenAIChat {
      model: model, base_url: base_url,
      auth: BearerAuth { key: key }, headers: headers, max_tokens: null,
    }
  }
}


// ----------------------------------------------------------------------------
// §3  A first-party provider with a different wire shape AND a different auth
//     shape — Anthropic. Proves the registry below routes across genuinely
//     divergent codecs, not just base_url variants of one codec.
// ----------------------------------------------------------------------------

// A non-2xx response from Anthropic (or Anthropic-on-Bedrock). 529 is the
// Anthropic "overloaded" signal -> treat as a rate-limit-class backoff hint.
class AnthropicHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429 || self.status == 529; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError { /* same status error can surface mid-stream */ }
}

// Typed view of the Anthropic Messages response. `content` is a list of blocks;
// the forced-tool output lands in a `tool_use` block whose `input` is the value.
// `input` is an arbitrary object, so it stays `json` and is re-serialized for SAP.
class AnthropicBlock    { type: string, input: baml.json.json }
class AnthropicResponse { content: AnthropicBlock[] }

class Anthropic {
  model: string
  base_url: string
  auth: Auth
  headers: map<string, string>
  max_tokens: int

  implements ChatModelOptions { model as model, api_key as null }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Anthropic: system prompt hoisted, forced-tool for structured output.
      let schema = baml.schema.json_schema(reflect.type_of<T>(), false);
      let tools = #"[ { "name": "record_output", "input_schema": {{ schema }} } ]"#;
      let headers: map<string, string> = {};
      for (let k in self.headers.keys()) {
        match (self.headers.get(k)) { null => {}, let v: string => { headers.set(k, v); } }
      }
      headers.set("content-type", "application/json");
      let base = baml.http.Request {
        method: "POST",
        url: "{{ self.base_url }}/v1/messages",
        headers: headers,
        body: baml.llm.anthropic_body(prompt, self.model, self.max_tokens, tools,
                                      #"{ "type": "tool", "name": "record_output" }"#),
      };
      self.auth.apply(base)    // x-api-key, OR SigV4 when base_url is Bedrock
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw AnthropicHttpError { status: from.status_code }; }
      // Decode the envelope and pull the first tool_use block's `input` object;
      // re-serialize it to a string so SAP can coerce it into T.
      let env = baml.json.from_json<AnthropicResponse>(baml.json.parse(from.text()));
      let input = match (env.content.find((b: AnthropicBlock) -> bool { b.type == "tool_use" })) {
        null => "",
        let b: AnthropicBlock => baml.json.stringify(b.input),
      };
      baml.sap.parse<T>(input)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }

  function anthropic(model: string) -> Anthropic {
    Anthropic {
      model: model, base_url: "https://api.anthropic.com",
      auth: ApiKeyHeaderAuth { key: env.ANTHROPIC_API_KEY, version: "2023-06-01" },
      headers: {}, max_tokens: 1024,
    }
  }
  // Anthropic-on-Bedrock: same Messages codec, DIFFERENT auth (SigV4) and a
  // Bedrock base_url + model-id namespacing. Auth divergence absorbed by a field.
  function on_bedrock(model: string, region: string) -> Anthropic {
    Anthropic {
      model: model,
      base_url: "https://bedrock-runtime.{{ region }}.amazonaws.com",
      auth: SigV4Auth { service: "bedrock", region: region },
      headers: {}, max_tokens: 1024,
    }
  }
}


// ----------------------------------------------------------------------------
// §4  Gemini — the ?key= query-param auth shape, third divergent codec.
// ----------------------------------------------------------------------------

// A non-2xx response from Gemini (generativelanguage) or Vertex.
class GeminiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError { /* same status error can surface mid-stream */ }
}

// Typed view of the Gemini generateContent response:
// `candidates[0].content.parts[0].text`.
class GeminiPart      { text: string }
class GeminiContent   { parts: GeminiPart[] }
class GeminiCandidate { content: GeminiContent }
class GeminiResponse  { candidates: GeminiCandidate[] }

class Gemini {
  model: string
  base_url: string
  auth: Auth
  headers: map<string, string>

  implements ChatModelOptions { model as model, api_key as null }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let schema = baml.schema.gemini_schema(reflect.type_of<T>());   // different DIALECT
      let gen_cfg = #"{ "responseMimeType": "application/json", "responseSchema": {{ schema }} }"#;
      let headers: map<string, string> = {};
      for (let k in self.headers.keys()) {
        match (self.headers.get(k)) { null => {}, let v: string => { headers.set(k, v); } }
      }
      headers.set("content-type", "application/json");
      let base = baml.http.Request {
        method: "POST",
        url: "{{ self.base_url }}/v1beta/models/{{ self.model }}:generateContent",
        headers: headers,
        body: baml.llm.gemini_body(prompt, gen_cfg),
      };
      self.auth.apply(base)   // QueryKeyAuth appends ?key=, OR GcpOAuthAuth on Vertex
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (!from.ok()) { throw GeminiHttpError { status: from.status_code }; }
      // Decode the envelope and read `candidates[0].content.parts[0].text`.
      let env = baml.json.from_json<GeminiResponse>(baml.json.parse(from.text()));
      let text = match (env.candidates.at(0)) {
        null => "",
        let cand: GeminiCandidate => match (cand.content.parts.at(0)) {
          null => "",
          let part: GeminiPart => part.text,
        },
      };
      baml.sap.parse<T>(text)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini parse failed");
    }
  }

  function gemini(model: string) -> Gemini {
    Gemini {
      model: model, base_url: "https://generativelanguage.googleapis.com",
      auth: QueryKeyAuth { key: env.GEMINI_API_KEY, param: "key" }, headers: {},
    }
  }
  // Vertex: same generateContent codec, OAuth bearer instead of ?key=.
  function on_vertex(model: string, project: string, region: string) -> Gemini {
    Gemini {
      model: model,
      base_url: "https://{{ region }}-aiplatform.googleapis.com/v1/projects/{{ project }}/locations/{{ region }}/publishers/google",
      auth: GcpOAuthAuth { scope: "https://www.googleapis.com/auth/cloud-platform" }, headers: {},
    }
  }
}


// ----------------------------------------------------------------------------
// §5  Prefix routing / a registry is JUST a function returning a Provider.
//     This is the heart of the scenario: LiteLLM's prefix routing and Vercel's
//     createProviderRegistry both collapse into "parse a string, `match` the
//     prefix, construct a provider." Because `client` is sugar for a function
//     returning a Provider, the registry IS a client. No new machinery.
//
//     `resolve("anthropic/claude-sonnet-4-5")` -> Anthropic{...}
//     `resolve("internal/gpt-4.1")`            -> OpenAIChat{ proxy base_url }
//
//     Note the return type: the existential `Provider`. That is exactly why a
//     registry is portable (any backend) AND why a registry cannot promise a
//     capability at compile time (see evaluation OQ-5).
// ----------------------------------------------------------------------------

// An unroutable model spec: the prefix matches no known backend. A deliberate
// domain failure on the resolve-then-call path, so it implements CallError.
class UnknownProviderError {
  prefix: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class Registry {
  // split "prefix/rest" once; rest may itself contain slashes (openrouter,
  // bedrock model ids), so only the FIRST segment is the routing key.
  function resolve(spec: string) -> Provider
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    // split on the FIRST "/": `prefix` is the routing key, `rest` is everything
    // after it (which may itself contain slashes — openrouter / bedrock ids).
    let cut = spec.index_of("/");
    let prefix = if (cut < 0) { spec } else { spec.substring(0, cut) };
    let rest = if (cut < 0) { "" } else { spec.substring(cut + 1, spec.length()) };
    match (prefix) {
      "openai"     => OpenAIChat.openai(rest),
      "anthropic"  => Anthropic.anthropic(rest),
      "gemini"     => Gemini.gemini(rest),
      "bedrock"    => Anthropic.on_bedrock(rest, env.AWS_REGION),
      // a custom-prefixed internal GATEWAY: OpenAI-compatible, different host.
      "internal"   => OpenAIChat.compatible(rest, env.GATEWAY_URL, env.GATEWAY_KEY,
                                            { "x-team": "search" }),
      // OpenAI-compatible aggregators are all the SAME class, different base_url.
      "groq"       => OpenAIChat.compatible(rest, "https://api.groq.com/openai/v1", env.GROQ_API_KEY, {}),
      "together"   => OpenAIChat.compatible(rest, "https://api.together.xyz/v1", env.TOGETHER_API_KEY, {}),
      "ollama"     => OpenAIChat.compatible(rest, "http://localhost:11434/v1", "ollama", {}),
      _            => { throw UnknownProviderError { prefix: prefix }; },
    }
  }
}


// ----------------------------------------------------------------------------
// §6  The lowest-common-denominator bridge, and its escape hatch.
//
//     A "universal surface" that normalizes everything to one shape exposes
//     just ONE capability: request/response (`HttpProvider.call`/`call_with`).
//     `Provider` is a bare marker — it guarantees NO interaction — so the LCD
//     bridge must PICK a capability to forward, and `Normalized` picks
//     `HttpProvider`. Wrapping a provider in `Normalized` deliberately ERASES
//     every other capability (Streaming/Tools/...) — that is the LCD trap, made
//     explicit. The escape hatch is to NOT wrap: hold the concrete provider (or
//     `match` the existential back to a richer interface).
// ----------------------------------------------------------------------------

class Normalized {
  inner: Provider
  // Only the request/response surface (`call`/`call_with`) and the inherited
  // combinators survive. No Streaming/Tools/etc. exposed — a normalized handle
  // is request/response ONLY, by construction. `Normalized` IS still a Provider
  // (the marker) via `HttpProvider requires Provider`; it just forwards the one
  // capability it chooses to expose.
  implements HttpProvider {
    type Body = unknown
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => return h.call<T>(prompt);
        _ => { throw baml.errors.Unsupported { message: "normalized backend cannot produce a value" }; }
      }
    }
    // Forward the value+sidecar primitive too, so usage/cost is not dropped at
    // the normalization layer (the P1 fix): route the projection to the inner
    // http-capable provider's own call_with.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => return h.call_with<T, U>(prompt, project);
        _ => { throw baml.errors.Unsupported { message: "normalized backend cannot produce a value" }; }
      }
    }
  }
}

// The escape hatch as a library helper: recover a richer capability from a
// normalized/existential handle when the backend actually supports it. Returns
// the streaming view or errors — this is the "providerOptions pass-through"
// analog, but typed: you ask for the capability and get it or a clear failure.
class Escape {
  function as_streaming(p: Provider) -> Streaming
      throws baml.ExtendUnknownError<baml.errors.StreamError> {
    match (p) {
      let s: Streaming => s,
      _ => { throw baml.errors.Unsupported { message: "backend does not support streaming" }; },
    }
  }
}


// ----------------------------------------------------------------------------
// §7  A pure-BAML combinator gateways actually want: RoundRobin across keys /
//     endpoints. (Reliability scenario owns Retry/Fallback; this one shows the
//     gateway-flavored sibling — spread load across SAME-shape backends.) It is
//     a plain non-generic class that `implements HttpProvider` and forwards
//     `call`/`call_with` to the picked member, exactly the Fallback pattern.
// ----------------------------------------------------------------------------

// Misconfiguration: a round-robin with no members. A deliberate domain failure
// on the call path, so it implements CallError.
class EmptyPoolError {
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class RoundRobin {
  pool: Provider[]
  cursor: KVCounter      // host-backed atomic cursor; round-robin needs state

  implements HttpProvider {
    type Body = unknown
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (self.pool.length() == 0) {
        throw EmptyPoolError {};
      }
      let i = self.cursor.next_mod(self.pool.length());   // atomic ++
      match (self.pool.at(i)) {
        let h: HttpProvider => return h.call<T>(prompt);
        _ => { throw baml.errors.Unsupported { message: "round-robin member cannot produce a value" }; }
      }
    }
    // Forward value+sidecar to the SAME picked member so usage/cost (per-key
    // spend, per-endpoint latency) is not dropped by the load-spreader (P1 fix).
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (self.pool.length() == 0) {
        throw EmptyPoolError {};
      }
      let i = self.cursor.next_mod(self.pool.length());   // atomic ++
      match (self.pool.at(i)) {
        let h: HttpProvider => return h.call_with<T, U>(prompt, project);
        _ => { throw baml.errors.Unsupported { message: "round-robin member cannot produce a value" }; }
      }
    }
  }
}

// Net-new tiny host primitive: an atomic counter for round-robin / least-busy.
class KVCounter {
  _handle: $rust_type
  function next_mod(self, n: int) -> int { $rust_io_function }
}
