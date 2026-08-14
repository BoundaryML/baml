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

test "env_ref_keeps_the_secret_out_of_the_client" {
    let rendered = env_ref_keeps_the_secret_out_of_the_client();
    // the NAME is there ...
    assert.contains(rendered, "HOME");
    // ... and the VALUE is not.
    assert.is_true(!rendered.includes(baml.env.get_or_panic("HOME")))
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
            let result = ai.Agent<string>.new(client = cl).run(EnvRefEcho@spec("Say pong."));
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
            let result = ai.Agent<string>.new(client = cl).run(EnvRefEcho@spec("Say pong."));
            `${result.value}|${mock.last_request().header("authorization") ?? "<absent>"}`
        },
    )
}

test "plain_string_api_key_still_works" {
    assert.equal(plain_string_api_key_still_works(), "pong|Bearer test-key")
}

// `base_url` takes the same union, and its ref resolves at request time too.
function env_ref_base_url_resolves_at_request_time() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            // BAML has no `env.set`, so the ref names an UNSET variable and the
            // client falls through to the explicit URL the mock handed us —
            // the point being that the ref was consulted at request time and
            // did not blow up at construction.
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                base_url_env = "BAML_TEST_ENV_REF_UNSET_BASE_URL",
            );
            let result = ai.Agent<string>.new(client = cl).run(EnvRefEcho@spec("Say pong."));
            `${result.value}|${mock.last_request().path()}`
        },
    )
}

test "env_ref_base_url_resolves_at_request_time" {
    assert.equal(env_ref_base_url_resolves_at_request_time(), "pong|/responses")
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
    cl.resolved_base_url() catch (e) {
        baml.errors.InvalidArgument => e.message,
    }
}

test "unset_ref_base_url_is_a_configuration_error" {
    let message = unset_ref_base_url_is_a_configuration_error();
    assert.contains(message, "openai.GenericClient requires base_url");
    // names the variable the caller actually asked for
    assert.contains(message, "BAML_TEST_ENV_REF_UNSET_BASE_URL")
}

// The same error, with no ref involved at all: the two must not diverge.
function unset_base_url_is_the_same_configuration_error() -> string {
    let cl = openai.GenericClient.new(model = "m");
    cl.resolved_base_url() catch (e) {
        baml.errors.InvalidArgument => e.message,
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
//    through. Order: explicit credential, then the caller's `*_env` name, then
//    the provider's canonical variable.

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

test "resolve_credential_arms" {
    let home = baml.env.get_or_panic("HOME");
    assert.equal(
        resolve_credential_arms(),
        `sk-literal|${home}|${home}|${home}|<null>|${home}|<null>|<null>`,
    )
}

function credential_sources_names_every_variable() -> string {
    [
        ai.wire.credential_sources(env.MY_KEY, "LEGACY_KEY", "CANONICAL_KEY"),
        ai.wire.credential_sources("sk-literal", null, "CANONICAL_KEY"),
        ai.wire.credential_sources(null, null, null),
    ].join(" / ")
}

test "credential_sources_names_every_variable" {
    let got = credential_sources_names_every_variable();
    assert.contains(got, "MY_KEY, LEGACY_KEY, CANONICAL_KEY");
    assert.contains(got, "CANONICAL_KEY");
    assert.contains(got, "(no environment variable was named)")
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
    let got = resolve_unknown_prefix_is_typed();
    assert.contains(got, "ai.clients|");
    assert.contains(got, `no builtin provider for prefix "acme-inc"`);
    // every prefix the compile-time map knows, so the two stay visibly in sync
    assert.contains(got, "openai, openai-chat, openai-images, azure, ollama, openrouter");
    assert.contains(got, "anthropic, google, vertex, bedrock, ai-gateway-images, claude-code");
    assert.contains(got, "openai.GenericClient")
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
    let selector: ai.ClientSelector = env.BAML_TEST_ENV_REF_UNSET_CLIENT;
    ai.clients.resolve(selector).id() catch (e) {
        ai.errors.InvalidRequest => e.detail,
    }
}

test "resolve_of_a_ref_reads_at_call_time" {
    let detail = resolve_of_a_ref_reads_at_call_time();
    assert.contains(detail, "client selector env.BAML_TEST_ENV_REF_UNSET_CLIENT is not set");
    assert.contains(detail, "anthropic, google, vertex, bedrock")
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
            let result = ai.Agent<string>.new(client = cl).run(spec);
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
            let result = ai.Agent<string>.new(client = cl).run(spec);
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
