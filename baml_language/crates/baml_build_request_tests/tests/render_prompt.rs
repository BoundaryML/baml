//! Snapshot tests for prompt rendering.

use std::{fs, path::Path};

use baml_build_request_tests::{
    get_first_function_name_from_file, load_baml_file, render_prompt_for_fixture, PromptSnapshot,
    RenderedPromptSnapshot,
};

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
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let snapshot = run_render_prompt_fixture(&fixture).expect("fixture run");

            let snapshot_name = format!("{}_render_prompt", fixture_name.replace('-', "_"));
            insta::assert_yaml_snapshot!(snapshot_name, &snapshot);
        });
    });
}

fn run_render_prompt_fixture(path: &Path) -> anyhow::Result<PromptSnapshot> {
    let contents = fs::read_to_string(path)?;
    let fixture_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();

    let (db, source) = load_baml_file(&contents);

    let func_name = get_first_function_name_from_file(&db, source)
        .ok_or_else(|| anyhow::anyhow!("No function found in fixture"))?;

    let rendered = render_prompt_for_fixture(&contents, &func_name)?;

    Ok(PromptSnapshot {
        fixture: fixture_name,
        function: func_name,
        prompt: RenderedPromptSnapshot::from(&rendered),
    })
}
