// `env.NAME` as a LATE-BOUND reference, in the two positions that used to
// force an eager read: a client's credential options, and a `client:` selector.
//
// The motivation, in one sentence: `client Foo = ...` is evaluated during
// `$init`, which cannot run io sysops, so an eager env read there died with an
// opaque `InitFailed` — and hosts routinely load their secrets AFTER the
// runtime initializes, which makes an eager snapshot wrong even when it works.
// `env.NAME` now desugars to `baml.env.ref("NAME")`, pure data carrying the
// variable's NAME, and every read happens at request/call time.
//
// Per the corpus convention (../ns_llm_mock/README.md §4.1), every case body is
// a top-level `function` and the `test` block only asserts — test-block locals
// are boxed by the VM and miscompile.
//
// Environment variables used here:
//   * HOME — read for the "a set variable reaches the wire" case. Every corpus
//     host has it (see ../ns_env/, which asserts on PATH/HOME the same way),
//     and nothing here depends on its value beyond round-tripping it.
//   * BAML_TEST_ENV_REF_UNSET* — deliberately never set anywhere. The
//     unset-variable cases below must not depend on a provider key being
//     absent from the developer's shell, so they use a client option with NO
//     env fallback (`openai.GenericClient.base_url`) rather than an api_key.

function EnvRefEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// A dynamic `client:`: the variable is unset, so the `??` fallback selects the
// client — at CALL time, not during `$init`. This is the shape a deployment
// uses to override its model from the environment.
function EnvRefDynamicEcho(question: string) -> string {
    client: env.BAML_TEST_ENV_REF_UNSET_CLIENT.get() ?? "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// The same thing written explicitly, for the case where the selector is built
// by ordinary code rather than by the `client:` sugar.
function EnvRefResolvedEcho(question: string) -> string {
    client: ai.clients.resolve("openai/gpt-4o-mini")
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// ---------------------------------------------------------------------------
// 1. `env.NAME` is data, and the value is read at request time.

function env_ref_is_pure_data() -> string {
    let r = env.HOME;
    `${r.name}|${r.get() == baml.env.get("HOME")}|${env.BAML_TEST_ENV_REF_UNSET.get() == null}`
}

test "env_ref_is_pure_data" {
    assert.equal(env_ref_is_pure_data(), "HOME|true|true")
}

function env_ref_or_falls_back() -> string {
    `${env.BAML_TEST_ENV_REF_UNSET.or("fallback")}|${env.HOME.or("fallback")}`
}

test "env_ref_or_falls_back" {
    assert.equal(env_ref_or_falls_back(), `fallback|${baml.env.get_or_panic("HOME")}`)
}

// The whole point of carrying the NAME: the constructed client never holds the
// secret, so printing / journaling / serializing it cannot leak one.
function env_ref_keeps_the_secret_out_of_the_client() -> string {
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = env.HOME);
    cl.to_string()
}

// Both halves of that claim as one comparable string: the NAME is there, and
// the VALUE is not.
function env_ref_secret_visibility() -> string {
    let rendered = env_ref_keeps_the_secret_out_of_the_client();
    `name=${rendered.includes("HOME")}|value=${rendered.includes(baml.env.get_or_panic("HOME"))}`
}

test "env_ref_keeps_the_secret_out_of_the_client" {
    assert.equal(env_ref_secret_visibility(), "name=true|value=false")
}

// ---------------------------------------------------------------------------
// 2. A client credential given as `env.NAME` reaches the wire.

function env_ref_api_key_reaches_the_wire() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = env.HOME,
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(EnvRefEcho@spec("Say pong."));
            `${result.value}|${mock.last_request().header("authorization") ?? "<absent>"}`
        },
    )
}

test "env_ref_api_key_reaches_the_wire" {
    assert.equal(
        env_ref_api_key_reaches_the_wire(),
        `pong|Bearer ${baml.env.get_or_panic("HOME")}`,
    )
}

// A plain string in the same position still works, positionally, exactly as
// before the union — this is the shape every other corpus namespace uses.
function plain_string_api_key_still_works() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(EnvRefEcho@spec("Say pong."));
            `${result.value}|${mock.last_request().header("authorization") ?? "<absent>"}`
        },
    )
}

