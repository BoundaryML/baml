// =============================================================================
// implement.baml — Agents inside workflows, workflows inside agents
//
// STD-LIB / library-author code. Builds on the canonical primitives in
// ../_conventions.md (Provider, HttpProvider, Tools, ExecutionContext) and
// reuses the observability records from ../32-observability (Usage, Cost,
// UsageReporting, obs_*).
//
// The scenario nests in BOTH directions:
//
//   (A) AGENT-AS-STEP. A workflow step's body is an agent run — a Provider with
//       Tools driving a multi-turn loop. The outer graph is deterministic and
//       replayable; the inner loop is intrinsically nondeterministic. The
//       boundary is: run the whole agent loop as ONE checkpointed activity whose
//       recorded JSON output is replayed verbatim on re-execution, never
//       re-sampled. (background §6 "two nondeterminism models meet"; §"what's
//       hard": an agent loop spans multiple model calls and must still fit one
//       durable step.)
//
//   (B) WORKFLOW-AS-TOOL. A whole durable workflow is handed to a model as ONE
//       Tool. Because the proposal says "a client is a function returning a
//       Provider" and a workflow is also just a function, a workflow IS a
//       Provider: it has `call<T>(prompt) -> T`. We wrap it so the agent's tool
//       dispatch invokes it like any other tool, and its tokens/cost roll up.
//
// The teaching points are the two HARD things from background §6:
//   1. Determinism meets the nondeterministic loop -> a Durable combinator that
//      records-and-replays a single agent activity.
//   2. Trace + cost roll-up across the boundary -> a rollup that attributes
//      inner-agent tokens to the enclosing step's span.
//
// Net-new surface introduced here (flagged in evaluation.md):
//   - the `Workflow` interface (a durable graph; `run` + `as_tool`)
//   - the `Durable` combinator (records/replays a Provider call as an activity)
//   - the `DurableCtx` host seam (the checkpointer: record/replay journal)
//   - the `WorkflowProvider` adapter (a Workflow surfaced as a Provider)
//   - `rollup_usage` on the agent/durable boundary (cost attribution)
//   - host seams: baml.durable.* (journal), baml.json.* (canonical encode)
// =============================================================================


// -----------------------------------------------------------------------------
// 0. The checkpointer host seam.
//
//    This is the irreducible new host primitive. The base Provider model has
//    nowhere to express "this call already ran in a prior attempt; don't run it
//    again, hand me back what it returned". That is server/engine state
//    (Temporal history, DBOS step log, LangGraph checkpointer — background §7),
//    and it is NOT pure BAML. We model the engine as an opaque `DurableCtx`
//    threaded through the workflow, with exactly two operations the combinator
//    needs: a memoized `activity` and a deterministic clock/id source.
//
//    `activity` is record-and-replay: the FIRST time a (ctx, key) is seen the
//    body runs and its result is journaled; every replay returns the journaled
//    value WITHOUT running the body. The body is `() -> string` because the
//    journal can only hold serializable data (background §"suspend/resume is a
//    serialization problem" — no closures, no live sockets). The caller encodes
//    its typed result to a canonical string and decodes the replayed one.
// -----------------------------------------------------------------------------

class DurableCtx { _engine: $rust_type }     // Temporal/DBOS/Inngest handle, opaque

// Run `body` exactly once per (ctx, key) across all replays. On first execution
// the returned string is appended to the engine journal; on replay the body is
// skipped and the journaled string is returned. `key` must be stable across
// replays (it identifies the activity in the event history) — the workflow
// author supplies a step id, not a random value, exactly as Temporal requires.
function durable_activity(ctx: DurableCtx, key: string, body: () -> string) -> string {
  $rust_io_function
}

// A deterministic, replay-safe id (the engine returns the SAME id on replay).
// Used to mint idempotency keys inside an activity without breaking replay.
function durable_id(ctx: DurableCtx, key: string) -> string { $rust_io_function }

