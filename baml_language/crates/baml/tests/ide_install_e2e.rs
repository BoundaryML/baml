//! End-to-end coverage for the wrapper's first-run IDE installation path.

use std::process::Command;

#[test]
fn ide_install_attempts_toolchain_setup_on_a_fresh_install() {
    let home = tempfile::tempdir().unwrap();
    let manifest_base_url = "http://127.0.0.1:1/manifest";
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["ide", "install", "--code"])
        .current_dir(home.path())
        .env("BAML_HOME", home.path())
        .env("HOME", home.path())
        .env("BAML_MANIFEST_BASE_URL", manifest_base_url)
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to set up the BAML toolchain required by `baml ide install`"),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("failed to fetch {manifest_base_url}/canary.json")),
        "{stderr}"
    );
    assert!(
        !stderr.contains("no BAML toolchain is installed"),
        "{stderr}"
    );
}
