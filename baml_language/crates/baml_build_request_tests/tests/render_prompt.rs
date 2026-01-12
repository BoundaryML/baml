//! Snapshot tests for prompt rendering.

use std::{fs, path::Path};

use baml_build_request_tests::{
    PromptSnapshot, derive_function_name, render_prompt_for_fixture,
};
use serde::Serialize;

/// Result of attempting to render a fixture.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum FixtureResult {
    Success(PromptSnapshot),
    Error { fixture: String, error: String },
}

#[test]
fn render_prompt_snapshots() {
    let snapshot_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots");

    insta::with_settings!({snapshot_path => snapshot_root}, {
        insta::glob!("../testdata", "*.baml", |relative| {
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(relative);
            let fixture_name = Path::new(relative)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let result = match run_render_prompt_fixture(&fixture, &fixture_name) {
                Ok(snapshot) => FixtureResult::Success(snapshot),
                Err(e) => FixtureResult::Error {
                    fixture: fixture_name.clone(),
                    error: e.to_string(),
                },
            };

            let snapshot_name = format!(
                "{}_render_prompt",
                fixture_name.trim_end_matches(".baml").replace('-', "_")
            );
            insta::assert_yaml_snapshot!(snapshot_name, &result);
        });
    });
}

fn run_render_prompt_fixture(path: &Path, fixture_name: &str) -> anyhow::Result<PromptSnapshot> {
    let contents = fs::read_to_string(path)?;

    let func_name = derive_function_name(fixture_name);
    let rendered = render_prompt_for_fixture(&contents, fixture_name)?;

    Ok(PromptSnapshot {
        fixture: fixture_name.to_string(),
        function: func_name,
        prompt: rendered,
    })
}
