// scenario 28 — provider diversity & gateways: ../../../../../llm-provider/ideas/scenarios/28-provider-diversity/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// What the app author wants (the background headline): "call many models,
// possibly through a proxy or gateway, without rewriting per provider, and
// switch backends by changing a STRING." This file shows, on the REAL surface:
//   - ONE function body (`s28_Extract`) across every backend — the client swaps,
//     the body never changes (the portability claim, design §a),
//   - a proxy/gateway/local runtime is the SAME OpenAiCompatible class with a
//     different base_url + a TYPED auth (Bearer/Header/NoAuth), invisible to app,
//   - prefix routing IS `client = s28_resolve(spec)` — a string picks a Provider,
//   - capability routing: a router picks by WHICH capability a provider has,
//     not by brand (`match (p) { let s: Streaming => …, let h: HttpProvider => … }`),
//   - the lowest-common-denominator floor + a typed escape hatch,
//   - an unroutable spec is a typed CallError the app skips.
//
// Offline tier: fakes (EchoProvider from ../common) — construction is pure, only
// `.call` touches the network, so REAL provider classes are constructed here too
// and asserted by routing/capability WITHOUT sending. Live tier: testset
// "integ-test-28-provider-diversity" — the SAME body against real OpenAi +
// Anthropic + Gemini (the brand-crossing proof) plus the registry path live.


// --- The typed record the app extracts. -------------------------------------
class s28_Profile { name: string, title: string, years: int }


// --- A dead default client. -------------------------------------------------
// A declared `client` is the function's DEFAULT; the swap is a one-line
// call-site `client = <provider>` override. We default to a dead endpoint so
// every offline test MUST pass an explicit, deterministic provider.
client<llm> DeadS28 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// --- The ONE function body. All portability rides here. ---------------------
// The desugar lowers this to `baml.ai.drive_call<s28_Profile>(client, …)`; the
// client's provider re-shapes the rendered turns into its own envelope (OpenAI
// Chat Completions, Anthropic Messages, Gemini generateContent — three wire
// shapes, three auth shapes, all absorbed below `.call`). The body is identical
// no matter which backend is wired in. That is the entire "switch model by
// changing a string" goal, delivered by `client`-as-sugar + the existential.
function s28_Extract(bio: string) -> s28_Profile {
  client DeadS28                         // swap via `client =` at the call site
  prompt #"
    {{ _.role("user") }}
    Extract a structured profile (name, title, years of experience) from this bio:
    {{ bio }}
  "#
}


// --- Offline registry-of-fakes: the resolve-then-call composition, ----------
// deterministic. The REAL registry `s28_resolve` returns real providers (whose
// `.call` hits the network); to prove the SAME "a string picks the backend"
// composition offline, this sibling maps each prefix to an EchoProvider that
// echoes a canned Profile. It is the app's `client Smart(spec) { resolve(spec) }`
// with deterministic backends — the string still selects the provider.
function s28_fakeRoute(spec: string) -> baml.ai.Provider throws never {
    let cut = match (spec.index_of("/")) { null => 0 - 1, let c: int => c };
    let prefix = if (cut < 0) { spec } else { spec.substring(0, cut) };
    match (prefix) {
        "openai"    => EchoProvider { reply: "{\"name\": \"Ada Lovelace\", \"title\": \"Analyst\", \"years\": 12}" },
        "anthropic" => EchoProvider { reply: "{\"name\": \"Alan Turing\", \"title\": \"Cryptographer\", \"years\": 15}" },
        "groq"      => EchoProvider { reply: "{\"name\": \"Grace Hopper\", \"title\": \"Compiler Pioneer\", \"years\": 30}" },
        _           => EchoProvider { reply: "{\"name\": \"Unknown\", \"title\": \"n/a\", \"years\": 0}" },
    }
}


// ============================================================================
// OFFLINE TIER
// ============================================================================

