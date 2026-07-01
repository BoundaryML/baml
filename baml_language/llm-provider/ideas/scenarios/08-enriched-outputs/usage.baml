// ============================================================================
// Scenario 08 — Enriched outputs: APPLICATION-author code.
//
// What the app author writes to (a) score a classification with logprobs and
// (b) get a sourced answer. The interesting part is what they CAN'T write
// portably: there is no `Foo.cited_or_grounded(...)` companion, because there
// is no shared citation type. The app picks a dimension and lives with the
// provider divergence — or normalizes by hand.
//
// CARRIER: the answer AND its enrichment come back together from ONE call via
// `Foo.call_with(prompt, project)`, which returns the tuple `(T, U)`. `project`
// reads the normalized `ResponseMeta`: `m.logprobs()` for scores (typed
// `Unsupported` when the provider can't), and a runtime `match (m)` for the
// divergent `Cited` / `Grounded` / `Annotated` dimensions. No `*$Result<T>`
// wrapper, no `.logprobs` companion method.
// ============================================================================

class Sentiment {
  label: string          // "positive" | "negative" | "neutral"
}

class Answer {
  text: string
}


// ---------------------------------------------------------------------------
// Clients. Plain config; enriched output is NOT a separate companion — it is a
// `project` over the response sidecar. The request-side knobs (top_logprobs, the
// search/citation tool) are provider-private fields the client may set when the
// app knows it wants that dimension; they are not per-call companions.
// ---------------------------------------------------------------------------
client GptScored() {
  OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, want_logprobs: 5 }
}

client GeminiScored() {
  Gemini { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY, want_logprobs: 5, want_search: true }
}

client Claude() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024, want_citations: true }
}

// A fallback whose FIRST member can't score. Fallback implements HttpProvider and
// forwards `call_with`, so the projection survives the route; if Anthropic wins,
// `m.logprobs()` projects Unsupported (the winner's view — see implement.baml's
// residual note).
client Resilient() {
  Claude().fallback_to(OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, want_logprobs: 5 })
}


// ---------------------------------------------------------------------------
// (1) LOGPROBS — choice-scoring. Background's exact use case: compare the
// confidence of the answer instead of trusting the sampled word.
//
// The scores ride the response sidecar: `Classify.call_with(review, m =>
// m.logprobs())` returns `(Sentiment, Supported<Logprobs>)` in one call. There
// is no `.logprobs` companion and no `Scored$Result<T>`. When the provider can't
// score (Anthropic), the projected `U` is the typed `Unsupported` — the app
// branches on it, instead of catching an `Unsupported` THROW.
// ---------------------------------------------------------------------------
function Classify(review: string) -> Sentiment {
  client: GptScored()
  prompt #"Is this review positive, negative, or neutral? {{ review }}"#
}

// A deliberate domain failure the app raises when the scored confidence is too
// low. It is a typed CallError so callers can branch on it (it is not a network,
// rate-limit, or parse failure, so every classifier returns false).
class LowConfidence {
  confidence: float
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// App code: score the classification, threshold on confidence. The tuple is
// destructured at the call site — `(value, scores)` — and `scores` is a
// `Supported<Logprobs>`, so "this provider cannot score" is a typed branch, not
// a thrown error.
function ConfidentClassify(review: string) -> Sentiment
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let (value, scores): (Sentiment, Supported<Logprobs>) =
      Classify.call_with(review, m => m.logprobs());
  match (scores) {
    let lp: Logprobs => {
      // lp.tokens[i].logprob is per-token; for a one-word label the label token's
      // logprob is the confidence. (Tokenizer-bound: a multi-token label is summed.)
      let confidence: float = score_of_label(value.label, lp);
      if (confidence < -2.3) {     // ~10% probability floor
        throw LowConfidence { confidence: confidence };
      }
      return value;
    },
    let u: Unsupported => {
      // The client cannot score (e.g. swapped to Claude). Honest typed branch:
      // accept the unscored label rather than fail. (An app that REQUIRES scoring
      // would `throw` here instead — its policy, on the value channel.)
      return value;
    },
  }
}

function score_of_label(label: string, scores: Logprobs) -> float { $rust_io_function }


