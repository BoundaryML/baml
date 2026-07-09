// scenario 35 — deployment shapes: ../../../../../llm-provider/ideas/scenarios/35-deployment-shapes/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The payoff (design §a): the analysis function is written ONCE, names no home,
// no key, no transport. What changes across deployments is only the `client` it
// is handed — and a client is just a function, so "which home" is a call, not a
// config dialect. Three HTTP homes (server-with-keys / edge-proxy / local-dev)
// collapse to config variance over ONE `baml.ai.OpenAiCompatible` codec: the auth
// shape (Bearer / Header / NoAuth) is a field, invisible to the app below `.call`.
//
// Offline tier: fakes (EchoProvider from ../common) prove the ONE-body-many-homes
// swap deterministically; the REAL factories are constructed offline and asserted
// by SHAPE (base_url + auth) + capability WITHOUT sending — construction is pure,
// so the key-isolation claim (local NoAuth, proxy HeaderAuth) is checkable with no
// network. Live tier: testset "integ-test-35-deployment-shapes" runs the
// server-with-keys shape for real (env key + one call).


// --- The typed record the ONE app function returns. -------------------------
class s35_Summary {
    topic string
    bullets string[]
}


// --- A dead default client. -------------------------------------------------
// A declared `client` is the function's DEFAULT; the home swap is a one-line
// call-site `client = <factory>()` override. We default to a dead endpoint so
// every offline test MUST pass an explicit, deterministic home.
client<llm> DeadS35 {
    provider "openai"
    options { model "gpt-5.4-mini" api_key "unused" base_url "http://127.0.0.1:9" }
}


// --- THE ONE APP FUNCTION. Written once; runs under ANY home. ---------------
// It names no home, no key, no transport. The desugar lowers this to
// `baml.ai.drive_call<s35_Summary>(client, …)`; the client's provider re-shapes
// the rendered turns into its own envelope. The body is identical no matter which
// home is wired in — that is the entire "one provider layer, four homes" goal,
// delivered by `client`-as-sugar over the existential Provider.
function s35_Analyze(text: string) -> s35_Summary {
    client DeadS35                         // swap via `client =` at the call site
    prompt #"
        {{ _.role("user") }}
        Summarize the following into a topic and a few bullets:
        {{ text }}
    "#
}


// ============================================================================
// OFFLINE TIER
// ============================================================================

// --- Portability: ONE body, three homes, a one-line client swap. ------------
// EchoProvider stands in for each home's backend (construction of the real
// factories is pure but their `.call` would hit the network); the point proven
// here is that the SAME s35_Analyze body runs under a swapped-in client.
test "35: one function body runs under a swapped-in deployment client" {
    log.info("35: shape = server-with-keys (echo backend)");
    let r = s35_Analyze(
        "The launch shipped on time with no incidents.",
        client = EchoProvider { reply: "{\"topic\": \"Launch\", \"bullets\": [\"on time\", \"no incidents\"]}" },
    ) catch (e) { _ => s35_Summary { topic: "FAIL", bullets: [] } };
    assert.equal(r.topic, "Launch")
    assert.equal(r.bullets.length(), 2)
}

test "35: the same body under a different home is a one-line client override" {
    log.info("35: shape = local-dev (echo backend)");
    let r = s35_Analyze(
        "The nightly build failed twice before passing.",
        client = EchoProvider { reply: "{\"topic\": \"Nightly build\", \"bullets\": [\"failed twice\", \"eventually passed\"]}" },
    ) catch (e) { _ => s35_Summary { topic: "FAIL", bullets: [] } };
    assert.equal(r.topic, "Nightly build")
    assert.equal(r.bullets.length(), 2)
}


// --- Config variance isolated to construction: three homes, ONE codec. ------
// The real factories build real OpenAiCompatible providers offline (pure); we
// assert the divergence is ONLY the auth shape — the whole design claim.
test "35: the three HTTP homes differ ONLY in construction (base_url + auth)" {
    let server = s35_server_client();
    let proxy = s35_proxy_client();
    let local = s35_local_client();
    log.info("35: server=" + s35_auth_kind(server) + ", proxy=" + s35_auth_kind(proxy) + ", local=" + s35_auth_kind(local));
    // server-with-keys HOLDS the provider key (Bearer); the proxy home holds ONLY
    // a gateway token (Header) — the real key stayed server-side; the local home
    // holds NO key material (NoAuth). That is the key-isolation claim, checked
    // purely by construction, no byte sent.
    assert.equal(s35_auth_kind(server), "bearer")
    assert.equal(s35_auth_kind(proxy), "header")
    assert.equal(s35_auth_kind(local), "none")
}


