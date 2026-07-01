// ─────────────────────────────────────────────────────────────────────────────
// usage.baml — Model cascades & semantic routing  (APPLICATION-author code)
//
// Two divergent backends appear throughout: OpenAI (has logprobs) and Anthropic
// (does not). The point is to show where portability holds and where it leaks.
// ─────────────────────────────────────────────────────────────────────────────

class Answer {
  text: string
}

// ─── leaf clients: zero-arg providers (config only) ───────────────────────────
client Mini() {
  OpenAI { model: "gpt-4.1-mini", api_key: env.OPENAI_API_KEY, logprobs: false }
}
client Full() {
  OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, logprobs: false }
}
client Haiku() {
  Anthropic { model: "claude-haiku-4", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}
client Opus() {
  Anthropic { model: "claude-opus-4", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
}


// ─────────────────────────────────────────────────────────────────────────────
// (1) Cheap-first cascade — escalate on low confidence.
//
// Fluent form: reads like with_retry/fallback_to. Because Mini() is an OpenAI with
// logprobs, the confidence gate is a REAL signal. Most traffic settles on Mini.
// ─────────────────────────────────────────────────────────────────────────────
client SmartAnswer() {
  Mini().escalate_to(Full(), 0.72)
}

function Answer1(q: string) -> Answer {
  client: SmartAnswer()
  prompt #"{{ q }}"#
}

// Same cascade, but cheap tier is Anthropic Haiku, which has NO logprobs. This
// COMPILES and runs — but at runtime the Cascade's `match (self.cheap)` finds a
// ConfidenceProvider whose `call_scored` returns confidence=1.0 source="none",
// so it NEVER escalates. The cascade silently becomes "always Haiku". This is the
// leak the background calls out; usage looks identical, behavior is degraded.
client SmartAnswerAnthropic() {
  Haiku().escalate_to(Opus(), 0.72)
}

function Answer2(q: string) -> Answer {
  client: SmartAnswerAnthropic()
  prompt #"{{ q }}"#
}


// ─────────────────────────────────────────────────────────────────────────────
// (2) Judge-gated escalation — the honest path for no-logprob backends.
//
// Haiku produces, a (cheap) judge scores, Opus rescues below threshold. Works
// uniformly across providers because it does not depend on logprobs — at the
// cost of a billed judge call on every request.
// ─────────────────────────────────────────────────────────────────────────────
client JudgedAnswer() {
  Haiku().judged_by(Mini(), Opus(), 0.6)   // judge = cheap OpenAI mini, strong = Opus
}

function Answer3(q: string) -> Answer {
  client: JudgedAnswer()
  prompt #"{{ q }}"#
}


// ─────────────────────────────────────────────────────────────────────────────
// (3a) Semantic / classifier routing as a DYNAMIC CLIENT.
//
// This is the proposal's headline claim: a router that picks a Provider per
// request is JUST a function returning a Provider (client sugar, §3). No
// combinator class, no new primitive. The classifier here is a cheap upfront
// LLM call whose result chooses the tier.
//
// NOTE: the client takes the request text as an arg, so the routing decision can
// depend on the request. This is impossible with a static client block.
// ─────────────────────────────────────────────────────────────────────────────
client RoutedByClassifier(q: string) {
  // classifyDifficulty is itself a billed call (router-is-a-call). It runs at
  // CLIENT-construction time, before the main generation.
  if (classify_difficulty(q) == "hard") { Full() } else { Mini() }
}

function Answer4(q: string) -> Answer {
  client: RoutedByClassifier(q)     // the client clause takes any provider expression, incl. args
  prompt #"{{ q }}"#
}

// the classifier is an ordinary BAML function over its own (cheap) client.
function classify_difficulty(q: string) -> string {
  client: Mini()
  prompt #"
    Classify the difficulty of answering this request as exactly one word,
    either "easy" or "hard":
    {{ q }}
  "#
}


// ─────────────────────────────────────────────────────────────────────────────
// (3b) Semantic routing over labeled exemplars — the stateful SemanticRouter
// combinator. Here the "classifier" is embedding KNN, not a generation.
// ─────────────────────────────────────────────────────────────────────────────
client ExemplarRouted() {
  SemanticRouter {
    exemplars: [
      Exemplar { text: "what's 2+2",                 tier: "easy" },
      Exemplar { text: "reset my password",          tier: "easy" },
      Exemplar { text: "prove the Riemann hypothesis", tier: "hard" },
      Exemplar { text: "audit this contract for risk", tier: "hard" },
    ],
    easy: Mini(),
    hard: Opus(),
  }
}

function Answer5(q: string) -> Answer {
  client: ExemplarRouted()
  prompt #"{{ q }}"#
}


// ─────────────────────────────────────────────────────────────────────────────
// (4) Composition: a cascade WHOSE strong tier is itself judge-gated, behind a
// retry. Reads as a combinator chain because every layer returns a Provider.
// ─────────────────────────────────────────────────────────────────────────────
client Layered() {
  Mini()
    .escalate_to(Haiku().judged_by(Mini(), Opus(), 0.6), 0.72)
    .with_retry(2)
}

function Answer6(q: string) -> Answer {
  client: Layered()
  prompt #"{{ q }}"#
}
