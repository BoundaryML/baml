// ============================================================================
// Scenario 01 — Plain text in / text out
//   usage.baml — APPLICATION-author code. End-user surface.
//
// Goal the app author has: "send some text, get text back." This file shows:
//   - how little a one-shot client/function is,
//   - that the SAME function body works across Anthropic/OpenAI/Gemini
//     despite the three system-prompt placements and the assistant/"model"
//     role split (all absorbed inside the providers — §implement.baml),
//   - how usage is read via the .metered companion (a capability match),
//   - how a refusal surfaces as a typed channel the app branches on,
//   - how a combinator chain reads.
// ============================================================================


// --- One-shot clients. `client` is sugar for `function -> Provider`. --------
// Provider-specific params are just fields. max_tokens is REQUIRED on Anthropic
// and the app author simply sets it; on OpenAI it is optional and omitted.

client Claude() {
  Anthropic {
    model: "claude-opus-4-8",
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 1024,            // required field of THIS provider
    temperature: 0.2,           // 0..1 on Anthropic — the app author owns the range
  }
}

client GPT() {
  OpenAI {
    model: "gpt-4.1",
    api_key: env.OPENAI_API_KEY,
    temperature: 0.7,           // 0..2 on OpenAI — different range, same field name
    // max_tokens omitted: optional on this provider
  }
}

client Flash() {
  Gemini {
    model: "gemini-2.5-flash",
    api_key: env.GEMINI_API_KEY,
    temperature: 0.7,
  }
}


// --- The actual single-turn function. ---------------------------------------
// This is the whole "text in / text out" surface. `render_prompt` produces a
// provider-agnostic PromptAst from the template (system + user turns); the
// provider re-shapes it. The function body is identical no matter which client
// is wired in — that's the portability claim.

function Summarize(work: string) -> string {
  client: Claude()                         // swap to GPT() / Flash() unchanged
  prompt #"
    {{ _.role("system") }}
    You are a terse assistant.

    {{ _.role("user") }}
    Summarize the plot of {{ work }} in one line.
  "#
}

// Swapping the provider is a one-line edit; the body above never changes. To
// prove it, the same prompt against two divergent providers:
function SummarizeViaGpt(work: string) -> string {
  client: GPT()
  prompt #"
    {{ _.role("system") }}
    You are a terse assistant.

    {{ _.role("user") }}
    Summarize the plot of {{ work }} in one line.
  "#
}


// --- Usage: the .metered companion. -----------------------------------------
// `Summarize(work)` uses `.call` and discards usage. When the app wants tokens,
// it calls the `.metered` companion. That companion `match`es the client's
// provider for the Metered capability and, if present, returns value + usage.
// Because all three providers in implement.baml implement Metered, this works
// for any of them. If a provider could NOT report usage, the `_` arm decides
// what "no usage" means (here: zeros, since usage is a refinement of call).
//
// Companion desugaring (what the compiler generates for `.metered`):
function Summarize$metered(work: string) -> Metered_Result<string> {
  let p = Claude();
  let prompt = render_prompt("Summarize", { work: work });
  match (p) {
    let m: Metered => m.call_metered<string>(prompt),
    _ => Metered_Result<string> {           // degrade: still answer, just no tokens
      value: p.call<string>(prompt),
      usage: Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 },
    },
  }
}

// App code reading usage for cost accounting:
function SummarizeWithCost(work: string) -> string throws baml.errors.LlmClient {
  let r = Summarize$metered(work);
  baml.sys.log("tokens in=" + r.usage.input_tokens + " out=" + r.usage.output_tokens);
  r.value
}


// --- Refusals: a refusal is not the answer. ---------------------------------
// `Summarize` throws `Refused` (the typed channel from implement.baml) when the
// model declines. The app branches on it instead of getting an empty string.

function SummarizeSafe(work: string) -> string {
  Summarize(work) catch (e) {
    let r: Refused => "[declined: " + r.category + "]",   // distinct from a real answer
    _              => "[error]",
  }
}


// --- Combinator chain reads. ------------------------------------------------
// Orchestration is NOT a provider field — it wraps the provider. Retry/fallback
// are the inherited default methods on Provider. A resilient client reads
// left-to-right; the function body is still the plain single-turn one.

client Resilient() {
  Claude().with_retry(2).fallback_to(GPT())      // try Claude twice, then GPT
}

function SummarizeResilient(work: string) -> string {
  client: Resilient()
  prompt #"
    {{ _.role("system") }}
    You are a terse assistant.

    {{ _.role("user") }}
    Summarize the plot of {{ work }} in one line.
  "#
}