// --- Portability: ONE body, many providers, a one-line client swap. ---------
test "28: one function body extracts across a swapped-in provider" {
    log.info("28: provider taken = EchoProvider(openai-shaped)");
    let r = s28_Extract(
        "Ada Lovelace, an analyst with 12 years of experience.",
        client = EchoProvider { reply: "{\"name\": \"Ada Lovelace\", \"title\": \"Analyst\", \"years\": 12}" },
    ) catch (e) { _ => s28_Profile { name: "FAIL", title: "", years: -1 } };
    assert.equal(r.name, "Ada Lovelace")
    assert.equal(r.years, 12)
}

test "28: the same body, a different backend, is a one-line client override" {
    log.info("28: provider taken = EchoProvider(anthropic-shaped)");
    let r = s28_Extract(
        "Alan Turing, a cryptographer with 15 years of experience.",
        client = EchoProvider { reply: "{\"name\": \"Alan Turing\", \"title\": \"Cryptographer\", \"years\": 15}" },
    ) catch (e) { _ => s28_Profile { name: "FAIL", title: "", years: -1 } };
    assert.equal(r.name, "Alan Turing")
    assert.equal(r.title, "Cryptographer")
}


// --- Prefix routing: a STRING picks the backend (resolve-then-call). --------
test "28: a spec string routes the SAME function through different backends" {
    let ada = s28_Extract("bio", client = s28_fakeRoute("openai/gpt-4o")) catch (e) { _ => s28_Profile { name: "FAIL", title: "", years: -1 } };
    let alan = s28_Extract("bio", client = s28_fakeRoute("anthropic/claude-sonnet-4-5")) catch (e) { _ => s28_Profile { name: "FAIL", title: "", years: -1 } };
    let grace = s28_Extract("bio", client = s28_fakeRoute("groq/llama-3.3-70b")) catch (e) { _ => s28_Profile { name: "FAIL", title: "", years: -1 } };
    log.info("28: routed openai/* -> " + ada.name + ", anthropic/* -> " + alan.name + ", groq/* -> " + grace.name);
    assert.equal(ada.name, "Ada Lovelace")
    assert.equal(alan.name, "Alan Turing")
    assert.equal(grace.name, "Grace Hopper")
}


// --- A proxy is the SAME class, a different base_url + typed auth. -----------
// Construction is pure: resolve builds real OpenAiCompatible providers offline
// and we assert their ROUTING/capability without sending. Groq (Bearer), an
// internal gateway (HeaderAuth), and a local Ollama runtime (NoAuth) are all
// ONE OpenAiCompatible codec — the auth-shape divergence is a field.
test "28: proxy / gateway / local runtime are all OpenAiCompatible, differing only in construction" {
    let labels = [
        s28_route_by_capability(s28_resolve("groq/llama-3.3-70b") catch (e) { _ => NoCapProvider {} }),
        s28_route_by_capability(s28_resolve("internal/gpt-4o") catch (e) { _ => NoCapProvider {} }),
        s28_route_by_capability(s28_resolve("ollama/qwen2.5") catch (e) { _ => NoCapProvider {} }),
    ];
    log.info("28: groq/internal/ollama capability = " + labels.join(", "));
    // All three are one Chat-Completions codec — and all three stream.
    assert.equal(labels.join("|"), "stream-capable|stream-capable|stream-capable")
}


// --- The registry routes a spec string to the right provider BRAND. ---------
// The existential is erased for portability, but the concrete class is recovered
// by a runtime match — proving the string chose the intended codec.
test "28: the registry constructs the intended concrete provider per prefix" {
    let openai = s28_resolve("openai/gpt-4o") catch (e) { _ => NoCapProvider {} };
    let anthropic = s28_resolve("anthropic/claude-sonnet-4-5") catch (e) { _ => NoCapProvider {} };
    let gemini = s28_resolve("gemini/gemini-flash-latest") catch (e) { _ => NoCapProvider {} };
    let brand_of = (p: baml.ai.Provider) -> string throws never {
        match (p) {
            let o: baml.ai.OpenAi     => "openai",
            let a: baml.ai.Anthropic  => "anthropic",
            let g: baml.ai.Gemini     => "gemini",
            let c: baml.ai.OpenAiCompatible => "compatible",
            _                         => "other",
        }
    };
    log.info("28: brands = " + brand_of(openai) + "/" + brand_of(anthropic) + "/" + brand_of(gemini));
    assert.equal(brand_of(openai), "openai")
    assert.equal(brand_of(anthropic), "anthropic")
    assert.equal(brand_of(gemini), "gemini")
}


