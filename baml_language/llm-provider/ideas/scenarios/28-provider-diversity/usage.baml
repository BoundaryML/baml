// ============================================================================
// Scenario 28 — Provider diversity & gateways
//   usage.baml — APPLICATION-author code. End-user surface.
//
// What the app author wants: "call many models, possibly through a proxy or
// gateway, without rewriting per provider, and switch models by changing a
// string." This file shows:
//   - a proxy is the SAME provider with a different base_url (one line),
//   - auth-shape divergence is invisible to the app (it's a provider field),
//   - prefix routing / a registry is just a client that calls Registry.resolve,
//   - the function body NEVER changes across backends (the portability claim),
//   - the LCD trap + escape hatch read honestly in app code.
// ============================================================================


// --- One-shot clients. `client` is sugar for `function -> Provider`. --------
// Each is a one-liner because the named constructors in implement.baml encode
// the endpoint + auth shape. The app author never writes a Bearer/x-api-key/
// ?key= string — those live inside the provider.

client GPT() {
  OpenAIChat.openai("gpt-4.1")                  // Bearer, api.openai.com
}

client Claude() {
  Anthropic.anthropic("claude-sonnet-4-5")      // x-api-key + version header
}

client Flash() {
  Gemini.gemini("gemini-2.5-flash")             // ?key= query param
}


// --- A proxy / gateway is the SAME class, a different base_url. --------------
// The single most common interop trick in the wild, as ONE line of app config.
// Groq, Together, Ollama, an internal gateway — all OpenAIChat with a different
// base_url + key + (for gateways) routing headers. No new provider, no new fn.

client Groq() {
  // OpenAI-compatible: identical codec, different endpoint + key.
  OpenAIChat.compatible("llama-3.3-70b-versatile",
                        "https://api.groq.com/openai/v1", env.GROQ_API_KEY, {})
}

client LocalOllama() {
  OpenAIChat.compatible("qwen2.5", "http://localhost:11434/v1", "ollama", {})
}

client InternalGateway() {
  // A gateway that keys off a header. The header is part of ROUTING, so it's a
  // provider field, not a transport-envelope detail bolted on at the combinator.
  OpenAIChat.compatible("gpt-4.1", env.GATEWAY_URL, env.GATEWAY_KEY,
                        { "x-team": "search", "x-tenant": "team-a" })
}


// --- Auth-shape divergence is invisible to the app. -------------------------
// Same model family (Claude) on two clouds with two DIFFERENT auth shapes
// (x-api-key vs SigV4). The app picks a constructor; the divergence is a field.

client ClaudeDirect() {
  Anthropic.anthropic("claude-sonnet-4-5")              // x-api-key
}

client ClaudeOnBedrock() {
  Anthropic.on_bedrock("anthropic.claude-3-5-sonnet-20241022-v2:0", "us-east-1")  // SigV4
}


// --- The portability claim: ONE function body, swap the client line. --------
// `Profile` extracts a typed record. The body is identical no matter which of
// the clients above is wired in — OpenAI, Groq, local Ollama, Claude, Gemini,
// Bedrock. That is the entire "switch model by changing a string" goal, and the
// model delivers it because the wire/auth divergence is absorbed below `call`.

class Profile { name: string, title: string, years: int }

function Extract(bio: string) -> Profile {
  client: GPT()                 // <- swap to Groq() / LocalOllama() / Claude() / Flash(), body unchanged
  prompt #"
    {{ _.role("user") }}
    Extract a structured profile from this bio: {{ bio }}
  "#
}


// --- Prefix routing: switch backend by changing a STRING. -------------------
// `RoutedExtract(spec, bio)` resolves the provider from the spec string at call
// time. "anthropic/claude-sonnet-4-5", "internal/gpt-4.1", "groq/llama-3.3-70b",
// "bedrock/anthropic.claude-3-5-sonnet-...-v2:0" all flow through one function.
// The client expression is `Registry.resolve(spec)` — a Provider-typed value,
// which is exactly what `client:` accepts.

