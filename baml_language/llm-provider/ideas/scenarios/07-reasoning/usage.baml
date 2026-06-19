// ============================================================================
// Scenario 07 — Reasoning models
// usage.baml — APPLICATION-author code. Uses implement.baml.
//
// Shows: how little the app writes for the common case; how reasoning is pulled
// out-of-band with the `.think` companion; how the two control models look
// side-by-side (categorical effort vs token budget); how continuity threads
// across turns; and where portability leaks (PlainChat can't think).
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
// Diagnose("...")            -> Diagnosis           (reasoning thrown away)
// Diagnose.think("...")      -> WithReasoning<Diagnosis>   (reasoning surfaced)

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
// 3. Surfacing reasoning out-of-band: the `.think` companion.
//    The app gets the answer AND the displayable/billable reasoning blocks AND
//    the opaque continuation handle — without the answer type `Diagnosis` being
//    polluted by reasoning fields.
// ---------------------------------------------------------------------------
function DiagnoseAndShow(build_log: string) -> string throws baml.errors.LlmClient {
  let result: WithReasoning<Diagnosis> = Diagnose.think(build_log);

  let total_reasoning_tokens: int = 0;
  for (let block in result.reasoning) {
    total_reasoning_tokens = total_reasoning_tokens + block.reasoning_tokens;
    // `block.summary` is human-readable for OpenAI/Anthropic, empty for
    // encrypted-only OpenAI. Never contaminates `result.answer`.
    $rust_io_function;   // log/display block.summary in a side panel
  }

  "root cause: " + result.answer.root_cause
    + " | reasoning tokens: " + baml.string.from_int(total_reasoning_tokens)
}

// ---------------------------------------------------------------------------
// 4. Continuity across turns — the cross-turn "what's hard" inside a single file.
//    Turn 1 produces a continuation handle; turn 2 threads it back in via
//    `.resume`. The app NEVER inspects the handle — opaque round-trip.
//
//    This needs the underlying provider, not the `client` existential, because
//    `.resume` lives on the `Continuity` capability. We negotiate with a match
//    (the §5 pattern): if the client's provider can resume, use it; else error.
// ---------------------------------------------------------------------------
function MultiTurnDiagnose(first_log: string, followup: string) -> Diagnosis throws baml.errors.LlmClient {
  let p = DeepThinker();

  // Turn 1: get the answer plus the opaque continuity state.
  let turn1: WithReasoning<Diagnosis> = match (p) {
    let t: Thinks => t.think<Diagnosis>(render_prompt(first_log)),
    _ => throw baml.errors.Unsupported { message: "client cannot surface reasoning" },
  };

  match (turn1.continue_from) {
    let state: ContinuationState => {
      // Turn 2: feed the prior reasoning back in, losslessly.
      let turn2: WithReasoning<Diagnosis> = match (p) {
        let c: Continuity => c.resume<Diagnosis>(render_prompt(followup), state),
        _ => throw baml.errors.Unsupported { message: "client cannot resume reasoning" },
      };
      return turn2.answer;
    },
    _ => {
      // No continuity state (e.g. effort=none or a non-reasoning model):
      // honestly fall back to a fresh call.
      return turn1.answer;
    },
  }
}

// ---------------------------------------------------------------------------
// 5. Portability leak, shown on purpose: PlainChat has NO Thinks capability.
//    `.think` must degrade — it can still return WithReasoning<T>, but with an
//    EMPTY reasoning list and null continuation. The companion's `_` arm decides.
// ---------------------------------------------------------------------------
client CheapChat() {
  PlainChat { model: "gpt-4o-mini", api_key: env.OPENAI_API_KEY }
}

function DiagnoseCheap(build_log: string) -> WithReasoning<Diagnosis> throws baml.errors.LlmClient {
  let p = CheapChat();
  match (p) {
    let t: Thinks => t.think<Diagnosis>(render_prompt(build_log)),
    // Degrade: a non-reasoning model still answers; reasoning is just absent.
    _ => WithReasoning<Diagnosis> {
      answer: p.call<Diagnosis>(render_prompt(build_log)),
      reasoning: [],
      continue_from: null,
    },
  }
}

// ---------------------------------------------------------------------------
// 6. Combinator chain reads normally — reasoning providers are just Providers.
//    Fallback from the categorical-effort model to the token-budget model. The
//    `.think` companion routes through Fallback's runtime delegation. NOTE the
//    leak this exposes: a `Fallback` claims Thinks if ANY member does, but the
//    continuation handle from one provider can't be resumed on the other
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
//    KNOWS the provider can think + resume. Now `.think`/`.resume` need no match.
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

function DiagnoseStrict(build_log: string) -> WithReasoning<Diagnosis> throws baml.errors.LlmClient {
  // No match needed: OpenAIReasoning statically implements Thinks.
  StrictThinker().think<Diagnosis>(render_prompt(build_log))
}
