// Live provider smoke tests — the opt-in `::live::` group.
//
// These are the ONLY tests in this corpus that touch the network. They are
// excluded by the `offline` profile that a bare `baml-cli test` (and CI) runs,
// and selected only by `--profile live`; see ../baml.toml. Run them with:
//
//   cd /Users/aaron/projects/baml
//   infisical run -- baml_language/target/debug/baml-cli test \
//       --from baml_language/crates/baml_tests/baml_src --profile live
//
// Two independent gates, both deliberate:
//   1. the profile — `-x "::live::"` prunes this testset subtree *before* its
//      body runs (baml_std/testing/registry.baml:644-659), so an offline run
//      never reads env and never constructs a request;
//   2. registration — each leaf is registered only when its provider's key is
//      present, so `--profile live` without a key silently runs fewer tests
//      instead of failing. There is no "skipped" outcome in this runner
//      (Outcome = pass | fail | error), hence the `log.warn` (visible with
//      `--logs warn`).
//
// `with testing.Sequential()` matters: the default testset runner spawns every
// child at once (registry.baml:483-492), which fires all providers concurrently
// and trips rate limits.
//
// The always-registered `preconditions` leaf keeps a keyless `--profile live`
// from selecting nothing, which exits 5 (crates/baml_cli/src/lib.rs:93-94).
//
// Model ids are live-verified as of 2026-08-12; `claude-3-5-haiku-latest` and
// `gemini-2.0-flash` both 404 now. Keep them current when this file is touched.
//
// Per the corpus convention, every case body is a top-level `function` and the
// `test` block only asserts (test-block local-boxing VM bug).

function LiveSmokeOpenAi(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

function LiveSmokeAnthropic(question: string) -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

function LiveSmokeGoogle(question: string) -> string {
    client: "google/gemini-2.5-flash"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// Deliberately weak assertions: these prove the wire path (auth, request shape,
// response parse) works against the real endpoint, not what the model says.
function live_openai_answers() -> bool {
    LiveSmokeOpenAi("Say pong.").trim().length() > 0
}

function live_anthropic_answers() -> bool {
    LiveSmokeAnthropic("Say pong.").trim().length() > 0
}

function live_google_answers() -> bool {
    LiveSmokeGoogle("Say pong.").trim().length() > 0
}

testset "live" with testing.Sequential() {
    // Always registered: guarantees a non-empty selection for `--profile live`.
    test "preconditions" {
        assert.is_true(true)
    }

    if (baml.env.get("OPENAI_API_KEY") != null) {
        test "openai" { assert.is_true(live_openai_answers()) }
    } else {
        log.warn("live::openai not registered: OPENAI_API_KEY is unset")
    }

    if (baml.env.get("ANTHROPIC_API_KEY") != null) {
        test "anthropic" { assert.is_true(live_anthropic_answers()) }
    } else {
        log.warn("live::anthropic not registered: ANTHROPIC_API_KEY is unset")
    }

    if (baml.env.get("GOOGLE_API_KEY") != null) {
        test "google" { assert.is_true(live_google_answers()) }
    } else {
        log.warn("live::google not registered: GOOGLE_API_KEY is unset")
    }
}
