// =============================================================================
// implement.baml — Streaming & observability of a WORKFLOW
//
// STD-LIB / library-author code. Builds on the canonical primitives
// (Provider / HttpProvider / Streaming / Tools / ExecutionContext) and reuses
// the `baml.obs` span seam + `Traced` combinator pattern from scenario 32.
//
// The teaching points for THIS scenario (the layer ABOVE a single model call):
//
//   1. A workflow is a Provider (the marker; ../provider-as-marker.md) that
//      `implements HttpProvider` — request/response is a real capability it has,
//      since a run produces one typed value. `Workflow.call<T>` runs the graph to
//      that result; the graph IS the call body. So a workflow composes with
//      retry/fallback/trace exactly like a model.
//
//   2. A workflow emits a HIERARCHY keyed by step id, not a flat token feed:
//      step-start / step-result / step-suspended / workflow-finish. That is a
//      net-new capability `Steppable requires Provider` whose method returns a
//      `baml.llm.Stream<StepEvent, T>` — the SAME Stream type the model uses,
//      but the items are step events, not tokens.
//
//   3. MULTIPLEXING: a step that calls a *streaming model* produces an inner
//      token stream nested inside the outer step-event stream. We fold the
//      inner `Streaming` stream into the outer step stream as `step-token`
//      events tagged with the step id — one ordered feed for a UI.
//
//   4. CORRELATION: each step opens an obs span; every model call inside the
//      step nests under that step's span (the §32 `Traced` combinator, parented
//      to the step span). So a step graph renders as a span tree.
//
//   5. REPLAY / TIME-TRAVEL: each step's recorded input/output is checkpointed
//      (a `StepLog`). `Workflow.replay_step` re-runs ONE step against its
//      recorded input. Replay fidelity is only as good as step purity — we make
//      the nondeterminism boundary explicit.
//
// Everything net-new is flagged in evaluation.md.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. Reused, verbatim, from scenario 32 (the span seam). We do NOT redefine it;
//    we cite the shape so this file reads standalone. These are host primitives.
//
//   class SpanId  { _raw: $rust_type }
//   function obs_start(name, parent: SpanId?, attrs) -> SpanId    // $rust_io_function
//   function obs_finish(span: SpanId, attrs, error: string?) -> null
//   function obs_event(span: SpanId, name, attrs) -> null
//
// And the `Traced` combinator: `Traced { inner: Provider, name, parent: SpanId?,
// price, export_messages }` which opens a span per .call/.stream/.run_tools and
// nests it under `parent`. We reuse it here to parent MODEL spans under STEP
// spans — that is the whole correlation story.
// -----------------------------------------------------------------------------


// -----------------------------------------------------------------------------
// 0b. NET-NEW capability error channel. A workflow engine is a capability BEYOND
//     call / stream / realtime / tools: it runs a durable step graph, suspends,
//     replays. So it gets its own capability-error interface (rule 4 of the error
//     model) — `baml.errors.StepError` — declared here as net-new surface and
//     flagged in evaluation.md. `Steppable.stream_steps` / `replay_step` and the
//     internal graph helpers declare `baml.ExtendUnknownError<baml.errors.StepError>`.
//
//   interface baml.errors.StepError {
//     function is_network_error(self) -> bool   // a model call inside a step failed on the wire
//     function is_replay_error(self) -> bool    // re-run / time-travel could not be served
//     function is_missing_checkpoint(self) -> bool // no durable record for the requested step
//   }
//
// Concrete StepErrors the engine raises on purpose:

class StepFailed {
  step_id: string
  detail: string
  implements baml.errors.StepError {
    function is_network_error(self)     -> bool { return false; }
    function is_replay_error(self)      -> bool { return false; }
    function is_missing_checkpoint(self) -> bool { return false; }
  }
}

class MissingCheckpoint {
  step_id: string
  implements baml.errors.StepError {
    function is_network_error(self)      -> bool { return false; }
    function is_replay_error(self)       -> bool { return true; }
    function is_missing_checkpoint(self) -> bool { return true; }
  }
}

class UnknownStep {
  step_id: string
  implements baml.errors.StepError {
    function is_network_error(self)      -> bool { return false; }
    function is_replay_error(self)       -> bool { return false; }
    function is_missing_checkpoint(self) -> bool { return false; }
  }
}