function RoutedExtract(spec: string, bio: string) -> Profile throws baml.errors.LlmClient {
  client: Registry.resolve(spec)        // a string -> a Provider; this is the bridge
  prompt #"
    {{ _.role("user") }}
    Extract a structured profile from this bio: {{ bio }}
  "#
}

// App code that does literal model-string switching, LiteLLM-style:
function ExtractMany(bio: string) -> Profile[] throws baml.errors.LlmClient {
  let specs = [
    "openai/gpt-4.1",
    "anthropic/claude-sonnet-4-5",
    "gemini/gemini-2.5-flash",
    "groq/llama-3.3-70b-versatile",
    "internal/gpt-4.1",
  ];
  let out: Profile[] = [];
  for (let s in specs) {
    out = baml.array.push(out, RoutedExtract(s, bio));
  }
  out
}


// --- A registry-backed client with a gateway default + per-tenant override. -
// `client` takes params, so a tenant-aware registry client is ordinary code.

client Smart(spec: string) {
  Registry.resolve(spec)
}

function TenantExtract(tenant: string, bio: string) -> Profile throws baml.errors.LlmClient {
  // enterprise tenants hit the internal gateway; everyone else hits a cheap proxy
  let spec = if (tenant == "enterprise") { "internal/gpt-4.1" } else { "groq/llama-3.3-70b-versatile" };
  client: Smart(spec)
  prompt #"
    {{ _.role("user") }}
    Extract a structured profile from this bio: {{ bio }}
  "#
}


// --- Round-robin across keys / endpoints (gateway-flavored load spread). -----
// Combinators are inherited, but RoundRobin needs a pool, so we build it
// explicitly. Two OpenAI keys + a Together fallback target, one shape.

client Balanced() {
  RoundRobin {
    pool: [
      OpenAIChat.openai("gpt-4.1"),                                          // key A (env default)
      OpenAIChat.compatible("gpt-4.1", "https://api.openai.com/v1", env.OPENAI_API_KEY_B, {}),  // key B
      OpenAIChat.compatible("llama-3.3-70b-versatile",                       // 3rd endpoint, same shape
                            "https://api.together.xyz/v1", env.TOGETHER_API_KEY, {}),
    ],
    cursor: baml.kv.counter("extract-pool"),
  }
}

function BalancedExtract(bio: string) -> Profile {
  client: Balanced()
  prompt #"
    {{ _.role("user") }}
    Extract a structured profile from this bio: {{ bio }}
  "#
}


// --- The lowest-common-denominator trap, made explicit in app code. ---------
// `Cheap` normalizes its backend to base-Provider only: streaming and any
// bespoke capability are ERASED by `Normalized`. This is the honest cost of a
// universal surface — you get portability and lose the ceiling.

client Cheap(spec: string) {
  Normalized { inner: Registry.resolve(spec) }
}

// This function can ONLY use `.call`. There is no `.stream` companion that will
// find a Streaming impl, because the client returns a Normalized handle.
function CheapExtract(spec: string, bio: string) -> Profile throws baml.errors.LlmClient {
  client: Cheap(spec)
  prompt #"
    {{ _.role("user") }}
    Extract a structured profile from this bio: {{ bio }}
  "#
}


// --- The escape hatch: reach past the normalized surface for a feature. ------
// When the app genuinely needs streaming AND wants registry routing, it must
// NOT normalize. It resolves the existential and asks for the capability,
// getting it or a clear typed failure — the typed analog of providerOptions.

function StreamIfPossible(spec: string, bio: string)
    -> baml.llm.Stream<Profile, Profile> throws baml.errors.LlmClient {
  let p = Registry.resolve(spec);              // existential Provider
  let s = Escape.as_streaming(p);              // recover Streaming or throw
  s.stream<Profile, Profile>(render_prompt("StreamIfPossible", { bio: bio }))
}
