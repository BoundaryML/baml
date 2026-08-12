/// Integration test that builds the native module and runs the Python test suite.
///
/// Requires `uv` to be installed. Run with:
///
///     cargo test -p bridge_python -- --ignored
///
#[test]
#[ignore]
fn pytest() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // Build and install the native extension into the local venv.
    let develop = std::process::Command::new("uv")
        .args(["run", "maturin", "develop", "--uv"])
        .current_dir(manifest_dir)
        .status()
        .expect("failed to run `uv` — is it installed?");
    assert!(develop.success(), "maturin develop failed");

    // Run the Python test suite.
    let pytest = std::process::Command::new("uv")
        .args(["run", "pytest", "tests/", "-v"])
        .current_dir(manifest_dir)
        .status()
        .expect("failed to run pytest");
    assert!(pytest.success(), "pytest failed");
}
