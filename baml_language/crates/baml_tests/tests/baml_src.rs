//! Executes all BAML tests in `crates/baml_tests/baml_src/` via `baml_cli`.
//!
//! Compiler-phase and bytecode snapshots for the same corpus live in the
//! single-compile pass in `src/corpus.rs`.

/// The CLI's compile workload is allocation-dominated; its binary installs
/// mimalloc for exactly this reason (see `baml_cli/src/main.rs`). The CLI now
/// runs in-process here, so this test binary installs the same allocator to
/// keep the corpus compile at the binary's speed.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Execute `baml test` in-process via [`baml_cli::run_cli`].
///
/// In-process rather than a subprocess so the CLI is built once, as part of
/// this test binary's own dependency graph — `cargo run -p baml_cli` re-
/// resolves features for `baml_cli` alone and rebuilds its whole dependency
/// chain inside the test (15+ minutes cold), serialized against every other
/// test that shells out to cargo.
#[test]
fn baml_test() {
    // The CLI's bytecode cache lives under the cargo target dir — outside the
    // source tree that the `corpus_snapshots`/`emit_determinism`/
    // `link_units_oracle` tests scan (the default would be
    // `baml_src/.baml/cache`) — and stays warm across runs, so an unchanged
    // corpus recompiles nothing. The cache is content-addressed with the
    // compiler fingerprint in the key, so staleness is a miss, never a wrong
    // hit.
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/baml_tests sits two levels under the workspace root")
        .to_path_buf();
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .map(|dir| {
            if dir.is_absolute() {
                dir
            } else {
                workspace_root.join(dir)
            }
        })
        .unwrap_or_else(|| workspace_root.join("target"));
    // SAFETY: `set_var` races with *any* concurrent environment access, so
    // the obligation is single-threadedness, not merely "no other writers".
    // This is the only `#[test]` in this binary (the Prompt Fiddle check runs
    // inside it, on this same thread, below), so no other test thread exists
    // to call `getenv` while this writes.
    unsafe {
        std::env::set_var("BAML_CACHE_DIR", target_dir.join("baml-corpus-cache"));
    }

    // Folded into this test rather than a sibling `#[test]` so the `set_var`
    // above stays sound: libtest would run a sibling concurrently on another
    // thread, and its compiler stack reads the environment.
    //
    // This cross-workspace include is intentionally cursed: Prompt Fiddle owns
    // the demo, while this existing test binary checks it without a second
    // compiler build.
    let demo = include_str!("../../../../typescript2/app-promptfiddle/src/playground/default.baml");
    baml_db::testing::compile_multi_file(&[("baml_src/main.baml", demo)]);

    let argv = vec![
        "baml".to_string(),
        "test".to_string(),
        "--from".to_string(),
        concat!(env!("CARGO_MANIFEST_DIR"), "/baml_src").to_string(),
    ];
    let code = baml_cli::run_cli(argv).expect("baml test should not error");
    assert!(
        matches!(code, baml_cli::ExitCode::Success),
        "baml test exited with {code:?}"
    );
}
