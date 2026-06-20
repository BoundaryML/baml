// ============================================================================
// usage.baml  —  APPLICATION-author code for structured / typed output.
// ----------------------------------------------------------------------------
// The whole point of this scenario: the app author writes ONE return type and
// ONE prompt, and the SAME function works across four wire encodings. The four
// encodings live entirely in the provider; nothing here mentions response_format,
// text.format, tool injection, or responseSchema.
// ============================================================================


// The one desired type. It flows in as T to build_request<T> AND parse<T>.
class Event {
  title: string
  date: string
  attendees: string[]
}


// --- clients are sugar for `function Name(args) -> Provider { body }` --------

client GptStructured() {
  OpenAIChat { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, max_tokens: null }
}

client GptResponses() {
  OpenAIResponses { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, max_tokens: null }
}

client ClaudeStructured() {
  Anthropic { model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}

client GeminiStructured() {
  Gemini { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY }
}


// --- the SAME function body, four providers ---------------------------------
// Portability proof: only the `client:` line differs. The `-> Event` return type
// is the T that each provider lowers to its own wire format and parses back.

function ExtractEvent_OpenAI(text: string) -> Event {
  client: GptStructured()
  prompt #"Extract the calendar event from: {{ text }}"#
}

function ExtractEvent_Responses(text: string) -> Event {
  client: GptResponses()
  prompt #"Extract the calendar event from: {{ text }}"#
}

function ExtractEvent_Claude(text: string) -> Event {
  // Anthropic has no response_format — this still works because the provider
  // injects a forced `record_output` tool whose input_schema IS Event.
  client: ClaudeStructured()
  prompt #"Extract the calendar event from: {{ text }}"#
}

function ExtractEvent_Gemini(text: string) -> Event {
  // Gemini lowers Event to its UPPERCASE OpenAPI dialect under responseSchema.
  client: GeminiStructured()
  prompt #"Extract the calendar event from: {{ text }}"#
}


// --- strict-vs-nonstrict negotiation via the combinator ---------------------
// A recursive/union type that OpenAI strict mode rejects (400). The provider's
// build_request already picks strict=false when strict_supports(T) is false; but
// for a type that LOOKS strict-friendly yet still 400s at runtime, the app can
// wrap a strict primary with a looser retry. Composition is a function call.

class OrgNode {
  name: string
  reports: OrgNode[]      // $ref cycle: strict mode 400s on this.
}

client StrictOrLoose() {
  StrictFallback {
    primary: OpenAIChat { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, max_tokens: null },
    // the looser retry: Gemini is always-constrained and tolerant of cycles
    // via prompt+schema, a different engine for the same T.
    loose: Gemini { model: "gemini-2.5-flash", api_key: env.GEMINI_API_KEY },
  }
}

function ExtractOrg(text: string) -> OrgNode {
  client: StrictOrLoose()
  prompt #"Build the reporting tree from: {{ text }}"#
}


// --- a combinator chain reads left-to-right ---------------------------------
// retry transient failures, then fall back to Claude if OpenAI is down. The
// `-> Event` type still threads through every layer's call<Event>.

function ExtractResilient(text: string) -> Event {
  client: GptStructured().with_retry(2).fallback_to(ClaudeStructured())
  prompt #"Extract the calendar event from: {{ text }}"#
}


// --- handling a refusal at the call site ------------------------------------
// A refusal is a typed error, NOT a malformed Event. The app distinguishes it
// from a transport/parse failure by catching the concrete `Refused` specifically,
// then triaging the rest by capability interface, with UnknownError as the floor.

function ExtractEventSafe(text: string) -> Event {
  client: ClaudeStructured()
  prompt #"Extract the calendar event from: {{ text }}"#
}

// app-level orchestration over the function's .call companion. Runtime match,
// most-specific first: concrete Refused -> CallError interface -> UnknownError.
function TryExtract(text: string) -> Event? {
  ExtractEventSafe(text) catch (e) {
    let r: Refused                 => { null },          // model declined: no Event exists.
    let c: baml.errors.CallError   => { throw c; },      // rate-limit / parse / transport: propagate.
    let u: baml.UnknownError       => { throw u; },      // exotic/foreign failure: propagate.
  }
}
