// ============================================================================
// Scenario 07 — Reasoning models
// usage.baml — APPLICATION-author code. Uses implement.baml.
//
// Shows: how little the app writes for the common case; how reasoning *text* is
// pulled out-of-band with `call_with` + a `ResponseMeta` projection (NOT a bespoke
// `.think` companion); how the two control models look side-by-side (categorical
// effort vs token budget); how the continuity HANDLE rides the same projection but
// is threaded back through the `Continuity.resume` capability; and where
// portability leaks (PlainChat's response carries no ReasoningMeta).
// ============================================================================

class Diagnosis {
  root_cause: string
  fix: string
  confidence: float
}

// ---------------------------------------------------------------------------
// 1. The common case: app author writes a normal client + a normal -> T function.
//    Effort/verbosity/summary are just provider fields. Reasoning is DISCARDED
//    on the plain call (.call<T>), exactly like today.
// ---------------------------------------------------------------------------
client DeepThinker() {
  OpenAIReasoning {
    model: "gpt-5.5",
    api_key: env.OPENAI_API_KEY,
    effort: high,            // categorical knob
    verbosity: low,          // independent answer-length knob
    summary: "auto",
    include_encrypted: false,   // server-side continuity by default
  }
}

// Plain function: one prompt in, Diagnosis out. No mention of reasoning at all.
function Diagnose(build_log: string) -> Diagnosis {
  client: DeepThinker()
  prompt #"Diagnose why this build is flaky:\n{{ build_log }}"#
}
// Diagnose("...")                 -> Diagnosis     (reasoning thrown away — the 90% path)
// Diagnose.call_with("...", proj) -> (Diagnosis, U)  (reasoning surfaced via the projection)

// ---------------------------------------------------------------------------
// 2. The Anthropic side — the OTHER control model. Same `Diagnose`-shaped output,
//    but the client uses a TOKEN budget, not an effort category. Note the app
//    must also set max_tokens > thinking_budget (Anthropic constraint).
// ---------------------------------------------------------------------------
client BudgetThinker() {
  AnthropicReasoning {
    model: "claude-sonnet-4-5",
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 8192,         // REQUIRED, and must exceed the budget
    thinking_budget: 4096,    // token budget instead of effort
  }
}

function DiagnoseClaude(build_log: string) -> Diagnosis {
  client: BudgetThinker()
  prompt #"Diagnose why this build is flaky:\n{{ build_log }}"#
}

// ---------------------------------------------------------------------------
// 3. Surfacing reasoning *text* out-of-band: `call_with` + a ResponseMeta projection.
//    The app gets the answer AND the displayable/billable reasoning blocks as a
//    TUPLE — without the answer type `Diagnosis` being polluted by reasoning fields,
//    and without a bespoke `.think -> WithReasoning<T>` companion. The reasoning
//    dimension is a NOVEL dimension, so the projector reaches it by `match` and
//    falls back to `Unsupported` if the provider's response has no `ReasoningMeta`.
// ---------------------------------------------------------------------------
function DiagnoseAndShow(build_log: string) -> string
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // U here is Supported<ReasoningBlock[]>; combinators forward call_with, so this
  // survives a Fallback wrapping the client (the old `.think` companion did not).
  let (answer, reasoning): (Diagnosis, Supported<ReasoningBlock[]>) =
    Diagnose.call_with(build_log, m => match (m) {
      let rm: ReasoningMeta => rm.reasoning_blocks(),
      _                     => Unsupported { reason: "no reasoning meta on this response" },
    });

  let total_reasoning_tokens: int = 0;
  match (reasoning) {
    let blocks: ReasoningBlock[] => {
      for (let block in blocks) {
        total_reasoning_tokens = total_reasoning_tokens + block.reasoning_tokens;
        // `block.summary` is human-readable for OpenAI/Anthropic, empty for
        // encrypted-only OpenAI. Never contaminates `answer`.
        $rust_io_function;   // log/display block.summary in a side panel
      }
    },
    let u: Unsupported => { $rust_io_function; },   // note "no reasoning" in the side panel
  }

  "root cause: " + answer.root_cause
    + " | reasoning tokens: " + total_reasoning_tokens.to_string()
}

// ---------------------------------------------------------------------------
// 4. Continuity across turns — the cross-turn "what's hard" inside a single file.
//    The PARTITION is visible here: extracting the continuation HANDLE is a product
//    sidecar (it accompanies turn 1's answer), so it rides `call_with` + a
//    `ContinuationMeta` projection. But THREADING it back is a stateful round-trip,
//    so turn 2 uses the `Continuity.resume` capability — `(T, U)` cannot express
//    "carry this state into the next turn." The app NEVER inspects the handle.
//
//    `.resume` needs the `Continuity` capability, negotiated with a match (the §5
//    pattern): if the client's provider can resume, use it; else error.
// ---------------------------------------------------------------------------
// This function spans TWO capability channels: turn 1 rides `call_with` (CallError)
// and turn 2 rides `resume` (ContinuityError). The channel is their union, plus the
// universal wrapper. (`baml.errors.Unsupported` implements every capability error
// interface, so the `_` arm below is legal on either side.)
function MultiTurnDiagnose(first_log: string, followup: string) -> Diagnosis
    throws baml.ExtendUnknownError<baml.errors.CallError | baml.errors.ContinuityError> {
  let p = DeepThinker();

  // Turn 1: get the answer AND the opaque continuity handle as a tuple, projected
  // off the normalized ResponseMeta. No `.think`, no `WithReasoning<T>` carrier.
  let (answer1, cont): (Diagnosis, Supported<ContinuationState>) =
    p.call_with<Diagnosis, Supported<ContinuationState>>(render_prompt(first_log),
      m => match (m) {
        let cm: ContinuationMeta => cm.continuation(),
        _                        => Unsupported { reason: "no continuity on this response" },
      });

  match (cont) {
    let state: ContinuationState => {
      // Turn 2: feed the prior reasoning back in, losslessly — the STATEFUL half,
      // on the Continuity capability (NOT a sidecar). We only need the answer here.
      let answer2: Diagnosis = match (p) {
        let c: Continuity => c.resume<Diagnosis>(render_prompt(followup), state),
        _ => throw baml.errors.Unsupported { message: "client cannot resume reasoning" },
      };
      return answer2;
    },
    _ => {
      // No continuity state (e.g. effort=none or a non-reasoning model):
      // honestly fall back to turn 1's answer.
      return answer1;
    },
  }
}