test "plain_string_api_key_still_works" {
    assert.equal(plain_string_api_key_still_works(), "pong|Bearer test-key")
}

// `base_url` takes the same union, and its ref resolves at request time too.
//
// BAML has no `env.set`, and the mock's port is ephemeral, so a ref cannot be
// made to *carry* the mock URL. What is asserted instead is the two halves of
// the same claim: constructing with a ref to an UNSET variable is fine, and at
// REQUEST time that ref resolves to "not configured" (null — the client's next
// rung) rather than blowing up; and a literal in the same option reaches the
// wire.
function env_ref_base_url_resolves_at_request_time() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            // construction never reads env: this is safe even though the
            // variable does not exist anywhere ...
            let unset_ref = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = env.BAML_TEST_ENV_REF_UNSET_BASE_URL,
            );
            // ... and the read happens HERE, resolving to null — "not
            // configured", which is what selects the provider default.
            let fell_through = unset_ref.resolved_base_url() == null;
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(EnvRefEcho@spec("Say pong."));
            `${fell_through}|${result.value}|${mock.last_request().path()}`
        },
    )
}

test "env_ref_base_url_resolves_at_request_time" {
    assert.equal(env_ref_base_url_resolves_at_request_time(), "true|pong|/responses")
}

// ---------------------------------------------------------------------------
// 3. An `env.NAME` whose variable is UNSET behaves exactly like "not
//    configured", and the error names the variable that was consulted.
//
//    `openai.GenericClient.base_url` is the deterministic place to assert this:
//    it is required, and unlike every api_key it has no canonical environment
//    variable to fall through to, so the outcome does not depend on which
//    provider keys the developer happens to have exported.

function unset_ref_base_url_is_a_configuration_error() -> string {
    let cl = openai.GenericClient.new(model = "m", base_url = env.BAML_TEST_ENV_REF_UNSET_BASE_URL);
    // A TYPED `ai.errors.InvalidRequest`, not `baml.errors.InvalidArgument`:
    // the class survives `ai.errors.normalize`, which is what lets an
    // `ai.Fallback` skip a misconfigured member instead of aborting.
    cl.resolved_base_url() catch (e) {
        ai.errors.InvalidRequest => e.detail,
    }
}

test "unset_ref_base_url_is_a_configuration_error" {
    assert.contains(
        unset_ref_base_url_is_a_configuration_error(),
        "openai.GenericClient requires base_url",
    );
    // names the variable the caller actually asked for
    assert.contains(
        unset_ref_base_url_is_a_configuration_error(),
        "BAML_TEST_ENV_REF_UNSET_BASE_URL",
    )
}

// The same error, with no ref involved at all: the two must not diverge.
function unset_base_url_is_the_same_configuration_error() -> string {
    let cl = openai.GenericClient.new(model = "m");
    cl.resolved_base_url() catch (e) {
        ai.errors.InvalidRequest => e.detail,
    }
}

test "unset_base_url_is_the_same_configuration_error" {
    assert.contains(
        unset_base_url_is_the_same_configuration_error(),
        "openai.GenericClient requires base_url",
    )
}

// ---------------------------------------------------------------------------
// 4. `ai.wire.resolve_credential` — the union collapse every client routes
//    through, and the WHOLE chain: the explicit credential (a literal, or an
//    `env.NAME` ref read here), then the provider's canonical variable.

function resolve_credential_arms() -> string {
    [
        // literal
        ai.wire.resolve_credential("sk-literal", "HOME") ?? "<null>",
        // a blank literal is "not configured", so the fallback wins
        ai.wire.resolve_credential("   ", "HOME") ?? "<null>",
        // ... and so is a ref to an unset variable
        ai.wire.resolve_credential(env.BAML_TEST_ENV_REF_UNSET, "HOME") ?? "<null>",
        // set ref
        ai.wire.resolve_credential(env.HOME, null) ?? "<null>",
        // unset ref: null, so the caller's own chain continues
        ai.wire.resolve_credential(env.BAML_TEST_ENV_REF_UNSET, null) ?? "<null>",
        // null credential falls back to the named variable
        ai.wire.resolve_credential(null, "HOME") ?? "<null>",
        // ... and to nothing when there is no canonical variable
        ai.wire.resolve_credential(null, null) ?? "<null>",
        ai.wire.resolve_credential(null, "BAML_TEST_ENV_REF_UNSET") ?? "<null>",
    ].join("|")
}

