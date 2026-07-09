// scenario 28 — provider diversity & gateways: ../../../../../llm-provider/ideas/scenarios/28-provider-diversity/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design hand-rolled OpenAIChat / Anthropic / Gemini providers, an `Auth`
// interface with five variants, and a `Registry` resolver. On the REAL surface
// the providers and the auth-as-typed-field story are already stdlib:
//   - baml.ai.OpenAi / Anthropic / Gemini are real HttpProviders, each baking
//     its own auth shape (OpenAi Bearer, Anthropic x-api-key + version header,
//     Gemini ?key= query param) inside its own build_request.
//   - baml.ai.OpenAiCompatible is the design's "a proxy is the SAME class, a
//     different base_url" item MADE REAL: one Chat-Completions codec, an
//     arbitrary `base_url`, and a TYPED auth union
//     `BearerAuth | HeaderAuth | NoAuth` (Groq/Together = Bearer, an internal
//     gateway = HeaderAuth, a local Ollama/vLLM runtime = NoAuth). Auth-shape
//     divergence is a field, invisible to the app (design teaching point #1/#2).
//   - baml.ai.OpenAiStrict is HttpProvider but NOT Streaming — a real, routable
//     backend that sits ON the lowest-common-denominator floor (design §6).
//
// So the genuinely net-new library surface here is:
//   1. the REGISTRY — `s28_resolve(spec)`: prefix routing collapses into a
//      function returning the existential `Provider` (design's whole story —
//      LiteLLM/Vercel-registry are just a `match` on a string prefix), and
//   2. the typed unroutable-spec error (`s28_UnknownProviderError`), and
//   3. the capability-router + escape-hatch helpers that negotiate off the
//      existential `Provider` at runtime (design §5/§6: route-by-string XOR
//      static capability — you pick).


// ---------------------------------------------------------------------------
// NET-NEW: an unroutable model spec. The prefix matched no known backend — a
// deliberate DOMAIN failure on the resolve-then-call path, so it implements the
// `CallError` channel (all classifiers false: it is neither network, rate-limit,
// nor parse). A LiteLLM-style batch triages it through the SAME channel as every
// backend's HTTP error, with no provider named (design §a error-model claim).
// ---------------------------------------------------------------------------
class s28_UnknownProviderError {
    prefix: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
}


// ---------------------------------------------------------------------------
// Offline-safe key read: use the real env var when present (live tier), else a
// placeholder so construction stays PURE offline (construction never touches the
// network — only `.call`/`.stream` do). This is what lets `s28_resolve` be
// exercised offline for its ROUTING (which brand, which capability) without any
// key material, and live for a real call.
// ---------------------------------------------------------------------------
function s28_env(name: string) -> string throws never {
    (baml.env.get(name) catch (e) { _ => null }) ?? "offline-placeholder-key"
}


