// =============================================================================
// 15-guardrails / usage.baml  —  APPLICATION AUTHOR CODE
//
// How an app author attaches input/output guardrails to an agent. The whole point
// is REUSE: define the guardrails once, then `.guarded(...)` any provider with
// them — no per-agent copy-pasted wrapper. We show the same guardrail set applied
// to two divergent providers (Anthropic chat + a ClaudeCode harness) to prove the
// combinator is portable across very different inner providers.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. Reusable guardrail definitions (write once, share everywhere)
// -----------------------------------------------------------------------------

// A cheap deterministic input check — no model call, no latency.
function SsnGuard() -> Guardrail {
  CodeGuard {
    guard_name: "ssn",
    pattern: "\\b\\d{3}-\\d{2}-\\d{4}\\b",
  }
}

// A model-based input guardrail: PII detection is an LLM call. The judge is a
// small, cheap model — and because the judge is itself a Provider, we retry it.
function PiiGuard() -> Guardrail {
  ModelGuard {
    guard_name: "pii",
    judge: Anthropic { model: "claude-haiku-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 256 }
             .with_retry(2),
    rubric: "Set flagged=true if the text contains personally identifying information (SSN, credit card, full home address).",
  }
}

// A model-based OUTPUT guardrail: toxicity. Same shape, different rubric.
function ToxicityGuard() -> Guardrail {
  ModelGuard {
    guard_name: "toxicity",
    judge: Anthropic { model: "claude-haiku-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 256 },
    rubric: "Set flagged=true if the text is harassing, hateful, or sexually explicit.",
  }
}


// -----------------------------------------------------------------------------
// 2. Apply the SAME guardrails to two divergent providers
// -----------------------------------------------------------------------------
//
// Provider A: an Anthropic chat model. Diverges from OpenAI in build_request
// (system as a top-level field, forced-tool for structured output, x-api-key
// header) — but Guarded never sees any of that; it wraps the existential Provider.

client AssistantChat() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 2048 }
    .guarded(
      [SsnGuard(), PiiGuard()],     // input guards: race + cancel
      [ToxicityGuard()],            // output guards: sequential
    )
}

// Provider B: a ClaudeCode HARNESS (background §8) — NOT an HttpProvider at all;
// its transport is a subprocess. The guardrail combinator is identical: the app
// author writes the exact same `.guarded(...)` chain. This is the portability win.

client CodeAgent() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: container,
    permission_mode: "acceptEdits",
    allowed_tools: ["Read", "Edit", "Bash", "Grep"],
  }
    .guarded(
      [SsnGuard(), PiiGuard()],     // same input guards
      [ToxicityGuard()],            // same output guard
    )
}


// -----------------------------------------------------------------------------
// 3. Functions that use the guarded clients
// -----------------------------------------------------------------------------

class TaxAnswer { summary: string, next_steps: string[] }

// A plain .call: input guards race the model; output guard runs after. A trip is
// a typed throw the app catches.
function FileTaxes(question: string) -> TaxAnswer {
  client: AssistantChat()
  prompt #"Help the user with their taxes. Question: {{ question }}"#
}

class Patch { files_changed: string[], diff: string }

// The harness goes through the Tools companion (.run_tools). Guarded forwards
// Tools, so input is guarded before the loop and output is guarded at the
// terminal value. Same guardrails, agentic provider.
function FixBug(task: string) -> Patch {
  client: CodeAgent()
  prompt #"Fix the bug: {{ task }}"#
}


// -----------------------------------------------------------------------------
// 4. Catching tripwires (typed, input-vs-output distinguished by TYPE)
// -----------------------------------------------------------------------------
//
// The app distinguishes an input trip from an output trip the way the SDK does —
// by catching distinct exception types. This is end-user code that would live in
// the host language, shown here in BAML's catch syntax for completeness.

function FileTaxesSafely(question: string) -> TaxAnswer {
  client: AssistantChat()
  prompt #"Help the user with their taxes. Question: {{ question }}"#
  // pseudo-handler illustrating the two typed catch arms:
  //   FileTaxes(question) catch (e) {
  //     let it: InputTripwire  => { log("blocked on input:", it.info);  fallback_answer() },
  //     let ot: OutputTripwire => { log("blocked on output:", ot.info); refusal_answer() },
  //   }
}
