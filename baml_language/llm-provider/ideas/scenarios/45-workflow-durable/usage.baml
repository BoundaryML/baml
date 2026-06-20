// ─────────────────────────────────────────────────────────────────────────────
// Durable execution — application-author code.
//
// A billing workflow that survives crashes: extract intent from a free-text
// request, then charge a card. Two providers to prove portability: Anthropic and
// OpenAI (background shows their divergence in build_request — id-vs-name tool
// correlation, response_format vs forced-tool encoding). The durability story is
// identical across both because `Durable` wraps the existential `Provider`.
// ─────────────────────────────────────────────────────────────────────────────

class BillingRequest { customer: string, prompt: string }
class Intent  { amount_cents: int, reason: string }
class Receipt { charge_id: string, amount_cents: int }

// Plain clients — config only. The durability combinator is NOT baked into the
// client, because the step log and coordinate only exist once a run is in
// flight (the engine assigns the run id). So the client stays portable across
// engines; durability is applied at the call site inside the workflow body.
client Claude() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}
client GPT() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

// The model functions are ordinary. They name a client; the durability wrapper
// is layered in by the workflow, not here — so these functions are equally
// usable in a non-durable context.
function ExtractIntent(req: BillingRequest) -> Intent {
  client: Claude()
  prompt #"
    Extract the billing intent from this request.
    Customer: {{ req.customer }}
    {{ req.prompt }}
  "#
}

// ─────────────────────────────────────────────────────────────────────────────
// The durable workflow body.
//
// DETERMINISM RULES (background §4): the body must be replay-safe — no wall
// clock, no rng, no direct I/O in the orchestration code. All nondeterminism is
// pushed into checkpointed steps. The two model-shaped sources of nondeterminism
// here are: (1) the LLM call, made replay-safe by `Durable`; (2) the charge tool
// side effect, made replay-safe by `DurableDispatch` + an idempotency key.
//
// `log` and `run` come from the engine; the body itself reads neither clock nor
// random — it just names step coordinates.
// ─────────────────────────────────────────────────────────────────────────────

function BillingWorkflow(req: BillingRequest, log: StepLog, run: string) -> Receipt
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // Step 1: the model call, wrapped so it is RECORDED-and-replayed, never
  // re-sampled. On first run it samples once and records; on replay after a
  // crash it returns the recorded Intent without touching Anthropic.
  //
  // We reach past the `client` sugar deliberately: the sugar returns the bare
  // provider, and we wrap it. `Claude()` is a function returning Provider, so
  // `.durable(...)` is just the combinator method from DurableExt.
  let extract_coord: StepCoord = StepCoord { run: run, step: "extract" };
  let intent: Intent =
    Claude()
      .durable(log, extract_coord)
      .call<Intent>(render_intent_prompt(req));

  // Step 2: the side-effecting charge. This is a TOOL step, not a model step —
  // the danger is double-charging, not re-sampling. We run the tool through a
  // DurableDispatch so a replay returns the recorded charge id instead of
  // charging again, AND we pass an idempotency key into the tool for the
  // crash-after-effect-before-record window (belt and suspenders).
  let charge_coord: StepCoord = StepCoord { run: run, step: "charge" };
  let prior: string? = log.recorded(charge_coord);
  let receipt: Receipt =
    match (prior) {
      let hit: string => durable_decode<Receipt>(durable_envelope_body(hit)),
      null => {
        // idempotency key tied to (run, step) — survives replay, dedupes at Stripe.
        let key: string = run + ":charge";
        let r: Receipt = charge_card(req.customer, intent.amount_cents, key);
        log.record(charge_coord, durable_envelope_new("charge", durable_encode<Receipt>(r)));
        r
      }
    };

  receipt
} catch (e) {
  // Foreign step-log Io from the charge step normalizes into UnknownError on the
  // call channel; a known CallError (the durable step's NonDeterministicReplay,
  // the provider's typed errors) passes through unchanged.
  _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "billing workflow run '" + run + "' failed");
}

// ─────────────────────────────────────────────────────────────────────────────
// Portability check: same workflow shape against OpenAI, and an agentic variant
// where the charge is a TOOL the model calls (so DurableDispatch is exercised).
// ─────────────────────────────────────────────────────────────────────────────

function AgenticBillingWorkflow(req: BillingRequest, log: StepLog, run: string) -> Intent
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  // Tools-capable client. The model decides when to charge; the loop dispatches
  // tools through DurableDispatch so each tool call is a checkpointed,
  // idempotent step. Replay re-enters the loop but recorded tool calls are
  // replayed, not re-invoked.
  let p = GPT();                                    // OpenAI: id-keyed tool correlation
  let ctx: ExecutionContext = billing_tool_router();
  let durable_ctx: DurableDispatch = DurableDispatch { inner: ctx, log: log, run: run };

  // Wrap the dispatch side, then run the provider's own tool loop. We hand the
  // durable dispatcher to run_tools as the ExecutionContext-shaped router.
  match (p) {
    let t: Tools => t.run_tools<Intent>(render_intent_prompt(req), billing_tools(), as_ctx(durable_ctx)),
    _ => throw baml.errors.Unsupported { message: "client is not tool-capable" },
  }
}

// ── helpers the app author already has ───────────────────────────────────────
function render_intent_prompt(req: BillingRequest) -> baml.llm.PromptAst { $rust_io_function }
function charge_card(customer: string, amount_cents: int, idempotency_key: string) -> Receipt { $rust_io_function }
function billing_tool_router() -> ExecutionContext { $rust_io_function }
function billing_tools() -> Tool[] { $rust_io_function }
function as_ctx(d: DurableDispatch) -> ExecutionContext { $rust_io_function }   // adapt DurableDispatch into the ctx seam
