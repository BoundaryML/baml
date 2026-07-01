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
//       Provider — and, having a request/response shape, it implements the
//       HttpProvider capability that OWNS `call<T>(prompt) -> T` (../provider-as-marker.md:
//       the bare `Provider` marker carries no `call`). We wrap it so the agent's
//       tool dispatch invokes it like any other tool, and its tokens/cost roll up.
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
//   - `DurableMeta` — a ResponseMeta over the journaled record (cost attribution)
//   - host seams: baml.durable.* (journal). Journal serialization is the REAL
//     stdlib JSON surface (baml.json.to_string<T> / from_string<T>), not net-new.
//
// Sidecar model (../value-sidecar-model.md, _conventions.md "Value + sidecar
// model"): the PRODUCT sidecars at the agent/step boundary — the rolled-up
// `usage` and the replayed-echo bit — ride `call_with` + `ResponseMeta`, NOT a
// bespoke `StepRecord<T>` companion. The durable engine, the journal, and the
// WorkflowError channel are NOT product sidecars; they stay exactly where the
// engine/error work left them (record/replay is an engine round-trip, not a
// `(T, U)` accompaniment).
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

// The journal serialization seam is the REAL stdlib JSON surface — no bespoke
// host shim. `baml.json.to_string<T>(value)` produces the canonical JSON string
// the journal records (replay-stable bytes); `baml.json.from_string<T>(text)`
// coerces a recorded string back to its typed shape on replay (throwing
// JsonParseError/JsonDecodeError, which the call sites map to DurableDecodeError).


// -----------------------------------------------------------------------------
// 0b. NET-NEW capability error channel: the durable workflow engine.
//
//    This scenario introduces a capability beyond call/stream/realtime/tools — a
//    durable graph executor (record/replay, checkpointed activities, nested
//    journals). Its failures are not "a model call failed" (CallError) nor "a
//    tool loop failed" (ToolError); they are engine failures: a journal decode
//    that no longer matches the recorded schema, a non-deterministic replay, a
//    child-scope mismatch. So `Workflow.run` / `DurableAgentStep.run` declare a
//    dedicated `baml.errors.WorkflowError` channel (flagged net-new in
//    evaluation.md), with classifiers a durable engine actually triages on.
// -----------------------------------------------------------------------------

interface baml.errors.WorkflowError {
  function is_replay_mismatch(self) -> bool      // recorded journal no longer matches code
  function is_decode_error(self) -> bool         // a journaled value failed to decode to T
}

// The canonical `baml.errors.Unsupported` (in _conventions.md) implements only the
// four CORE capability-error interfaces (Call/Stream/Tool/Realtime) — it predates
// this scenario's net-new WorkflowError channel, so it does NOT implement it. But a
// Tools-capability-negotiation `_` arm inside a `Workflow.run` body needs to throw
// Unsupported on the WorkflowError channel (see usage.baml SupportWorkflow.run), and
// rule 2 makes that legal ONLY if Unsupported implements WorkflowError. So this
// scenario EXTENDS Unsupported with the missing implements block — exactly the
// "introduce the primitive you need, flag it net-new" pattern the conventions call
// for. (All classifiers false: an Unsupported is neither a replay mismatch nor a
// decode error — flagged as a semantic-oddity tension in evaluation.md (c).)
implements baml.errors.WorkflowError for baml.errors.Unsupported {
  function is_replay_mismatch(self) -> bool { return false; }
  function is_decode_error(self)    -> bool { return false; }
}

// A journaled record could not be decoded back to its typed shape on replay (the
// recorded bytes no longer match the activity's output schema). This can surface
// on the call path (Durable.call_with decoding a single answer) AND on the workflow
// path (DurableAgentStep.run_with / Workflow.run decoding a Journaled<T>), so it
// implements both capability interfaces.
class DurableDecodeError {
  step_id: string
  detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return true; }
  }
  implements baml.errors.WorkflowError {
    function is_replay_mismatch(self) -> bool { return true; }
    function is_decode_error(self)    -> bool { return true; }
  }
}

