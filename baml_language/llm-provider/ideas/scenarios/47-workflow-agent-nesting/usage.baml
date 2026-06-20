// =============================================================================
// usage.baml — APPLICATION author code.
//
// Two nestings, both directions, end to end:
//
//   (A) agent-as-step: a support workflow whose `triage` step is a full agent
//       loop, run as ONE checkpointed durable activity (Mastra's createStep with
//       supportAgent.generate inside — background §6).
//
//   (B) workflow-as-tool: an onboarding agent handed an entire durable
//       `provision` workflow as a single tool (Pydantic AI's @agent.tool that
//       runs a graph — background §6).
//
// Two providers with documented divergence are used to prove portability:
//   - Anthropic (forced-tool structured output, positional tool_result order)
//   - OpenAI    (response_format / function-call structured output, id-keyed)
// Both are just `Tools` providers; the nesting code never branches on which.
// =============================================================================

// -----------------------------------------------------------------------------
// The two agent clients. Plain config — a client is sugar for a function
// returning a Provider (proposal §3). Both providers implement Tools.
// -----------------------------------------------------------------------------

client ClaudeAgent() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
}

client GptAgent() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}


// =============================================================================
// (A) AGENT-AS-STEP — an agent loop is one durable workflow step.
// =============================================================================

class Ticket  { body: string }
class Triage  { category: string, urgency: int }

// The tools the triage agent may use during its loop (e.g. look up the customer).
function triage_tools() -> Tool[] {
  [ Tool { name: "lookup_customer", description: "fetch the customer record by email",
           parameters: reflect.type_of<CustomerQuery>() } ]
}
class CustomerQuery { email: string }

// The support workflow. `ctx: DurableCtx` is the engine handle, threaded in by
// whoever runs the graph. Each step is a checkpointed activity. The triage step's
// body is a WHOLE AGENT LOOP, fenced behind ONE Durable activity: on replay the
// agent's model calls never re-run — the recorded `Triage` is returned verbatim.
class SupportWorkflow {
  implements Workflow {
    type Input = Ticket
    type Output = string            // the resolved reply

    function run(self, input: Ticket, ctx: DurableCtx) -> string
        throws baml.ExtendUnknownError<baml.errors.WorkflowError> {
      // ---- Step 1: triage — an AGENT, run as a single durable, cost-rolled step.
      // The provider could be either ClaudeAgent() or GptAgent(); we pick Claude
      // here. The nesting machinery does not care which — Anthropic's forced-tool
      // structured-output encoding and positional tool_result threading all stay
      // inside Anthropic's Tools impl, below the ToolCall/ToolResult seam.
      let agent: Tools = match (ClaudeAgent()) {
        let t: Tools => t,
        // Capability negotiation: this client's provider does not implement Tools.
        // Unsupported is legal on the WorkflowError channel (it implements every
        // capability-error interface).
        _ => { throw baml.errors.Unsupported { message: "triage client is not a Tools agent" }; },
      };

      let step = DurableAgentStep {
        agent: agent,
        ctx: ctx,
        step_id: "triage",                    // STABLE key — replay-safe
        tools: triage_tools(),
        price: Price { input_per_mtok: 3.0, output_per_mtok: 15.0, cache_read_per_mtok: 0.3 },
        parent_span: null,
      };

      // The agent step is a value+sidecar boundary: `run_with` hands back the
      // typed `Triage` AND a projected sidecar as a TUPLE. We project the rolled-up
      // usage and the replayed-echo bit off the DurableMeta (a ResponseMeta) — no
      // bespoke StepRecord companion. Same code on first run and on replay; only
      // `was_replayed` flips.
      let (triage, meta): (Triage, (Usage, bool)) =
        step.run_with<Triage, (Usage, bool)>(
          render_prompt({ "ticket": input.body }),
          app_exec(),
          m => (m.usage(), match (m) { let r: Replayable => r.replayed(), _ => false }),
        );
      let (rolled_usage, was_replayed) = meta;     // tuple destructuring (net-new surface)

      // ---- Step 2..N: a deterministic branch on the agent's recorded output.
      // This is the outer graph's determinism: same `triage.category` -> same path.
      if (triage.category == "billing") {
        durable_activity(ctx, "billing.reply", () => { handle_billing(input) })
      } else {
        durable_activity(ctx, "tech.reply", () => { handle_tech(input) })
      }
    }
  }
}

// Deterministic leaf steps (no model) — ordinary durable activities.
function handle_billing(t: Ticket) -> string { "billing team will follow up: " + t.body }
function handle_tech(t: Ticket) -> string     { "engineering will investigate: " + t.body }

// The app's tool-dispatch context (proposal §7). app_exec() wires lookup_customer
// to real code; with_meter is added by the rollup at the step boundary.
function app_exec() -> ExecutionContext { $rust_io_function }


// =============================================================================
// (B) WORKFLOW-AS-TOOL — a whole durable workflow handed to a model as one tool.
// =============================================================================

class Provision  { customer_id: string }
class Provisioned { account_url: string }

// A durable provisioning graph: create account -> seed config -> send welcome.
// Each step is its own checkpointed activity. This is the kind of multi-step
// durable process you do NOT want a model improvising — so the model only
// decides WHETHER to run it; the graph decides HOW.
class ProvisionWorkflow {
  implements Workflow {
    type Input = Provision
    type Output = Provisioned

    function run(self, input: Provision, ctx: DurableCtx) -> Provisioned
        throws baml.ExtendUnknownError<baml.errors.WorkflowError> {
      let acct = durable_activity(ctx, "create_account", () => { input.customer_id + ".acct" });
      let _seed = durable_activity(ctx, "seed_config",   () => { acct + ":seeded" });
      let url   = durable_activity(ctx, "send_welcome",  () => { "https://app/" + acct });
      Provisioned { account_url: url }
    }
  }
}

