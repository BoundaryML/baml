// ============================================================================
// usage.baml  —  Capability negotiation from the APPLICATION author's seat.
// ----------------------------------------------------------------------------
// End-user code. Shows the three things an app author actually does:
//   1. Nothing — call a Negotiated client and let it figure out the gradient.
//   2. Ask, declaratively, before sending (the supportedUrls/LiteLLM pattern).
//   3. Negotiate a BEHAVIORAL capability with the proposal's §5 `match`.
//
// Two divergent providers throughout (OpenAIChat vs Gemini) so the leaks show.
// ============================================================================


class Resume {
  name: string
  // a UNION field — OpenAI strict says Maybe/Yes, Gemini's Capabilities says No.
  contact: Email | Phone
}
class Email { email: string }
class Phone { phone: string }


// ----------------------------------------------------------------------------
// (1) THE EASY PATH — the app author writes almost nothing.
//     `Negotiated` wraps the real provider and a degraded twin; the gradient
//     (Yes/Maybe/No + warn + fallback) is entirely inside the combinator. The
//     app author never sees Support, never writes a `match`.
// ----------------------------------------------------------------------------

// the "degraded" twin: same provider, no structured-output constraint, just
// prompt-based JSON. (In practice a flag on the provider; shown as a sibling
// client for clarity.)
client OpenAIPromptJson() {
  OpenAIChat { model: "gpt-4o", api_key: env.OPENAI_API_KEY, max_tokens: null }
}

client SmartExtractor() {
  Negotiated {
    inner: OpenAIChat { model: "gpt-4o", api_key: env.OPENAI_API_KEY, max_tokens: null },
    degraded: OpenAIPromptJson(),
  }
}

// One line of client config, then an ordinary function. The Resume.contact
// union triggers the `Maybe` branch inside Negotiated: it probes, and if the
// 400 comes back it downgrades to prompt JSON and records a warning. The plain
// `.call` path here ignores that warning (the 90% path, zero tax) — to READ it
// the caller projects it off `call_with` (see ExtractChecked below).
function Extract(bio: string) -> Resume {
  client: SmartExtractor()
  prompt #"Extract a Resume from: {{ bio }}"#
}

// The author who DOES care whether a silent downgrade happened projects the
// warnings sidecar off `call_with` — same product-sidecar path as usage. Because
// Negotiated forwards `call_with` and wraps the meta in a DowngradeMeta, the
// downgrade note survives the combinator and lands in `m.warnings()`.
function ExtractChecked(bio: string) -> (Resume, Warning[]) {
  let p = SmartExtractor();
  // `client` returns the bare `Provider` marker, which has no `call_with` — so
  // match it to HttpProvider first (the universal companion-as-match, now applied
  // to the value+sidecar path too). Negotiated implements HttpProvider, so it matches.
  match (p) {
    let h: HttpProvider => h.call_with<Resume, Warning[]>(
      baml.llm.render_prompt(bio),
      m => match (m) {
        let w: Warnings => w.warnings(),   // downgrade notes, if any
        _               => [],             // provider with no warnings dimension
      }),
    _ => throw baml.errors.Unsupported { message: "client has no request/response sidecar" },
  }
}


// Same function body, Gemini behind it. Here Negotiated's `match` on
// Capabilities hits Gemini's `structured_output<Resume>() == No` (unions are
// forbidden in the OpenAPI subset) and goes STRAIGHT to degraded — no wasted
// 400. This is the payoff of the declarative layer over pure runtime probing.
client GeminiPromptJson() {
  Gemini { model: "gemini-2.0-flash", api_key: env.GEMINI_API_KEY }
}
client SmartExtractorGemini() {
  Negotiated {
    inner: Gemini { model: "gemini-2.0-flash", api_key: env.GEMINI_API_KEY },
    degraded: GeminiPromptJson(),
  }
}
function ExtractGemini(bio: string) -> Resume {
  client: SmartExtractorGemini()
  prompt #"Extract a Resume from: {{ bio }}"#
}


// ----------------------------------------------------------------------------
// (2) THE "ASK FIRST" PATH — the app author wants to KNOW before sending,
//     e.g. to pick a model, warn the user, or choose a code path. This is the
//     LiteLLM supports_*/get_model_info pattern. It requires the app author to
//     `match` the client's provider down to Capabilities — capability is a
//     RUNTIME promise (proposal §5), so there is no compile-time `.structured_output`.
// ----------------------------------------------------------------------------

client Candidate() {
  Gemini { model: "gemini-2.0-flash", api_key: env.GEMINI_API_KEY }
}

