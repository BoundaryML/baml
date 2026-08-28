// `env.X` desugars to a LATE-BOUND reference: `baml.env.ref("X")`.
//
// It used to desugar to `baml.env.get_or_panic("X")` — an eager read. That was
// wrong in two ways:
//
//   1. `client Foo = ...` declarations are evaluated during `$init`, which
//      cannot perform io. An eager env read there died at runtime with an
//      opaque `InitFailed`. The `client EnvRefDesugarClient = ...` declaration
//      below is that exact repro; today it is a passing test.
//   2. Hosts routinely load secrets *after* the runtime initializes, so a
//      snapshot taken at declaration time is stale.
//
// The `Ref` carries the variable NAME only — a secret is never captured into a
// constructed value. Reads happen at use time via `.get()` / `.get_or_panic()`
// / `.or(fallback)`.
//
// Compiler side: crates/baml_compiler2_ast/src/lower_expr_body.rs
// (`lower_env_access_expr`). Stdlib side: baml_std/baml/ns_env/env.baml.
//
// Corpus convention: every case body is a top-level `function`, `test` blocks
// only assert (test-block locals are boxed by the VM).

// ─── the reference is pure data ──────────────────────────────────────────────

// The declared return type is the static-type assertion: `env.X` must be a
// `baml.env.Ref`, not a string. If the desugar regressed to `get_or_panic`,
// this would fail to compile with a string-vs-Ref mismatch.
function EnvRefOf() -> baml.env.Ref {
    let r = env.BAML_TEST_ENV_REF_SOME_VAR;
    r
}

// The reference carries the NAME, never a value — reading it is io-free, so
// this works whether or not the variable is set.
function EnvRefName() -> string {
    let r = env.BAML_TEST_ENV_REF_SOME_VAR;
    r.name
}

// ─── reading through the reference ───────────────────────────────────────────

// `.get()` on an unset variable is null (no panic) — the difference from the
// old eager desugar, which threw `baml.panics.UserPanic` on the `env.X` itself.
function EnvRefUnsetGet() -> string? {
    let r = env.BAML_TEST_ENV_REF_UNSET;
    r.get()
}

// `.or(fallback)` yields the fallback for an unset variable.
function EnvRefUnsetOr() -> string {
    let r = env.BAML_TEST_ENV_REF_UNSET;
    r.or("fallback-value")
}

// The `??` chain over `.get()` — the shape hosts write by hand.
function EnvRefGetChain() -> string {
    env.BAML_TEST_ENV_REF_UNSET.get() ?? "chained-fallback"
}

// A reference built by name is the same thing the sugar produces.
function EnvRefExplicit() -> string {
    let r = baml.env.ref("BAML_TEST_ENV_REF_UNSET");
    r.or("explicit-fallback")
}

// ─── the `$init` repro: a client declaration reading env ─────────────────────

// THIS DECLARATION IS THE TEST. It is evaluated during `$init`, which cannot
// do io. With the old eager desugar the `env.X` read here aborted startup with
// `InitFailed`; with the late-bound `Ref` the field just holds the name, and
// the credential is read when a request is built.
client EnvRefDesugarClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = env.BAML_TEST_ENV_REF_UNSET,
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

// The client expression is resolved when the function's `@spec` projection
// construction), not at `$init` — reading it back exercises that path.
function EnvRefDesugarFnClientId() -> string {
    EnvRefDesugarFn@spec().default_client.id()
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
    EnvRefDynClientFn@spec().default_client.id()
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

// THE TIMING PROOF. The client expression lives in the attached `Fn@spec`
// companion body, so it runs when the function is CALLED, not during `$init`.
// If it ran at `$init`, the throw above would abort this namespace's
// initialization and take every other test in this file down with it. Instead
// nothing happens until `@spec()` is invoked here.
function EnvRefBadClientThrowsAtCallTime() -> string {
    {
        EnvRefBadClientFn@spec();
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