class ReplayFailed {
  step_id: string
  detail: string
  implements baml.errors.StepError {
    function is_network_error(self)      -> bool { return false; }
    function is_replay_error(self)       -> bool { return true; }
    function is_missing_checkpoint(self) -> bool { return false; }
  }
}


// -----------------------------------------------------------------------------
// 1. The step-event vocabulary. This is the workflow's answer to "tokens" — the
//    items of the outer stream. A flat tagged union (a `kind` discriminant)
//    because the background shows NO shared schema across frameworks, so we pick
//    one explicitly: Mastra's step-start / step-result / step-suspended /
//    workflow-finish, plus a step-token frame for multiplexed model streaming.
//
//    Keyed by `step_id` so a UI can build the hierarchy; `parent_id` gives the
//    nesting (a sub-workflow step's children carry its id as parent).
// -----------------------------------------------------------------------------

class StepEvent {
  kind: string            // "step-start" | "step-token" | "step-result" | "step-suspended" | "workflow-finish"
  step_id: string         // which step this event belongs to ("" for workflow-finish)
  parent_id: string?      // the enclosing step id, for sub-workflows; null at top level
  span: SpanId?           // the obs span for this step — correlation handle for traces

  // payload, sparsely populated per kind:
  token: string?          // step-token: one model delta multiplexed up from an inner Streaming
  output: unknown?        // step-result: the step's recorded output (also checkpointed)
  suspend: SuspendPayload? // step-suspended: what a human/event must supply to resume
  result: unknown?        // workflow-finish: the workflow's final value
}

class SuspendPayload {
  reason: string          // "approval" | "wait-for-event" | ...
  schema: type            // the shape `resume` must supply (first-class `type`)
  prompt: string          // human-readable ask for the approver UI
}


// -----------------------------------------------------------------------------
// 2. A Step. The unit of the graph. A step is a named, typed activity whose body
//    is an arbitrary computation over a `StepContext` — crucially, the body may
//    itself call a model (a Provider). We keep the body abstract via an
//    interface so the graph can hold heterogeneous steps in one array.
//
//    `run` returns `StepOutcome`: a value OR a suspend request (the workflow
//    answer to Iterator's Item | Done — here Done is "produced a value", the
//    other arm is "suspended, awaiting resume").
// -----------------------------------------------------------------------------

class StepOutcome {
  value: unknown?            // present => step completed with this output
  suspend: SuspendPayload?   // present => step suspended; workflow must persist + wait
}

interface Step {
  function id(self) -> string
  // The step body. `ctx` carries the recorded input, the step's obs span (so any
  // model call inside nests correctly), and the resume value if we're re-entering
  // a suspended step.
  function run(self, ctx: StepContext) -> StepOutcome
      throws baml.ExtendUnknownError<baml.errors.StepError>
}

// What a step body sees. This is the workflow's ExecutionContext analogue, but
// for steps rather than tool dispatch. It threads the AMBIENT span (so model
// spans correlate) and the resume value (so a re-entered step skips to where it
// suspended). `input` is the recorded input for THIS step — replay hands the
// same StepContext with a recorded input to re-run one step in isolation.
class StepContext {
  input: unknown           // the step's input (from the prior step or the workflow input)
  step_span: SpanId        // the obs span opened for this step; parent for model calls
  resume: unknown?         // set on re-entry after a suspend; null on first entry
  emit: (StepEvent) -> null // sink for step-token frames the step pushes while a model streams
}


// -----------------------------------------------------------------------------
// 3. The checkpoint log. Per-step input+output recorded so we can (a) resume a
//    suspended workflow and (b) time-travel: re-run ONE step against its
//    recorded input. Host-backed because durable persistence is not pure BAML.
//
//    `StepRecord` is the time-travel unit — input + output + the span, so replay
//    can re-attach to the same trace position.
// -----------------------------------------------------------------------------

class StepRecord {
  step_id: string
  input: unknown
  output: unknown?          // null while the step is in-flight / suspended
  span: SpanId
  suspended: bool
}