// The expected joined value, built where a local is safe.
function resolve_credential_arms_expected() -> string {
    let home = baml.env.get_or_panic("HOME");
    `sk-literal|${home}|${home}|${home}|<null>|${home}|<null>|<null>`
}

test "resolve_credential_arms" {
    assert.equal(resolve_credential_arms(), resolve_credential_arms_expected())
}

function credential_sources_names_every_variable() -> string {
    [
        ai.wire.credential_sources(env.MY_KEY, "CANONICAL_KEY"),
        ai.wire.credential_sources("sk-literal", "CANONICAL_KEY"),
        ai.wire.credential_sources(null, null),
    ].join(" / ")
}

test "credential_sources_names_every_variable" {
    // the ref's own variable comes first, then the provider's canonical one
    assert.contains(credential_sources_names_every_variable(), "MY_KEY, CANONICAL_KEY");
    assert.contains(credential_sources_names_every_variable(), "CANONICAL_KEY");
    assert.contains(
        credential_sources_names_every_variable(),
        "(no environment variable was named)",
    )
}

// Some providers authenticate through the URL rather than a header — Vertex AI
// express mode signs with `?key=…` — and the native HTTP layer renders
// `reqwest::Error`, whose text can embed the request URL. So the transport
// failure a caller sees must not carry the key. `ai.wire.redact_url_secrets` is
// what every such message goes through (`ai.wire.send_as`, `_gm_sse_failure`).
function redact_url_secrets_scrubs_credentials() -> string {
    [
        ai.wire.redact_url_secrets(
            "error sending request for url (https://x/v1/m:generateContent?alt=sse&key=SEKRIT): refused",
        ),
        ai.wire.redact_url_secrets("?api_key=abc&access_token=def&x=1"),
        // nothing credential-shaped is left alone
        ai.wire.redact_url_secrets("connection closed before message completed"),
    ].join(" / ")
}

test "redact_url_secrets_scrubs_credentials" {
    assert.equal(
        redact_url_secrets_scrubs_credentials(),
        "error sending request for url (https://x/v1/m:generateContent?alt=sse&key=REDACTED): refused"
            + " / ?api_key=REDACTED&access_token=REDACTED&x=1"
            + " / connection closed before message completed",
    )
}

// A BLANK credential is "not configured" at every rung, not just the literal
// one: `export KEY=` must reach the client's own "no credential configured"
// error rather than putting `Authorization: Bearer ` on the wire. The blank
// literal arm above covers the explicit spelling; this covers the ref, whose
// variable is never set anywhere in the corpus.
function blank_credential_is_unconfigured() -> string {
    [
        `literal=${ai.wire.resolve_credential("", null) ?? "<null>"}`,
        `spaces=${ai.wire.resolve_credential("  \t ", null) ?? "<null>"}`,
        `ref=${ai.wire.resolve_credential(env.BAML_TEST_ENV_REF_UNSET, null) ?? "<null>"}`,
    ].join("|")
}

test "blank_credential_is_unconfigured" {
    assert.equal(blank_credential_is_unconfigured(), "literal=<null>|spaces=<null>|ref=<null>")
}

