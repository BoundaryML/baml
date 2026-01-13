//! Snapshot tests for prompt rendering using the engine's implementation.
//!
//! These tests use the old render_prompt implementation from engine/baml-runtime
//! for comparison with the new implementation in baml_language.

use std::{collections::HashMap, fs, path::Path};

use baml_build_request_tests::{derive_function_name, derive_test_name};
use baml_ids::FunctionCallId;
use baml_runtime::{BamlRuntime, InternalRuntimeInterface};
use baml_types::BamlValue;
use internal_baml_core::feature_flags::FeatureFlags;
use serde::Serialize;

/// Snapshot of a rendered prompt from the engine.
#[derive(Debug, Serialize)]
struct EnginePromptSnapshot {
    fixture: String,
    function: String,
    prompt: internal_baml_jinja::RenderedPrompt,
}

/// Result of attempting to render a fixture using the engine.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum FixtureResult {
    Success(EnginePromptSnapshot),
    Error { fixture: String, error: String },
}

#[test]
fn render_prompt_engine_snapshots() {
    let snapshot_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots_engine");

    insta::with_settings!({snapshot_path => snapshot_root}, {
        insta::glob!("../testdata", "*.baml", |relative| {
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(relative);
            let fixture_name = Path::new(relative)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let result = match run_render_prompt_engine_fixture(&fixture, &fixture_name) {
                Ok(snapshot) => FixtureResult::Success(snapshot),
                Err(e) => FixtureResult::Error {
                    fixture: fixture_name.clone(),
                    error: e.to_string(),
                },
            };

            let snapshot_name = format!(
                "{}_render_prompt_engine",
                fixture_name.trim_end_matches(".baml").replace('-', "_")
            );
            insta::assert_json_snapshot!(snapshot_name, &result);
        });
    });
}

fn run_render_prompt_engine_fixture(path: &Path, fixture_name: &str) -> anyhow::Result<EnginePromptSnapshot> {
    let contents = fs::read_to_string(path)?;

    let func_name = derive_function_name(fixture_name);

    // Use tokio runtime to run the async function
    let rt = tokio::runtime::Runtime::new()?;
    let rendered = rt.block_on(render_prompt_for_fixture_engine(&contents, fixture_name))?;

    Ok(EnginePromptSnapshot {
        fixture: fixture_name.to_string(),
        function: func_name,
        prompt: rendered,
    })
}

/// Render a prompt for a fixture file using the engine's BamlRuntime.
async fn render_prompt_for_fixture_engine(
    baml_content: &str,
    fixture_name: &str,
) -> anyhow::Result<internal_baml_jinja::RenderedPrompt> {
    // Create the runtime from file content
    let files: HashMap<&str, &str> = [("test.baml", baml_content)].into_iter().collect();

    // Provide dummy env vars for API keys so prompts can render
    let env_vars: HashMap<String, String> = [
        ("OPENAI_API_KEY", "test-openai-key"),
        ("ANTHROPIC_API_KEY", "test-anthropic-key"),
        ("GOOGLE_API_KEY", "test-google-key"),
        ("AWS_ACCESS_KEY_ID", "test-aws-key"),
        ("AWS_SECRET_ACCESS_KEY", "test-aws-secret"),
        ("AWS_REGION", "us-east-1"),
        ("AZURE_OPENAI_API_KEY", "test-azure-key"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();

    let runtime = BamlRuntime::from_file_content(".", &files, env_vars.clone(), FeatureFlags::new())
        .map_err(|e| anyhow::anyhow!("Failed to create runtime: {}", e))?;

    // Create context
    let ctx_manager = runtime.create_ctx_manager(BamlValue::Null, None);
    let ctx = ctx_manager
        .create_ctx(None, None, env_vars, vec![FunctionCallId::new()])
        .map_err(|e| anyhow::anyhow!("Failed to create context: {}", e))?;

    // Derive function and test names from fixture name
    let func_name = derive_function_name(fixture_name);
    let test_name = derive_test_name(fixture_name);

    // Get test params using the engine's get_test_params
    let params = runtime
        .get_test_params(&func_name, &test_name, &ctx, false)
        .map_err(|e| anyhow::anyhow!("Failed to get test params: {}", e))?;

    // Render the prompt using the engine's implementation
    let (prompt, _scope, _metadata) = runtime
        .render_prompt(&func_name, &ctx, &params, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to render prompt: {}", e))?;

    Ok(prompt)
}