// Canonical JSON encode/decode — the journal serialization seam. `encode<T>`
// must be deterministic (sorted keys) so the recorded bytes are replay-stable.
function json_encode<T>(value: T) -> string { $rust_io_function }
function json_decode<T>(text: string) -> T throws baml.errors.Parse { $rust_io_function }


// -----------------------------------------------------------------------------
// 1. The `Durable` combinator. THIS is the agent/step boundary (direction A).
//
//    It wraps ANY Provider — most importantly a Tools agent — and turns one
//    `.call` (which for an agent is the ENTIRE multi-turn loop) into a single
//    record-and-replay activity. The whole loop becomes one journal entry: the
//    nondeterminism (every model turn) is fenced behind `durable_activity`, so a
//    replay never re-samples ANY of the agent's model calls — it returns the one
//    recorded final answer. That is precisely the line background §6 says is
//    "easy to get subtly wrong when the agent loop itself spans multiple model
//    calls": here it is impossible to get wrong, because the loop is opaque to
//    the engine — it is one activity body.
//
//    `Durable` is a plain (non-generic) combinator, like Fallback/Retry. It is
//    constructed with the engine ctx and a stable step id. It forwards `.call`
//    (and only `.call`): streaming/realtime are NOT replay-safe (you cannot
//    replay a live duplex), so we deliberately do NOT implement Streaming or
//    Realtime here — a durable step yields a value, not a live session.
// -----------------------------------------------------------------------------

class Durable {
  inner: Provider          // the agent (Tools) or any provider whose call we record
  ctx: DurableCtx          // the engine journal
  step_id: string          // stable activity key — author-supplied, replay-stable

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // Record-and-replay. On first run: execute inner.call (the FULL agent
      // loop), encode the result, journal it. On replay: durable_activity skips
      // the body and returns the journaled string; we decode it back to T. The
      // agent's model calls never execute on replay.
      let recorded: string = durable_activity(self.ctx, self.step_id, () => {
        let value: T = self.inner.call<T>(prompt);
        json_encode<T>(value)
      });
      json_decode<T>(recorded) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "durable replay decode failed" }; }
      }
    }
  }

  // Combinator constructor sugar, mirroring Provider.with_retry / .cached.
  // (We add it as a static rather than a Provider default method because it
  //  needs the engine ctx, which only exists inside a running workflow.)
  function wrap(inner: Provider, ctx: DurableCtx, step_id: string) -> Durable {
    Durable { inner: inner, ctx: ctx, step_id: step_id }
  }
}


// -----------------------------------------------------------------------------
// 2. Trace + cost roll-up across the boundary (background §6 "what's hard":
//    tokens spent by an inner agent must roll up to the outer step).
//
//    The inner agent makes N model calls; each reports its own Usage (reusing
//    UsageReporting from ../32-observability). The enclosing step wants ONE
//    number. We define a `CostMeter` that the agent's ExecutionContext.emit
//    feeds, and a rollup that sums child usage and attaches it to the step span.
//
//    The subtlety: a Durable step that REPLAYS spent zero new tokens (it didn't
//    call the model), but the step's recorded cost is the cost of the ORIGINAL
//    run. So the journal must also carry the usage, not just the answer. We
//    encode (value, usage) together.
// -----------------------------------------------------------------------------

// What a durable agent step journals: the typed answer AND its rolled-up usage,
// so cost attribution survives replay (replay re-reports the recorded cost, and
// correctly reports ZERO freshly-spent tokens).
class StepRecord<T> {
  value: T
  usage: Usage             // summed across every inner model turn (reused record)
  fresh: bool              // false on replay — "no tokens were actually spent now"
}

// A mutable accumulator the agent loop feeds via ExecutionContext.emit. Each
// model turn emits a UsageEvent; the meter folds them into a running total.
class CostMeter { _cell: $rust_type }    // host-backed mutable cell (atomic add)

function meter_new() -> CostMeter { $rust_io_function }
function meter_add(m: CostMeter, u: Usage) -> null { $rust_io_function }
function meter_total(m: CostMeter) -> Usage { $rust_io_function }

// The event the agent emits per model turn (rides ExecutionContext.emit — the
// event surface flagged net-new in proposal §7). The rollup listens for these.
class UsageEvent { usage: Usage }

