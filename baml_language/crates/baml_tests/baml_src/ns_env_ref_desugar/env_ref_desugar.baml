// The explicit `baml.env.ref("X")` helper constructs a late-bound reference.
// The `env.X` syntax is intentionally different: it is a strict eager lookup
// and is pinned independently by the env tests.
//
// The `Ref` carries the variable NAME only — a secret is never captured into a
// constructed value. Reads happen at use time via `.get()` / `.get_or_panic()`
// / `.or(fallback)`.
//
// Stdlib side: baml_std/baml/ns_env/env.baml.
//
// Corpus convention: every case body is a top-level `function`, `test` blocks
// only assert (test-block locals are boxed by the VM).

// ─── the reference is pure data ──────────────────────────────────────────────

// The declared return type pins the explicit helper's static type.
function EnvRefOf() -> baml.env.Ref {
    let r = baml.env.ref("BAML_TEST_ENV_REF_SOME_VAR");
    r
}

// The reference carries the NAME, never a value — reading it is io-free, so
// this works whether or not the variable is set.
function EnvRefName() -> string {
    let r = baml.env.ref("BAML_TEST_ENV_REF_SOME_VAR");
    r.name
}

// ─── reading through the reference ───────────────────────────────────────────

// `.get()` on an unset variable is null (no panic).
function EnvRefUnsetGet() -> string? {
    let r = baml.env.ref("BAML_TEST_ENV_REF_UNSET");
    r.get()
}

// `.or(fallback)` yields the fallback for an unset variable.
function EnvRefUnsetOr() -> string {
    let r = baml.env.ref("BAML_TEST_ENV_REF_UNSET");
    r.or("fallback-value")
}

// The `??` chain over `.get()` — the shape hosts write by hand.
function EnvRefGetChain() -> string {
    baml.env.ref("BAML_TEST_ENV_REF_UNSET").get() ?? "chained-fallback"
}

// A reference built by name can use the same fallback surface.
function EnvRefExplicit() -> string {
    let r = baml.env.ref("BAML_TEST_ENV_REF_UNSET");
    r.or("explicit-fallback")
}

// ─── the `$init` repro: a client declaration reading env ─────────────────────

// This declaration pins that the explicit helper stays pure during `$init`.
client EnvRefDesugarClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = baml.env.ref("BAML_TEST_ENV_REF_UNSET"),
    base_url = "http://localhost:1234",
);

// Reaching the client value at all proves `$init` completed. (Member access
// straight off a top-level `client`/`let` binding does not resolve today —
// `EnvRefDesugarClient.id()` reports `unresolved name: id` — so bind it to an
// `ai.Client` local first and dispatch through the interface.)
function EnvRefClientId() -> string {
    let c: ai.Client = EnvRefDesugarClient;
    c.id()
}

// An LLM function declared against the env-reading client. Never called — the
// point is that declaring it compiles and initializes.
function EnvRefDesugarFn() -> string {
    client: EnvRefDesugarClient
    prompt: `hi`
}

// The client expression is resolved when the FUNCTION is called (at `$spec`
// construction), not at `$init` — reading it back exercises that path.
function EnvRefDesugarFnClientId() -> string {
    EnvRefDesugarFn$spec().default_client.id()
}

// ─── dynamic client selection ────────────────────────────────────────────────

// A runtime `"provider/model"` string in `client:` position: not a literal, so
// it takes the `ai.clients.resolve` path rather than the compile-time map.
function EnvRefDynSelector() -> string {
    "openai/gpt-4o-mini"
}

function EnvRefDynClientFn() -> string {
    client: EnvRefDynSelector()
    prompt: `hi`
}

function EnvRefDynClientId() -> string {
    EnvRefDynClientFn$spec().default_client.id()
}

// ─── evaluation timing: call time, not `$init` ───────────────────────────────

// A selector with a prefix `ai.clients.resolve` does not know. Resolution
// throws — which makes this file a decisive timing probe.
function EnvRefBadSelector() -> string {
    "no-such-provider/some-model"
}

function EnvRefBadClientFn() -> string {
    client: EnvRefBadSelector()
    prompt: `hi`
}

// THE TIMING PROOF. The client expression lives in the synthesized `<Fn>$spec`
// companion body, so it runs when the function is CALLED, not during `$init`.
// If it ran at `$init`, the throw above would abort this namespace's
// initialization and take every other test in this file down with it. Instead
// nothing happens until `$spec()` is invoked here.
function EnvRefBadClientThrowsAtCallTime() -> string {
    {
        EnvRefBadClientFn$spec();
        "resolved-unexpectedly"
    } catch (e) {
        ai.errors.InvalidRequest => "threw-at-call-time"
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

test "env_ref_sugar_is_a_ref" {
    assert.equal(EnvRefName(), "BAML_TEST_ENV_REF_SOME_VAR")
}

test "env_ref_static_type_is_ref" {
    assert.equal(EnvRefOf().name, "BAML_TEST_ENV_REF_SOME_VAR")
}

test "env_ref_unset_get_is_null" {
    assert.equal(EnvRefUnsetGet(), null)
}

test "env_ref_unset_or_uses_fallback" {
    assert.equal(EnvRefUnsetOr(), "fallback-value")
}

test "env_ref_get_chain_uses_fallback" {
    assert.equal(EnvRefGetChain(), "chained-fallback")
}

test "env_ref_explicit_ref_matches_sugar" {
    assert.equal(EnvRefExplicit(), "explicit-fallback")
}

test "env_ref_client_declaration_initializes" {
    assert.equal(EnvRefClientId(), "openai/gpt-4o-mini")
}

test "env_ref_client_declaration_reaches_spec" {
    assert.equal(EnvRefDesugarFnClientId(), "openai/gpt-4o-mini")
}

test "env_ref_dynamic_string_selector_resolves" {
    assert.equal(EnvRefDynClientId(), "openai/gpt-4o-mini")
}

// This test passing at all — alongside the eight above, in the same namespace
// as a client expression that throws — is the evidence that client selection
// happens at call time and not during `$init`.
test "env_ref_client_expr_evaluates_at_call_time" {
    assert.equal(EnvRefBadClientThrowsAtCallTime(), "threw-at-call-time")
}
