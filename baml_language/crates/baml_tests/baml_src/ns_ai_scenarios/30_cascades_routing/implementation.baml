// scenario 30 — cascades & routing: ../../../../../llm-provider/ideas/scenarios/30-cascades-routing/README.md
//   implementation.baml — the LIBRARY surface (what a std-lib / app-library author writes).
//
// Cost-driven model selection: handle most requests on a cheap tier and reach an
// expensive one only for the hard cases. The design draws three shapes; all three
// land on the REAL surface as ORDINARY BAML over plain LLM functions (per the
// "prefer LLM functions" directive) rather than the design's bespoke
// `ConfidenceProvider`/`Cascade`-`implements HttpProvider` combinators:
//
//   (1) cheap-first CASCADE, escalate on a checkable QUALITY gate       -> s30_gated_cascade
//   (2) confidence-gated cascade (self-reported signal, design's Scored) -> s30_confidence_cascade
//   (3) JUDGE-gated escalation (a separate billed model scores cheap)    -> s30_judge_gated
//   (4) semantic / classifier ROUTING (task-class -> Provider map)       -> s30_route / s30_routed_answer
//
// The load-bearing distinction from scenario 29's Fallback: Fallback escalates on a
// THROWN error (failure-driven); a Cascade escalates on the QUALITY of a *successful*
// cheap answer (the gate can fail even though nothing threw). Routing picks the tier
// BEFORE generation, off the request itself — the design's headline "routing is just a
// function returning a Provider" (client-as-function).


// ─── the tiered stages are the SAME LLM function, swapped by `client =` ─────────
// The desugar lowers each body to `baml.ai.drive_call<T>(client, …$render_prompt(…))`;
// the driver functions below compose these by overriding `client` per stage. We
// default to a dead client so every offline test passes an explicit deterministic
// provider (design §a: the swap is a one-line call-site `client =` override).
client<llm> DeadS30 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

/// The plain answer stage — text in, text out. Cheap and strong tiers are THIS
/// function with different clients (the cascade never looks inside a tier).
function s30_answer(q: string) -> string {
  client DeadS30
  prompt #"
    {{ _.role("system") }}
    You are a terse assistant. Answer in one short line.

    {{ _.role("user") }}
    {{ q }}
  "#
}

/// The scored answer stage — the design's `Scored<T>`: a value WITH a
/// self-reported 0..1 confidence. On the real surface the signal is just a field
/// on the structured output (app logic), not a `ConfidenceProvider` capability.
class s30_ScoredAnswer {
  answer: string,
  confidence: float,   // 0..1, self-reported by the model
}

function s30_answer_scored(q: string) -> s30_ScoredAnswer {
  client DeadS30
  prompt #"
    {{ _.role("system") }}
    Answer concisely, then self-report your confidence in [0,1].
    {{ ctx.output_format }}

    {{ _.role("user") }}
    {{ q }}
  "#
}

/// The judge stage (design's `Verdict`): a SEPARATE billed model scores the cheap
/// candidate. Reused across providers because it does not depend on logprobs — at
/// the cost of a billed judge call on every request.
class s30_Verdict {
  score: float,        // 0..1
  reason: string,
}

function s30_judge(q: string, candidate: string) -> s30_Verdict {
  client DeadS30
  prompt #"
    {{ _.role("system") }}
    Score how well the ANSWER addresses the QUESTION, in [0,1], with a one-line reason.
    {{ ctx.output_format }}

    {{ _.role("user") }}
    QUESTION: {{ q }}
    ANSWER: {{ candidate }}
  "#
}

/// The classifier stage — a cheap upfront call that picks the tier. The design's
/// "semantic routing" over exemplars collapses to this on the real surface.
enum s30_TaskClass { easy, hard }

function s30_classify_difficulty(q: string) -> s30_TaskClass {
  client DeadS30
  prompt #"
    {{ _.role("system") }}
    Classify how hard this request is to answer well: exactly "easy" or "hard".
    {{ ctx.output_format }}

    {{ _.role("user") }}
    {{ q }}
  "#
}


// ─── the outcome carrier — records WHICH tier answered (the cost breadcrumb) ────
// The design's evaluation flags that a combinator "surfaces one member's answer and
// never records which member"; returning the tier makes the cost story assertable
// (cheap-path proven to never touch the strong tier).
class s30_CascadeOutcome {
  answer: string,
  escalated: bool,     // true iff the strong tier was reached
  tier: string,        // "cheap" | "strong" — observability
}


// ─────────────────────────────────────────────────────────────────────────────
// (1) Cheap-first cascade, escalate on a CHECKABLE gate.
//
// The gate is app logic (design: "the escalation judge/confidence check is itself
// app logic"): does the cheap answer contain a required token? A SUCCESSFUL cheap
// answer that fails the gate STILL escalates — that is the quality-driven vs
// failure-driven distinction from Fallback.
// ─────────────────────────────────────────────────────────────────────────────
function s30_gated_cascade(
  cheap: baml.ai.Provider,
  strong: baml.ai.Provider,
  q: string,
  must_include: string,
) -> s30_CascadeOutcome
    throws baml.errors.CallError | baml.errors.StreamError | baml.errors.UnknownError | baml.errors.InvalidArgument | baml.errors.LlmClient | baml.errors.RenderPrompt {
  let first = s30_answer(q, client = cheap);
  if (first.to_lower_case().includes(must_include.to_lower_case())) {
    log.info("30 cascade: cheap answer passed the gate — settled on cheap tier");
    return s30_CascadeOutcome { answer: first, escalated: false, tier: "cheap" };
  }
  log.info("30 cascade: cheap answer failed the gate — escalating to strong tier");
  let second = s30_answer(q, client = strong);
  s30_CascadeOutcome { answer: second, escalated: true, tier: "strong" }
}


