// scenario 02 — structured output: ../../../../../llm-provider/ideas/scenarios/02-structured-output/README.md
// The whole point: the app author writes ONE return type (`-> S02Event`) and ONE
// prompt, and the SAME function works across four providers that put that schema on
// the wire four different ways — OpenAI Chat (`response_format`), OpenAI Responses
// (`text.format`), Anthropic (schema-append + SAP), Gemini (`responseSchema`). The
// four encodings live entirely in the provider; nothing here mentions response_format,
// text.format, tool injection, or responseSchema. The `-> S02Event` return type is
// the `T` each provider lowers to its own wire format (build_request<T>) and parses
// back (parse<T>). Offline tier proves SAP recovery + strict schema lowering + the
// refusal/StrictFallback discipline; the `integ-test-02-structured` testset exercises
// the four real wire encodings against live APIs.

// The one desired type. It flows in as T to build_request<T> AND parse<T>.
class S02Event {
    title: string,
    date: string,
    attendees: string[],
}

// A recursive type ($ref cycle) that OpenAI strict mode rejects at runtime (400).
// Used by ExtractOrg to motivate the strict→loose fallback (§6).
class S02OrgNode {
    name: string,
    reports: S02OrgNode[],
}

// The swappable default client. Offline tests override it with the shared fakes
// (`EchoProvider`, …); live tests override it with a real native provider. (An
// inline native provider can't be a declared client on the real surface, so the
// design's four `client Gpt.../Claude.../Gemini...` sugar blocks become the four
// native-provider overrides in the live testset below.)
client<llm> DeadS02 {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// --- the SAME function body, four provider encodings ------------------------
// Portability proof: only the wire encoding differs (chosen by the provider the
// client resolves to). The `-> S02Event` return type is the single T that each
// provider lowers and parses back. Design: ExtractEvent_OpenAI / _Responses /
// _Claude / _Gemini.

// Encoding #1 — OpenAI Chat Completions: strict `response_format.json_schema`
// (real provider `baml.ai.OpenAiStrict`; schema lowered via baml.schema.json_schema).
function ExtractEvent_OpenAI(text: string) -> S02Event {
    client DeadS02
    prompt #"Extract the calendar event from: {{ text }}"#
}

// Encoding #2 — OpenAI Responses API: SAME schema, nested under `text.format`
// (real provider `baml.ai.OpenAiResponses`). Only the envelope differs.
function ExtractEvent_Responses(text: string) -> S02Event {
    client DeadS02
    prompt #"Extract the calendar event from: {{ text }}"#
}

// Encoding #3 — Anthropic: no response_format. The real provider appends the
// output schema to the prompt and SAP-parses the reply (the design's forced-tool
// injection is one way to do this; the real stdlib provider uses schema-append).
function ExtractEvent_Claude(text: string) -> S02Event {
    client DeadS02
    prompt #"Extract the calendar event from: {{ text }}"#
}

// Encoding #4 — Gemini: `responseSchema` + `responseMimeType` in Gemini's own
// UPPERCASE OpenAPI dialect (real provider `baml.ai.Gemini`).
function ExtractEvent_Gemini(text: string) -> S02Event {
    client DeadS02
    prompt #"Extract the calendar event from: {{ text }}"#
}

// --- strict-vs-nonstrict negotiation via the S02StrictFallback combinator ----
// A recursive type that OpenAI strict mode 400s on. The strict/non-strict choice
// is INSIDE each provider's build_request, so the combinator holds two members and
// retries the whole call: strict primary → on a non-refusal failure, the always-
// constrained loose member. Composition is a function call. (Driven via the client
// override in tests; a bare S02StrictFallback literal can't be a declared client.)
function ExtractOrg(text: string) -> S02OrgNode {
    client DeadS02
    prompt #"Build the reporting tree from: {{ text }}"#
}

// --- a combinator chain reads left-to-right ---------------------------------
// retry transient failures, then fall back to a second provider. The `-> S02Event`
// type threads through every layer's call<S02Event> untouched. Design:
// `GptStructured().with_retry(2).fallback_to(ClaudeStructured())`.
function ExtractResilient(text: string) -> S02Event {
    client DeadS02
    prompt #"Extract the calendar event from: {{ text }}"#
}

// --- handling a refusal at the call site ------------------------------------
// A refusal is a typed error (S02Refused), NOT a malformed S02Event. The app
// distinguishes it from a transport/parse failure by catching the concrete refusal
// first, then triaging the rest by capability interface, with UnknownError the floor.
function ExtractEventSafe(text: string) -> S02Event {
    client DeadS02
    prompt #"Extract the calendar event from: {{ text }}"#
}

// app-level orchestration over the function's call. Runtime match, most-specific
// first: concrete S02Refused → CallError interface → UnknownError.
function TryExtract(text: string, provider: baml.ai.Provider) -> S02Event? {
    ExtractEventSafe(text, client = provider) catch (e) {
        let r: S02Refused               => null,       // model declined: no S02Event exists.
        let c: baml.errors.CallError    => throw c,    // rate-limit / parse / transport: propagate.
        let u: baml.errors.UnknownError => throw u,    // exotic/foreign failure: propagate.
    }
}


// ============================================================================
// OFFLINE TESTS (deterministic, no network / no keys)
// ============================================================================

test "02: SAP recovers the declared type from the provider reply" {
    let e = ExtractEvent_OpenAI("Launch on 2026-01-01 with Ada", client = EchoProvider {
        reply: "{\"title\": \"Launch\", \"date\": \"2026-01-01\", \"attendees\": [\"Ada\"]}",
    }) catch (er) {
        _ => S02Event { title: "failed", date: "", attendees: [] },
    };
    assert.equal(e.title, "Launch")
    assert.equal(e.date, "2026-01-01")
    assert.equal(e.attendees.length(), 1)
}

test "02: SAP is schema-aligned — prose around the JSON still parses" {
    let e = ExtractEvent_OpenAI("x", client = EchoProvider {
        reply: "Sure! Here you go: {\"title\": \"Sync\", \"date\": \"2026-02-02\", \"attendees\": []} hope that helps",
    }) catch (er) {
        _ => S02Event { title: "failed", date: "", attendees: [] },
    };
    assert.equal(e.title, "Sync")
}

test "02: one type, four entry points — the SAME reply decodes through every encoding" {
    // Portability headline: only the wire encoding differs per function; the T is
    // identical, so one reply body decodes to the same S02Event through all four.
    let reply = "{\"title\": \"Demo\", \"date\": \"2026-03-03\", \"attendees\": [\"Ada\", \"Bob\"]}";
    let a = ExtractEvent_OpenAI("x", client = EchoProvider { reply: reply }) catch (e) { _ => S02Event { title: "f", date: "", attendees: [] } };
    let b = ExtractEvent_Responses("x", client = EchoProvider { reply: reply }) catch (e) { _ => S02Event { title: "f", date: "", attendees: [] } };
    let c = ExtractEvent_Claude("x", client = EchoProvider { reply: reply }) catch (e) { _ => S02Event { title: "f", date: "", attendees: [] } };
    let d = ExtractEvent_Gemini("x", client = EchoProvider { reply: reply }) catch (e) { _ => S02Event { title: "f", date: "", attendees: [] } };
    assert.equal(a.title, "Demo")
    assert.equal(b.title, "Demo")
    assert.equal(c.title, "Demo")
    assert.equal(d.title, "Demo")
    assert.equal(d.attendees.length(), 2)
}

test "02: T flows into build_request — strict lowering adds additionalProperties" {
    // The design's build_request<T> lowers reflect.type_of<T>() to schema JSON; strict
    // mode (OpenAiStrict's response_format) toggles additionalProperties:false + all-required.
    let strict = baml.schema.json_schema(reflect.type_of<S02Event>(), true) catch (e) { _ => "ERR" };
    let loose = baml.schema.json_schema(reflect.type_of<S02Event>(), false) catch (e) { _ => "ERR" };
    assert.is_true(strict.includes("\"properties\""))
    assert.is_true(strict.includes("additionalProperties"))
    assert.is_true(!loose.includes("additionalProperties"))
}

test "02: StrictFallback degrades a non-refusal failure to the loose member" {
    // strict primary fails (not a refusal) → the always-constrained loose member serves it.
    let e = ExtractOrg("x", client = S02StrictFallback {
        primary: AlwaysFailProvider {},
        loose: EchoProvider { reply: "{\"name\": \"CEO\", \"reports\": []}" },
    }) catch (er) {
        _ => S02OrgNode { name: "failed", reports: [] },
    };
    assert.equal(e.name, "CEO")
}

test "02: StrictFallback treats a refusal as authoritative — never retries it" {
    // primary declines → propagate the refusal; the loose member is NOT called
    // (running it would let a decline masquerade as a schema-rejection retry).
    let outcome = ExtractOrg("x", client = S02StrictFallback {
        primary: S02RefusingProvider {},
        loose: EchoProvider { reply: "{\"name\": \"leaked\", \"reports\": []}" },
    }) catch (e) {
        let r: S02Refused            => "refused",
        let c: baml.errors.CallError => "other-call-error",
        _                            => "unknown",
    };
    assert.equal(outcome, "refused")
}

test "02: a fluent retry→fallback chain recovers a transient failure" {
    // ExtractResilient's design client: primary.with_retry(2).fallback_to(secondary).
    let e = ExtractResilient("x", client = AlwaysFailProvider {}
        .with_retry(2)
        .fallback_to(EchoProvider { reply: "{\"title\": \"Recovered\", \"date\": \"2026-04-04\", \"attendees\": []}" })
    ) catch (er) {
        _ => S02Event { title: "failed", date: "", attendees: [] },
    };
    assert.equal(e.title, "Recovered")
}

test "02: a refusal is a typed outcome, not a malformed Event" {
    // TryExtract returns null on a decline (no Event exists) and distinguishes it
    // from a success — the design's refusal-as-first-class-error.
    let declined = TryExtract("x", S02RefusingProvider {});
    assert.is_true(declined == null)

    let ok = TryExtract("x", EchoProvider {
        reply: "{\"title\": \"Real\", \"date\": \"2026-05-05\", \"attendees\": []}",
    });
    match (ok) {
        let e: S02Event => assert.equal(e.title, "Real"),
        null            => assert.is_true(false),
    }
}


// ============================================================================
// LIVE TIER — the four real wire encodings (excluded by default; run with keys)
// ============================================================================
testset "integ-test-02-structured" {
    // Encoding #1 — OpenAI Chat Completions strict response_format.json_schema.
    test "02 live: structured extraction via OpenAI strict json_schema" {
        let e = ExtractEvent_OpenAI("Ada Lovelace and Bob meet for Launch on 2026-01-01.", client = baml.ai.OpenAiStrict {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
            extra_headers: null,
            extra_body: null,
        }) catch (er) {
            _ => S02Event { title: "live failed", date: "", attendees: [] },
        };
        assert.equal(e.date, "2026-01-01")
        assert.is_true(e.attendees.length() >= 1)
    }

    // Encoding #2 — OpenAI Responses API text.format (same schema, different envelope).
    test "02 live: the same type through the OpenAI Responses text.format encoding" {
        let e = ExtractEvent_Responses("Ada and Bob meet for Launch on 2026-01-01.", client = baml.ai.OpenAiResponses {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
            store: null,
        }) catch (er) {
            _ => S02Event { title: "live failed", date: "", attendees: [] },
        };
        assert.equal(e.date, "2026-01-01")
    }

    // Encoding #3 — Anthropic (schema-append + SAP; no response_format).
    test "02 live: the same type through the Anthropic schema-append encoding" {
        let e = ExtractEvent_Claude("Ada and Bob meet for Launch on 2026-01-01.", client = baml.ai.Anthropic {
            model: "claude-haiku-4-5-20251001",
            api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
            base_url: null,
            max_tokens: null,
            anthropic_version: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }) catch (er) {
            _ => S02Event { title: "live failed", date: "", attendees: [] },
        };
        assert.equal(e.date, "2026-01-01")
    }

    // Encoding #4 — Gemini responseSchema in its UPPERCASE OpenAPI dialect.
    test "02 live: the same type through the Gemini responseSchema encoding" {
        let e = ExtractEvent_Gemini("Ada and Bob meet for Launch on 2026-01-01.", client = baml.ai.Gemini {
            model: "gemini-2.5-flash",
            api_key: baml.env.get_or_panic("GEMINI_API_KEY"),
            base_url: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }) catch (er) {
            _ => S02Event { title: "live failed", date: "", attendees: [] },
        };
        assert.equal(e.date, "2026-01-01")
    }
}
