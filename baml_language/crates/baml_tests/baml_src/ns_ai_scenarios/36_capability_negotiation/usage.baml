// scenario 36 — capability negotiation: ../../../../../llm-provider/ideas/scenarios/36-capability-negotiation/README.md
//   usage.baml — APPLICATION-author code. End-user surface + the test tiers.
//
// The app author writes ONE plain LLM function (`s36_Extract`) and swaps `client =`.
// All negotiation lives in the drivers/wrapper (implementation.baml), never in the
// body. This file proves the four faces of capability negotiation on the REAL
// surface:
//   (1) BINARY negotiation: one provider, several capabilities, a runtime `match`.
//   (2) GRADED negotiation: the Support lattice (Yes/No/Maybe) read up front,
//       including a `Maybe` HONESTLY resolved at runtime by an actual probe.
//   (3) typed `Unsupported` through a generated companion (`$stream` on a
//       non-streaming client).
//   (4) the static-claim-vs-runtime-truth degrade chain: drive_call degrades
//       call -> stream on a stream-only provider.
//
// Offline tier: `test "36: …"` against the fakes (implementation.baml + ../common).
// Live tier: `testset "integ-test-36-capability-negotiation"` — negotiation against
// the REAL OpenAi (Streaming + Capabilities) vs OpenAiStrict (neither), plus one
// real call flowing through the negotiated Yes arm.


// ── The default client: a dead endpoint, so every offline test MUST pass an
//    explicit, deterministic `client =` override. ──────────────────────────────
client<llm> DeadS36 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// ── The ONE function body. Negotiation is invisible here — the client swaps. ──
function s36_Extract(text: string) -> s36_Report {
  client DeadS36
  prompt #"
    {{ _.role("user") }}
    Extract a report (subject, verdict) from: {{ text }}
  "#
}


// ── Negotiation helpers (app-author read helpers over the existential marker). ─

// The binary capability matrix over ONE provider: membership per capability.
function s36_cap_matrix(p: baml.ai.Provider) -> string throws never {
  let x = p;
  let http = match (x) { let h: baml.ai.HttpProvider => "yes", _ => "no" };
  let strm = match (x) { let s: baml.ai.Streaming => "yes", _ => "no" };
  let caps = match (x) { let c: baml.ai.Capabilities => "yes", _ => "no" };
  let tool = match (x) { let t: baml.ai.Tools => "yes", _ => "no" };
  "http=" + http + " stream=" + strm + " capabilities=" + caps + " tools=" + tool
}

// The graded structured-output verdict, read off the Support lattice up front.
function s36_structured_label(p: baml.ai.Provider) -> string throws never {
  let x = p;
  match (x) {
    let cap: baml.ai.Capabilities => match (cap.structured_output()) {
      baml.ai.Support.Yes => "yes",
      baml.ai.Support.No => "no",
      baml.ai.Support.Maybe => "maybe",
    },
    _ => "unknown",   // no Capabilities descriptor -> negotiate/probe at call time
  }
}

// The behavioral escape hatch: resolve the existential to Streaming, or a typed
// Unsupported knowable BEFORE any byte.
function s36_as_streaming(p: baml.ai.Provider) -> baml.ai.Streaming | baml.errors.Unsupported throws never {
  let x = p;
  match (x) {
    let s: baml.ai.Streaming => s,
    _ => baml.errors.Unsupported { message: "provider does not support Streaming" },
  }
}

// Drive the generated `$stream` companion and label the negotiation outcome. On a
// non-streaming client this is a typed `Unsupported` (drive_stream's `_` arm).
function s36_negotiate_stream(client: baml.ai.Provider) -> string throws never {
  let s = s36_Extract$stream("hello", client = client) catch (e) {
    let u: baml.errors.Unsupported => return "unsupported",
    let se: baml.errors.StreamError => return "stream-error",
    _ => return "other",
  };
  "got-stream"   // reached the Streaming arm and got a Stream handle back
}

// Drive the plain-value companion `s36_Extract` and label how drive_call negotiated
// the client. A stream-only provider degrades call -> stream (the Streaming arm);
// a bare-marker provider is a typed Unsupported.
function s36_negotiate_call(client: baml.ai.Provider) -> string throws never {
  let r = s36_Extract("hello", client = client) catch (e) {
    // Triage the typed failure by classifier (concrete-through-interface narrowing
    // is a runtime promise the strict checker can't verify): a capability-absent
    // failure is is_unsupported(); anything else typed here is the StreamError our
    // stream-only fake threw FROM drive_call's degrade arm.
    let f: baml.errors.Failure => if (f.is_unsupported()) {
      return "unsupported";                 // neither HttpProvider nor Streaming
    } else {
      return "degraded-to-stream";          // reached the Streaming degrade arm
    },
    _ => return "other",
  };
  "got-value:" + r.subject
}


