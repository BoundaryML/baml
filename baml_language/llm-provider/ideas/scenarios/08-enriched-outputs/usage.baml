// ============================================================================
// Scenario 08 — Enriched outputs: APPLICATION-author code.
//
// What the app author writes to (a) score a classification with logprobs and
// (b) get a sourced answer. The interesting part is what they CAN'T write
// portably: there is no `Foo.cited_or_grounded(...)` companion, because there
// is no shared citation type. The app picks a capability and lives with the
// provider divergence — or normalizes by hand.
// ============================================================================

class Sentiment {
  label: string          // "positive" | "negative" | "neutral"
}

class Answer {
  text: string
}


// ---------------------------------------------------------------------------
// Clients. Plain config; enriched output is NOT a client field — it's a
// companion on the function (.logprobs / .cited / .grounded / .annotated).
// ---------------------------------------------------------------------------
client GptScored() {
  OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY }
}

client GeminiScored() {
  Gemini { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY }
}

client Claude() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}

// A fallback whose FIRST member can't score. The .logprobs companion will skip
// Anthropic and route to OpenAI; if neither were Scored it would throw.
client Resilient() {
  Claude().fallback_to(OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY })
}


// ---------------------------------------------------------------------------
// (1) LOGPROBS — choice-scoring. Background's exact use case: compare the
// confidence of the answer instead of trusting the sampled word.
//
// `.logprobs` is the capability companion. It MATCHES the client's provider
// against `Scored`. There is no `.call` fallback — if the provider can't score,
// the companion throws Unsupported. That is the honest binary.
// ---------------------------------------------------------------------------
function Classify(review: string) -> Sentiment {
  client: GptScored()
  prompt #"Is this review positive, negative, or neutral? {{ review }}"#
}

// Generated companion (shown longhand to make the match explicit):
function Classify$logprobs(review: string, top: int) -> Scored$Result<Sentiment> {
  let p = GptScored();
  match (p) {
    let s: Scored => s.logprobs<Sentiment>(render_prompt(review), top),
    _            => throw baml.errors.Unsupported { message: "client cannot expose logprobs" },
  }
}

// App code: score the classification, threshold on confidence.
function ConfidentClassify(review: string) -> Sentiment throws baml.errors.LlmClient {
  let r: Scored$Result<Sentiment> = Classify$logprobs(review, 5);
  // r.scores.tokens[i].logprob is per-token; for a one-word label the label token's
  // logprob is the confidence. (Tokenizer-bound: a multi-token label must be summed.)
  let confidence: float = score_of_label(r.value.label, r.scores);
  if (confidence < -2.3) {     // ~10% probability floor
    throw baml.errors.LlmClient { message: "low-confidence classification" };
  }
  return r.value;
}

function score_of_label(label: string, scores: Logprobs) -> float { $rust_io_function }


// ---------------------------------------------------------------------------
// The portability proof — and the leak. Swapping GptScored() for Claude() makes
// `.logprobs` throw at RUNTIME, not compile time, because the client returns the
// existential Provider. There is NO Anthropic logprobs path; this is correct
// behavior, but it is a runtime promise (see evaluation.md, OQ#5).
// ---------------------------------------------------------------------------
function ClassifyOnGemini(review: string) -> Scored$Result<Sentiment> {
  let p = GeminiScored();
  match (p) {
    let s: Scored => s.logprobs<Sentiment>(render_prompt(review), 5),  // Gemini fills `avg`; OpenAI doesn't
    _            => throw baml.errors.Unsupported { message: "no logprobs" },
  }
}

function ClassifyResilient(review: string) -> Scored$Result<Sentiment> {
  let p = Resilient();         // Fallback claims Scored; routes past Anthropic to OpenAI
  match (p) {
    let s: Scored => s.logprobs<Sentiment>(render_prompt(review), 5),
    _            => throw baml.errors.Unsupported { message: "no scored member" },
  }
}


// ---------------------------------------------------------------------------
// (2) SOURCED ANSWERS — and here the abstraction REFUSES to unify. Anthropic's
// `.cited` returns CitedBlock[] (spans into your docs); Gemini's `.grounded`
// returns GroundingMetadata (answer-segment -> retrieved chunk). They are
// different companions returning different types. The app must choose.
// ---------------------------------------------------------------------------
function Research(question: string) -> Answer {
  client: Claude()
  prompt #"Answer using the attached documents: {{ question }}"#
}

function ResearchGrounded(question: string) -> Answer {
  client: GeminiScored()       // reuse the Gemini client; it's Grounded
  prompt #"Answer with up-to-date sources: {{ question }}"#
}

// Anthropic path: per-document citations.
function Research$cited(question: string) -> Citable$Result<Answer> {
  let p = Claude();
  match (p) {
    let c: Citable => c.cited<Answer>(render_prompt(question)),
    _             => throw baml.errors.Unsupported { message: "client cannot cite documents" },
  }
}

// Gemini path: grounding metadata. A DIFFERENT companion, a DIFFERENT type.
function ResearchGrounded$grounded(question: string) -> Grounded$Result<Answer> {
  let p = GeminiScored();
  match (p) {
    let g: Grounded => g.grounded<Answer>(render_prompt(question)),
    _              => throw baml.errors.Unsupported { message: "client cannot ground" },
  }
}


// ---------------------------------------------------------------------------
// If an app wants "a sourced answer regardless of provider," IT writes the
// normalization — the std lib does not. This is the honest cost: the app owns
// the lowest-common-denominator "claim -> source" shape, because the three
// providers do not share one. Note the match tries EACH capability in turn.
// ---------------------------------------------------------------------------
class SourcedClaim {
  text: string
  source_uris: string[]     // app's chosen common shape; lossy on purpose
}

function SourcedAnswer(question: string, p: Provider) -> SourcedClaim[] throws baml.errors.LlmClient {
  match (p) {
    let c: Citable  => normalize_citations(c.cited<Answer>(render_prompt(question))),
    let g: Grounded => normalize_grounding(g.grounded<Answer>(render_prompt(question))),
    let a: Annotated => normalize_annotations(a.annotated<Answer>(render_prompt(question))),
    _ => throw baml.errors.Unsupported { message: "client exposes no source metadata" },
  }
}

function normalize_citations(r: Citable$Result<Answer>) -> SourcedClaim[] { $rust_io_function }
function normalize_grounding(r: Grounded$Result<Answer>) -> SourcedClaim[] { $rust_io_function }
function normalize_annotations(r: Annotated$Result<Answer>) -> SourcedClaim[] { $rust_io_function }

function render_prompt(s: string) -> baml.llm.PromptAst { $rust_io_function }
