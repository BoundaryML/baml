// =============================================================================
// usage.baml — APPLICATION-author code: building and OBSERVING a workflow.
//
// A 3-step research workflow:
//   1. fetch    — pure I/O, no model (a deterministic step)
//   2. extract  — calls OpenAI with STREAMING; tokens are multiplexed up into
//                 the workflow step-event stream
//   3. review   — calls Anthropic; suspends for human approval before finishing
//
// Two providers that DIVERGE (OpenAI SSE deltas vs Anthropic event grammar) are
// used inside steps. The app sees ONE step-event stream and never the wire
// difference — the §04 stream readers + §32 spans do the work below the seam.
//
// Shows:
//   - how little the app writes to RUN the workflow (it's just `.call`)
//   - subscribing to per-step events with .stream_steps (the Steppable companion)
//   - multiplexed model tokens arriving as step-token frames inside step 2
//   - a step-suspended event pausing the run for approval
//   - per-step replay/time-travel via .replay_step
// =============================================================================


// ---- the workflow's typed shapes -------------------------------------------

class Doc      { url: string, text: string }
class Findings { summary: string, citations: string[] }
class Review   { findings: Findings, approved: bool }


// ---- step 1: deterministic fetch, NO model. -------------------------------
// A workflow step need not touch a model at all — it is just a Provider-free
// activity. (This is why a step's body is `StepContext -> StepOutcome`, not a
// model call.) Replay of this step is trivially safe: it is pure given its input
// IF the fetch is cached; we flag the I/O nondeterminism in evaluation.md.

class FetchStep {
  implements Step {
    function id(self) -> string { "fetch" }
    function run(self, ctx: StepContext) -> StepOutcome throws baml.errors.LlmClient {
      let url: string = baml.flow.as_string(ctx.input);     // the workflow input
      let text: string = http_get_text(url) catch (e) { _ => { throw baml.errors.LlmClient { message: "fetch failed" }; } };
      StepOutcome { value: Doc { url: url, text: text }, suspend: null }
    }
  }
}


// ---- step 2: extract findings, STREAMING the model. ------------------------
// This is the multiplexing teaching point. The step drives OpenAI's Streaming
// companion via StepStreaming.pipe, which forwards each model delta UP into the
// workflow stream as a step-token frame tagged "extract", and nests the model
// span under the step span. The app author writes almost nothing — pipe does it.

class ExtractStep {
  model: Provider           // injected: OpenAI (SSE). Could be Gemini (chunked JSON) — same code.
  implements Step {
    function id(self) -> string { "extract" }
    function run(self, ctx: StepContext) -> StepOutcome throws baml.errors.LlmClient {
      let doc: Doc = baml.flow.as_doc(ctx.input);
      // TStream = FindingsPartial (deep-partial streamed to the UI), TFinal = Findings.
      // Every token the model emits becomes a step-token in the OUTER stream.
      let findings: Findings = StepStreaming.pipe<FindingsPartial, Findings>(
        ctx,
        self.model,
        render_prompt(doc.text),
      );
      StepOutcome { value: findings, suspend: null }
    }
  }
}

class FindingsPartial {     // the deep-partial surfaced while streaming
  summary: string?
  citations: string[]?
}


// ---- step 3: review, then SUSPEND for human approval. ----------------------
// Calls Anthropic (NON-streaming here, to show a step that does a plain .call —
// still correlated under the step span via Traced) and then suspends. On first
// entry it produces a draft and suspends; on resume (ctx.resume populated) it
// finalizes with the human's approval decision.

class ReviewStep {
  model: Provider           // Anthropic
  implements Step {
    function id(self) -> string { "review" }
    function run(self, ctx: StepContext) -> StepOutcome throws baml.errors.LlmClient {
      // RE-ENTRY: if we have a resume value, the human already approved/rejected.
      match (ctx.resume) {
        let decision: bool => {
          let findings: Findings = baml.flow.as_findings(ctx.input);
          return StepOutcome { value: Review { findings: findings, approved: decision }, suspend: null };
        }
        _ => { }   // first entry, fall through
      }

      // FIRST ENTRY: do a plain (non-streaming) model call, correlated under the
      // step span, then suspend awaiting human approval.
      let findings: Findings = baml.flow.as_findings(ctx.input);
      let traced: Traced = Traced { inner: self.model, name: "review.model", parent: ctx.step_span, price: null, export_messages: false };
      let _draft: string = traced.call<string>(render_prompt(findings.summary));   // e.g. risk notes
      StepOutcome {
        value: null,
        suspend: SuspendPayload {
          reason: "approval",
          schema: reflect.type_of<bool>(),       // resume must supply a bool decision
          prompt: "Approve these findings for publication?",
        },
      }
    }
  }
}


// ---- assembling the workflow as a CLIENT. ----------------------------------
// A `client` is sugar for `function -> Provider`; a Workflow IS a Provider, so a
// workflow IS a client. The two divergent model providers are injected into the
// steps as fields — config only.

client ResearchFlow() {
  Workflow {
    name: "research",
    steps: [
      FetchStep {},
      ExtractStep { model: OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY } },
      ReviewStep  { model: Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 } },
    ],
  }
}

