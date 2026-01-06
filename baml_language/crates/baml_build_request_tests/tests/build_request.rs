//! Snapshot tests for HTTP request construction.
//!
//! TODO: Implement once build_request is available.

use std::path::Path;

#[test]
#[ignore = "build_request not yet implemented"]
fn build_request_snapshots() {
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

            let _snapshot = run_build_request_fixture(&fixture).expect("fixture run");

            let snapshot_name = format!("{}_build_request", fixture_name.replace('-', "_"));
            // insta::assert_yaml_snapshot!(snapshot_name, &snapshot);
            let _ = snapshot_name;
        });
    });
}

fn run_build_request_fixture(_path: &Path) -> anyhow::Result<()> {
    // TODO: Implement once build_request is available
    // 1. Load BAML file
    // 2. Parse and extract function + client config
    // 3. Build HTTP request (method, URL, headers, body)
    // 4. Return serializable snapshot
    anyhow::bail!("build_request not yet implemented")
}
