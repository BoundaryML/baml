// scenario 35 — deployment shapes: ../../../../../llm-provider/ideas/scenarios/35-deployment-shapes/README.md
//   implementation.baml — the STD-LIB / library-author half.
//
// The design: ONE provider layer, four homes (server-with-keys, edge/proxy,
// browser/local, durable/cron). What changes between homes is NOT the provider's
// wire behavior — it is three orthogonal things: (1) WHERE the secret comes from,
// (2) WHO holds the long-lived key, (3) which transports survive. The design
// hand-rolled an `OpenAIChat` class whose ONE divergence point was `auth: Credential`
// (a net-new capability interface with BearerKey / IamCredential / EphemeralToken
// variants), plus net-new host surface (`host_can_hold`, `SessionMint.for_browser`,
// `TransportUnviable`).
//
// On the REAL surface that "one provider, auth is the divergence" story is ALREADY
// stdlib: `baml.ai.OpenAiCompatible { model, base_url, auth }` is the Chat-Completions
// codec against ANY base_url, with a TYPED auth union
//   `BearerAuth { token } | HeaderAuth { name, value } | NoAuth {}`.
// So the three HTTP deployment shapes collapse to THREE calls of ONE constructor
// that differ ONLY in `base_url` + `auth` — exactly the design's teaching point,
// with zero net-new surface:
//
//   server-with-keys  ->  BearerAuth   (the app holds the provider key, from env)
//   edge / proxy      ->  HeaderAuth   (the key stays SERVER-SIDE behind a gateway;
//                                       the app carries only a gateway token header)
//   local / dev       ->  NoAuth       (a local runtime — NO key material at all)
//
// "A client is a function returning a Provider" is the portability mechanism: each
// home is a zero-/small-arg FACTORY over the same class, and the app function is
// written once and parameterized on the provider it is handed.
//
// DIVERGENCE from the design (flagged in the report): the browser EPHEMERAL-KEY
// mint (`SessionMint.for_browser`), the realtime SOCKET home, and `host_can_hold`
// transport-viability probing are net-new HOST surface that does not exist on the
// real `baml.ai` model. The realized surface keeps the browser shape as its OTHER
// safe pattern — same-origin PROXY (HeaderAuth gateway, key held server-side) —
// which IS expressible today; the socket/ephemeral-token machinery is not built.


// ---------------------------------------------------------------------------
// Offline-safe key read: use the real env var when present (live tier), else a
// placeholder so CONSTRUCTION stays pure offline. Construction never touches the
// network — only `.call`/`.stream` do — so every factory below is exercisable
// offline for its SHAPE (base_url + auth) with no key material. (Mirrors s28_env.)
// ---------------------------------------------------------------------------
function s35_env(name: string) -> string throws never {
    (baml.env.get(name) catch (e) { _ => null }) ?? "offline-placeholder-key"
}


// ===========================================================================
// THE THREE HOMES — one class (`OpenAiCompatible`), three factories, differing
// ONLY in construction (`base_url` + typed `auth`). "Which home" is a function
// call (design §a: a client is a function returning a Provider).
// ===========================================================================

// (1) SERVER-WITH-KEYS. The app process holds the long-lived provider key
//     (read from an env var) and speaks Bearer straight to the provider.
function s35_server_client() -> baml.ai.Provider throws never {
    baml.ai.OpenAiCompatible {
        model: "gpt-5.4-mini",
        base_url: "https://api.openai.com/v1",
        auth: baml.ai.BearerAuth { token: s35_env("OPENAI_API_KEY") },
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
    }
}

// (2) EDGE / PROXY. The provider key is held SERVER-SIDE behind a gateway; the
//     app (edge function, or a browser talking to same-origin) never sees it. It
//     carries only a scoped GATEWAY token in a custom header — HeaderAuth. Same
//     class, same wire body; only base_url + auth changed.
function s35_proxy_client() -> baml.ai.Provider throws never {
    baml.ai.OpenAiCompatible {
        model: "gpt-5.4-mini",
        base_url: s35_env_or("LLM_GATEWAY_URL", "https://gateway.internal/v1"),
        auth: baml.ai.HeaderAuth { name: "x-gateway-key", value: s35_env("LLM_GATEWAY_TOKEN") },
        extra_headers: { "x-app": "deployment-shapes" },
        extra_body: null,
        append_output_schema: null,
    }
}

// (3) LOCAL / DEV. A local runtime (Ollama /v1, vLLM, llama.cpp server): NO key
//     material is constructible here at all — NoAuth. The single most important
//     key-isolation claim: this home cannot leak a secret because it never holds
//     one. Same OpenAiCompatible codec, pointed at localhost.
function s35_local_client() -> baml.ai.Provider throws never {
    baml.ai.OpenAiCompatible {
        model: "llama-3.3-70b",
        base_url: "http://localhost:11434/v1",
        auth: baml.ai.NoAuth {},
        extra_headers: null,
        extra_body: null,
        append_output_schema: null,
    }
}

// (4) DURABLE / CRON. Combinators wrap the server provider UNCHANGED (design §a:
//     "nobody is watching, retry hard" is pure combinator stacking; the provider
//     body never learns it is in a cron job). Retry over a Fallback to a second
//     region-shaped endpoint — the same reliability posture as scenario 29, here
//     just a different HOME reusing the identical primitives.
function s35_cron_client() -> baml.ai.Provider throws never {
    baml.ai.Retry {
        inner: baml.ai.Fallback {
            members: [s35_server_client(), s35_proxy_client()],
        },
        max: 5,
        base_delay_ms: null,
    }
}


// ---------------------------------------------------------------------------
// A tiny env helper with a caller-supplied fallback (base URLs, not secrets).
// ---------------------------------------------------------------------------
function s35_env_or(name: string, fallback: string) -> string throws never {
    (baml.env.get(name) catch (e) { _ => null }) ?? fallback
}


// ---------------------------------------------------------------------------
// KEY-ISOLATION probe, off the EXISTENTIAL Provider (design's "recover the
// concrete via a runtime match" theme). Reports which auth shape a home carries
// WITHOUT sending a byte — the offline proof that the divergence is construction,
// and that the local home holds no key while the proxy home holds only a gateway
// token (never the provider key).
//   "bearer" -> the provider key is in this process (server home)
//   "header" -> only a gateway token; the real key is server-side (proxy home)
//   "none"   -> no key material at all (local home)
// ---------------------------------------------------------------------------
function s35_auth_kind(p: baml.ai.Provider) -> string throws never {
    match (p) {
        let c: baml.ai.OpenAiCompatible => match (c.auth) {
            let b: baml.ai.BearerAuth => "bearer",
            let h: baml.ai.HeaderAuth => "header",
            _                         => "none",
        },
        _ => "unknown",
    }
}


// ---------------------------------------------------------------------------
// Capability probe off the existential: does this home negotiate the request/
// response floor, and can it stream? SSE is universal — every HTTP home streams
// (design: "the streaming companion is viable in ALL homes because SSE is
// universal"). Used to prove all three shapes clear the same capability bar.
// ---------------------------------------------------------------------------
function s35_capability_of(p: baml.ai.Provider) -> string throws never {
    match (p) {
        let s: baml.ai.Streaming    => "stream-capable",
        let h: baml.ai.HttpProvider => "call-only",
        _                           => "bare-marker",
    }
}