// ---------------------------------------------------------------------------
// The portability proof — and the leak. Swapping GptScored() for Claude() does
// NOT fail to compile: the client returns the existential Provider, and
// `m.logprobs()` still type-checks (it returns `Supported<Logprobs>`). The
// difference shows up as a VALUE — `Unsupported` — at runtime, not a throw. This
// is the model's honest seam: per-field absence is a runtime promise (P2/OQ#5),
// now carried as a typed `Unsupported` value rather than a thrown sentinel.
// ---------------------------------------------------------------------------
function ClassifyOnGemini(review: string) -> (Sentiment, Supported<Logprobs>) {
  Classify.call_with(review, m => m.logprobs())   // Gemini fills `avg`; OpenAI doesn't
}

function ClassifyResilient(review: string) -> (Sentiment, Supported<Logprobs>)
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // Resilient() is a Fallback; it implements HttpProvider, so call_with is reached
  // by matching the existential Provider for the HttpProvider capability. If
  // Anthropic wins, the projected logprobs is Unsupported (winner's view) — see
  // implement.baml.
  match (Resilient()) {
    let h: HttpProvider => h.call_with<Sentiment, Supported<Logprobs>>(render_prompt(review), m => m.logprobs()),
    _ => throw baml.errors.Unsupported { message: "resilient client cannot produce a value+sidecar" },
  }
}


// ---------------------------------------------------------------------------
// (2) SOURCED ANSWERS — and here the abstraction REFUSES to unify. Anthropic's
// citations are CitedBlock[] (spans into your docs); Gemini's grounding is
// GroundingMetadata (answer-segment -> retrieved chunk). They are different
// dimensions returning different types, reached by matching the projected
// ResponseMeta against the external `Cited` / `Grounded` interface. The app must
// choose which dimension to project.
// ---------------------------------------------------------------------------
function Research(question: string) -> Answer {
  client: Claude()
  prompt #"Answer using the attached documents: {{ question }}"#
}

function ResearchGrounded(question: string) -> Answer {
  client: GeminiScored()       // reuse the Gemini client; its response is Grounded
  prompt #"Answer with up-to-date sources: {{ question }}"#
}

// Anthropic path: per-document citation blocks, projected off the sidecar. The
// `match (m)` reaches the external `Cited` dimension; a `_` arm makes the
// "this provider doesn't cite" case a typed empty rather than a throw.
function Research$cited(question: string) -> (Answer, CitedBlock[]) {
  Research.call_with(question, m => match (m) {
    let c: Cited => c.cited_blocks(),
    _            => [],
  })
}

// Gemini path: grounding metadata. A DIFFERENT dimension, a DIFFERENT type —
// reached by matching the SAME projected ResponseMeta against `Grounded`.
function ResearchGrounded$grounded(question: string) -> (Answer, GroundingMetadata) {
  ResearchGrounded.call_with(question, m => match (m) {
    let g: Grounded => g.grounding(),
    _              => GroundingMetadata { chunks: [], supports: [] },
  })
}


// ---------------------------------------------------------------------------
// If an app wants "a sourced answer regardless of provider," IT writes the
// normalization — the std lib does not. This is the honest cost: the app owns
// the lowest-common-denominator "claim -> source" shape, because the three
// providers do not share one. The single `project` match tries EACH dimension in
// turn over ONE response sidecar — note the escape hatch (`let o: OpenAIResponse`)
// would ride OQ5 wrapper-transparency if it reached for a provider-only raw field.
// ---------------------------------------------------------------------------
class SourcedClaim {
  text: string
  source_uris: string[]     // app's chosen common shape; lossy on purpose
}

function SourcedAnswer(question: string, p: Provider) -> SourcedClaim[]
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // call_with is an HttpProvider capability now — match the marker `p` for it.
  match (p) {
    let h: HttpProvider => {
      let (answer, claims): (Answer, SourcedClaim[]) =
          h.call_with<Answer, SourcedClaim[]>(render_prompt(question), m => match (m) {
            let c: Cited     => normalize_citations(c.cited_blocks()),
            let g: Grounded  => normalize_grounding(g.grounding()),
            let a: Annotated => normalize_annotations(a.annotations()),
            _ => [],     // no source dimension on this provider — typed empty, not a throw
          });
      return claims;
    },
    _ => throw baml.errors.Unsupported { message: "provider has no request/response sidecar" },
  }
}

function normalize_citations(blocks: CitedBlock[]) -> SourcedClaim[] { $rust_io_function }
function normalize_grounding(gm: GroundingMetadata) -> SourcedClaim[] { $rust_io_function }
function normalize_annotations(anns: Annotation[]) -> SourcedClaim[] { $rust_io_function }

function render_prompt(s: string) -> baml.llm.PromptAst { $rust_io_function }