// A workflow-as-tool produced an Output whose runtime type does not unify with
// the T the agent's tool dispatch expected (the existential downcast failed). It
// arises on the call path of WorkflowProvider's HttpProvider call/call_with.
class WorkflowOutputMismatch {
  detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return true; }
  }
  implements baml.errors.WorkflowError {
    function is_replay_mismatch(self) -> bool { return false; }
    function is_decode_error(self)    -> bool { return true; }
  }
}


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
//    constructed with the engine ctx and a stable step id. It implements the
//    HttpProvider capability (which OWNS call/call_with — ../provider-as-marker.md;
//    the base `Provider` is a bare marker) and forwards `.call` AND `.call_with`
//    (the request/response value+sidecar method): streaming/realtime are NOT
//    replay-safe (you cannot replay a live duplex), so we deliberately do NOT
//    implement Streaming or Realtime here — a durable step yields a value, not a
//    live session.
//
//    Forwarding `call_with` is what keeps the inner agent's usage/replayed-bit
//    from being silently dropped through the journal — the exact P1 win the
//    value+sidecar model is for. The journal must SERIALIZE the projected
//    metadata (the bytes are what replay returns), so the projection runs over a
//    `DurableMeta` (§2) that reads usage from the recorded payload and answers
//    `replayed` from the engine's live/replay bit — the SAME (value, ResponseMeta)
//    pair on first run and on replay, only the `replayed` dimension flips.
// -----------------------------------------------------------------------------

class Durable {
  inner: Provider          // the agent (Tools) or any provider whose call we record
  ctx: DurableCtx          // the engine journal
  step_id: string          // stable activity key — author-supplied, replay-stable