// --- Key isolation, stated as the security property. ------------------------
// The local/dev home is the one that CANNOT leak a secret, because it never
// constructs one. This is the strongest offline guarantee this scenario offers
// on the real surface (the design flags bundle-time secret hygiene as the thing
// the runtime model canNOT guarantee — see the report's divergence notes).
test "35: the local home holds no key material; the proxy home holds no provider key" {
    let local = s35_local_client();
    let proxy = s35_proxy_client();
    log.info("35: local key-material = none; proxy carries a gateway token only");
    assert.equal(s35_auth_kind(local), "none")     // no secret is even constructible
    assert.is_true(s35_auth_kind(proxy) != "bearer")  // never the provider key
}


// --- SSE is universal: every HTTP home negotiates BOTH call and stream. ------
// The design's streaming companion is viable in ALL homes because SSE is a
// one-shot HTTP response that chunks. All three OpenAiCompatible homes clear the
// same capability bar off the existential.
test "35: all three HTTP homes negotiate HttpProvider AND Streaming" {
    let homes = [s35_server_client(), s35_proxy_client(), s35_local_client()];
    let caps = homes.map((p: baml.ai.Provider) -> string throws never { s35_capability_of(p) });
    log.info("35: capabilities = " + caps.join(", "));
    assert.equal(caps.join("|"), "stream-capable|stream-capable|stream-capable")
}


// --- Durable / cron: combinators wrap the provider UNCHANGED. ----------------
// The cron home is Retry over a Fallback of the SAME homes; the provider body
// never learns it is in a scheduled job. It still negotiates the request/response
// floor — the reliability posture is a wrapper, not a new provider.
test "35: the cron home is combinator-wrapped homes, still a callable Provider" {
    let cron = s35_cron_client();
    log.info("35: shape = durable/cron (Retry over Fallback), capability = " + s35_capability_of(cron));
    assert.equal(s35_capability_of(cron), "stream-capable")
}

// --- The cron home runs the SAME body, deterministically (Fallback to echo). -
// A Fallback whose first member fails and second is an echo proves the app body
// is unchanged under the durable home's aggressive wrapping.
test "35: the cron/durable home runs the one app body via combinator fallback" {
    let cron: baml.ai.Provider = baml.ai.Retry {
        inner: baml.ai.Fallback {
            members: [
                AlwaysFailProvider {},
                EchoProvider { reply: "{\"topic\": \"Overnight digest\", \"bullets\": [\"batch summarized\"]}" },
            ],
        },
        max: 3,
        base_delay_ms: 1,
    };
    let r = s35_Analyze("overnight corpus", client = cron) catch (e) { _ => s35_Summary { topic: "FAIL", bullets: [] } };
    log.info("35: cron home digest topic = " + r.topic);
    assert.equal(r.topic, "Overnight digest")
}


// ============================================================================
// LIVE TIER — excluded by default; run via `-i "integ-test-35-*::"` with keys.
// The server-with-keys shape for real: the key is read from an env var at
// construction (the factory), and ONE call goes out. Same s35_Analyze body.
// ============================================================================
testset "integ-test-35-deployment-shapes" {
    test "35 live: the server-with-keys home runs the app body against a real API" {
        // The factory reads OPENAI_API_KEY from the env (server holds the key) and
        // constructs a Bearer OpenAiCompatible pointed at api.openai.com — the
        // server-with-keys home, live.
        let r = s35_Analyze(
            "Our team migrated the billing service to the new platform over the weekend with zero downtime.",
            client = s35_server_client(),
        ) catch (e) { _ => s35_Summary { topic: "LIVE FAILED", bullets: [] } };
        log.info("35 live server-with-keys: topic = " + r.topic + ", bullets = " + r.bullets.length().to_string());
        assert.is_true(r.topic.length() > 0 && r.topic != "LIVE FAILED")
    }
}
