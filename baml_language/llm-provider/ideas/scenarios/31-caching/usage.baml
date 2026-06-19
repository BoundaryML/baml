// =============================================================================
// Caching — application-author code
// =============================================================================
//
// The ergonomics split exactly along the three shapes. For (A) automatic /
// inline / routing-hint caching, the app author writes almost nothing — it's a
// provider field or a one-line combinator. For (D) the Gemini explicit
// resource, the app author has to confront a lifecycle, and the code gets
// visibly longer.

// -----------------------------------------------------------------------------
// (C) OpenAI automatic prefix cache — zero work. Just set the routing hint.
// -----------------------------------------------------------------------------
// One field. The prefix (a large system prompt) gets cached automatically; the
// hint groups this tenant's traffic onto one cache machine. Nothing else.

client SupportGPT(tenant: string) {
  OpenAI {
    model: "gpt-4.1",
    api_key: env.OPENAI_API_KEY,
    prompt_cache_key: "tenant-" + tenant,    // the ONLY cache lever OpenAI gives us
  }
}

function DraftReply(tenant: string, ticket: string) -> string {
  client: SupportGPT(tenant)
  prompt #"
    {{ LARGE_SYSTEM_PROMPT }}
    Ticket: {{ ticket }}
    Draft a reply.
  "#
}
// DraftReply("acme", "...")  ->  .call ; cached_tokens > 0 after the first hit.

// -----------------------------------------------------------------------------
// (B) Anthropic inline annotation — name the breakpoints. Still just fields.
// -----------------------------------------------------------------------------
// The app declares WHICH stable segments to cache. The renderer must emit those
// segments byte-stably and at the front — that's the contract, enforced by
// putting the big guide in the system block and nothing volatile ahead of it.

client StyledClaude() {
  Anthropic {
    model: "claude-sonnet-4-5",
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 1024,
    cache_breakpoints: ["system"],           // cache everything up to the system block
    cache_ttl: "1h",                          // extended: worth it above the hit-rate threshold
  }
}

function DraftStyled(doc: string) -> string {
  client: StyledClaude()
  prompt #"
    {# system: the large stable style guide — byte-stable, at the front #}
    {{ LARGE_STYLE_GUIDE }}
    {# user turn — the only thing that changes per call #}
    Draft a reply to: {{ doc }}
  "#
}

// -----------------------------------------------------------------------------
// (A) Client-side response cache — the combinator, one method.
// -----------------------------------------------------------------------------
// Totally provider-agnostic because it never touches the wire. Useful for
// deterministic eval runs / fixtures: identical prompt -> skip the model.

client CachedEval() {
  OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY }
    .cached(RedisKV { url: env.REDIS_URL })   // .cached is a default method on Provider
}

function Classify(text: string) -> Label {
  client: CachedEval()
  prompt #"Classify: {{ text }}"#
}

// -----------------------------------------------------------------------------
// (D) Gemini explicit resource — the app MUST own the lifecycle. Longest code.
// -----------------------------------------------------------------------------
// This is the leak the proposal can't paper over. The app:
//   1. cannot use a plain `client` for the cached path — `client` is config-only
//      and returns the existential `Provider`, which erases `ManagedCache`.
//   2. has to drop to a function written longhand with a concrete/capability
//      return type to get the `create_cache` / `call_with_cache` methods.
//   3. has to own create + delete (the storage bill) explicitly.

// (1) A precise client: NOT the `client` sugar, because we need the ManagedCache
//     capability statically. This is the proposal's documented escape hatch
//     (§5: "drop the sugar, write -> ConcreteProvider").
function ContractAnalyst() -> Gemini {
  Gemini { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY }
}

// (2) A whole analysis session over ONE large contract, sharing one cache.
//     CacheScope guarantees the handle is deleted (and the storage bill stops)
//     even if a question throws.
function AnalyzeContract(contract: string, questions: string[]) -> string[] throws baml.errors.LlmClient {
  let gemini: Gemini = ContractAnalyst();

  CacheScope.with_cache<baml.llm.PromptAst, string[]>(
    gemini,
    // the reusable prefix: the whole contract, rendered once.
    baml.llm.render(#"You are a contract analyst. Document:\n{{ contract }}"#, { contract: contract }),
    3600,                                      // ttl seconds; storage billed while alive
    (provider, handle) => {
      for (let q in questions) {
        // each question references the handle; only `q` is new input.
        provider.call_with_cache<string>(handle, baml.llm.render(#"{{ q }}"#, { q: q }))
      }
    },
  )
}
// AnalyzeContract(big_pdf_text, ["termination clause?", "indemnity cap?"])
//   -> ONE create_cache, N cheap call_with_cache, ONE delete_cache.

// -----------------------------------------------------------------------------
// (E) Caching x fallback — the interaction the background flags.
// -----------------------------------------------------------------------------
// A cache prefix/handle is PROVIDER-LOCAL. So caching must sit INSIDE each
// fallback member, never wrap the Fallback. Each member keeps its own native
// caching; switching members re-pays the prefix (unavoidable — different vendor,
// different cache). The response-cache combinator (A) CAN wrap the whole thing,
// because it's vendor-neutral.

client ResilientCached(tenant: string) {
  // native prefix caching lives per-provider (inside each member):
  OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, prompt_cache_key: "t-" + tenant }
    .fallback_to(
      Anthropic {
        model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024,
        cache_breakpoints: ["system"], cache_ttl: "5m",
      }
    )
    // vendor-neutral response cache CAN wrap the fallback (it keys on the prompt,
    // not on any provider's cache):
    .cached(RedisKV { url: env.REDIS_URL })
}

function ResilientDraft(tenant: string, ticket: string) -> string {
  client: ResilientCached(tenant)
  prompt #"{{ LARGE_SYSTEM_PROMPT }}\nTicket: {{ ticket }}\nDraft a reply."#
}
// On the happy path: Redis miss -> OpenAI (prefix-cached) -> Redis put.
// On OpenAI failure: Fallback -> Anthropic (its OWN inline cache, cold) -> the
// OpenAI prefix cache is simply not used. The full prefix is re-paid on Claude.

// -----------------------------------------------------------------------------
// A trivial KV impl so the examples type-check.
// -----------------------------------------------------------------------------
class RedisKV {
  url: string
  implements KV {
    function get(self, key: string) -> string? throws baml.errors.Io { $rust_io_function }
    function put(self, key: string, value: string, ttl_secs: int) -> null throws baml.errors.Io { $rust_io_function }
  }
}

class Label { category: string, confidence: float }