// baml.flow (net-new namespace) — the durable step log. A KV-ish journal keyed
// by (run_id, step_id). All host boundaries.
class StepLog { _handle: $rust_type }

function flow_open_log(run_id: string) -> StepLog { $rust_io_function }
function flow_put(log: StepLog, rec: StepRecord) -> null { $rust_io_function }
function flow_get(log: StepLog, step_id: string) -> StepRecord? { $rust_io_function }
function flow_new_run_id() -> string { $rust_io_function }


// -----------------------------------------------------------------------------
// 4. The Steppable capability — the workflow analogue of Streaming.
//
//    `Streaming` gives "the same call, delivered incrementally as tokens".
//    `Steppable` gives "the same call, delivered incrementally as STEP EVENTS".
//    Same Stream type, different item type. A Workflow implements BOTH Provider
//    (run to result) and Steppable (run while emitting the step-event hierarchy).
//
//    Note the return: `baml.llm.Stream<StepEvent, T>`. TStream = StepEvent (the
//    per-step frame), TFinal = T (the workflow's typed result). This is the
//    EXACT shape Streaming uses — we are reusing baml.llm.Stream unchanged, only
//    instantiating its first type parameter with our event type.
// -----------------------------------------------------------------------------

interface Steppable requires Provider {
  function stream_steps<T>(self, prompt: baml.llm.PromptAst)
      -> baml.llm.Stream<StepEvent, T>
      throws baml.ExtendUnknownError<baml.errors.StepError>

  // Time-travel: re-run ONE step against its recorded input, outside the run.
  // Returns the step's recomputed outcome — for debugging a flaky step or
  // resuming after editing recorded state.
  function replay_step<T>(self, run_id: string, step_id: string) -> StepOutcome
      throws baml.ExtendUnknownError<baml.errors.StepError>
}


// -----------------------------------------------------------------------------
// 5. The Workflow provider itself. A plain class holding an ordered list of
//    steps (a linear graph for this scenario; branch/parallel are more steps).
//
//    - `implements HttpProvider` => .call runs the graph to its result (drains
//      the step stream, returns TFinal). Under the marker-`Provider` revision
//      (../provider-as-marker.md) `call`/`call_with` are an HttpProvider
//      capability, not a base-`Provider` method — the workflow is request/response
//      (it produces a typed value), so it implements HttpProvider. This is what
//      makes a workflow composable with Retry/Fallback/Traced.
//    - `implements Steppable` => .stream_steps emits the hierarchy AND replay.
//
//    The model spans correlate because each step opens a step span and wraps any
//    model provider it uses with `Traced{ parent: step_span }`. That wrapping is
//    the step author's job (see usage.baml), but the workflow GIVES them the
//    span via StepContext.step_span.
// -----------------------------------------------------------------------------

class Workflow {
  name: string
  steps: Step[]

  implements HttpProvider {
    type Body = unknown
    // Run to result: drive the step stream and return only the final value.
    // Built ON stream_steps so there is ONE execution path (no drift between the
    // observed run and the unobserved run — the §5 background's central risk).
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let s: baml.llm.Stream<StepEvent, T> = self.stream_steps<T>(prompt);
      // Drain the event stream, ignoring frames; final() yields TFinal.
      while (true) {
        match (s.next()) {
          let _e: StepEvent => { },              // discard progress frames on the .call path
          let _done: baml.llm.StreamDone => { break; }
        }
      }
      s.final()
    } catch (e) {
      // `call` exposes the HttpProvider channel (CallError), but draining the graph
      // raises StepError. A StepError is foreign to CallError, so normalize it
      // (and any host failure) into the universal wrapper. A genuine CallError
      // raised by a model deep inside a step passes through with_message unchanged.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "workflow " + self.name + " call failed");
    }

