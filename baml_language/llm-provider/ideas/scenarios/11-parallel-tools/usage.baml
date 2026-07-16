// =============================================================================
// 11 — Parallel tool calls  ::  usage.baml  (APPLICATION author code)
// =============================================================================
//
// What the app author writes to get concurrent tool dispatch. The headline:
// they DO NOT touch a provider, a transcript, or any wire code. Choosing
// "parallel" is choosing a `Dispatcher` and handing it to the run, exactly like
// choosing retry/fallback is choosing a combinator. Portability test: the SAME
// app code runs over Anthropic (positional results) and Gemini (no ids) — the
// divergence is entirely below the seam, so usage doesn't change.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. The tools. The ONLY new thing the app author writes for parallelism is the
//    `effect:` annotation — declaring which tools are safe to fan out. Omitting
//    it defaults to Write (sequenced), which is the safe failure mode.
// -----------------------------------------------------------------------------
class SearchArgs  { query: string }
class WeatherArgs { city: string }
class ChargeArgs  { account: string, cents: int }
class EmailArgs   { to: string, body: string }

function search_tool() -> Tool {
  Tool {
    name: "search_web",
    description: "Search the web for a query.",
    parameters: SearchArgs,
    effect: ToolEffect::ReadOnly,        // pure read — safe to parallelize
  }
}
function weather_tool() -> Tool {
  Tool {
    name: "get_weather",
    description: "Current weather for a city.",
    parameters: WeatherArgs,
    effect: ToolEffect::ReadOnly,        // pure read
  }
}
function charge_tool() -> Tool {
  Tool {
    name: "charge_card",
    description: "Charge a customer's card.",
    parameters: ChargeArgs,
    // no effect: => defaults to Write => the dispatcher will SEQUENCE it
  }
}
function email_tool() -> Tool {
  Tool {
    name: "send_email",
    description: "Send an email.",
    parameters: EmailArgs,              // also defaults to Write
  }
}


// -----------------------------------------------------------------------------
// 2. The clients. Pure config — provider fields only. Note Anthropic's `max_tokens`
//    is just a field (no options blob). These are the two divergent providers we
//    prove portability across.
// -----------------------------------------------------------------------------
client Claude() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
}

client Flash() {
  Gemini { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY }
}


// -----------------------------------------------------------------------------
// 3. Building an ExecutionContext with a chosen dispatch strategy.
//
// The app registers handlers (host-side), then picks ONE Dispatcher. This is the
// entire surface area of "make my tools run in parallel": swap the strategy field.
// -----------------------------------------------------------------------------

// Host-side: register the function bodies behind each tool name + bind deps.
function build_registry() -> Registry { $rust_io_function }

function parallel_ctx() -> ExecutionContext {
  ExecutionContext {
    function_name: "Research",
    args: {},
    registry: build_registry(),
    dispatcher: ParallelDispatch {},           // run-all, concurrent, order-preserving
  }
}

function effect_aware_ctx() -> ExecutionContext {
  ExecutionContext {
    function_name: "Checkout",
    args: {},
    registry: build_registry(),
    dispatcher: EffectAwareDispatch {},        // fan out reads, sequence writes
  }
}

function limited_ctx(max_concurrent: int) -> ExecutionContext {
  ExecutionContext {
    function_name: "Research",
    args: {},
    registry: build_registry(),
    dispatcher: LimitedDispatch { limit: max_concurrent },   // TaskGroup-capped fan-out
  }
}

function strict_ctx() -> ExecutionContext {
  ExecutionContext {
    function_name: "Pipeline",
    args: {},
    registry: build_registry(),
    dispatcher: StopOnFirstErrorDispatch {},   // first failure aborts the turn
  }
}


// -----------------------------------------------------------------------------
// 4. The functions. Identical shape to scenario 09's tool function: a prompt and
//    a client. The `.run_tools` companion (from the Tools capability) takes the
//    tools and the ctx. Parallelism is invisible at THIS layer — it rides in ctx.
// -----------------------------------------------------------------------------