// The onboarding agent. The model is given the ENTIRE provision workflow as one
// tool. When it calls "run_provisioning", the agent's tool dispatch invokes the
// WorkflowProvider, which decodes the args, runs the durable graph under a child
// journal, and hands the typed result back as a tool result — and the workflow's
// step tokens/cost roll up to the agent turn that called it.
//
// Note the provider DIVERGENCE here is invisible: we run this with GptAgent()
// (OpenAI, id-keyed tool calls); the same code runs with ClaudeAgent()
// (Anthropic, positional). The workflow-as-tool seam is provider-agnostic
// because Tool / ToolCall / ToolResult are the canonical neutral vocabulary.
function onboarding_tools(ctx: DurableCtx) -> Tool[] {
  let wf = ProvisionWorkflow {};
  [ wf.as_tool("run_provisioning", "Provision a new customer account end to end.") ]
}

// Dispatch: when the model calls run_provisioning, route it to the workflow
// adapter. This is the bridge between the two nondeterminism models — the model
// chose to call it (nondeterministic); the body runs deterministically/durably.
function onboarding_dispatch(ctx: DurableCtx, call: ToolCall) -> ToolResult
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  if (call.name == "run_provisioning") {
    // call/call_with live on the HttpProvider capability now (../provider-as-marker.md);
    // a workflow-as-tool is request/response, so it IS an HttpProvider. Bind the
    // capability type, not the bare `Provider` marker (which has no `call`).
    let provider: HttpProvider = WorkflowProvider { wf: ProvisionWorkflow {}, ctx: ctx };
    // The workflow runs as a provider call; its Output comes back typed.
    let out: Provisioned = provider.call<Provisioned>(call_as_prompt(call));
    ToolResult { id: call.id, output: out }          // id correlation is the agent's job
  } else {
    // The model asked for a tool this dispatcher does not serve — a genuine
    // tool-dispatch failure on the ToolError channel.
    throw UnknownToolError { name: call.name };
  }
} catch (e) {
  // provider.call surfaces CallErrors here, foreign to the ToolError channel this
  // dispatcher presents to the agent loop; normalize them (UnknownToolError, a
  // known ToolError, passes through unchanged).
  _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "onboarding tool dispatch failed");
}

// A tool the agent requested that this dispatcher does not serve. A concrete
// ToolError so the app can branch on it distinctly from a transport failure.
class UnknownToolError {
  name: string
  implements baml.errors.ToolError {
    function is_unknown_tool(self) -> bool { return true; }
  }
}
function call_as_prompt(call: ToolCall) -> baml.llm.PromptAst { $rust_io_function }

// The onboarding agent function. It is an ordinary Tools agent; the workflow is
// just one of its tools. run_tools drives the loop; our dispatch routes the
// workflow tool to the durable graph.
function Onboard(goal: string, ctx: DurableCtx) -> string
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let agent: Tools = match (GptAgent()) {
    let t: Tools => t,
    // Capability negotiation: legal on the ToolError channel (Unsupported
    // implements every capability-error interface).
    _ => { throw baml.errors.Unsupported { message: "onboarding client is not a Tools agent" }; },
  };
  agent.run_tools<string>(
    render_prompt({ "goal": goal }),
    onboarding_tools(ctx),
    app_exec_with_dispatch(ctx),       // routes run_provisioning -> onboarding_dispatch
  )
}
function app_exec_with_dispatch(ctx: DurableCtx) -> ExecutionContext { $rust_io_function }


// =============================================================================
// (C) THE COMPOSITION: both directions at once.
//
//   An OUTER support workflow has an agent step (A); that agent, mid-loop, can
//   reach for the provision workflow as a tool (B). The agent step is ONE durable
//   activity from the outer graph's view; the provision workflow it invokes is a
//   CHILD durable scope. The whole thing replays deterministically because every
//   nondeterministic model call is fenced inside `Durable`/`DurableAgentStep`.
//
//   The app author writes almost nothing for the nesting itself: a `match` to
//   assert the Tools capability, a DurableAgentStep with a stable step_id, and
//   `as_tool` to expose a workflow. The four structured-output encodings, the
//   id-vs-name correlation, and the record/replay journal are all library code.
// =============================================================================
function ResolveTicket(ticket: string, ctx: DurableCtx) -> string
    throws baml.ExtendUnknownError<baml.errors.WorkflowError> {
  let wf = SupportWorkflow {};
  wf.run(Ticket { body: ticket }, ctx)
}

// Top-level consumer: catch by runtime match, most-specific first. The new
// WorkflowError channel triages exactly like the core channels — concrete durable
// errors first (DurableDecodeError), then the capability interface (use its
// classifiers), then baml.UnknownError as the escape hatch. Note baml.errors.
// Unsupported (the Tools-negotiation failure inside the workflow) is concrete, so
// it is caught by its own type before the interface arm.
function ResolveTicketSafe(ticket: string, ctx: DurableCtx) -> string {
  ResolveTicket(ticket, ctx) catch (e) {
    let u: baml.errors.Unsupported => "[no agent capability: " + u.message + "]";
    let d: DurableDecodeError      => "[journal mismatch at step " + d.step_id + ": " + d.detail + "]";
    let w: baml.errors.WorkflowError => {
      if (w.is_replay_mismatch()) { "[non-deterministic replay; manual recovery needed]" }
      else { "[workflow failed]" }
    };
    let x: baml.UnknownError       => "[unexpected: " + x.message.join("; ") + "]";
  }
}