  // call/call_with are an HttpProvider capability (../provider-as-marker.md): the
  // base `Provider` is a bare marker. `Durable` forwards the request/response shape
  // (a single recorded value + its projected sidecar), so it implements HttpProvider,
  // not the marker. Streaming/Realtime are deliberately NOT implemented (you cannot
  // replay a live duplex). The combinator factories are inherited via `requires Provider`.
  implements HttpProvider {
    type Body = unknown      // Durable forwards an inner call; it has no wire body of its own

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // call is DERIVED: project nothing, keep the value (the 90% path, zero tax).
      let (value, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      value
    }

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Record-and-replay. On first run: execute inner.call_with (the FULL agent
      // loop), capturing BOTH the value and the provider's usage off its
      // ResponseMeta; encode (value, usage) together so cost survives replay; and
      // journal that. On replay: durable_activity skips the body and returns the
      // journaled string, which still carries the usage. Either way we rebuild a
      // ResponseMeta over the recorded payload and let the caller `project` it.
      let recorded: string = durable_activity(self.ctx, self.step_id, () => {
        // capture the inner usage as the journal's serializable cost record. `inner`
        // is the bare marker `Provider`, so we match it for the HttpProvider capability
        // that owns call_with (../provider-as-marker.md §3): a Durable wraps a
        // request/response provider (the agent loop's run_tools surfaces as one).
        let (value, used): (T, Usage) = match (self.inner) {
          let h: HttpProvider => h.call_with<T, Usage>(prompt, m => m.usage()),
          _ => { throw baml.errors.Unsupported { message: "durable inner provider has no call_with" }; },
        };
        baml.json.to_string<Journaled<T>>(Journaled<T> { value: value, usage: used })
      });
      // A failed decode of the journaled record is a replay-mismatch — a concrete,
      // classified durable error (is_parse_error -> true), not an opaque wrap.
      let rec: Journaled<T> = baml.json.from_string<Journaled<T>>(recorded) catch (e) {
        _ => { throw DurableDecodeError { step_id: self.step_id, detail: "durable replay decode failed" }; }
      };
      // The projection runs over a DurableMeta: usage comes from the journaled
      // record (so replay re-reports the recorded cost), `replayed` from the live
      // bit (true when this pass did NOT spend fresh tokens). The tuple is the
      // value+sidecar product — no bespoke StepRecord companion.
      (rec.value, project(DurableMeta { rolled_usage: rec.usage, was_replayed: !durable_is_live(self.ctx) }))
    } catch (e) {
      // inner.call_with (the agent loop) and durable_activity can throw foreign
      // engine failures; known CallErrors (incl. DurableDecodeError) pass through.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "durable call failed");
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
//    feeds, sum it, and surface the total as a PRODUCT sidecar via `call_with`.
//
//    The value+sidecar move: usage and "was this a replayed echo" are NOT bundled
//    into a bespoke `StepRecord<T> { value, usage, fresh }` companion. They are a
//    `ResponseMeta` the caller projects off `call_with`. The journal still has to
//    SERIALIZE the usage (the recorded bytes are what replay returns), but that is
//    a private serialization payload (`Journaled<T>`), not the public return shape:
//    the boundary hands back `(T, U)`, where U is whatever the caller projected.
//
//    The subtlety the journal carries: a Durable step that REPLAYS spent zero new
//    tokens (it did not call the model), but the recorded cost is the ORIGINAL
//    run's. So `DurableMeta.usage()` reads the journaled total (replay re-reports
//    it) while `DurableMeta.replayed()` answers the live/replay bit (true on a
//    replay = "no fresh tokens spent now").
// -----------------------------------------------------------------------------

// PRIVATE journal payload — the serializable (value, usage) the engine records so
// cost attribution survives replay. NOT a return type and NOT a sidecar companion:
// it never crosses the public boundary; `DurableMeta` reads it, `call_with` returns
// a tuple. (Canonical-encoded; usage is the reused ../32-observability record.)
class Journaled<T> {
  value: T
  usage: Usage             // summed across every inner model turn (reused record)
}

// The replayed-echo bit is a FRAMEWORK ResponseMeta dimension (value+sidecar model
// §3, _conventions.md FrameworkMeta). A durable step reports it like timing/attempt.
interface Replayable requires ResponseMeta {
  function replayed(self) -> bool          // true => this pass returned a journaled echo
}

// `DurableMeta` is the ResponseMeta the durable boundary projects: usage from the
// journaled record, `replayed` from the engine's live bit. It is a leaf meta (no
// inner provider response to forward to — the journal is the source of truth), so
// the provider dimensions it cannot reconstruct off the recorded bytes report
// Unsupported rather than null. This is the same interface/concrete/Supported shape
// as OpenAIResponse/AnthropicResponse, sourced from the journal instead of the wire.
class DurableMeta {
  rolled_usage: Usage      // the journaled, rolled-up total (distinct field name, per FrameworkMeta)
  was_replayed: bool       // !durable_is_live(ctx) — captured at projection time
  implements ResponseMeta {
    function usage(self)         -> Usage                 { self.rolled_usage }
    function finish_reason(self) -> string                { "durable_step" }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "durable: not journaled" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "durable: not journaled" } }
    function reasoning(self)     -> Supported<string>     { Unsupported { reason: "durable: not journaled" } }
  }
  implements Replayable {
    function replayed(self) -> bool { self.was_replayed }
  }
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
//
//    Its public method is `call_with` — the value+sidecar primitive. The answer
//    `T` and the projected sidecar `U` come back as a TUPLE `(T, U)`; there is no
//    `StepRecord<T>` companion. The rolled-up usage and the replayed-echo bit are
//    a `DurableMeta` (a ResponseMeta) the caller projects. The journal still
//    serializes (value, usage) via the private `Journaled<T>` payload, but that
//    never surfaces — replay re-reads it through `DurableMeta`, off the public
//    `(T, U)` surface.
// -----------------------------------------------------------------------------

class DurableAgentStep {
  agent: Tools             // the inner agent — a provider with the Tools capability
  ctx: DurableCtx
  step_id: string
  tools: Tool[]
  price: Price             // per-model price table (reused from ../32-observability)
  parent_span: SpanId?     // the OUTER workflow's span, for roll-up nesting

  // Run the agent loop as one checkpointed, cost-attributed activity, returning
  // the value AND a projected sidecar `U` as a tuple. `T` is the step's output
  // type. `project` reads the DurableMeta (rolled-up usage + replayed bit).
  // `exec` is the app's tool-dispatch context.
  function run_with<T, U>(self, prompt: baml.llm.PromptAst, exec: ExecutionContext,
                          project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.WorkflowError> {
    let span: SpanId = obs_start("workflow.step", self.parent_span, { "step.id": self.step_id });

    // The activity body runs the FULL agent loop once. We thread a CostMeter
    // through ExecutionContext.emit so each inner model turn's usage accrues, then
    // journal the PRIVATE (value, usage) payload so cost survives replay.
    let recorded: string = durable_activity(self.ctx, self.step_id, () => {
      let meter: CostMeter = meter_new();
      // exec.with_meter installs an emit handler that folds UsageEvent -> meter.
      // (ExecutionContext gains an emit surface in proposal §7; here we use it.)
      let exec2: ExecutionContext = exec.with_meter(meter);
      let value: T = self.agent.run_tools<T>(prompt, self.tools, exec2);
      let total: Usage = meter_total(meter);
      baml.json.to_string<Journaled<T>>(Journaled<T> { value: value, usage: total })
    });

    let rec: Journaled<T> = baml.json.from_string<Journaled<T>>(recorded) catch (e) {
      _ => {
        obs_finish(span, {}, "durable replay decode failed");
        throw DurableDecodeError { step_id: self.step_id, detail: "durable step decode failed" };
      }
    };

    // Cost attribution survives replay: usage comes off the journaled record, so
    // first-run and replay attach the SAME recorded cost to the span ledger.
    let dollars: float = Cost.of(rec.usage, self.price);
    obs_finish(span, Cost.attrs(rec.usage, dollars), null);

    // The value+sidecar product: project the DurableMeta. `replayed` is the
    // engine's live bit (durable_activity gives no direct "was-replayed" flag, so
    // we infer it: a replay returns the journaled echo, not a fresh model run).
    // The caller decides what to read — usage, the replayed bit, or both.
    (rec.value, project(DurableMeta { rolled_usage: rec.usage, was_replayed: !durable_is_live(self.ctx) }))
  } catch (e) {
    // The activity body runs the inner agent loop (run_tools -> ToolError) and the
    // engine journal (foreign host failures). Both are foreign to THIS capability;
    // normalize onto the WorkflowError channel. DurableDecodeError, being a known
    // WorkflowError, passes through unchanged.
    _ => throw baml.UnknownError.with_message<baml.errors.WorkflowError>(e, "durable agent step failed");
  }

  // Sugar for the 90% step path: run the agent loop, keep only the value.
  // (Derived from run_with, projecting nothing — the call/call_with relationship.)
  function run<T>(self, prompt: baml.llm.PromptAst, exec: ExecutionContext) -> T
      throws baml.ExtendUnknownError<baml.errors.WorkflowError> {
    let (value, _): (T, null) = self.run_with<T, null>(prompt, exec, m => null);
    value
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
  function run(self, input: Input, ctx: DurableCtx) -> Output
      throws baml.ExtendUnknownError<baml.errors.WorkflowError>

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

  // A workflow-as-tool is a request/response provider: one tool-call in, one typed
  // Output back. So it implements the HttpProvider capability that OWNS call/call_with
  // (../provider-as-marker.md) — the bare `Provider` marker carries no `call`. It runs
  // a durable graph rather than a wire round-trip, so it does NOT stream or go realtime.
  implements HttpProvider {
    type Body = unknown      // the workflow Output is the "body"; no wire envelope

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The agent's tool dispatch handed us the model's arguments encoded in the
      // prompt. Decode to the workflow's Input, run the durable graph, return
      // Output as T. (Input/Output are the workflow's associated types; T is
      // unified with Output at the dispatch site — see usage.baml.)
      let input: Workflow.Input = baml.llm.tool_args<Workflow.Input>(prompt) catch (e) {
        // A model that produced un-decodable tool args is a parse failure on the
        // call path — a classified WorkflowOutputMismatch (is_parse_error -> true).
        _ => { throw WorkflowOutputMismatch { detail: "could not decode workflow input" }; }
      };
      // The child workflow runs under its OWN durable scope, so its steps get
      // their own journal entries (nested durability — background §6 "whether
      // the inner agent is itself durable"). self.wf.run throws on the WorkflowError
      // channel; the trailing catch re-homes any non-CallError onto this channel.
      let out: Workflow.Output = self.wf.run(input, self.ctx.child(self.wf_key()));
      // Output unifies with T at the call site; cast is the existential bridge.
      let value: T = baml.cast.checked<T>(out) catch (e) {
        _ => { throw WorkflowOutputMismatch { detail: "workflow output type mismatch" }; }
      };
      // value+sidecar: the workflow's per-step usage rolls up through its own
      // DurableAgentSteps, so THIS adapter layer adds none — it projects a leaf
      // DurableMeta with zero adapter-local usage (and not-replayed, since the
      // child graph just ran). The caller still gets the (T, U) product shape.
      (value, project(DurableMeta {
        rolled_usage: Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0,
                              cache_write_tokens: 0, reasoning_tokens: 0 },
        was_replayed: false,
      }))
    } catch (e) {
      // self.wf.run surfaces WorkflowErrors here, foreign to the CallError channel
      // an HttpProvider call must present; normalize them (WorkflowOutputMismatch, a
      // known CallError, passes through unchanged).
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "workflow provider call failed");
    }

    // call DERIVES from call_with (project nothing, drop the sidecar) — the spine
    // relationship, mirroring Durable above.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (value, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      value
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
//   baml.llm.tool_args<T>(prompt) -> T            // decode model tool-call args
//   baml.cast.checked<T>(v) -> T                  // existential downcast
//   meter_new / meter_add / meter_total          // cost accumulator cell
//
//   Reused unchanged from the REAL stdlib: baml.json.to_string<T> / from_string<T>
//   (canonical journal serialization — NOT a net-new host seam).
//   Reused unchanged from ../32-observability: Usage, Cost, Price,
//   obs_start/obs_finish, SpanId, UsageReporting.
//   Reused unchanged from ../_conventions.md: Provider (the bare marker),
//   HttpProvider (owns call/call_with), Tools, Tool, ToolCall/ToolResult,
//   ExecutionContext (+ emit), Fallback/Retry combinators.
//
//   NET-NEW error surface (flagged in evaluation.md): the durable engine is a
//   capability beyond call/stream/realtime/tools, so it gets its own capability
//   error interface `baml.errors.WorkflowError` (is_replay_mismatch /
//   is_decode_error). Because that channel is net-new, the canonical
//   `baml.errors.Unsupported` (which implements only the four CORE interfaces) is
//   EXTENDED here with `implements baml.errors.WorkflowError for baml.errors.Unsupported`
//   so capability-negotiation `_` arms can throw it on the WorkflowError channel.
//   Concrete errors: DurableDecodeError (implements WorkflowError
//   AND CallError — a journal decode can fail on Durable.call_with's single-answer
//   path and on a step's Journaled<T> path), WorkflowOutputMismatch (the existential
//   Output->T cast). Per-capability channels:
//     Durable.call / WorkflowProvider.call -> CallError   (they are HttpProvider.call)
//     DurableAgentStep.run / Workflow.run  -> WorkflowError
//   Foreign errors crossing a capability boundary (inner agent's ToolError, host
//   engine faults) are normalized with baml.UnknownError.with_message onto the
//   receiving channel — the provenance cost discussed in evaluation.md (c).
// =============================================================================

// ExecutionContext extension used above — installs a usage-meter emit handler.
// (proposal §7 flags ExecutionContext gaining an event-`emit` surface; this is
//  the concrete handler the rollup needs.)
interface MeteredContext requires ExecutionContext {
  function with_meter(self, m: CostMeter) -> ExecutionContext
}
