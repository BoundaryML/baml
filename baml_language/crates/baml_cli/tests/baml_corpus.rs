//! Execute the shared BAML runtime corpus with Cargo's prebuilt CLI.
//!
//! This test belongs to `baml_cli` so `CARGO_BIN_EXE_baml-cli` selects the exact
//! binary built by the outer invocation, including its profile and features.
//! Compiler snapshots remain in `baml_tests::corpus`.

/// Execute `baml test`
#[test]
fn baml_test() {
    let executed = run_baml_tests(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../baml_tests/baml_src"),
        "medium",
    );
    // Only a debug `baml-cli` (the test's own profile) records opcodes.
    if cfg!(debug_assertions) {
        assert_runs_every_opcode(&executed);
    }
}

/// Opcodes no corpus program runs. `bex_vm`'s
/// `opcodes_the_corpus_never_runs_pass_the_operand_depth_check` runs these
/// same ones, under the same debug-build checks; keep the two lists equal.
const OPCODES_RUN_ELSEWHERE: &[&str] = &[
    // A compiled body's resume stub; the corpus runs bytecode only.
    "ResumeCompiled",
    // Raises; no correct program reaches it.
    "Unreachable",
    // Kept for older bytecode; the compiler no longer emits them.
    "AllocVariant",
    "TraceHookRich",
    "JumpIfFalse",
    // Map access compiles to `baml.Map` calls; these handlers refuse to run.
    "LoadMapElement",
    "StoreMapElement",
    // Corpus comparisons of these kinds take the typed or driver paths.
    "LtEq",
    "GtEq",
    "CmpBigintNotEq",
];

/// Every instruction a debug `baml-cli` runs passes the dispatch loop's
/// operand-depth assertion, which ties the load-time stack check's effect
/// table to the VM's handlers. Running every opcode means no handler escapes
/// that assertion.
fn assert_runs_every_opcode(executed: &[String]) {
    let missing: Vec<String> = (0..=u8::MAX)
        .filter_map(|byte| bex_vm_types::bytecode::OpCode::try_from(byte).ok())
        .map(|op| format!("{op:?}"))
        .filter(|name| !executed.contains(name) && !OPCODES_RUN_ELSEWHERE.contains(&name.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "the corpus never runs these opcodes, so their handlers escape the \
         debug-build stack-effect check: {missing:?}"
    );
}

#[test]
fn tracing_disabled() {
    let _ = run_baml_tests(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/trace_disabled"),
        "off",
    );
}

/// Run the corpus with a debug `baml-cli`, returning the opcodes it ran.
fn run_baml_tests(project: &str, telemetry: &str) -> Vec<String> {
    // Isolate the CLI's home per run. Without this, the CLI would write its
    // build cache straight into the source tree that the
    // `corpus_snapshots`/`emit_determinism` tests scan concurrently, and
    // successive runs would share (and could corrupt) that cache.
    let tmp = tempfile::tempdir().expect("tempdir for corpus cache");
    let executed_opcodes = tmp.path().join("executed-opcodes.txt");
    // The build cache lives under the cargo target dir -- outside the source
    // tree the `corpus_snapshots`/`emit_determinism` tests
    // scan -- and stays warm across runs, so an unchanged corpus recompiles
    // nothing. It is content-addressed with the compiler fingerprint in the
    // key, so staleness is a miss, never a wrong hit.
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/baml_cli sits two levels under the workspace root")
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
    // Keep the build cache warm across runs: point `$BAML_HOME/build/cache`
    // at the persistent directory (the symlink is skipped where unavailable,
    // leaving a cold per-run cache).
    #[cfg(unix)]
    {
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::create_dir_all(home.join("build")).unwrap();
        std::os::unix::fs::symlink(&cache_dir, home.join("build").join("cache")).unwrap();
    }
    #[cfg(not(unix))]
    let _ = &cache_dir;
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["test", "--project-dir", project])
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_TELEMETRY", telemetry)
        .env("BAML_HOME", &home)
        .env("BAML_DEBUG_EXECUTED_OPCODES", &executed_opcodes)
        .status()
        .expect("baml_cli test should not fail");
    assert!(status.success());
    if !cfg!(debug_assertions) {
        return Vec::new();
    }
    std::fs::read_to_string(&executed_opcodes)
        .expect("a debug baml-cli writes the opcodes it ran")
        .lines()
        .map(str::to_string)
        .collect()
}