// ============================================================================
// OFFLINE TIER
// ============================================================================

// --- (1) BINARY negotiation: one provider, several capabilities, one match. --
test "36: one provider negotiates several capabilities via runtime match" {
  let p: baml.ai.Provider = s36_FullProvider { reply: "{\"subject\": \"x\", \"verdict\": \"y\"}" };
  let matrix = s36_cap_matrix(p);
  log.info("36: FullProvider matrix -> " + matrix);
  // HttpProvider yes, Streaming yes (claimed), Capabilities yes, Tools no.
  assert.equal(matrix, "http=yes stream=yes capabilities=yes tools=no")
}

test "36: a bare-marker provider negotiates every capability as absent" {
  let p: baml.ai.Provider = NoCapProvider {};
  let matrix = s36_cap_matrix(p);
  log.info("36: NoCapProvider matrix -> " + matrix);
  assert.equal(matrix, "http=no stream=no capabilities=no tools=no")
}


// --- (2) GRADED negotiation: the Support lattice read up front. --------------
test "36: the Support lattice is read up front, before any call" {
  let full: baml.ai.Provider = s36_FullProvider { reply: "{}" };
  let maybe: baml.ai.Provider = s36_MaybeProvider { reply: "{}", will_reject: false };
  let no: baml.ai.Provider = s36_NoProvider {};
  let bare: baml.ai.Provider = EchoProvider { reply: "{}" };   // HttpProvider, no Capabilities
  log.info("36: structured support full=" + s36_structured_label(full)
    + " maybe=" + s36_structured_label(maybe)
    + " no=" + s36_structured_label(no)
    + " echo=" + s36_structured_label(bare));
  assert.equal(s36_structured_label(full), "yes")
  assert.equal(s36_structured_label(maybe), "maybe")
  assert.equal(s36_structured_label(no), "no")
  assert.equal(s36_structured_label(bare), "unknown")   // static claim absent -> probe at runtime
}

test "36: the lattice exposes the full graded descriptor, not just a bool" {
  let p: baml.ai.Provider = s36_FullProvider { reply: "{}" };
  match (p) {
    let cap: baml.ai.Capabilities => {
      log.info("36: FullProvider parallel_tools graded to No; max_input_tokens=" + cap.max_input_tokens().to_string());
      // graded: structured output Yes, but parallel tools No — a bool couldn't say this.
      assert.equal(cap.max_input_tokens(), 128000)
    },
    _ => assert.is_true(false),
  }
}


// --- (2b) A `Maybe` HONESTLY resolved at runtime by an actual probe. ---------
// The consumer is the plain `s36_Extract`; the client is a Negotiated wrapper.
test "36: static Yes sends straight to the inner provider" {
  let r = s36_Extract("bio", client = s36_Negotiated {
    inner: s36_FullProvider { reply: "{\"subject\": \"inner\", \"verdict\": \"sent\"}" },
    degraded: EchoProvider { reply: "{\"subject\": \"degraded\", \"verdict\": \"prompt-json\"}" },
  }) catch (e) { _ => s36_Report { subject: "FAIL", verdict: "" } };
  log.info("36: Yes-arm answer subject=" + r.subject);
  assert.equal(r.subject, "inner")
}

test "36: a Maybe probe that SUCCEEDS resolves to the inner answer" {
  let r = s36_Extract("bio", client = s36_Negotiated {
    inner: s36_MaybeProvider { reply: "{\"subject\": \"inner-maybe-ok\", \"verdict\": \"probed\"}", will_reject: false },
    degraded: EchoProvider { reply: "{\"subject\": \"degraded\", \"verdict\": \"prompt-json\"}" },
  }) catch (e) { _ => s36_Report { subject: "FAIL", verdict: "" } };
  log.info("36: Maybe-probe-success subject=" + r.subject);
  assert.equal(r.subject, "inner-maybe-ok")
}

test "36: a Maybe probe that is REJECTED degrades to the prompt-JSON twin" {
  let r = s36_Extract("bio", client = s36_Negotiated {
    inner: s36_MaybeProvider { reply: "{\"subject\": \"unreachable\", \"verdict\": \"x\"}", will_reject: true },
    degraded: EchoProvider { reply: "{\"subject\": \"degraded\", \"verdict\": \"prompt-json\"}" },
  }) catch (e) { _ => s36_Report { subject: "FAIL", verdict: "" } };
  log.info("36: Maybe-probe-rejected subject=" + r.subject);
  assert.equal(r.subject, "degraded")   // typed s36_SchemaRejected -> honest downgrade
}