// ---------------------------------------------------------------------------
// 5. Portability leak, shown on purpose: PlainChat's response carries NO
//    ReasoningMeta. The degradation is now UNIFORM — there is no separate `.think`
//    path to fall off. The SAME `call_with` projection runs against every provider;
//    PlainChat's response simply misses the `ReasoningMeta` match and lands on the
//    projector's `_` arm (`Unsupported`). No companion-vs-call branching.
// ---------------------------------------------------------------------------
client CheapChat() {
  PlainChat { model: "gpt-4o-mini", api_key: env.OPENAI_API_KEY }
}

function DiagnoseCheap(build_log: string) -> (Diagnosis, Supported<ReasoningBlock[]>)
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // Identical to the reasoning-model call site — the provider's response decides
  // whether the projection yields blocks or `Unsupported`. PlainChat yields the latter.
  CheapChat().call_with<Diagnosis, Supported<ReasoningBlock[]>>(render_prompt(build_log),
    m => match (m) {
      let rm: ReasoningMeta => rm.reasoning_blocks(),
      _                     => Unsupported { reason: "non-reasoning model" },
    })
}

// ---------------------------------------------------------------------------
// 6. Combinator chain reads normally — reasoning providers are just Providers.
//    Fallback from the categorical-effort model to the token-budget model.
//    Because `call_with` is an `HttpProvider` capability that Fallback forwards,
//    the reasoning projection routes through Fallback's delegation and the sidecar
//    is NOT dropped (the win over the old
//    `.think` companion). NOTE the leak this still exposes: the continuation
//    HANDLE projected from one member can't be resumed on the other
//    (origin_provider guard throws) — see evaluation.md (d).
// ---------------------------------------------------------------------------
client ResilientThinker() {
  DeepThinker().fallback_to(BudgetThinker())
}

function DiagnoseResilient(build_log: string) -> Diagnosis {
  client: ResilientThinker()
  prompt #"Diagnose why this build is flaky:\n{{ build_log }}"#
}

// ---------------------------------------------------------------------------
// 7. The compile-time escape hatch: drop the `client` sugar so the type system
//    KNOWS the concrete provider. The concrete `OpenAIReasoning` statically IS an
//    `HttpProvider`, so `call_with` is available with no companion match needed to
//    project reasoning; `.resume` (the `Continuity` capability it also implements)
//    likewise needs no match.
// ---------------------------------------------------------------------------
function StrictThinker() -> OpenAIReasoning {
  OpenAIReasoning {
    model: "gpt-5.5",
    api_key: env.OPENAI_API_KEY,
    effort: xhigh,
    verbosity: medium,
    summary: "detailed",
    include_encrypted: true,    // client-side continuity: WE hold the encrypted state
  }
}

function DiagnoseStrict(build_log: string) -> (Diagnosis, Supported<ReasoningBlock[]>)
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // call_with is statically available because StrictThinker returns a concrete
  // HttpProvider; the projection still matches ReasoningMeta because we KNOW
  // OpenAIReasoningResponse implements it.
  StrictThinker().call_with<Diagnosis, Supported<ReasoningBlock[]>>(render_prompt(build_log),
    m => match (m) {
      let rm: ReasoningMeta => rm.reasoning_blocks(),
      _                     => Unsupported { reason: "unreachable: openai reasoning" },
    })
}

// ---------------------------------------------------------------------------
// 8. Catching across the two channels, most-specific-first. This is the only
//    place the app sees the NEW error surface directly. The cross-provider
//    continuity failure (the Fallback leak in evaluation.md (d)) is a CONCRETE,
//    catchable error now — not an opaque message string. The function does NOT
//    re-throw, so it declares no `throws`.
// ---------------------------------------------------------------------------
function MultiTurnSafe(first_log: string, followup: string) -> string {
  MultiTurnDiagnose(first_log, followup) catch (e) {
    // Concrete first: the origin-mismatch guard fired (resumed foreign state).
    let x: CrossProviderContinuity =>
      "[cannot resume " + x.state_provider + " state on " + x.expected_provider + "]";
    // Interface: any other continuity failure — branch on the new classifier.
    let c: baml.errors.ContinuityError =>
      if (c.is_cross_provider()) { "[cross-provider continuity]" }
      else if (c.is_network_error()) { "[continuity network error — retry]" }
      else { "[continuity failed]" };
    // Interface: a turn-1 (call-path) failure, triaged without the concrete class.
    let call_err: baml.errors.CallError =>
      if (call_err.is_rate_limit()) { "[rate limited]" } else { "[call failed]" };
    // Escape hatch: anything foreign that got normalized into the wrapper.
    let u: baml.UnknownError => "[unexpected: " + u.message.join("; ") + "]";
  }
}