    // call_with is the value+sidecar primitive on HttpProvider (../provider-as-marker.md).
    // A workflow runs a GRAPH — there is no single normalized HTTP body, hence no
    // single `ResponseMeta` to project: a run can touch zero, one, or many model
    // calls across its steps, each with its own usage/sidecar. So a workflow has no
    // request/response sidecar of its own; we do NOT fabricate a fake graph-level
    // meta. The honest move is to decline the sidecar (the same shape Durable.call_with
    // takes when its inner has no http capability). Per-step usage/cost is the
    // Steppable observability path's job (step spans + the §32 Traced combinator),
    // not a single `(T, U)` tuple. `call` above stays the value path.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      throw baml.errors.Unsupported { message: "workflow " + self.name + " has no single request/response sidecar; observe per-step usage via .steps" };
    }
  }

  implements Steppable {
    function stream_steps<T>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<StepEvent, T>
        throws baml.ExtendUnknownError<baml.errors.StepError> {
      let run_id: string = flow_new_run_id();
      let log: StepLog = flow_open_log(run_id);
      let wf_span: SpanId = obs_start("workflow." + self.name, null, { "run_id": run_id });

      // The seed carries: remaining steps, the value flowing between steps, the
      // durable log, the workflow span, and a queue of buffered step-token frames
      // that the inner model stream produced (multiplexing — see fold_step).
      let seed: WorkflowState = WorkflowState {
        steps: self.steps,
        cursor: 0,
        carry: baml.flow.input_value(prompt),   // workflow input, host-extracted from the prompt
        log: log,
        wf_span: wf_span,
        run_id: run_id,
        pending: [],                             // buffered StepEvents not yet emitted
      };

      // Unfold the workflow into a Stream<StepEvent, T>. Each `next()` either
      // flushes one buffered frame (a multiplexed token) or advances one step.
      baml.llm.stream_unfold<WorkflowState, StepEvent, T>(
        seed,
        st => { Workflow.advance(st) },          // step: emit one StepEvent
        st => { Workflow.finish<T>(st) },        // done: validate + build TFinal
      )
    } catch (e) {
      // Host failures setting up the run (log open, span open, run id) are foreign;
      // normalize them. A StepError raised while draining passes through unchanged.
      _ => throw baml.UnknownError.with_message<baml.errors.StepError>(e, "workflow " + self.name + " stream_steps failed");
    }

    function replay_step<T>(self, run_id: string, step_id: string) -> StepOutcome
        throws baml.ExtendUnknownError<baml.errors.StepError> {
      let log: StepLog = flow_open_log(run_id);
      let rec: StepRecord? = flow_get(log, step_id);
      match (rec) {
        null => { throw MissingCheckpoint { step_id: step_id }; }
        let r: StepRecord => {
          // Re-open a span UNDER the original step span so the replay shows as a
          // sibling re-run in the trace (time-travel correlation).
          let replay_span: SpanId = obs_start("replay." + step_id, r.span, { "run_id": run_id });
          let target: Step = Workflow.find_step(self.steps, step_id);
          // Re-run with the RECORDED input. Purity is the caller's promise:
          // emit is a no-op here (we are not driving a live UI), resume is null.
          let ctx: StepContext = StepContext {
            input: r.input,
            step_span: replay_span,
            resume: null,
            emit: e => { null },                 // replay drops frames; we want only the outcome
          };
          let outcome: StepOutcome = target.run(ctx) catch (e) {
            _ => { obs_finish(replay_span, {}, "replay threw"); throw ReplayFailed { step_id: step_id, detail: "replay threw" }; }
          };
          obs_finish(replay_span, { "replayed": "true" }, null);
          outcome
        }
      }
    } catch (e) {
      // Foreign host failures (log open, span open) normalize; MissingCheckpoint /
      // ReplayFailed pass through with_message unchanged as known StepErrors.
      _ => throw baml.UnknownError.with_message<baml.errors.StepError>(e, "replay of step " + step_id + " failed");
    }
  }

  // ----- find a step by id (replay / branch targeting) -----
  function find_step(steps: Step[], step_id: string) -> Step
      throws baml.ExtendUnknownError<baml.errors.StepError> {  // static
    for (let s in steps) {
      if (s.id() == step_id) { return s; }
    }
    throw UnknownStep { step_id: step_id }
  }

  // ----- one pull of the workflow stream -----
  //
  // This is the heart of the scenario. On each pull we EITHER:
  //   (A) flush a buffered frame (a step-token multiplexed up from a model that
  //       streamed inside the current step) — drains `pending` FIFO, or
  //   (B) advance to the next step: emit step-start, run the step (which may push
  //       more step-token frames into `pending`), emit step-result/suspended.
  //
  // Because step bodies push their inner model tokens into `pending` via
  // ctx.emit, the outer stream interleaves "step-token" frames between
  // "step-start" and "step-result" — ONE ordered feed.
  function advance(st: WorkflowState) -> StreamStep<WorkflowState, StepEvent>
      throws baml.ExtendUnknownError<baml.errors.StepError> {  // static
    // (A) flush any buffered multiplexed frames first.
    if (st.pending.length() > 0) {
      let head: StepEvent = st.pending.0;
      let rest: StepEvent[] = st.pending.slice(1, st.pending.length());
      let st2: WorkflowState = WorkflowState {
        steps: st.steps, cursor: st.cursor, carry: st.carry, log: st.log,
        wf_span: st.wf_span, run_id: st.run_id, pending: rest,
      };
      return StreamStep<WorkflowState, StepEvent> { state: st2, emit: head, done: false };
    }

    // No more steps => the unfold is done; `finish` builds TFinal.
    if (st.cursor >= st.steps.length()) {
      return StreamStep<WorkflowState, StepEvent> { state: st, emit: null, done: true };
    }

    let step: Step = st.steps[st.cursor];
    let sid: string = step.id();

    // resume-aware re-entry: if this step was checkpointed as suspended and we
    // now have a resume value recorded, re-enter it; else fresh.
    let prior: StepRecord? = flow_get(st.log, sid);

    // open the STEP span under the workflow span — correlation root for any
    // model call this step makes.
    let step_span: SpanId = obs_start("step." + sid, st.wf_span, { "step_id": sid });

    // record the input BEFORE running (checkpoint for replay).
    flow_put(st.log, StepRecord { step_id: sid, input: st.carry, output: null, span: step_span, suspended: false });

    // The step's emit pushes inner model tokens into a LOCAL collector we then
    // splice into `pending`. We accumulate via a host cell because closures here
    // capture by value; `flow_collector` is a tiny mutable host buffer.
    let collector: TokenCollector = baml.flow.new_collector();
    let ctx: StepContext = StepContext {
      input: st.carry,
      step_span: step_span,
      resume: Workflow.resume_value(prior),
      emit: e => { baml.flow.collect(collector, e); },   // inner token -> buffer
    };

    let start_evt: StepEvent = StepEvent {
      kind: "step-start", step_id: sid, parent_id: null, span: step_span,
      token: null, output: null, suspend: null, result: null,
    };

    // Run the step. Its body may stream a model, pushing step-token frames into
    // `collector` as it goes.
    let outcome: StepOutcome = step.run(ctx) catch (e) {
      // A step that raises a typed StepError (or a model CallError, which is itself
      // foreign to this StepError channel) is recorded against the step span and
      // re-raised as a domain StepFailed tagged with the step id. The classifier
      // detail is carried for triage; deeper structure is available on `e` via the
      // span/log if the host needs it.
      let se: baml.errors.StepError => { obs_finish(step_span, {}, "step threw"); throw StepFailed { step_id: sid, detail: "step error" }; }
      _ => { obs_finish(step_span, {}, "step threw"); throw StepFailed { step_id: sid, detail: "step " + sid + " failed" }; }
    };

    // Pull whatever the step buffered (the multiplexed model tokens), tag them
    // with this step id, and queue them AFTER step-start, BEFORE step-result.
    let buffered: StepEvent[] = baml.flow.drain(collector);

    match (outcome.suspend) {
      let sp: SuspendPayload => {
        // SUSPEND: checkpoint as suspended, emit step-suspended, STOP advancing
        // (the unfold ends; the workflow is durably paused until resumed).
        flow_put(st.log, StepRecord { step_id: sid, input: st.carry, output: null, span: step_span, suspended: true });
        let susp_evt: StepEvent = StepEvent {
          kind: "step-suspended", step_id: sid, parent_id: null, span: step_span,
          token: null, output: null, suspend: sp, result: null,
        };
        obs_finish(step_span, { "suspended": "true" }, null);
        // queue: step-start, ...tokens, step-suspended; mark done.
        let queued: StepEvent[] = ([ start_evt ] + buffered) + [ susp_evt ];
        let st2: WorkflowState = WorkflowState {
          steps: st.steps, cursor: st.cursor, carry: st.carry, log: st.log,
          wf_span: st.wf_span, run_id: st.run_id,
          pending: queued.slice(1, queued.length()),  // emit the first now, buffer the rest
        };
        return StreamStep<WorkflowState, StepEvent> { state: st2, emit: queued.0, done: false };
      }
      _ => {
        // COMPLETED: record output, emit step-result, carry the output to the
        // next step.
        let out: unknown = outcome.value ?? null;
        flow_put(st.log, StepRecord { step_id: sid, input: st.carry, output: out, span: step_span, suspended: false });
        let result_evt: StepEvent = StepEvent {
          kind: "step-result", step_id: sid, parent_id: null, span: step_span,
          token: null, output: out, suspend: null, result: null,
        };
        obs_finish(step_span, {}, null);
        let queued: StepEvent[] = ([ start_evt ] + buffered) + [ result_evt ];
        let st2: WorkflowState = WorkflowState {
          steps: st.steps, cursor: st.cursor + 1, carry: out, log: st.log,
          wf_span: st.wf_span, run_id: st.run_id,
          pending: queued.slice(1, queued.length()),
        };
        return StreamStep<WorkflowState, StepEvent> { state: st2, emit: queued.0, done: false };
      }
    }
  } catch (e) {
    // Foreign host failures (durable log put/get, span open/finish) normalize into
    // the wrapper; deliberate StepErrors (StepFailed, UnknownStep) pass through.
    _ => throw baml.UnknownError.with_message<baml.errors.StepError>(e, "advancing step graph failed");
  }

  // ----- build the workflow's final typed value from the drained run -----
  function finish<T>(st: WorkflowState) -> T
      throws baml.ExtendUnknownError<baml.errors.StepError> {  // static
    obs_finish(st.wf_span, { "status": "finished" }, null);
    // The final carry is the last step's output; coerce to the workflow's
    // declared return type via SAP (the carry is `unknown`).
    baml.flow.coerce<T>(st.carry)
  } catch (e) {
    // SAP coercion of the untyped carry can reject; route it through the wrapper as
    // a StepError-channel failure rather than inventing a typed parse error here.
    _ => throw baml.UnknownError.with_message<baml.errors.StepError>(e, "coercing workflow result failed");
  }

  // ----- resume value extraction (null on a fresh step) -----
  function resume_value(prior: StepRecord?) -> unknown? {  // static
    match (prior) {
      let r: StepRecord => { if (r.suspended) { r.output } else { null } },
      _ => null,
    }
  }
}

