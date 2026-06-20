// ============================================================================
// usage.baml — APPLICATION-author code for streaming.
//
// Demonstrates:
//   - how little the app author writes (config-only `client`s)
//   - the .stream companion: same function, richer delivery
//   - token streaming (TStream = string) AND partial structured output
//     (TStream = a deep-partial) off the SAME function, chosen by the consumer
//   - two divergent providers (OpenAI SSE, Gemini chunked-JSON) used
//     interchangeably — portability proof
//   - a combinator chain (fallback + retry) that still streams
//   - the fallback-to-call degrade for a non-streaming provider
// ============================================================================


// ---- config-only clients (sugar for `function Name() -> Provider`) ----------

client GPT4o() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

client Claude() {
  Anthropic { model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY, max_tokens: 1024 }
}

client GeminiFlash() {
  Gemini { model: "gemini-2.0-flash", api_key: env.GEMINI_API_KEY }
}

// A streaming-capable client that survives provider outage: OpenAI (SSE) first,
// Gemini (chunked JSON) second. Fallback forwards .stream by routing to the
// first member that implements Streaming — both do, with totally different wire
// readers, and the app author never sees the difference.
client ResilientStream() {
  GPT4o().with_retry(2).fallback_to(GeminiFlash())
}

// A client whose provider does NOT stream — to show the .stream companion's
// honest degrade path (buffer_as_stream).
client Canned() {
  EchoModel { reply: "{ \"title\": \"Lunch\", \"date\": \"Friday\", \"attendees\": [\"Alice\", \"Bob\"] }" }
}


// ---- the target schema for structured output --------------------------------

class Meeting {
  title: string
  date: string
  attendees: string[]
}

// The deep-partial the UI consumes while streaming: every field optional, arrays
// grow as items arrive. This is TStream; `Meeting` is TFinal.
class MeetingPartial {
  title: string?
  date: string?
  attendees: string[]?
}


// ============================================================================
// (A) Token streaming — a plain prose function.  TStream = string.
// ============================================================================

function Haiku(topic: string) -> string {
  client: GPT4o()
  prompt #"Write a haiku about {{ topic }}."#
}

// The app author calls the .stream companion. No new function body — the
// companion is generated from Haiku (conventions §companions): it `match`es the
// provider for Streaming, else buffers. For a typewriter UI, TStream = string,
// TFinal = string.
function PrintHaiku(topic: string) -> string throws baml.ExtendUnknownError<baml.errors.StreamError> {
  let s: baml.llm.Stream<string, string> = Haiku.stream<string, string>(topic);
  while (true) {
    match (s.next()) {
      let delta: string => { baml.io.print(delta); }   // typewriter
      let _done: baml.llm.StreamDone => { break; }
    }
  }
  s.final()                                              // the normal, whole message
}


// ============================================================================
// (B) Streaming PARTIAL STRUCTURED OUTPUT — same machinery, richer TStream.
//
// The wire stream is IDENTICAL to (A); the only difference is the consumer asks
// for TStream = MeetingPartial instead of string, so each step is a tolerant
// SAP partial parse of the growing JSON buffer. This proves the background
// claim: partial structured output is a CLIENT-SIDE interpretation of the same
// deltas, not a distinct API mode.
// ============================================================================

function ExtractMeeting(blurb: string) -> Meeting {
  client: GPT4o()
  prompt #"Extract the meeting from: {{ blurb }}"#
}

function RenderMeetingLive(blurb: string) -> Meeting throws baml.ExtendUnknownError<baml.errors.StreamError> {
  // Note the two independent type params: partials are MeetingPartial, the
  // validated final is Meeting.
  let s: baml.llm.Stream<MeetingPartial, Meeting> =
    ExtractMeeting.stream<MeetingPartial, Meeting>(blurb);

  while (true) {
    match (s.next()) {
      let p: MeetingPartial => {
        // UI reads optionals as they fill in: title first, then date, then
        // attendees appended one by one.
        render_partial(p.title ?? "...", p.date ?? "...", p.attendees ?? []);
      }
      let _done: baml.llm.StreamDone => { break; }
    }
  }
  s.final()                                              // required-field validation happens HERE
}


// ============================================================================
// (C) Portability: the SAME function, streamed across two divergent transports.
//
// ResilientStream is OpenAI(SSE)+retry, falling back to Gemini(chunked-JSON).
// The app code below is byte-for-byte what you'd write for a single provider.
// The two stream readers live entirely inside implement.baml.
// ============================================================================

function ExtractMeetingResilient(blurb: string) -> Meeting {
  client: ResilientStream()
  prompt #"Extract the meeting from: {{ blurb }}"#
}

function RenderMeetingResilient(blurb: string) -> Meeting throws baml.ExtendUnknownError<baml.errors.StreamError> {
  let s: baml.llm.Stream<MeetingPartial, Meeting> =
    ExtractMeetingResilient.stream<MeetingPartial, Meeting>(blurb);
  while (true) {
    match (s.next()) {
      let p: MeetingPartial => { render_partial(p.title ?? "...", p.date ?? "...", p.attendees ?? []); }
      let _done: baml.llm.StreamDone => { break; }
    }
  }
  s.final()
}


// ============================================================================
// (D) Fallback-to-call: a non-streaming provider still answers .stream.
//
// Canned()'s EchoModel implements only HttpProvider (call/call_with), not Streaming.
// The generated .stream companion `match`es for Streaming, misses, and takes the
// HttpProvider arm: buffer_as_stream. The consumer code is UNCHANGED — it just gets one partial
// (the whole object) then final, with no incrementality.
// ============================================================================

function ExtractCanned(blurb: string) -> Meeting {
  client: Canned()
  prompt #"Extract the meeting from: {{ blurb }}"#
}

function RenderCanned(blurb: string) -> Meeting throws baml.ExtendUnknownError<baml.errors.StreamError> {
  let s: baml.llm.Stream<MeetingPartial, Meeting> =
    ExtractCanned.stream<MeetingPartial, Meeting>(blurb);
  // Loop runs exactly once (one buffered partial), then done.
  while (true) {
    match (s.next()) {
      let p: MeetingPartial => { render_partial(p.title ?? "...", p.date ?? "...", p.attendees ?? []); }
      let _done: baml.llm.StreamDone => { break; }
    }
  }
  s.final()
}


// ============================================================================
// (E) Cancellation mid-stream — stop early, discard or surface the partial.
// ============================================================================

function FirstLine(topic: string) -> string throws baml.ExtendUnknownError<baml.errors.StreamError> {
  let s: baml.llm.Stream<string, string> = Haiku.stream<string, string>(topic);
  let mut acc: string = "";
  while (true) {
    match (s.next()) {
      let delta: string => {
        acc = acc + delta;
        if (acc.includes("\n")) {
          s.close();                                    // abort the HTTP request, stop accumulating
          return acc;
        }
      }
      let _done: baml.llm.StreamDone => { break; }
    }
  }
  acc
}


// ---- host-side UI sink (app provides this; declared extern here) ------------
function render_partial(title: string, date: string, attendees: string[]) -> null { $rust_io_function }
