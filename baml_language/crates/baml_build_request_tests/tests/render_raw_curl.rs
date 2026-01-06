//! Snapshot tests for raw curl command rendering.
//!
//! TODO: Implement once render_raw_curl is available in baml_jinja_runtime.

use std::path::Path;

#[test]
#[ignore = "render_raw_curl not yet implemented"]
fn render_raw_curl_snapshots() {
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

            let _snapshot = run_render_raw_curl_fixture(&fixture).expect("fixture run");

            let snapshot_name = format!("{}_render_raw_curl", fixture_name.replace('-', "_"));
            // insta::assert_yaml_snapshot!(snapshot_name, &snapshot);
            let _ = snapshot_name;
        });
    });
}

fn run_render_raw_curl_fixture(_path: &Path) -> anyhow::Result<()> {
    // TODO: Implement once render_raw_curl is available
    // 1. Load BAML file
    // 2. Parse and extract function + client config
    // 3. Generate curl command
    // 4. Return serializable snapshot
    anyhow::bail!("render_raw_curl not yet implemented")
}