// The unfold seed for the workflow. `pending` is the multiplexing buffer.
class WorkflowState {
  steps: Step[]
  cursor: int
  carry: unknown            // value flowing step -> step (and the workflow input at cursor 0)
  log: StepLog
  wf_span: SpanId
  run_id: string
  pending: StepEvent[]      // buffered frames (multiplexed model tokens + queued result)
}

// Net-new host buffer for collecting a step's inner model tokens before they are
// spliced into the outer stream. A tiny mutable cell — closures capture by value
// in BAML, so we cannot append to a local array from inside `ctx.emit`.
class TokenCollector { _cell: $rust_type }


// -----------------------------------------------------------------------------
// 6. The multiplexing helper a STEP body calls to forward an inner model stream
//    up into the outer step-event stream.
//
//    A step that wants live model tokens drives the model's Streaming companion
//    itself, and for each delta pushes a "step-token" StepEvent through
//    ctx.emit. This is what turns a nested token stream into outer step-token
//    frames. We give a ready-made helper so step authors don't hand-roll it.
//
//    Returns the model's validated final (TFinal of the inner stream), which the
//    step then uses as its own output.
// -----------------------------------------------------------------------------

class StepStreaming {
  // Drive an inner Streaming provider; multiplex each token up as a step-token,
  // and correlate the inner model span under THIS step's span.
  function pipe<TStream, TFinal>(
      ctx: StepContext,
      model: Provider,
      prompt: baml.llm.PromptAst,
  ) -> TFinal throws baml.ExtendUnknownError<baml.errors.StepError> {     // static
    let sid: string = baml.flow.span_step_id(ctx.step_span);  // recover the step id for tagging
    match (model) {
      let s: Streaming => {
        // Parent the model's own observability under the step span: wrap with the
        // §32 Traced combinator pointed at ctx.step_span. (Traced.stream opens a
        // model span as a CHILD of step_span — correlation done.)
        let traced: Traced = Traced { inner: model, name: "model.stream", parent: ctx.step_span, price: null, export_messages: false };
        let inner: baml.llm.Stream<TStream, TFinal> = traced.stream<TStream, TFinal>(prompt);
        while (true) {
          match (inner.next()) {
            let chunk: TStream => {
              // multiplex: one inner token -> one outer step-token frame.
              ctx.emit(StepEvent {
                kind: "step-token", step_id: sid, parent_id: null, span: ctx.step_span,
                token: baml.flow.render_token<TStream>(chunk), output: null, suspend: null, result: null,
              });
            }
            let _done: baml.llm.StreamDone => { break; }
          }
        }
        inner.final()
      }
      // The inner model does NOT stream: degrade to one .call. No step-token
      // frames are produced; the step still records its result. This mirrors the
      // §5 background note that multiplexing only applies to streaming steps.
      _ => {
        let traced: Traced = Traced { inner: model, name: "model.call", parent: ctx.step_span, price: null, export_messages: false };
        traced.call<TFinal>(prompt)
      }
    }
  } catch (e) {
    // The inner model raises CallError / StreamError — foreign to this helper's
    // StepError channel. Normalize into the wrapper, tagged with the step. The
    // app can still read the underlying model error off `.data`.
    _ => throw baml.UnknownError.with_message<baml.errors.StepError>(e, "step " + baml.flow.span_step_id(ctx.step_span) + " model pipe failed");
  }
}


