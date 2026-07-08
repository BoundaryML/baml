// scenario 04 — streaming: ../../../../../llm-provider/ideas/scenarios/04-streaming/README.md
//   usage.baml — APPLICATION-author code for streaming, on the REAL surface.
//
// The design's thesis: one `Streaming` capability spans OpenAI's flat SSE deltas,
// Anthropic's event grammar, and Gemini's chunked-JSON — and streaming PARTIAL
// STRUCTURED OUTPUT is not a separate API mode, it's the SAME token stream re-read
// by a tolerant client-side parser. The app picks `TStream = string` for a
// typewriter or `TStream = <deep-partial>` for a live-filling object off the
// IDENTICAL function.
//
// How that maps onto what actually ships (REALIZED.md §04):
//   - `Streaming` is real (`stream_messages` native + a `stream` string sugar),
//     returning `baml.llm.Stream<TStream, TFinal>` — pull-based: `.next()` for
//     partials, `.final()` for the validated answer.
//   - The `$stream` companion is generated on every LLM function and desugars to
//     `baml.ai.drive_stream<Partial<T>, T>(client, prompt)`. The partial `TStream`
//     is AUTO-DERIVED — `Foo$stream(...)` returns `Stream<T$stream | null, T>`, so
//     the design's hand-written `MeetingPartial` is just `s04_Meeting$stream`, and
//     token streaming's `TStream` is `string` (the `-> string` partial). The app
//     never spells the two type params — one companion, both deliveries.
//   - The three divergent wire readers (SSE flat / SSE events / Gemini chunked)
//     that the design's implement.baml folds by hand are NATIVE inside each real
//     provider (OpenAi / Anthropic / Gemini all `implements Streaming`). So the
//     design's whole library-author file is subsumed by the stdlib — there is no
//     custom provider left to author here (hence no implementation.baml).
//
// What the OFFLINE tier can prove deterministically: the companion's negotiation —
// a non-streaming provider is a TYPED `Unsupported`, and a fallback chain with no
// streaming member is an honest "no streaming member" — plus that the SAME
// consumer loop typechecks for both token (`string`) and structured
// (`s04_Meeting$stream`) delivery. The actual byte-level streaming across three
// real transports is the live tier (`testset "integ-test-04-streaming"`).
//
// BLOCKED (net-new host surface the design needed, not on the real surface):
//   - the honest `.stream` degrade for a non-streaming provider (design's
//     `buffer_as_stream`) needs `baml.llm.stream_unfold`/`stream_once` +
//     `baml.sap.partial_of` — none exist, so drive_stream is a typed `Unsupported`
//     instead of a one-shot buffered stream (see `baml.ai.drive_stream` docstring).
//   - mid-stream cancellation (design's `s.close()`) — `baml.llm.Stream` exposes
//     only `.next()`/`.final()`, no `.close()`.
//   These are recorded as `// BLOCKED: P8 ...` at their call sites below.


// --- config-only client. ----------------------------------------------------
// The design declares three named streaming clients (GPT4o / Claude / GeminiFlash)
// and swaps them into one function body. On the real surface a declared `client`
// is the DEFAULT; the swap is a one-line call-site `client =` override (offline
// via the fakes, live via the three native providers below). We default to a dead
// client so every offline test must pass an explicit, deterministic override.
client<llm> DeadS04 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// --- the target schema for structured output. -------------------------------
// The deep-partial the UI consumes while streaming (design's `MeetingPartial`) is
// NOT hand-written here — the `$stream` companion auto-derives it as
// `s04_Meeting$stream`: every scalar becomes optional, arrays grow as items
// arrive. `s04_Meeting` is `TFinal`; `s04_Meeting$stream` is `TStream`.
class s04_Meeting {
  title: string
  date: string
  attendees: string[]
}


// ============================================================================
// (A) Token streaming — a plain prose function. TStream = string.
// ============================================================================
function s04_Haiku(topic: string) -> string {
  client DeadS04
  prompt #"Write a haiku about {{ topic }}."#
}