// ─────────────────────────────────────────────────────────────────────────────
// (2) Confidence-gated cascade — the design's `Scored`/`ConfidenceProvider` shape,
// with the signal as a plain structured field. Escalate when the cheap tier's
// self-reported confidence is below `threshold`.
//
// The design's leak (evaluation §c) is inherent here too: a self-reported number
// is "presence, not calibration" — a backend that always reports 1.0 silently
// never escalates. We surface `source`/`tier` so the regression is observable, not
// silent (design's honest mitigation).
// ─────────────────────────────────────────────────────────────────────────────
function s30_confidence_cascade(
  cheap: baml.ai.Provider,
  strong: baml.ai.Provider,
  q: string,
  threshold: float,
) -> s30_CascadeOutcome
    throws baml.errors.CallError | baml.errors.StreamError | baml.errors.UnknownError | baml.errors.InvalidArgument | baml.errors.LlmClient | baml.errors.RenderPrompt {
  let scored = s30_answer_scored(q, client = cheap);
  log.info("30 confidence-cascade: cheap confidence=" + scored.confidence.to_string() + " threshold=" + threshold.to_string());
  if (scored.confidence >= threshold) {
    log.info("30 confidence-cascade: confident — settled on cheap tier");
    return s30_CascadeOutcome { answer: scored.answer, escalated: false, tier: "cheap" };
  }
  log.info("30 confidence-cascade: unsure — escalating to strong tier");
  let second = s30_answer(q, client = strong);
  s30_CascadeOutcome { answer: second, escalated: true, tier: "strong" }
}


// ─────────────────────────────────────────────────────────────────────────────
// (3) Judge-gated escalation — the honest path for backends without a native
// confidence signal. Cheap produces; a SEPARATE judge scores; below `min_score`
// the strong tier rescues. Worst case = three billed calls (cheap + judge + strong).
// ─────────────────────────────────────────────────────────────────────────────
function s30_judge_gated(
  cheap: baml.ai.Provider,
  judge: baml.ai.Provider,
  strong: baml.ai.Provider,
  q: string,
  min_score: float,
) -> s30_CascadeOutcome
    throws baml.errors.CallError | baml.errors.StreamError | baml.errors.UnknownError | baml.errors.InvalidArgument | baml.errors.LlmClient | baml.errors.RenderPrompt {
  let candidate = s30_answer(q, client = cheap);
  let v = s30_judge(q, candidate, client = judge);
  log.info("30 judge: score=" + v.score.to_string() + " reason=" + v.reason);
  if (v.score >= min_score) {
    log.info("30 judge: cheap candidate accepted");
    return s30_CascadeOutcome { answer: candidate, escalated: false, tier: "cheap" };
  }
  log.info("30 judge: below threshold — escalating to strong tier");
  let second = s30_answer(q, client = strong);
  s30_CascadeOutcome { answer: second, escalated: true, tier: "strong" }
}


// ─────────────────────────────────────────────────────────────────────────────
// (4) Semantic / classifier routing — the design's headline: routing is JUST a
// function returning a Provider (client-as-function). Two pieces:
//   - s30_route: the task-class -> Provider MAP, plain BAML, no LLM call.
//   - s30_routed_answer: a cheap classifier picks the tier, THEN we generate once
//     on the chosen tier (never a wasted cheap generation on a known-hard request).
// ─────────────────────────────────────────────────────────────────────────────
function s30_route(
  cls: s30_TaskClass,
  easy: baml.ai.Provider,
  hard: baml.ai.Provider,
) -> baml.ai.Provider throws never {
  match (cls) {
    s30_TaskClass.easy => easy,
    s30_TaskClass.hard => hard,
  }
}

function s30_routed_answer(
  classifier: baml.ai.Provider,
  easy: baml.ai.Provider,
  hard: baml.ai.Provider,
  q: string,
) -> s30_CascadeOutcome
    throws baml.errors.CallError | baml.errors.StreamError | baml.errors.UnknownError | baml.errors.InvalidArgument | baml.errors.LlmClient | baml.errors.RenderPrompt {
  let cls = s30_classify_difficulty(q, client = classifier);
  let tier = match (cls) { s30_TaskClass.easy => "easy", s30_TaskClass.hard => "hard" };
  log.info("30 router: classified as " + tier + " — dispatching to that tier");
  let chosen = s30_route(cls, easy, hard);
  let answer = s30_answer(q, client = chosen);
  s30_CascadeOutcome { answer: answer, escalated: cls == s30_TaskClass.hard, tier: tier }
}


// ─── cost-story fixture — counts wire calls to prove the cheap path is cheap ────
// The design's cost claim ("only the residual pays the premium") is assertable by
// counting: a settled cascade calls the cheap tier once and the strong tier zero
// times. Mirrors 29's CountingEcho with an s30_ prefix.
class s30_CountingProvider {
  reply: string,
  calls: int,

  implements baml.ai.Provider {}

  implements baml.ai.HttpProvider {
    type Body = string

    function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
      throw baml.errors.UnknownError { data: null, message: ["s30 counting: no request"] }
    }
    function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
      throw baml.errors.UnknownError { data: null, message: ["s30 counting: no send"] }
    }
    function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
      baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["s30 counting parse failed"] } }
    }
    function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
      baml.ai.LegacyMeta {}
    }
    function call_messages_with<T, V, E2>(
      self,
      messages: baml.ai.ChatMessage[],
      project: (baml.ai.ResponseMeta) -> V throws E2,
    ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
      self.calls = self.calls + 1;
      let value: T = baml.sap.parse<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s30 counting parse failed"] },
      };
      baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
    }
  }
}
