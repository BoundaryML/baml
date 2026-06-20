// ============================================================================
// Scenario 01 — Plain text in / text out
//   usage.baml — APPLICATION-author code. End-user surface.
//
// Goal the app author has: "send some text, get text back." This file shows:
//   - how little a one-shot client/function is,
//   - that the SAME function body works across Anthropic/OpenAI/Gemini
//     despite the three system-prompt placements and the assistant/"model"
//     role split (all absorbed inside the providers — §implement.baml),
//   - how usage is read via `.call_with` (a product sidecar projected from
//     the normalized ResponseMeta — returns a TUPLE (T, U)),
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


// --- Usage: the `.call_with` companion (a product sidecar). ------------------
// `Summarize(work)` uses `.call` and discards usage. When the app wants tokens,
// it calls `.call_with`, handing in a `project: (ResponseMeta) -> U` closure
// that reads exactly the dimension it wants off the normalized sidecar — here
// `m => m.usage()`. The result is a TUPLE `(T, U)`: the answer AND the usage.
// `call_with` lives on the BASE Provider, so EVERY provider has it and EVERY
// combinator forwards it — usage is not dropped under Fallback/Retry/Cache (the
// silent-zero-usage degrade the bespoke `Metered` companion suffered is gone).
// There is no capability `match` and no degrade arm: usage is not opt-in, it is
// a projection any ResponseMeta answers (a provider that genuinely can't report
// a dimension says so with `Unsupported`, not with a zeroed struct).
//
// Companion desugaring (what the compiler generates for `.call_with`):
function Summarize$call_with<U>(work: string, project: (ResponseMeta) -> U) -> (string, U)
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let p = Claude();
  let prompt = render_prompt("Summarize", { work: work });
  p.call_with<string, U>(prompt, project)
}

// App code reading usage for cost accounting. Note the tuple destructuring —
// `(T, U)` is net-new surface (../value-sidecar-model.md §6), flagged in
// evaluation.md §b.
function SummarizeWithCost(work: string) -> string
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let (answer, usage): (string, Usage) = Summarize.call_with(work, m => m.usage());
  baml.log.info("tokens in=" + usage.input_tokens.to_string() + " out=" + usage.output_tokens.to_string());
  answer
}

// Composing more than one sidecar is just a richer projection — still ONE call,
// still one tuple. logprobs come back `Unsupported` on all three chat providers
// here (typed "can't", not a silent zero), which the app can branch on.
function SummarizeWithUsageAndLogprobs(work: string) -> string
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let (answer, extras): (string, (Usage, Supported<Logprob[]>)) =
      Summarize.call_with(work, m => (m.usage(), m.logprobs()));
  match (extras.1) {
    let lp: Logprob[]   => baml.log.info("got " + lp.length().to_string() + " logprobs"),
    let u: Unsupported  => baml.log.info("logprobs unsupported: " + u.reason),
  }
  answer
}


// --- Refusals: a refusal is not the answer. ---------------------------------
// `Summarize` throws `Refused` (the typed channel from implement.baml) when the
// model declines. The app branches on it instead of getting an empty string.

function SummarizeSafe(work: string) -> string {
  Summarize(work) catch (e) {
    // Most-specific first: the concrete provider error, then the capability
    // interface (triaged via classifiers), then the UnknownError escape hatch.
    let r: Refused                => "[declined: " + r.category + "]",   // distinct from a real answer
    let c: baml.errors.CallError  => {                                   // any transport/HTTP failure
      if (c.is_rate_limit()) { "[rate-limited]" } else { "[error]" }
    },
    let u: baml.UnknownError      => "[error: " + u.message.join(" / ") + "]",
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