// The same two-rung chain, asserted once per CLIENT FAMILY: an explicit
// credential — a literal, or an `env.NAME` ref to a SET variable — wins over
// the provider's canonical variable, whether or not that variable happens to be
// exported in the shell running the corpus. HOME stands in for the set variable
// (see the header note); every client below reads it through the same
// `ai.wire.resolve_credential` chain, so a family that grew its own resolution
// order would show up here.
function explicit_credential_wins_per_client_family() -> string {
    [
        openai.ResponsesClient.new(model = "m", api_key = "sk-explicit").resolved_api_key(),
        openai.ChatClient.new(model = "m", api_key = env.HOME).resolved_api_key() ?? "<null>",
        openai.ImageClient.new(model = "m", api_key = "sk-explicit").resolved_api_key(),
        openai.AzureClient.new(model = "m", api_key = "sk-explicit").resolved_api_key() ?? "<null>",
        openai.OpenRouterClient.new(model = "m", api_key = "sk-explicit").resolved_api_key() ?? "<null>",
        openai.OllamaClient.new(model = "m", api_key = "sk-explicit").resolved_api_key() ?? "<null>",
        openai.GenericClient.new(model = "m", api_key = "sk-explicit").resolved_api_key() ?? "<null>",
        anthropic.Client.new(model = "m", api_key = "sk-explicit").resolved_api_key(),
        google.GeminiClient.new(model = "m", api_key = "sk-explicit").resolved_api_key(),
        google.VertexClient.new(model = "m", api_key = "sk-explicit").resolved_api_key() ?? "<null>",
        vercel.AiGatewayImageClient.new(model = "m", api_key = "sk-explicit").resolved_api_key(),
    ].join("|")
}

// One entry per client in the list above, in the same order.
function explicit_credential_expected() -> string {
    let home = baml.env.get_or_panic("HOME");
    let literal = "sk-explicit";
    [literal, home, literal, literal, literal, literal, literal, literal, literal, literal, literal]
        .join("|")
}

test "explicit_credential_wins_per_client_family" {
    assert.equal(explicit_credential_wins_per_client_family(), explicit_credential_expected())
}

// ---------------------------------------------------------------------------
// 5. `ai.clients.resolve` — the runtime twin of the compiler's literal
//    `client "provider/model"` lowering.

function resolve_shorthand_ids() -> string {
    [
        ai.clients.resolve("anthropic/claude-haiku-4-5").id(),
        ai.clients.resolve("openai/gpt-4o-mini").id(),
        ai.clients.resolve("google/gemini-2.5-flash").id(),
        ai.clients.resolve("openrouter/z-ai/glm-4.6").id(),
    ].join("|")
}

test "resolve_shorthand_ids" {
    assert.equal(
        resolve_shorthand_ids(),
        "anthropic/claude-haiku-4-5|openai/gpt-4o-mini|google/gemini-2.5-flash|openrouter/z-ai/glm-4.6",
    )
}

// A `root.Client` value passes straight through — resolve is the identity on
// anything already built, including a wrapper client.
function resolve_of_a_client_value_is_identity() -> string {
    let inner = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key");
    let wrapped = ai.clients.Retry.new(inner = inner, max_attempts = 2);
    `${ai.clients.resolve(inner).id()}|${ai.clients.resolve(wrapped).id()}`
}

test "resolve_of_a_client_value_is_identity" {
    assert.equal(resolve_of_a_client_value_is_identity(), "openai/gpt-4o-mini|openai/gpt-4o-mini")
}

// An unknown prefix is a typed `ai.errors.Failure`, not a panic, and it lists
// what it would have accepted.
function resolve_unknown_prefix_is_typed() -> string {
    ai.clients.resolve("acme-inc/super-model").id() catch (e) {
        ai.errors.InvalidRequest => `${e.provider}|${e.detail}`,
    }
}

test "resolve_unknown_prefix_is_typed" {
    assert.contains(resolve_unknown_prefix_is_typed(), "ai.clients|");
    assert.contains(resolve_unknown_prefix_is_typed(), `no builtin provider for prefix "acme-inc"`);
    // every prefix the compile-time map knows, so the two stay visibly in sync
    assert.contains(
        resolve_unknown_prefix_is_typed(),
        "openai, openai-chat, openai-images, azure, ollama, openrouter",
    );
    assert.contains(
        resolve_unknown_prefix_is_typed(),
        "anthropic, google, vertex, bedrock, ai-gateway-images, claude-code",
    );
    assert.contains(resolve_unknown_prefix_is_typed(), "openai.GenericClient")
}

function resolve_without_a_slash_is_typed() -> string {
    ai.clients.resolve("gpt-4o-mini").id() catch (e) {
        ai.errors.InvalidRequest => e.detail,
    }
}

test "resolve_without_a_slash_is_typed" {
    assert.contains(resolve_without_a_slash_is_typed(), `is not a "provider/model" string`)
}