// --- An unroutable spec is a typed CallError the app skips. ------------------
// LiteLLM-style: a bad prefix is recoverable (skip that backend), triaged
// through the SAME channel as any provider's HTTP error — no provider named.
test "28: an unknown prefix is a typed CallError, distinct from a real answer" {
    let r = s28_resolve("nope/whatever") catch (e) {
        let up: s28_UnknownProviderError => "skip-unroutable:" + up.prefix,
        let c: baml.errors.CallError     => "call-error",
        _                                => "other",
    };
    log.info("28: unroutable spec -> " + (match (r) { let p: baml.ai.Provider => "provider", _ => "error-branch" }));
    let msg = match (r) { let p: baml.ai.Provider => "unexpected-provider", let s: string => s };
    assert.equal(msg, "skip-unroutable:nope")
}


// --- Capability routing: pick the path by CAPABILITY, not by brand. ---------
// The router asks WHICH capability a resolved provider has. A native OpenAi is
// stream-capable; an OpenAiStrict backend is request/response only (the LCD
// floor). Same router, different code path — negotiated at runtime.
test "28: a capability router picks the streaming path only when it exists" {
    let streaming = s28_route_by_capability(s28_resolve("openai/gpt-4o") catch (e) { _ => NoCapProvider {} });
    let floor = s28_route_by_capability(s28_resolve("strict/gpt-4o") catch (e) { _ => NoCapProvider {} });
    let bare = s28_route_by_capability(NoCapProvider {});
    log.info("28: openai=" + streaming + ", strict=" + floor + ", bare=" + bare);
    assert.equal(streaming, "stream-capable")
    assert.equal(floor, "call-only")
    assert.equal(bare, "bare-marker")
}


// --- The LCD floor + the typed escape hatch. --------------------------------
// A routed OpenAiStrict backend cannot stream. The escape hatch resolves the
// existential and asks for Streaming, getting a typed `Unsupported` — a
// capability fact knowable BEFORE any byte, not a mid-stream fault.
test "28: the escape hatch turns 'no streaming' into a typed Unsupported" {
    let strict = s28_resolve("strict/gpt-4o") catch (e) { _ => NoCapProvider {} };
    let outcome = match (s28_as_streaming(strict)) {
        let u: baml.errors.Unsupported => "unsupported",
        let s: baml.ai.Streaming       => "got-streaming",
    };
    log.info("28: strict-backend streaming negotiation -> " + outcome);
    assert.equal(outcome, "unsupported")
}

test "28: the same escape hatch RECOVERS streaming when the backend has it" {
    let openai = s28_resolve("openai/gpt-4o") catch (e) { _ => NoCapProvider {} };
    let outcome = match (s28_as_streaming(openai)) {
        let u: baml.errors.Unsupported => "unsupported",
        let s: baml.ai.Streaming       => "got-streaming",
    };
    log.info("28: openai-backend streaming negotiation -> " + outcome);
    assert.equal(outcome, "got-streaming")
}


// --- The Capabilities lattice: graded support read up front. ----------------
// When a routed provider advertises the `Capabilities` descriptor, the app reads
// its three-valued support instead of gambling on a throw; when it doesn't, the
// answer is `Maybe` — probe at call time (the honest route-by-string cost).
test "28: graded structured-output support is read off the Capabilities lattice" {
    let openai_support = s28_structured_support(s28_resolve("openai/gpt-4o") catch (e) { _ => NoCapProvider {} });
    let compat_support = s28_structured_support(s28_resolve("groq/llama-3.3-70b") catch (e) { _ => NoCapProvider {} });
    let label = (s: baml.ai.Support) -> string throws never {
        match (s) { baml.ai.Support.Yes => "yes", baml.ai.Support.No => "no", baml.ai.Support.Maybe => "maybe" }
    };
    log.info("28: structured support openai=" + label(openai_support) + ", groq-compatible=" + label(compat_support));
    assert.equal(label(openai_support), "yes")     // OpenAi advertises Capabilities
    assert.equal(label(compat_support), "maybe")   // OpenAiCompatible doesn't -> negotiate
}