// ---------------------------------------------------------------------------
// THE REGISTRY (design §5). "Clients are functions that return a Provider" is
// the whole scenario: prefix routing / a provider registry collapses into a
// function that splits a spec on its FIRST "/", `match`es the routing key, and
// CONSTRUCTS a provider. The return type is the existential `Provider` marker —
// that erasure is exactly what makes one function accept any backend (design's
// portability mechanism) AND why the capability is a runtime, not compile-time,
// promise (design OQ-5).
//
//   "openai/gpt-4o"          -> OpenAi              (Bearer, api.openai.com)
//   "anthropic/claude-…"     -> Anthropic          (x-api-key + version header)
//   "gemini/gemini-flash-latest"-> Gemini             (?key= query param)
//   "groq/llama-3.3-70b"     -> OpenAiCompatible    (Bearer, groq base_url)
//   "together/…"             -> OpenAiCompatible    (Bearer, together base_url)
//   "ollama/qwen2.5"         -> OpenAiCompatible    (NoAuth, localhost runtime)
//   "internal/gpt-4o"        -> OpenAiCompatible    (HeaderAuth, gateway + x-team)
//   "strict/gpt-4o"          -> OpenAiStrict        (HttpProvider ONLY — LCD floor)
// ---------------------------------------------------------------------------
function s28_resolve(spec: string) -> baml.ai.Provider
    throws baml.errors.CallError | baml.errors.UnknownError {
    // Split on the FIRST "/": the prefix is the routing key, `rest` is the model
    // id (which may itself contain slashes — bedrock/openrouter ids). A single
    // index_of cut captures that where `.split("/")` would over-split.
    let cut = match (spec.index_of("/")) { null => 0 - 1, let c: int => c };
    let prefix = if (cut < 0) { spec } else { spec.substring(0, cut) };
    let rest = if (cut < 0) { "" } else { spec.substring(cut + 1, spec.length()) };
    match (prefix) {
        "openai" => baml.ai.OpenAi {
            model: rest, api_key: s28_env("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        },
        "anthropic" => baml.ai.Anthropic {
            model: rest, api_key: s28_env("ANTHROPIC_API_KEY"), base_url: null,
            max_tokens: null, anthropic_version: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        },
        "gemini" => baml.ai.Gemini {
            model: rest, api_key: s28_env("GOOGLE_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        },
        // OpenAI-compatible aggregators: SAME class, different base_url + Bearer.
        "groq" => baml.ai.OpenAiCompatible {
            model: rest, base_url: "https://api.groq.com/openai/v1",
            auth: baml.ai.BearerAuth { token: s28_env("GROQ_API_KEY") },
            extra_headers: null, extra_body: null, append_output_schema: null,
        },
        "together" => baml.ai.OpenAiCompatible {
            model: rest, base_url: "https://api.together.xyz/v1",
            auth: baml.ai.BearerAuth { token: s28_env("TOGETHER_API_KEY") },
            extra_headers: null, extra_body: null, append_output_schema: null,
        },
        // A local runtime (Ollama /v1, vLLM, llama.cpp): NoAuth.
        "ollama" => baml.ai.OpenAiCompatible {
            model: rest, base_url: "http://localhost:11434/v1",
            auth: baml.ai.NoAuth {},
            extra_headers: null, extra_body: null, append_output_schema: null,
        },
        // An internal gateway that keys off a header: the routing header is part
        // of provider IDENTITY (a field), plus HeaderAuth for the gateway key.
        "internal" => baml.ai.OpenAiCompatible {
            model: rest, base_url: s28_env("GATEWAY_URL"),
            auth: baml.ai.HeaderAuth { name: "api-key", value: s28_env("GATEWAY_KEY") },
            extra_headers: { "x-team": "search" },
            extra_body: null, append_output_schema: null,
        },
        // A backend that sits on the LCD floor: HttpProvider, but NOT Streaming.
        "strict" => baml.ai.OpenAiStrict {
            model: rest, api_key: s28_env("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null,
        },
        _ => { throw s28_UnknownProviderError { prefix: prefix }; },
    }
}


// ---------------------------------------------------------------------------
// Capability routing off the existential (design §5, gotcha: "interface-
// membership matching IS the capability-negotiation backbone"). A router that
// picks a code path by asking WHICH capability a resolved provider actually has
// — not by brand. `let s: Streaming =>` fires first (the richer capability),
// then the `HttpProvider` floor, then the bare marker.
// ---------------------------------------------------------------------------
function s28_route_by_capability(p: baml.ai.Provider) -> string throws never {
    match (p) {
        let s: baml.ai.Streaming    => "stream-capable",
        let h: baml.ai.HttpProvider => "call-only",
        _                           => "bare-marker",
    }
}


// ---------------------------------------------------------------------------
// The lowest-common-denominator ESCAPE HATCH (design §6 `Escape.as_streaming`).
// Recover the richer `Streaming` capability from an existential handle when the
// backend actually supports it, or get a clear typed failure. This is the typed
// analog of `providerOptions` pass-through: you ask for the capability and get
// it back, or a `baml.errors.Unsupported` — a capability fact knowable BEFORE
// any byte, distinct from a mid-stream transport fault.
//
// The design returns `Streaming` on the StreamError *throw* channel; on the real
// surface `never` does not coerce to a bare interface type in a match-arm join
// (throwing that arm is rejected), so the negotiation failure is surfaced as a
// TYPED value in the return union instead of a throw — same "you get it or a
// clear typed Unsupported" contract, just as data rather than an exception.
// ---------------------------------------------------------------------------
function s28_as_streaming(p: baml.ai.Provider) -> baml.ai.Streaming | baml.errors.Unsupported throws never {
    match (p) {
        let s: baml.ai.Streaming => s,
        _                        => baml.errors.Unsupported { message: "backend does not support streaming" },
    }
}


// ---------------------------------------------------------------------------
// The registry can only PROMISE the bare marker, but an app that wants graded
// support reads it up front off the `Capabilities` lattice (scenario 36's
// three-valued Yes/No/Maybe) when the routed provider offers it — else it must
// negotiate at call time. This is the honest "route-by-string XOR static
// capability" cost (design §c / OQ-5) surfaced as a helper.
// ---------------------------------------------------------------------------
function s28_structured_support(p: baml.ai.Provider) -> baml.ai.Support throws never {
    match (p) {
        let c: baml.ai.Capabilities => c.structured_output(),
        _                           => baml.ai.Support.Maybe,  // not advertised -> probe at call time
    }
}
