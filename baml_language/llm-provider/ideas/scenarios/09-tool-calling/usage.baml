// ============================================================================
// 09 — Function / tool calling basics  ::  usage.baml  (APPLICATION AUTHOR code)
// ============================================================================
//
// The promise: the app author declares tools ONCE, writes a function, and calls
// the `.run_tools` companion. Everything that diverges across OpenAI / Anthropic
// / Gemini (schema dialect, id-vs-name correlation, result threading, ordering)
// stays inside the provider, under the `Tools` seam. Swapping the client changes
// the provider — and nothing in the app code below.
// ============================================================================


// --- The tool's argument shape: an ordinary BAML class -> first-class `type` ---
class WeatherArgs {
  city: string
}

class Balance {
  amount: float
  currency: string
}

// --- The non-model state the model never sees (Pydantic-AI Deps shape). ---
//   Injected via ExecutionContext.deps; the model only ever supplies the
//   schema-typed `args`.
class BankingDeps {
  db_url: string
  customer_id: int
}

// The tools the model may call. `parameters` is a first-class `type` — the
// provider reflects it into JSON Schema (OpenAI/Anthropic) or the Gemini OpenAPI
// subset, the app author never writes a schema by hand.
function WeatherTools() -> Tool[] {
  [
    Tool {
      name: "get_weather",
      description: "Get the current weather for a city.",
      parameters: reflect.type_of<WeatherArgs>(),
    },
  ]
}

function BankingTools() -> Tool[] {
  [
    Tool {
      name: "account_balance",
      description: "Look up the signed-in customer's balance. `include_pending` toggles pending transactions.",
      parameters: reflect.type_of<AccountBalanceArgs>(),
    },
  ]
}
class AccountBalanceArgs { include_pending: bool }


// ----------------------------------------------------------------------------
// CLIENTS — config only, sugar for `function Name() -> Provider`.
// Provider-specific params dissolve into fields: Anthropic.max_tokens is just a
// field; there is no options blob.
// ----------------------------------------------------------------------------
client Gpt() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

client Claude() {
  Anthropic { model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}

client Flash() {
  Gemini { model: "gemini-2.0-flash", api_key: env.GEMINI_API_KEY }
}

// Composition is a function call. ToolFallback forwards the Tools capability by
// runtime delegation (binds the transcript to whichever member answers).
client ResilientTools() {
  ToolFallback { strategy: [Gpt(), Claude(), Flash()] }
}


// ----------------------------------------------------------------------------
// FUNCTION + the `.run_tools` companion.
//
// `Weather(city)` is the plain `.call` form. `Weather.run_tools(city, tools, ctx)`
// is the capability companion. The companion `match`es the client's provider for
// the `Tools` capability; tool-calling has NO honest fallback to a single `.call`
// (you can't fake a multi-turn loop), so the `_` arm ERRORS.
// ----------------------------------------------------------------------------
function Weather(city: string) -> string {
  client: Gpt()
  prompt #"What's the weather in {{ city }}? Use your tools."#
}
// Generated companion (shown for clarity; normally compiler-synthesized):
//
//   function Weather$run_tools(city: string, tools: Tool[], ctx: ExecutionContext) -> string {
//     let p = Gpt();
//     match (p) {
//       let tp: Tools => tp.run_tools<string>(render_prompt(city), tools, ctx),
//       _             => throw baml.errors.Unsupported { message: "client cannot run tools" },
//     }
//   }
//
//   call site:   Weather.run_tools("Tokyo", WeatherTools(), ctx)


// Same app code, a DIFFERENT provider with maximal wire divergence (Gemini: no
// ids, OpenAPI-subset schema, functionResponse threading). Nothing changes here.
function WeatherGemini(city: string) -> string {
  client: Flash()
  prompt #"What's the weather in {{ city }}? Use your tools."#
}

// And the portable form: the resilient client tries GPT -> Claude -> Gemini, each
// with its own dialect/threading, all under the seam.
function WeatherResilient(city: string) -> string {
  client: ResilientTools()
  prompt #"What's the weather in {{ city }}? Use your tools."#
}


// ----------------------------------------------------------------------------
// Dependency injection: the tool needs a DB + customer id the model never sees.
// `deps` rides on ExecutionContext; `dispatch` validates the model's `args`
// (`include_pending`) against AccountBalanceArgs and injects `deps` out-of-band.
// ----------------------------------------------------------------------------
function CustomerSupport(question: string) -> string {
  client: Claude()
  prompt #"Answer the customer: {{ question }}. Use tools for account data."#
}

// A host-side entry point an app would write to wire deps + tools and drive the loop.
// (Illustrative: the `ctx` carrying `deps` is constructed at the call boundary.)
function RunSupport(question: string, customer_id: int) -> string throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let ctx = ExecutionContext {
    function_name: "CustomerSupport",
    args: { "question": question },
    deps: BankingDeps { db_url: env.BANK_DB_URL, customer_id: customer_id },
  };
  // .run_tools drives begin/step/submit; `dispatch` runs `account_balance`,
  // injecting ctx.deps, returning a ToolResult whose id the provider correlates
  // back to the call (by id for Anthropic, by synthesized name+pos for Gemini).
  CustomerSupport.run_tools(question, BankingTools(), ctx)
}


// ----------------------------------------------------------------------------
// Custom dispatch policy WITHOUT touching threading: override only the loop body
// to add per-call approval/budget. begin/step/submit (and all wire correlation)
// are untouched — this is the proposal's claim that overriding `run_tools`
// changes dispatch POLICY only.
// ----------------------------------------------------------------------------
function ApprovedSupport(question: string, ctx: ExecutionContext) -> string throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let p = Claude();
  match (p) {
    let tp: Tools => {
      let t = tp.begin<string>(render_prompt(question), BankingTools());
      while (true) {
        match (tp.step<string>(t)) {
          let calls: ToolCalls => {
            // app policy: approve each call before dispatch; threading untouched.
            for (let c in calls.calls) {
              if (!app_approve(c.name, c.args)) {
                throw ToolCallDenied { tool_name: c.name };
              }
            }
            t = tp.submit(t, ctx.dispatch(calls.calls));
          },
          let value: string => { return value; },
        }
      }
      baml.sys.panic("unreachable")
    },
    _ => throw baml.errors.Unsupported { message: "client cannot run tools" },
  }
}
function app_approve(name: string, args: map<string, unknown>) -> bool { $rust_io_function }