class ResearchReport { summary: string, sources: string[] }

// Read-only fan-out. The model will likely emit several search_web + get_weather
// calls in one turn; ParallelDispatch runs them concurrently and feeds them back
// in original order. Works identically over Claude (positional) and Flash (no ids).
function Research(question: string) -> ResearchReport {
  client: Claude()
  prompt #"
    Answer using the tools. You may call multiple tools at once.
    Question: {{ question }}
  "#
}

// Same function body, DIFFERENT provider — the portability proof. The app code is
// byte-for-byte the parallel pattern; Gemini's id-lessness is handled below the seam.
function ResearchG(question: string) -> ResearchReport {
  client: Flash()
  prompt #"
    Answer using the tools. You may call multiple tools at once.
    Question: {{ question }}
  "#
}

class Receipt { ok: bool, note: string }

// A turn that may emit BOTH a read (search) and a write (charge_card / send_email).
// EffectAwareDispatch fans out the reads but sequences the writes — the app author
// expresses that purely by the `effect:` annotations on the tools, nothing here.
function Checkout(instruction: string) -> Receipt {
  client: Claude()
  prompt #"{{ instruction }}"#
}


// -----------------------------------------------------------------------------
// 5. The call sites — how the companion + ctx read at the application's edge.
//
// `.run_tools(args, tools, ctx)` is the Tools companion. The ctx carries the
// dispatch strategy. Swapping parallel <-> sequential <-> limited is ONE field.
// -----------------------------------------------------------------------------

function run_research_parallel(q: string) -> ResearchReport {
  // run-all concurrent fan-out over Anthropic.
  Research.run_tools(q, [search_tool(), weather_tool()], parallel_ctx())
}

function run_research_gemini(q: string) -> ResearchReport {
  // same call, Gemini. Same-function fan-out (two search_web) is correlated by the
  // provider's synthetic ids — the app never sees that.
  ResearchG.run_tools(q, [search_tool(), weather_tool()], parallel_ctx())
}

function run_research_rate_limited(q: string) -> ResearchReport {
  // cap to 3 concurrent tool executions to respect a downstream API rate limit.
  Research.run_tools(q, [search_tool(), weather_tool()], limited_ctx(3))
}

function run_checkout(instruction: string) -> Receipt {
  // reads fan out; charge_card / send_email run sequentially (effect-gated).
  Checkout.run_tools(instruction, [search_tool(), charge_tool(), email_tool()], effect_aware_ctx())
}

function run_strict_pipeline(q: string) -> ResearchReport {
  // stop-on-first-error: a single failed tool aborts the whole turn (throws).
  Research.run_tools(q, [search_tool(), weather_tool()], strict_ctx()) catch (e) {
    let err: baml.errors.ToolError => {
      // a tool failed and we cancelled the rest; surface a degraded report.
      ResearchReport { summary: "aborted: a tool failed", sources: [] }
    },
    let u: baml.UnknownError => {
      // a foreign/transport failure bubbled up through the tool loop.
      ResearchReport { summary: "aborted: a tool failed", sources: [] }
    },
  }
}


// -----------------------------------------------------------------------------
// 6. Composition: parallel dispatch under a retry/fallback combinator chain.
//
// The dispatcher (parallel) lives in ctx; reliability (retry/fallback) lives in the
// client combinator chain. They are ORTHOGONAL and compose without knowing about
// each other — the client returns a Provider, the ctx carries dispatch policy.
// -----------------------------------------------------------------------------
client ResilientTools() {
  Claude().fallback_to(Flash())          // both implement Tools; Fallback forwards it
}

function ResearchResilient(question: string) -> ResearchReport {
  client: ResilientTools()
  prompt #"Answer using tools; you may call several at once. {{ question }}"#
}

function run_resilient(q: string) -> ResearchReport {
  // parallel dispatch (ctx) + provider fallback (client). Independent axes.
  ResearchResilient.run_tools(q, [search_tool(), weather_tool()], parallel_ctx())
}