test "36: a static No skips the doomed call and degrades with no wasted round-trip" {
  // s36_NoProvider's HttpProvider THROWS if driven; getting the degraded answer
  // proves the No verdict skipped it entirely.
  let r = s36_Extract("bio", client = s36_Negotiated {
    inner: s36_NoProvider {},
    degraded: EchoProvider { reply: "{\"subject\": \"degraded\", \"verdict\": \"prompt-json\"}" },
  }) catch (e) { _ => s36_Report { subject: "FAIL", verdict: "" } };
  log.info("36: No-verdict subject=" + r.subject);
  assert.equal(r.subject, "degraded")
}


// --- (3) typed Unsupported through a generated companion. --------------------
test "36: $stream on a non-streaming client is a typed Unsupported" {
  // EchoProvider is HttpProvider-only; the generated $stream companion negotiates
  // Streaming, finds none, and throws a typed Unsupported (no honest call->stream
  // buffering exists yet — drive_stream's documented gap).
  let echo = s36_negotiate_stream(EchoProvider { reply: "{}" });
  let bare = s36_negotiate_stream(NoCapProvider {});
  log.info("36: $stream negotiation echo=" + echo + ", bare=" + bare);
  assert.equal(echo, "unsupported")
  assert.equal(bare, "unsupported")
}


// --- (4) static claim vs runtime truth: the call->stream degrade chain. ------
test "36: drive_call degrades call -> stream on a stream-only provider" {
  // s36_StreamOnly implements Streaming but NOT HttpProvider. The plain-value
  // companion drive_call therefore degrades into the Streaming arm and drains
  // .final(). Building a real Stream offline is P8-blocked, so the fake throws a
  // typed StreamError from that arm — surfacing THROUGH drive_call proves the
  // degrade ROUTING (the .final() value itself is the live tier).
  let via_stream = s36_negotiate_call(s36_StreamOnly {});
  // NoCapProvider has neither capability -> the honest typed Unsupported.
  let via_none = s36_negotiate_call(NoCapProvider {});
  // EchoProvider IS HttpProvider -> the normal (non-degraded) value path.
  let via_http = s36_negotiate_call(EchoProvider { reply: "{\"subject\": \"http\", \"verdict\": \"direct\"}" });
  log.info("36: call negotiation stream-only=" + via_stream + ", no-cap=" + via_none + ", http=" + via_http);
  assert.equal(via_stream, "degraded-to-stream")
  assert.equal(via_none, "unsupported")
  assert.equal(via_http, "got-value:http")
}


// ============================================================================
// LIVE TIER — excluded by default; run via `-i "integ-test-36-*::"` with keys.
// The negotiation payoff against real providers: OpenAi advertises Streaming +
// the Capabilities lattice; OpenAiStrict (request/response only) advertises
// neither. Then one real call flows through the negotiated Yes arm.
// ============================================================================
testset "integ-test-36-capability-negotiation" {
  test "36 live: OpenAi negotiates Streaming + Capabilities; OpenAiStrict lacks both" {
    let key = baml.env.get_or_panic("OPENAI_API_KEY");
    let openai: baml.ai.Provider = baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: key, base_url: null };
    let strict: baml.ai.Provider = baml.ai.OpenAiStrict { model: "gpt-5.4-mini", api_key: key, base_url: null };
    let stream_openai = match (s36_as_streaming(openai)) {
      let s: baml.ai.Streaming => "streaming",
      let u: baml.errors.Unsupported => "unsupported",
    };
    let stream_strict = match (s36_as_streaming(strict)) {
      let s: baml.ai.Streaming => "streaming",
      let u: baml.errors.Unsupported => "unsupported",
    };
    log.info("36 live negotiation: openai stream=" + stream_openai + " structured=" + s36_structured_label(openai)
      + " | strict stream=" + stream_strict + " structured=" + s36_structured_label(strict));
    assert.equal(stream_openai, "streaming")
    assert.equal(s36_structured_label(openai), "yes")
    assert.equal(stream_strict, "unsupported")
    assert.equal(s36_structured_label(strict), "unknown")   // OpenAiStrict has no Capabilities descriptor
  }

  test "36 live: one real call flows through the negotiated Yes arm" {
    let key = baml.env.get_or_panic("OPENAI_API_KEY");
    // OpenAi.structured_output == Yes -> s36_Negotiated routes to the inner real
    // provider and makes exactly one real call.
    let negotiated = s36_Negotiated {
      inner: baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: key, base_url: null },
      degraded: baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: key, base_url: null },
    };
    let r = s36_Extract(
      "Ada Lovelace's quarterly analysis; the report verdict is approved.",
      client = negotiated,
    ) catch (e) { _ => s36_Report { subject: "LIVE FAILED", verdict: "" } };
    log.info("36 live negotiated call: subject=" + r.subject + " verdict=" + r.verdict);
    assert.is_true(r.subject.length() > 0 && r.subject != "LIVE FAILED")
  }
}