// -----------------------------------------------------------------------------
// 7. StreamStep — reused verbatim from scenario 04. The per-pull result of an
//    unfold. (Cited so this file reads standalone.)
//
//   class StreamStep<S, TStream> { state: S, emit: TStream?, done: bool }
//
// And the unfold constructor, reused unchanged from scenario 04:
//
//   baml.llm.stream_unfold<S, TStream, TFinal>(seed, step, done)
//       -> baml.llm.Stream<TStream, TFinal>
//
// We instantiate TStream = StepEvent. No new Stream machinery — only a new item
// type. That is the whole reuse claim: a workflow's progress stream is a
// baml.llm.Stream like any other.
// -----------------------------------------------------------------------------

class StreamStep<S, TStream> {
  state: S
  emit: TStream?
  done: bool
}


// -----------------------------------------------------------------------------
// 8. Net-new host helpers used above (genuine boundaries):
//
//   baml.flow.input_value(prompt) -> unknown        // extract the workflow input from the PromptAst
//   baml.flow.new_collector() -> TokenCollector     // mutable token buffer
//   baml.flow.collect(c, e) -> null                 // push a StepEvent into the buffer
//   baml.flow.drain(c) -> StepEvent[]               // take + clear the buffer
//   baml.flow.coerce<T>(v: unknown) -> T            // SAP-coerce the final carry to T
//   baml.flow.span_step_id(span) -> string          // recover the step id tagged on a span
//   baml.flow.render_token<TStream>(chunk) -> string// stringify a partial for the step-token payload
//   (array helpers are real Array methods: `xs.length()` / `xs.slice(1, xs.length())`)
//
// declared here as the host seam:
// -----------------------------------------------------------------------------

function baml_flow_input_value(prompt: baml.llm.PromptAst) -> unknown { $rust_io_function }
function baml_flow_new_collector() -> TokenCollector { $rust_io_function }
function baml_flow_collect(c: TokenCollector, e: StepEvent) -> null { $rust_io_function }
function baml_flow_drain(c: TokenCollector) -> StepEvent[] { $rust_io_function }
function baml_flow_coerce<T>(v: unknown) -> T { $rust_io_function }
function baml_flow_span_step_id(span: SpanId) -> string { $rust_io_function }
function baml_flow_render_token<TStream>(chunk: TStream) -> string { $rust_io_function }
