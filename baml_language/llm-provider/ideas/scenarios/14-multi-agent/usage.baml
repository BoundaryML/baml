// ============================================================================
// 14-multi-agent / usage.baml   — APPLICATION AUTHOR CODE
// ============================================================================
//
// A support desk: a Triage router that hands off to Billing or TechSupport.
// We deliberately use TWO providers the background shows diverging on tools:
//   - Anthropic : id-correlated tool calls, positional tool_result ordering
//   - Gemini    : NO tool-call ids; can't do tools + structured output at once
// Both implement `Tools` (their own threading lives inside begin/step/submit),
// so the Handoff combinator sees only the seam — proving portability holds at
// the multi-agent layer even though the wire formats differ.
//
// (Anthropic / Gemini / OpenAI provider classes are defined in the shared
//  scenarios — here we only construct them. Assume `Anthropic`, `Gemini`,
//  `OpenAI` each `implements Tools`.)

// --- leaf agents: each is a client (a Provider-factory) + its own tools -------

client BillingModel() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}

client TechModel() {
  // Gemini: same Tools seam, different wire (no ids) — handled inside its impl.
  Gemini { model: "gemini-2.0-flash", api_key: env.GEMINI_API_KEY }
}

client RouterModel() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 512 }
}

// Tools the leaf agents own. (Tool is the proposal's first-class `type`.)
function billing_tools() -> Tool[] {
  [
    Tool { name: "lookup_invoice", description: "Look up an invoice by id.", parameters: reflect.type_of<InvoiceQuery>() },
    Tool { name: "process_refund", description: "Issue a refund.", parameters: reflect.type_of<RefundReq>() },
  ]
}
function tech_tools() -> Tool[] {
  [
    Tool { name: "search_docs", description: "Search product docs.", parameters: reflect.type_of<DocQuery>() },
    Tool { name: "create_ticket", description: "File a support ticket.", parameters: reflect.type_of<TicketReq>() },
  ]
}

class InvoiceQuery { invoice_id: string }
class RefundReq    { invoice_id: string, amount: float }
class DocQuery     { query: string }
class TicketReq    { summary: string, severity: string }

// Bundle each leaf as an Agent (Provider + name + description + tools).
function billing_agent() -> Agent {
  Agent { name: "Billing",
          description: "Handles invoices, refunds, subscription changes.",
          provider: BillingModel(), tools: billing_tools() }
}
function tech_agent() -> Agent {
  Agent { name: "TechSupport",
          description: "Handles bugs, outages, and how-do-I questions.",
          provider: TechModel(), tools: tech_tools() }
}

// ============================================================================
// (A) HANDOFF — model-routed, provider-swapping
// ============================================================================
//
// The whole router-as-provider is ONE combinator construction. Note: the app
// author commits to a state-threading model (History here) and a depth cap —
// the two things the background says are uncapped/footgun by default.

client SupportDesk() {
  Handoff {
    router: Agent { name: "Triage",
                    description: "Route the user to the right specialist. Do not answer directly.",
                    provider: RouterModel(), tools: [] },
    targets: [billing_agent(), tech_agent()],
    threading: History { max_chars: 12000 },   // <-- the truncation policy the SDKs lack
    max_depth: 4,                              // <-- caps handoff ping-pong (max_turns can't)
  }
}

// A `Handoff` implements Tools, so the function uses the `.run_tools` companion.
// The app author writes essentially nothing beyond the prompt.
function HandleTicket(message: string) -> string {
  client: SupportDesk()
  prompt #"
    {{ _.role("user") }}
    {{ message }}
  "#
}
// call site:
//   HandleTicket.run_tools("I was charged twice for my subscription.", [])
// -> Triage(Anthropic) calls transfer_to_Billing -> swap to Billing(Anthropic),
//    threading the conversation in -> Billing runs lookup_invoice/process_refund.

// ============================================================================
// (B) SUB-AGENT-AS-TOOL — parent stays in control
// ============================================================================
//
// Here the parent does NOT hand off; it CALLS a translator sub-agent like any
// tool and uses the result. The child is a Gemini agent exposed as a Tool.

function translator_agent() -> Agent {
  Agent { name: "translate",
          description: "Translate text to English. Returns the translation only.",
          provider: TechModel(), tools: [] }
}

// The parent's tools include the sub-agent-as-tool descriptor.
function Concierge(message: string) -> string {
  client: BillingModel()
  // The app registers the sub-agent's handler on the execution context (see below).
  prompt #"
    {{ _.role("system") }}
    You help international customers. Use the `translate` tool for non-English input.
    {{ _.role("user") }}
    {{ message }}
  "#
}
// call site wiring (pseudo, app glue):
//   let at = AgentTool { child: translator_agent(), output_type: reflect.type_of<string>() };
//   let ctx = baml.agent.root_ctx().register(at.descriptor().name, call => at.handle(call, ctx));
//   Concierge.run_tools(message, [at.descriptor()], ctx)
//   -> parent(Anthropic, id-correlated) calls `translate` -> child(Gemini) runs
//      to completion in an ISOLATED ctx -> result returned as the tool result,
//      correlated by ToolCall.id (opaque to the loop).

// ============================================================================
// (C) DETERMINISTIC ORCHESTRATION — code decides; plain functions/combinators
// ============================================================================
//
// A draft->edit pipeline. No model decides routing. `Sequential` threads via
// shared state (output_key == agent name), exactly ADK's SequentialAgent.

function drafter_agent() -> Agent {
  Agent { name: "draft", description: "Write a one-paragraph draft.",
          provider: BillingModel(), tools: [] }
}
function editor_agent() -> Agent {
  // editor's prompt reads {draft} from shared state injected by Sequential.
  Agent { name: "final", description: "Polish the draft.",
          provider: BillingModel(), tools: [] }
}

client WriterPipeline() {
  Sequential { steps: [drafter_agent(), editor_agent()] }
}

// Sequential implements only Provider.call, so this is a plain function (no
// companion needed) — code-decided routing reads as ordinary BAML.
function WriteSection(topic: string) -> string {
  client: WriterPipeline()
  prompt #"{{ _.role("user") }} Write about: {{ topic }}"#
}

// Fan-out + loop variants, same shape:
client ResearchFanout() {
  Parallel { branches: [
    Agent { name: "web",    description: "Search the web.",      provider: TechModel(),    tools: tech_tools() },
    Agent { name: "internal", description: "Search internal kb.", provider: BillingModel(), tools: [] },
  ] }
}

client RefineLoop() {
  Loop {
    body: [
      Agent { name: "critic",  description: "Critique the draft.", provider: BillingModel(), tools: [] },
      Agent { name: "reviser", description: "Revise the draft.",   provider: BillingModel(), tools: [] },
    ],
    max_iterations: 3,
    until: out => baml.agent.looks_done(out),     // code-decided stop
  }
}

// ============================================================================
// Combinators compose with the std orchestration ones (retry/fallback) for free,
// because every construction above returns a `Provider`.
// ============================================================================
client SupportDeskResilient() {
  SupportDesk().with_retry(2).cached(env_kv())
}
function env_kv() -> KV { $rust_io_function }