// A workflow composes with combinators like any provider: trace the WHOLE run as
// one logical call, with retries inside. (The per-step spans still nest under the
// workflow span; this outer Traced span brackets the run for top-level latency.)
client ResilientResearch() {
  Workflow {
    name: "research",
    steps: [
      FetchStep {},
      ExtractStep { model: OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY } },
      ReviewStep  { model: Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 } },
    ],
  }.with_retry(1).with_traced("research.run", null)
}


// ---- (A) RUN to result — the workflow is just a Provider. -------------------
// The app function uses the client like any other; .call drains the step stream
// internally and returns the typed Review. Zero observability code needed.

function Research(url: string) -> Review {
  client: ResearchFlow()
  prompt #"{{ url }}"#
}


// ---- (B) OBSERVE — subscribe to the per-step hierarchy. --------------------
// This is the Steppable companion: Research.steps(url) yields the step-event
// stream. We render a progress UI: step-start/result for the hierarchy, and the
// MULTIPLEXED step-token frames from step 2's streaming model — all in one
// ordered feed.

function watch_research(url: string) -> Review throws baml.errors.LlmClient {
  // `.steps` is the Steppable companion generated from Research, exactly like
  // `.stream` is the Streaming companion. TFinal = Review.
  let s: baml.llm.Stream<StepEvent, Review> = Research.steps<Review>(url);

  while (true) {
    match (s.next()) {
      let e: StepEvent => {
        if (e.kind == "step-start") {
          ui_step_start(e.step_id);
        }
        if (e.kind == "step-token") {
          // a model delta from INSIDE step 2, multiplexed up. The UI appends it
          // under the "extract" step's panel — hierarchy preserved via step_id.
          ui_step_token(e.step_id, e.token ?? "");
        }
        if (e.kind == "step-result") {
          ui_step_result(e.step_id);
        }
        if (e.kind == "step-suspended") {
          // step 3 paused: surface the approval ask. We DON'T resume here; the
          // run is durably suspended and `s` ends. Resume is a separate call (C).
          match (e.suspend) {
            let sp: SuspendPayload => { ui_request_approval(e.step_id, sp.prompt); }
            _ => { }
          }
        }
      }
      let _done: baml.llm.StreamDone => { break; }
    }
  }
  // final() validates the workflow's return type. If the workflow suspended,
  // final() throws "suspended" — the app catches and waits for the human.
  s.final()
}


// ---- (C) RESUME after approval — a fresh run that replays committed steps. --
// Mastra-style resume({ step, resumeData }). Here we re-run the workflow with the
// recorded run id; committed steps replay from the log, and the suspended
// `review` step re-enters with the human decision as ctx.resume.

function approve_and_finish(run_id: string, approved: bool) -> Review throws baml.errors.LlmClient {
  let p = ResearchFlow_provider();              // the client body as a provider value
  match (p) {
    let w: Steppable => {
      // record the human decision into the suspended step's checkpoint, then
      // replay JUST that step against its recorded input (time-travel of one step).
      baml.flow.record_resume(run_id, "review", approved);
      let outcome: StepOutcome = w.replay_step<Review>(run_id, "review") catch (e) {
        _ => { throw baml.errors.LlmClient { message: "resume failed" }; }
      };
      baml.flow.as_review(outcome.value ?? null)
    }
    _ => throw baml.errors.Unsupported { message: "client is not a steppable workflow" },
  }
}


// ---- (D) TIME-TRAVEL — re-run ONE step in isolation to debug it. -----------
// "Re-run this step": replay the flaky `extract` step against its recorded input
// without re-running fetch or review. Replay fidelity depends on step purity —
// here extract calls a model, so it is NOT deterministic; replay gives a fresh
// generation, which is exactly the §5 caveat. Useful for "does this step still
// behave?", not for byte-identical reproduction.

function debug_extract(run_id: string) -> Findings throws baml.errors.LlmClient {
  let p = ResearchFlow_provider();
  match (p) {
    let w: Steppable => {
      let outcome: StepOutcome = w.replay_step<Findings>(run_id, "extract") catch (e) {
        _ => { throw baml.errors.LlmClient { message: "replay failed" }; }
      };
      baml.flow.as_findings(outcome.value ?? null)
    }
    _ => throw baml.errors.Unsupported { message: "not steppable" },
  }
}


// ---- host-side sinks the app provides (declared extern) --------------------
function http_get_text(url: string) -> string throws baml.errors.LlmClient { $rust_io_function }
function ui_step_start(step_id: string) -> null { $rust_io_function }
function ui_step_token(step_id: string, token: string) -> null { $rust_io_function }
function ui_step_result(step_id: string) -> null { $rust_io_function }
function ui_request_approval(step_id: string, prompt: string) -> null { $rust_io_function }

// app-side coercions over the workflow's `unknown` carry (host-backed casts):
function baml_flow_as_string(v: unknown) -> string { $rust_io_function }
function baml_flow_as_doc(v: unknown) -> Doc { $rust_io_function }
function baml_flow_as_findings(v: unknown) -> Findings { $rust_io_function }
function baml_flow_as_review(v: unknown) -> Review { $rust_io_function }
function baml_flow_record_resume(run_id: string, step_id: string, decision: bool) -> null { $rust_io_function }
