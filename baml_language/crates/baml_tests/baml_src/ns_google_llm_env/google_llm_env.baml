// Tests for Google LLM provider env-var behavior, driven through
// `$build_request`.
//
// This file covers only the env-var scenario that reads NO environment
// variables: `options.enterprise true` with explicit `project_id`, `location`,
// and express-mode `query_params.key`. This configuration is deterministic
// regardless of ambient GOOGLE_* vars: explicit options win over env flags
// (sys_llm/src/build_request/mod.rs), explicit location/project leave no URL
// placeholders for auth-time env resolution, and `query_params.key` bypasses
// token-based auth (sys_llm/src/auth_request/vertex.rs). Safe to run in the
// shared corpus process.
//
// Other scenarios require either guaranteed-absent or guaranteed-set Google
// env vars (GOOGLE_API_KEY, GOOGLE_GENAI_USE_VERTEXAI, GOOGLE_CLOUD_PROJECT,
// GOOGLE_CLOUD_LOCATION, GOOGLE_GENAI_USE_ENTERPRISE). These cannot be
// expressed in BAML (no `baml.env.set` primitive) and cannot safely run in
// the shared corpus process (ambient GOOGLE_* credentials would affect outcomes).
// They remain in crates/baml_tests/tests/google_llm_env.rs.
//
// The `$build_request(...)` call is kept inside a top-level helper function
// (test-block local-boxing VM bug; calls are not dead-code-eliminated).

client<llm> GoogleEnvEnterpriseOptionsClient {
    provider google-ai
    options {
        model "gemini-2.0-flash"
        enterprise true
        project_id "options-project"
        location "us-west1"
        query_params {
            key "test-express-key"
        }
    }
}

function GoogleEnvEnterprisePrompt(input: string) -> string {
    client GoogleEnvEnterpriseOptionsClient
    prompt #"Say hello to ${input}"#
}

function google_env_enterprise_options_url() -> string {
    GoogleEnvEnterprisePrompt$build_request("world").url
}

// google-ai with `options.enterprise true` (no env flag) routes through
// Vertex Enterprise. The explicit location/project fields verify the shared
// runtime path with VertexAiOptions.
test "google_and_vertex_env_scenarios__options_enterprise_routes_vertex" {
    assert.equal(
        google_env_enterprise_options_url(),
        "https://us-west1-aiplatform.googleapis.com/v1/projects/options-project/locations/us-west1/publishers/google/models/gemini-2.0-flash:generateContent?key=test-express-key"
    )
}