// -----------------------------------------------------------------------------
// 3. A `DurableAgentStep`: the combinator that DOES the rollup, not just record.
//
//    It is Durable + cost-aware. It opens a step span, installs a CostMeter into
//    the agent's ExecutionContext so every inner turn's usage accrues, runs the
//    loop as ONE activity, journals (value + summed usage), and on both first-run
//    and replay attaches the rolled-up cost to the step span. This is the single
//    place direction-A's two hard problems are solved together.
// -----------------------------------------------------------------------------

class DurableAgentStep {
  agent: Tools             // the inner agent — a provider with the Tools capability
  ctx: DurableCtx
  step_id: string
  tools: Tool[]
  price: Price             // per-model price table (reused from ../32-observability)
  parent_span: SpanId?     // the OUTER workflow's span, for roll-up nesting

  // Run the agent loop as one checkpointed, cost-attributed activity.
  // `T` is the step's output type. `exec` is the app's tool-dispatch context.
  function run<T>(self, prompt: baml.llm.PromptAst, exec: ExecutionContext) -> StepRecord<T>
      throws baml.errors.LlmClient {
    let span: SpanId = obs_start("workflow.step", self.parent_span, { "step.id": self.step_id });

    // The activity body runs the FULL agent loop once. We thread a CostMeter
    // through ExecutionContext.emit so each inner model turn's usage accrues.
    let recorded: string = durable_activity(self.ctx, self.step_id, () => {
      let meter: CostMeter = meter_new();
      // exec.with_meter installs an emit handler that folds UsageEvent -> meter.
      // (ExecutionContext gains an emit surface in proposal §7; here we use it.)
      let exec2: ExecutionContext = exec.with_meter(meter);
      let value: T = self.agent.run_tools<T>(prompt, self.tools, exec2);
      let total: Usage = meter_total(meter);
      json_encode<StepRecord<T>>(StepRecord<T> { value: value, usage: total, fresh: true })
    });

    let rec: StepRecord<T> = json_decode<StepRecord<T>>(recorded) catch (e) {
      _ => {
        obs_finish(span, {}, "durable replay decode failed");
        throw baml.errors.LlmClient { message: "durable step decode failed" };
      }
    };

    // Cost attribution survives replay: on first run `fresh` is true (real
    // tokens); on replay the journaled record comes back with fresh=true too,
    // so we OVERRIDE it to false and report zero freshly-spent tokens while
    // still attaching the recorded cost to the span for the run's ledger.
    let dollars: float = Cost.of(rec.usage, self.price);
    obs_finish(span, Cost.attrs(rec.usage, dollars), null);

    // Mark whether THIS execution actually spent tokens. durable_activity gives
    // no direct "was-replayed" bit, so we infer it: an activity that re-ran would
    // have produced a brand-new journal entry this attempt. The engine exposes
    // it via a replay flag on the ctx (host seam).
    StepRecord<T> { value: rec.value, usage: rec.usage, fresh: durable_is_live(self.ctx) }
  }
}

// True only during the engine's live (non-replay) execution pass — the standard
// `info.is_replaying` bit every durable engine exposes (Temporal/DBOS/Inngest).
function durable_is_live(ctx: DurableCtx) -> bool { $rust_io_function }


// =============================================================================
// DIRECTION B: a whole workflow exposed to a model as one Tool.
// =============================================================================

// -----------------------------------------------------------------------------
// 4. The `Workflow` interface. A durable graph that takes typed input and
//    produces typed output. The KEY claim of the proposal is that this is just a
//    function returning a value — so a Workflow can BE a Provider.
//
//    `run` is the deterministic entry point (the graph executes, checkpointing
//    each step via DurableCtx). `as_tool` turns the whole workflow into a Tool a
//    model can call — its `parameters` are the workflow's input schema (the
//    first-class `type`), its name/description are how the model discovers it.
// -----------------------------------------------------------------------------

interface Workflow {
  type Input
  type Output

