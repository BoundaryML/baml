//! End-to-end env-var behavior for the Google LLM providers, driven through
//! real BAML programs calling `$build_request`.
//!
//! All scenarios live in ONE test: they mutate the real Google env vars
//! (process-global), so they must run sequentially — and this file is its own
//! test binary, so no other tests can race them. Every scenario is
//! network-free: errors fire before any credential/token IO, and the success
//! cases use Vertex express-mode (`query_params.key`) which skips token
//! minting.
//!
//! These scenarios stay in Rust because BAML has no `baml.env.set`, and the
//! shared corpus process may carry real ambient GOOGLE_* credentials — every
//! scenario here needs a Google env var either SET or guaranteed ABSENT.

#![allow(unsafe_code)]

use bex_engine::BexExternalValue;

const GOOGLE_ENV_VARS: &[&str] = &[
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GOOGLE_GENAI_USE_ENTERPRISE",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_LOCATION",
    "GOOGLE_CLOUD_QUOTA_PROJECT",
    "GOOGLE_APPLICATION_CREDENTIALS",
];

fn clear_google_env() {
    for var in GOOGLE_ENV_VARS {
        unsafe { std::env::remove_var(var) };
    }
}

async fn run(source: &str) -> Result<BexExternalValue, String> {
    let output = baml_tests::engine::run_test(
        source,
        "main",
        baml_tests::engine::IndexMap::new(),
        baml_tests::engine::OptLevel::One,
    )
    .await;
    output.result.map_err(|e| format!("{e:?}"))
}

#[tokio::test]
async fn google_and_vertex_env_scenarios() {
    // ------------------------------------------------------------------
    // Scenario 1: google-ai with `api_key env.GOOGLE_API_KEY`, var unset.
    // The client constructor's strict env read fails before any request.
    // ------------------------------------------------------------------
    clear_google_env();
    let err = run(r##"
        client<llm> G1 {
            provider google-ai
            options {
                model "gemini-2.0-flash"
                api_key env.GOOGLE_API_KEY
            }
        }
        function F1(input: string) -> string {
            client G1
            prompt `Say hello to ${input}`
        }
        function main() -> string {
            F1$build_request("world").url
        }
    "##)
    .await
    .expect_err("unset env.GOOGLE_API_KEY must fail");
    assert!(
        err.contains("GOOGLE_API_KEY"),
        "must name the missing env var: {err}"
    );

    // google-ai with NO api_key at all: request construction fails fast and
    // points to the Google AI and Vertex provider setup documentation.
    clear_google_env();
    let err = run(r##"
        client<llm> G2 {
            provider google-ai
            options {
                model "gemini-2.0-flash"
            }
        }
        function F2(input: string) -> string {
            client G2
            prompt `Say hello to ${input}`
        }
        function main() -> string {
            F2$build_request("world").url
        }
    "##)
    .await
    .expect_err("google-ai without api_key must fail");
    assert!(
        err.contains("Missing api_key for Google AI"),
        "must fail fast on the missing key: {err}"
    );
    assert!(
        err.contains("`baml describe google-ai`") && err.contains("`baml describe vertex-ai`"),
        "must point at both provider descriptions: {err}"
    );

    // ------------------------------------------------------------------
    // Scenario 2: google-ai with GOOGLE_GENAI_USE_VERTEXAI=true routes
    // through the Vertex backend: aiplatform URL, location/project from
    // GOOGLE_CLOUD_LOCATION / GOOGLE_CLOUD_PROJECT, no api_key needed.
    // Express-mode (`query_params.key`) keeps this token-free.
    // ------------------------------------------------------------------
    clear_google_env();
    unsafe {
        std::env::set_var("GOOGLE_GENAI_USE_VERTEXAI", "true");
        std::env::set_var("GOOGLE_CLOUD_PROJECT", "env-project");
        std::env::set_var("GOOGLE_CLOUD_LOCATION", "us-central1");
    }
    let url = run(r##"
        client<llm> G3 {
            provider google-ai
            options {
                model "gemini-2.0-flash"
                query_params {
                    key "test-express-key"
                }
            }
        }
        function F3(input: string) -> string {
            client G3
            prompt `Say hello to ${input}`
        }
        function main() -> string {
            F3$build_request("world").url
        }
    "##)
    .await
    .expect("flipped google-ai must build a Vertex request");
    assert_eq!(
        url,
        BexExternalValue::String(
            "https://us-central1-aiplatform.googleapis.com/v1/projects/env-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent?key=test-express-key"
                .to_string()
                .into()
        ),
    );

    // ------------------------------------------------------------------
    // Scenario 3: vertex-ai with GOOGLE_CLOUD_PROJECT set but
    // GOOGLE_CLOUD_LOCATION unset (and no options.location): actionable
    // error naming both fixes, before any credential IO.
    // ------------------------------------------------------------------
    clear_google_env();
    unsafe { std::env::set_var("GOOGLE_CLOUD_PROJECT", "env-project") };
    let vertex_source = r##"
        client<llm> V1 {
            provider vertex-ai
            options {
                model "gemini-2.0-flash"
            }
        }
        function F4(input: string) -> string {
            client V1
            prompt `Say hello to ${input}`
        }
        function main() -> string {
            F4$build_request("world").url
        }
    "##;
    let err = run(vertex_source)
        .await
        .expect_err("vertex without location must fail");
    assert!(
        err.contains("Could not resolve location for Vertex AI")
            && err.contains("GOOGLE_CLOUD_LOCATION"),
        "must name options.location and the env var: {err}"
    );

    // ------------------------------------------------------------------
    // Scenario 4: vertex-ai with BOTH GOOGLE_CLOUD_PROJECT and
    // GOOGLE_CLOUD_LOCATION unset: same location-first error — resolution
    // fails actionably before project/credential resolution is attempted.
    // ------------------------------------------------------------------
    clear_google_env();
    let err = run(vertex_source)
        .await
        .expect_err("vertex without location or project must fail");
    assert!(
        err.contains("Could not resolve location for Vertex AI"),
        "location is resolved (and fails) first: {err}"
    );

    // ------------------------------------------------------------------
    // Scenario 5: google-ai with GOOGLE_GENAI_USE_ENTERPRISE=true routes
    // through the Vertex/Enterprise backend. Location is unset, so it
    // defaults to the global endpoint (aiplatform.googleapis.com,
    // locations/global). Express-mode key keeps this token-free.
    // ------------------------------------------------------------------
    clear_google_env();
    unsafe {
        std::env::set_var("GOOGLE_GENAI_USE_ENTERPRISE", "true");
        std::env::set_var("GOOGLE_CLOUD_PROJECT", "env-project");
    }
    let url = run(r##"
        client<llm> G4 {
            provider google-ai
            options {
                model "gemini-2.0-flash"
                query_params {
                    key "test-express-key"
                }
            }
        }
        function F5(input: string) -> string {
            client G4
            prompt `Say hello to ${input}`
        }
        function main() -> string {
            F5$build_request("world").url
        }
    "##)
    .await
    .expect("enterprise env must build a global Vertex request");
    assert_eq!(
        url,
        BexExternalValue::String(
            "https://aiplatform.googleapis.com/v1/projects/env-project/locations/global/publishers/google/models/gemini-2.0-flash:generateContent?key=test-express-key"
                .to_string()
                .into()
        ),
    );

    clear_google_env();
}