// returns a human-readable verdict for a config screen. Note how much the app
// author has to write versus a `litellm.supports_function_calling(...)` call:
// a `match` (because the client returns the existential Provider), then a
// `match` on the Support lattice.
function CanThisModelDoStructuredOutput() -> string {
  let p = Candidate();
  match (p) {
    let cap: Capabilities => {
      match (cap.structured_output<Resume>()) {
        Yes   => { "yes" },
        No    => { "no — schema dialect unsupported; will use prompt JSON" },
        Maybe => { "maybe — will probe at runtime" },
      }
    },
    // the provider doesn't even expose declarative capabilities.
    _ => { "unknown — provider has no capability surface; try it and see" },
  }
}


// Pre-flight a context-window check (the get_model_info("max_input_tokens")
// pattern). Scalar, so no Support lattice — but still gated behind the `match`.
function FitsInContext(estimated_tokens: int) -> bool {
  let p = Candidate();
  match (p) {
    let cap: Capabilities => { estimated_tokens < cap.max_input_tokens() },
    _ => { true },   // can't know -> optimistically proceed; the 400 is the backstop.
  }
}


// ----------------------------------------------------------------------------
// (3) THE BEHAVIORAL-CAPABILITY PATH — "can it stream at all?" This is the
//     proposal's §5 native `match`, NOT the declarative layer. A streaming
//     companion degrades to `.call`. Shown to contrast the two negotiation
//     layers: `match`-on-interface (binary, behavioral) vs Capabilities
//     (graded, data). OpenAIChat implements Streaming; Gemini (above) does not.
// ----------------------------------------------------------------------------

client MaybeStreams() {
  OpenAIChat { model: "gpt-4o", api_key: env.OPENAI_API_KEY, max_tokens: null }
}

// The .stream companion the proposal generates: match Streaming, else buffer.
function Summarize$stream<TStream, TFinal>(doc: string)
    -> baml.llm.Stream<TStream, TFinal> {
  let p = MaybeStreams();
  match (p) {
    let s: Streaming    => s.stream<TStream, TFinal>(baml.llm.render_prompt(doc)),
    // degrade both ways: buffer an HttpProvider.call as a single-chunk stream.
    let h: HttpProvider => buffer_as_stream<TStream, TFinal>(h.call<TFinal>(baml.llm.render_prompt(doc))),
    _                   => throw baml.errors.Unsupported { message: "client cannot stream" },
  }
}


// ----------------------------------------------------------------------------
// (4) THE RESOLUTION-GRADIENT PATH — image input "but not at this resolution".
//     The app hands a possibly-huge image; ResolutionGate consults the SAME
//     Capabilities surface on a DIFFERENT axis and silently downscales. guard()
//     hands the (possibly downscaled) image back NEXT TO any downscale warning
//     as a tuple; the author threads that warning into the call's projection so
//     it lands in the SAME `m.warnings()` channel as a §1 schema downgrade.
//     Proves the gradient lattice generalizes past schemas — and that the one
//     warnings sidecar carries request-time and response-time downgrades alike.
// ----------------------------------------------------------------------------

client VisionModel() {
  OpenAIChat { model: "gpt-4o", api_key: env.OPENAI_API_KEY, max_tokens: null }
}

// returns the description AND any downgrade warnings (here: a downscale).
function DescribeImage(photo: image) -> (string, Warning[]) {
  // app author opts into the gate; guard() returns a (possibly downscaled)
  // image AND the warnings that pre-flight produced.
  let gate = ResolutionGate { inner: VisionModel(), max_edge: 2000 };
  let (safe, pre): (image, Warning[]) = gate.guard(photo);
  let p = VisionModel();
  // PreflightMeta folds the request-time downscale warning into the response's
  // warnings() so the projection reads request- and response-side notes as one.
  // `p` is the bare Provider marker — match to HttpProvider for the sidecar call.
  match (p) {
    let h: HttpProvider => h.call_with<string, Warning[]>(
      baml.llm.render_prompt_image("Describe this image:", safe),
      m => match (m) {
        let w: Warnings => pre.concat(w.warnings()),
        _               => pre,
      }),
    _ => throw baml.errors.Unsupported { message: "vision client has no request/response sidecar" },
  }
}


// ----------------------------------------------------------------------------
// (5) THE COMBINATOR CHAIN — negotiation composes with orchestration. A
//     Negotiated provider is still a Provider, so retry/fallback wrap it
//     unchanged. Reads top-to-bottom: negotiate the schema, then add resilience.
// ----------------------------------------------------------------------------

client Production() {
  Negotiated {
    inner: OpenAIChat { model: "gpt-4o", api_key: env.OPENAI_API_KEY, max_tokens: null },
    degraded: OpenAIPromptJson(),
  }.with_retry(2)
   .fallback_to(SmartExtractorGemini())
}

function ExtractProd(bio: string) -> Resume {
  client: Production()
  prompt #"Extract a Resume from: {{ bio }}"#
}