  // Deterministic durable execution. The engine ctx is threaded in (the workflow
  // does NOT own it — it is supplied by whoever runs the graph).
  function run(self, input: Input, ctx: DurableCtx) -> Output throws baml.errors.LlmClient

  // Surface this workflow as a Tool the model can invoke. The parameters are the
  // workflow's Input schema, reflected to a first-class `type`.
  function as_tool(self, name: string, description: string) -> Tool {
    Tool { name: name, description: description, parameters: reflect.type_of<Input>() }
  }
}

// -----------------------------------------------------------------------------
// 5. The `WorkflowProvider` adapter: a Workflow surfaced as a Provider.
//
//    This is what makes "a workflow IS a Provider" literal. The proposal says a
//    client is `function -> Provider`; a workflow is `function (input) -> output`.
//    The adapter bridges them: `call<T>(prompt)` decodes the model's tool args
//    out of the prompt into the workflow's Input, runs the durable graph, and
//    returns its Output as T. Now the workflow composes anywhere a Provider does
//    — it can be a member of a Fallback, be wrapped in Durable, etc.
//
//    Crucially this is where the TWO nondeterminism models meet (background §6):
//    the OUTER agent's tool dispatch is nondeterministic (the model chose to call
//    this tool); the INNER workflow is deterministic-and-durable. The adapter is
//    the seam — the agent sees an ordinary Tool result; the engine sees a child
//    workflow with its own journal.
// -----------------------------------------------------------------------------

class WorkflowProvider {
  wf: Workflow
  ctx: DurableCtx          // the engine handle to run the child graph under

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // The agent's tool dispatch handed us the model's arguments encoded in the
      // prompt. Decode to the workflow's Input, run the durable graph, return
      // Output as T. (Input/Output are the workflow's associated types; T is
      // unified with Output at the dispatch site — see usage.baml.)
      let input: Workflow.Input = baml.llm.tool_args<Workflow.Input>(prompt) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "could not decode workflow input" }; }
      };
      // The child workflow runs under its OWN durable scope, so its steps get
      // their own journal entries (nested durability — background §6 "whether
      // the inner agent is itself durable").
      let out: Workflow.Output = self.wf.run(input, self.ctx.child(self.wf_key()));
      // Output unifies with T at the call site; cast is the existential bridge.
      baml.cast.checked<T>(out) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "workflow output type mismatch" }; }
      }
    }
  }

  function wf_key(self) -> string { "child.workflow" }
}

// DurableCtx gains a `child` to scope a nested workflow's journal under the
// parent's history (so a workflow-as-tool inside an agent-as-step nests cleanly).
// Declared here as the host seam the adapter relies on.
function durable_child(ctx: DurableCtx, key: string) -> DurableCtx { $rust_io_function }


// =============================================================================
// 6. host seams introduced by this scenario (the genuine native boundaries)
//
//   baml.durable.activity(ctx, key, () -> string) -> string   // record/replay
//   baml.durable.id(ctx, key) -> string                       // replay-safe id
//   baml.durable.is_live(ctx) -> bool                         // info.is_replaying
//   baml.durable.child(ctx, key) -> DurableCtx                // nested journal
//   baml.json.encode<T>(T) -> string  /  decode<T>(string) -> T (canonical)
//   baml.llm.tool_args<T>(prompt) -> T            // decode model tool-call args
//   baml.cast.checked<T>(v) -> T                  // existential downcast
//   meter_new / meter_add / meter_total          // cost accumulator cell
//
//   Reused unchanged from ../32-observability: Usage, Cost, Price,
//   obs_start/obs_finish, SpanId, UsageReporting.
//   Reused unchanged from ../_conventions.md: Provider, Tools, Tool,
//   ToolCall/ToolResult, ExecutionContext (+ emit), Fallback/Retry combinators.
// =============================================================================

// ExecutionContext extension used above — installs a usage-meter emit handler.
// (proposal §7 flags ExecutionContext gaining an event-`emit` surface; this is
//  the concrete handler the rollup needs.)
interface MeteredContext requires ExecutionContext {
  function with_meter(self, m: CostMeter) -> ExecutionContext
}