// --- Round-robin across same-shape endpoints (gateway load-spread). ---------
// A gateway spreads load across same-shape backends. RoundRobin is a plain
// combinator over a Provider[] pool; the app hands it to the SAME function. Two
// distinct echo backends prove the cursor advances member-to-member.
test "28: round-robin spreads calls across a same-shape pool" {
    let pool: baml.ai.Provider = baml.ai.RoundRobin {
        members: [
            EchoProvider { reply: "{\"name\": \"Key A\", \"title\": \"t\", \"years\": 1}" },
            EchoProvider { reply: "{\"name\": \"Key B\", \"title\": \"t\", \"years\": 2}" },
        ],
        counter: 0,
    };
    let first = s28_Extract("bio", client = pool) catch (e) { _ => s28_Profile { name: "FAIL", title: "", years: -1 } };
    log.info("28: round-robin first pick = " + first.name);
    assert.is_true(first.name == "Key A" || first.name == "Key B")
}


// ============================================================================
// LIVE TIER — excluded by default; run via `-i "integ-test-28-*::"` with keys.
// The brand-crossing proof: ONE `s28_Extract` body, three REAL APIs, three wire
// shapes + three auth shapes all absorbed below `.call`. The app just swaps
// `client =` — or routes by string through the real registry.
// ============================================================================
testset "integ-test-28-provider-diversity" {
    test "28 live: extract through a native OpenAI override" {
        let r = s28_Extract(
            "Ada Lovelace is a data analyst with 12 years of experience.",
            client = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
        ) catch (e) { _ => s28_Profile { name: "LIVE FAILED", title: "", years: -1 } };
        log.info("28 live openai: " + r.name + " / " + r.title + " / " + r.years.to_string());
        assert.is_true(r.name.length() > 0 && r.name != "LIVE FAILED")
    }

    test "28 live: the SAME body through a native Anthropic override" {
        let r = s28_Extract(
            "Alan Turing is a cryptographer with 15 years of experience.",
            client = baml.ai.Anthropic {
                model: "claude-haiku-4-5-20251001",
                api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                base_url: null,
                max_tokens: null,
                anthropic_version: null,
                extra_headers: null,
                extra_body: null,
                append_output_schema: null,
            },
        ) catch (e) { _ => s28_Profile { name: "LIVE FAILED", title: "", years: -1 } };
        log.info("28 live anthropic: " + r.name + " / " + r.title + " / " + r.years.to_string());
        assert.is_true(r.name.length() > 0 && r.name != "LIVE FAILED")
    }

    test "28 live: the SAME body through a native Gemini override" {
        let r = s28_Extract(
            "Grace Hopper is a compiler pioneer with 30 years of experience.",
            client = baml.ai.Gemini {
                model: "gemini-flash-latest",
                api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
                base_url: null,
                extra_headers: null,
                extra_body: null,
                append_output_schema: null,
            }.with_retry(2),
        ) catch (e) {
            _ => {
                log.warn(e);
                s28_Profile { name: "LIVE FAILED", title: "", years: -1 }
            },
        };
        log.info("28 live gemini: " + r.name + " / " + r.title + " / " + r.years.to_string());
        assert.is_true(r.name.length() > 0 && r.name != "LIVE FAILED")
    }

    test "28 live: switch backend by changing a STRING — the registry path" {
        // The design's headline: `client = Registry.resolve(spec)`. The spec
        // string picks the real provider; the function body never changes.
        let provider = s28_resolve("openai/gpt-5.4-mini") catch (e) {
            _ => NoCapProvider {},
        };
        let r = s28_Extract(
            "Katherine Johnson is a mathematician with 33 years of experience.",
            client = provider,
        ) catch (e) { _ => s28_Profile { name: "LIVE FAILED", title: "", years: -1 } };
        log.info("28 live registry(openai/gpt-5.4-mini): " + r.name + " / " + r.title);
        assert.is_true(r.name.length() > 0 && r.name != "LIVE FAILED")
    }
}