// The app calls the `$stream` companion. No new function body — it is generated
// from s04_Haiku. For a typewriter UI, TStream = string, TFinal = string. The
// stream's `next()` yields `string | null | StreamFinished`: a text delta, a
// no-text frame (role-only / usage-only chunk), or the terminal sentinel.
function s04_stream_tokens(topic: string, client: baml.ai.Provider) -> string throws never {
  let s = s04_Haiku$stream(topic, client = client) catch (e) {
    let u: baml.errors.Unsupported => return "unsupported",   // no Streaming on this provider
    _ => return "no-stream",                                  // e.g. fallback with no streaming member
  };
  let acc: string = "";
  while (true) {
    match (s.next()) {
      let delta: string => { acc = acc + delta; }             // typewriter step
      null => {}                                              // no-text frame, skip
      let _done: baml.stream.StreamFinished => { break; }     // (design's StreamDone)
      // BLOCKED: P8 mid-stream cancel — design does `s.close()` here to abort the
      // socket early; `baml.llm.Stream` has no `.close()`, only next/final.
    } catch (e) {
      _ => return "mid-stream error",                         // StreamError channel
    }
  }
  s.final() catch (e) { _ => return "final error" }           // the normal, whole message
}


// ============================================================================
// (B) Streaming PARTIAL STRUCTURED OUTPUT — same machinery, richer TStream.
//
// The wire stream is IDENTICAL to (A); the only difference is TStream is
// `s04_Meeting$stream` instead of `string`, so each step is a tolerant partial
// of the growing JSON buffer. This is the design's central claim on the real
// surface: partial structured output is a CLIENT-SIDE read of the same deltas,
// selected by the return type of the function the companion was generated from —
// not a distinct API mode.
// ============================================================================
function s04_ExtractMeeting(blurb: string) -> s04_Meeting {
  client DeadS04
  prompt #"Extract the meeting from: {{ blurb }}"#
}

// Byte-identical consumer shape to (A) — only the partial arm's type differs.
function s04_stream_meeting(blurb: string, client: baml.ai.Provider) -> s04_Meeting throws never {
  let s = s04_ExtractMeeting$stream(blurb, client = client) catch (e) {
    let u: baml.errors.Unsupported => return s04_Meeting { title: "unsupported", date: "", attendees: [] },
    _ => return s04_Meeting { title: "no-stream", date: "", attendees: [] },
  };
  let steps: int = 0;
  while (true) {
    match (s.next()) {
      let part: s04_Meeting$stream => {
        // UI reads optionals as they fill in: title first, then date, then
        // attendees appended one by one (scalars are optional; the array grows).
        let _title: string = part.title ?? "...";
        let _date: string = part.date ?? "...";
        let _count: int = part.attendees.length();
        steps = steps + 1;
      }
      null => {}
      let _done: baml.stream.StreamFinished => { break; }
    } catch (e) {
      _ => return s04_Meeting { title: "mid-stream error", date: "", attendees: [] },
    }
  }
  s.final() catch (e) { _ => return s04_Meeting { title: "final error", date: "", attendees: [] } }
  // ^ required-field validation happens HERE, on the complete buffer — not in the
  //   tolerant per-step partials above.
}


// ============================================================================
// (C) Portability seam — the SAME consumer over a resilient chain.
//
// The design's `ResilientStream` is `GPT4o().with_retry(2).fallback_to(GeminiFlash())`
// and its `.stream` routes to the first Streaming member. On the real surface
// Fallback/Retry both `implements Streaming` and forward `stream_messages`, so the
// chain is just another `Provider` handed to the SAME `$stream` companion — the
// app never sees which transport won. Offline the fake members don't stream, so
// the honest outcome is "no streaming member"; the live tier proves the chain
// actually streams when its members do.
// ============================================================================


// ---- (D) capability negotiation probe (design's fallback-to-call seam) ------
// The design's non-streaming provider (EchoModel) still answers `.stream` via
// `buffer_as_stream`. That honest degrade is BLOCKED here — so instead the
// companion is a typed `Unsupported`, and an app can pre-flight a provider's
// shape: it may be an HttpProvider (has `.call`) yet NOT Streaming.
function s04_negotiate(p: baml.ai.Provider) -> string throws never {
  match (p) {
    let h: baml.ai.HttpProvider => "http",        // has a call codec, but...
    let s: baml.ai.Streaming    => "stream-only",
    _ => "neither",
  }
}


// ============================================================================
// Offline tier — deterministic, no network, no keys.
// ============================================================================

