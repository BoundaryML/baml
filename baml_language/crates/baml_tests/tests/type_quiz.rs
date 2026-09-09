//! Executes the type-system quiz conformance suite in `tools/type_quiz/` via
//! `baml_cli`, the same way `baml_src.rs` drives the corpus.
//!
//! The suite verifies every quiz item against the real compiler, so a
//! compiler change that alters a verdict, a diagnostic code, or a reflected
//! type kind fails here rather than leaving the quiz teaching a stale
//! language.

#[test]
fn type_quiz_conformance() {
    // Same isolation as the corpus runner: the CLI's cache and home live
    // outside the source tree, and the bytecode cache is content-addressed
    // with the compiler fingerprint so a stale hit is impossible.
    let tmp = tempfile::tempdir().expect("tempdir for quiz cache");
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/baml_tests sits two levels under the workspace root")
        .to_path_buf();
    let cache_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .map(|dir| {
            if dir.is_absolute() {
                dir
            } else {
                workspace_root.join(dir)
            }
        })
        .unwrap_or_else(|| workspace_root.join("target"))
        .join("baml-type-quiz-cache");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let status = std::process::Command::new("cargo")
        .args(["run", "-p", "baml_cli", "--", "test", "--from"])
        .arg(workspace_root.join("tools/type_quiz"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", &home)
        .env("BAML_CACHE_DIR", &cache_dir)
        .env("BAML_PROFILE_DIR", tmp.path().join("profiles-v1"))
        .status()
        .expect("baml_cli test should not fail");
    assert!(status.success());
}
