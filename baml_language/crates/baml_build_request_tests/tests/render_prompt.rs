//! Snapshot tests for prompt rendering.

use std::{fs, path::Path};

use baml_build_request_tests::derive_function_name;
use baml_db::{Setter, baml_workspace::Project};
use baml_llm_interface::RenderedPrompt;
use baml_program::{BamlMap, BamlProgram, context::DynamicBamlContext};
use baml_project::ProjectDatabase as RootDatabase;
use serde::Serialize;

/// Snapshot of a rendered prompt.
#[derive(Debug, Serialize)]
struct PromptSnapshot {
    fixture: String,
    function: String,
    prompt: RenderedPrompt,
}

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

    // Debug: print the actual rendered output for OutputUnions
    if fixture_name.contains("OutputUnions") {
        eprintln!("=== DEBUG OutputUnions rendered prompt ===");
        eprintln!("{:?}", rendered);
        eprintln!("===========================================");
    }

    Ok(PromptSnapshot {
        fixture: fixture_name.to_string(),
        function: func_name,
        prompt: rendered,
    })
}

/// Load a BAML file and create a database with proper project setup.
fn load_baml_file(content: &str) -> (RootDatabase, baml_db::SourceFile, Project) {
    let mut db = RootDatabase::default();

    // Create the project first
    let project = db.set_project_root(Path::new("/test"));

    // Add the file to the database
    let source = db.add_file("test.baml", content);

    // Wire the file into the project's file list
    project.set_files(&mut db).to(vec![source]);

    (db, source, project)
}

/// Render a prompt for a fixture file using BamlProgram.
fn render_prompt_for_fixture(
    baml_content: &str,
    fixture_name: &str,
) -> anyhow::Result<RenderedPrompt> {
    let (db, _source, project) = load_baml_file(baml_content);

    // Create the runtime
    let runtime = BamlProgram::with_project(db, project);

    // Derive function name from fixture name
    let func_name = derive_function_name(fixture_name);

    // Prepare the function with empty args
    let args = BamlMap::new();
    let prepared = runtime
        .prepare_function(&func_name, args)
        .map_err(|e| anyhow::anyhow!("Failed to prepare function '{}': {}", func_name, e))?;

    // Render the prompt through the runtime
    let dynamic_ctx = DynamicBamlContext::new();
    runtime
        .render_prompt(&prepared, &dynamic_ctx)
        .map_err(|e| anyhow::anyhow!("Failed to render prompt: {}", e))
}