// (A) A non-streaming provider (HttpProvider, no Streaming) is a typed Unsupported
// off the token companion. BLOCKED: P8 — the design expects buffer_as_stream here
// (one buffered partial then final); with no stream constructor the real surface
// negotiates to Unsupported instead.
test "04: token $stream on a non-streaming client is a typed Unsupported" {
  assert.equal(s04_stream_tokens("owls", EchoProvider { reply: "x" }), "unsupported")
}

// (B) The SAME degrade off the structured companion — richer TStream, identical
// negotiation. Proves partial structured output rides the same companion path.
test "04: structured $stream on a non-streaming client is the same typed Unsupported" {
  assert.equal(s04_stream_meeting("lunch friday", EchoProvider { reply: "{}" }).title, "unsupported")
}

// (C) A resilient chain is a Provider handed to the SAME companion. Offline its
// members are non-streaming fakes, so Fallback forwards `.stream`, finds no
// Streaming member, and the honest outcome is "no-stream" (UnknownError, distinct
// from Unsupported). The live tier proves the identical chain streams when a
// member does implement Streaming.
test "04: a resilient stream chain is a Provider handed to Foo$stream" {
  let resilient: baml.ai.Provider =
    AlwaysFailProvider {}.with_retry(2).fallback_to(EchoProvider { reply: "y" });
  assert.equal(s04_stream_tokens("owls", resilient), "no-stream")
}

// (D) Negotiation: EchoProvider HAS a call codec (HttpProvider) but is NOT
// Streaming — so `.stream` can't silently succeed. NoCapProvider is neither.
test "04: the .stream companion sees an HttpProvider that is not Streaming" {
  assert.equal(s04_negotiate(EchoProvider { reply: "x" }), "http")
  assert.equal(s04_negotiate(NoCapProvider {}), "neither")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The portability claim against real transports: ONE consumer loop, three
// providers, three wire formats (OpenAI flat SSE deltas, Anthropic's SSE event
// grammar, Gemini's chunked JSON) all absorbed inside each provider's native
// `stream_messages`. The app just swaps `client =` — and reads tokens OR a
// live-filling object off the same companion.
testset "integ-test-04-streaming" {
  // (A) token streaming — TStream = string — over OpenAI's flat SSE deltas.
  test "04 live: token stream over OpenAI SSE reassembles to the whole message" {
    let r = s04_stream_tokens(
      "the number one, in one short line",
      client = baml.ai.OpenAi {
        model: "gpt-5.4-mini",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true(r.length() > 0)
  }

  // (A) the SAME consumer over Anthropic's structured SSE event grammar.
  test "04 live: token stream over Anthropic's event grammar, same consumer" {
    let r = s04_stream_tokens(
      "the number one, in one short line",
      client = baml.ai.Anthropic {
        model: "claude-haiku-4-5-20251001",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
        base_url: null,
        max_tokens: 256,
        anthropic_version: null,
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
      },
    );
    assert.is_true(r.length() > 0)
  }

  // (B) partial structured output — TStream = s04_Meeting$stream — over Gemini's
  // chunked-JSON transport (NOT SSE). Same companion, richer partial; the final
  // is required-field validated by `.final()`.
  test "04 live: partial structured output over Gemini chunked JSON, then validated final" {
    let m = s04_stream_meeting(
      "Lunch with Alice and Bob on Friday to plan Q3.",
      client = baml.ai.Gemini {
        model: "gemini-2.5-flash",
        api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
        base_url: null,
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
      },
    );
    assert.is_true(m.title.length() > 0)
  }

  // (C) portability: the resilient chain (OpenAI primary, Gemini fallback) streams
  // through the SAME companion — the app code is byte-identical to a single
  // provider. Proves Fallback forwards Streaming to the first member whose stream
  // opens, across two divergent transports, invisibly.
  test "04 live: a fallback chain streams through the same companion" {
    let resilient: baml.ai.Provider =
      baml.ai.OpenAi {
        model: "gpt-5.4-mini",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      }.with_retry(2).fallback_to(baml.ai.Gemini {
        model: "gemini-2.5-flash",
        api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
        base_url: null,
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
      });
    let r = s04_stream_tokens("the number one, in one short line", client = resilient);
    assert.is_true(r.length() > 0)
  }
}
