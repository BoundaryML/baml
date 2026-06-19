// ============================================================================
// Suspend / resume & human-in-the-loop — APPLICATION AUTHOR CODE
// ============================================================================
//
// What the app author writes to get a human-approval workflow on top of the
// std-lib `implement.baml`. Two providers (HTTP chat model `Anthropic` and the
// `ClaudeCode` harness) prove the SAME suspend/resume surface ports across
// transports — or expose where it leaks.
// ============================================================================

// The thing the model is producing once approved.
class Refund {
  order_id: string
  amount: float
  approved: bool
}

// The resume schema: what a human reviewer hands back. This is the first-class
// `type` the suspend point validates against (Mastra's resumeSchema).
class ApprovalDecision {
  approved: bool
  note: string?
}

// ----------------------------------------------------------------------------
// CLIENTS — pure config. The `client` sugar still applies: each is a function
// returning a Provider. Suspendability is a property of the provider we choose,
// not anything the client syntax knows about.
// ----------------------------------------------------------------------------

// A chat model that can run a gated tool loop, wrapped so the `refund` tool
// requires human sign-off, then made durable against a persistent store.
client RefundDesk() {
  Durable {
    inner: ApprovalGate {
      inner: Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 },
      gated: ["refund"],
      decision_schema: ApprovalDecision,
    },
    store: pg_run_store(),            // host-provided RunStore (Postgres compare-and-set)
    build_version: env.BUILD_SHA,     // stamped so resume can detect schema drift
  }
}

// The SAME durable/approval surface over a harness instead of an HTTP model. The
// gate here is the subprocess permission prompt, not an HTTP tool call — but the
// app author writes the identical shape. (No ApprovalGate: ClaudeCode suspends on
// its OWN permission prompts, so it is already Suspendable.)
client CodeReviewer() {
  Durable {
    inner: ClaudeCode {
      model: "claude-sonnet-4-6",
      sandbox: container,
      permission_mode: "default",     // => parks on risky actions instead of acceptEdits
      allowed_tools: ["Read", "Edit", "Bash"],
    },
    store: pg_run_store(),
    build_version: env.BUILD_SHA,
  }
}

// ----------------------------------------------------------------------------
// FUNCTIONS — the workflow surface. Because suspend/resume needs a run id, the
// app does NOT call the plain `Approve(...)` companion; it calls the durable
// start/resume statics on the client's provider. We expose two thin wrappers so
// the app code reads naturally.
// ----------------------------------------------------------------------------

// Kick off a refund request. Returns either the finished Refund OR a suspended
// RunHandle whose `.pending.payload` is what to show the reviewer.
function StartRefund(order_id: string, amount: float, run_id: string) -> Refund | RunHandle {
  let d: Durable = RefundDesk();        // the Durable provider, concrete
  d.start_run<Refund>(run_id, render_prompt(#"
    The user asked to refund order {{ order_id }} for ${{ amount }}.
    Decide whether to issue the refund and call the `refund` tool if so.
  "#))
}

// Resume after a human clicks approve/deny in the UI. EXACTLY-ONCE is enforced
// inside Durable.resume_run (the store's claim): a double-click is a no-op that
// returns the current handle rather than re-issuing the refund.
function ResumeRefund(run_id: string, decision: ApprovalDecision) -> Refund | RunHandle {
  let d: Durable = RefundDesk();
  d.resume_run<Refund>(run_id, decision)
}

// The harness variant — identical call shape, different provider underneath.
function StartCodeFix(task: string, run_id: string) -> Patch | RunHandle {
  let d: Durable = CodeReviewer();
  d.start_run<Patch>(run_id, render_prompt(#"Fix: {{ task }}"#))
}
function ResumeCodeFix(run_id: string, decision: ApprovalDecision) -> Patch | RunHandle {
  let d: Durable = CodeReviewer();
  d.resume_run<Patch>(run_id, decision)
}

// ----------------------------------------------------------------------------
// DRIVER — how an app server actually uses the above across a process boundary.
// Sketched as a function to show the ergonomics; in real life start/resume are
// two different HTTP handlers in possibly two different processes.
// ----------------------------------------------------------------------------
function DemoRefundFlow(order_id: string, amount: float) -> Refund throws baml.errors.LlmClient {
  let run_id = baml.sys.uuid();

  // 1) start. The process may now exit; state is persisted in pg_run_store.
  match (StartRefund(order_id, amount, run_id)) {
    let done: Refund => { return done; },        // model decided no approval needed
    let h: RunHandle => {
      // 2) surface h.pending.payload (the proposed ToolCall) to a human UI.
      //    h.pending.resume_schema == ApprovalDecision tells the UI how to validate.
      //    ... time passes, possibly a redeploy, possibly process restart ...

      // 3) a human approves. Resume in a (possibly different) process.
      match (ResumeRefund(run_id, ApprovalDecision { approved: true, note: "ok per policy" })) {
        let done: Refund => { return done; },
        let still: RunHandle => {
          // suspended again at a further gate, or lost the claim race (double-click).
          throw baml.errors.LlmClient { message: "still pending: " + still.run_id };
        },
      }
    },
  }
}

class Patch { diff: string }