// A ref in a selector position is read at CALL time, and an unset one is a
// typed, CATCHABLE failure naming the variable — not a panic that escapes the
// caller's catch_all.
function resolve_of_a_ref_reads_at_call_time() -> string {
    let selector: ai.Selector = env.BAML_TEST_ENV_REF_UNSET_CLIENT;
    ai.clients.resolve(selector).id() catch (e) {
        ai.errors.InvalidRequest => e.detail,
    }
}

test "resolve_of_a_ref_reads_at_call_time" {
    assert.contains(
        resolve_of_a_ref_reads_at_call_time(),
        "client selector env.BAML_TEST_ENV_REF_UNSET_CLIENT is not set",
    );
    assert.contains(resolve_of_a_ref_reads_at_call_time(), "anthropic, google, vertex, bedrock")
}

// ---------------------------------------------------------------------------
// 6. A dynamic `client:` selector actually runs. Both LLM functions below pick
//    their client at call time; the mock proves the selection is a real client
//    and not a string that merely type-checked.

function dynamic_client_selector_runs_against_the_mock() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            // The spec's `client:` selected openai/gpt-4o-mini through the
            // `??` fallback; the run is pointed at the mock explicitly, since
            // the ephemeral port cannot appear in a declaration.
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let spec = EnvRefDynamicEcho@spec("Say pong.");
            let result = ai.Agent.new(client = cl).run(spec);
            `${spec.default_client.id()}|${result.value}`
        },
    )
}

test "dynamic_client_selector_runs_against_the_mock" {
    assert.equal(dynamic_client_selector_runs_against_the_mock(), "openai/gpt-4o-mini|pong")
}

function explicit_resolve_selector_runs_against_the_mock() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let spec = EnvRefResolvedEcho@spec("Say pong.");
            let result = ai.Agent.new(client = cl).run(spec);
            `${spec.default_client.id()}|${result.value}`
        },
    )
}

test "explicit_resolve_selector_runs_against_the_mock" {
    assert.equal(explicit_resolve_selector_runs_against_the_mock(), "openai/gpt-4o-mini|pong")
}

// ---------------------------------------------------------------------------
// 7. LIVE — real endpoints, gated exactly like ../ns_llm_live_smoke:
//    the `offline` profile prunes the `::live::` subtree before its body runs,
//    and each leaf registers only when its key is present. Run with
//
//      cd /Users/aaron/projects/baml
//      infisical run -- baml_language/target/debug/baml-cli test \
//          --from baml_language/crates/baml_tests/baml_src --profile live \
//          -i "::live::env_ref"

// A client whose credential is a late-bound `env.OPENAI_API_KEY` ref — the
// declaration shape that used to be impossible, since reading env during
// `$init` is not allowed.
client LiveEnvRefOpenAi = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = env.OPENAI_API_KEY,
);

function LiveEnvRefEcho(question: string) -> string {
    client: LiveEnvRefOpenAi
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// Call-time client selection: the variable is unset, so `??` picks the default,
// and the pick happens on the call rather than at $init.
function LiveEnvRefFallbackEcho(question: string) -> string {
    client: env.BAML_TEST_CLIENT_UNSET.get() ?? "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// Deliberately weak: these prove the credential/selection path reaches a real
// endpoint, not what the model says.
function live_env_ref_client_answers() -> bool {
    LiveEnvRefEcho("Say pong.").trim().length() > 0
}

function live_env_ref_fallback_selection_answers() -> bool {
    LiveEnvRefFallbackEcho("Say pong.").trim().length() > 0
}

testset "live" with testing.Sequential() {
    // Always registered: keeps `--profile live` from selecting nothing here,
    // which exits 5 (crates/baml_cli/src/lib.rs:93-94).
    test "env_ref_preconditions" {
        assert.is_true(true)
    }

    if (baml.env.get("OPENAI_API_KEY") != null) {
        test "env_ref_declared_client" { assert.is_true(live_env_ref_client_answers()) }

        test "env_ref_fallback_selection" {
            assert.is_true(live_env_ref_fallback_selection_answers())
        }
    } else {
        log.warn("live::env_ref not registered: OPENAI_API_KEY is unset")
    }
}
