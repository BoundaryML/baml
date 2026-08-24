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
    // The bytecode cache lives under the cargo target dir -- outside the source
    // tree the `corpus_snapshots`/`emit_determinism`/`link_units_oracle` tests
    // scan -- and stays warm across runs, so an unchanged corpus recompiles
    // nothing. It is content-addressed with the compiler fingerprint in the
    // key, so staleness is a miss, never a wrong hit. Passing it through the
    // subprocess environment avoids the `set_var` soundness obligation.
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
        .join("baml-corpus-cache");
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
        .env("BAML_CACHE_DIR", &cache_dir)
        // The profiler otherwise lands `<project>/.baml/profiles-v1` in the
        // corpus source tree.
        .env("BAML_PROFILE_DIR", tmp.path().join("profiles-v1"))
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
    baml_project::testing::compile_multi_file(&[("baml_src/main.baml", source)]);
}
