//! Executes all BAML tests in `crates/baml_tests/baml_src/` via `baml_cli`.
//!
//! Compiler-phase and bytecode snapshots for the same corpus live in the
//! single-compile pass in `src/corpus.rs`.

/// Execute `baml test`
#[test]
fn baml_test() {
    // Isolate the CLI's bytecode cache and home per run. Without this, the CLI
    // writes `<project>/.baml/cache` straight into the source tree that the
    // `corpus_snapshots`/`emit_determinism`/`link_units_oracle` tests scan
    // concurrently, and successive runs share (and can corrupt) that cache.
    let tmp = tempfile::tempdir().expect("tempdir for corpus cache");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let status = std::process::Command::new("cargo")
        .args([
            "run",
            "-p",
            "baml_cli",
            "--",
            "test",
            "--from",
            concat!(env!("CARGO_MANIFEST_DIR"), "/baml_src"),
        ])
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", &home)
        .env("BAML_CACHE_DIR", tmp.path().join("cache"))
        .status()
        .expect("baml_cli test should not fail");
    assert!(status.success());
}

#[test]
fn promptfiddle_demo_compiles() {
    // This cross-workspace include is intentionally cursed: Prompt Fiddle owns
    // the demo, while this existing test binary checks it without a second compiler build.
    let source =
        include_str!("../../../../typescript2/app-promptfiddle/src/playground/default.baml");
    baml_db::testing::compile_multi_file(&[("baml_src/main.baml", source)]);
}
