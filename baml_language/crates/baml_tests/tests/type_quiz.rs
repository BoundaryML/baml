//! Executes the type-system quiz conformance suite in `tools/type_quiz/` via
//! `baml_cli`, the same way `baml_src.rs` drives the corpus.
//!
//! The suite verifies every quiz item against the real compiler, so a
//! compiler change that alters a verdict, a diagnostic code, or a reflected
//! type kind fails here rather than leaving the quiz teaching a stale
//! language.

/// The workspace root, which `crates/baml_tests` sits two levels under.
fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/baml_tests sits two levels under the workspace root")
        .to_path_buf()
}

/// Every `.baml` file of the quiz package, in a stable order.
///
/// The CLI writes a cache under `.baml/` and build output under `target/`,
/// neither of which is source; a plain recursive walk would format-check
/// generated files and fail on whatever happened to be cached.
fn quiz_sources(package: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable dir entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|ext| ext == "baml") {
                out.push(path);
            }
        }
    }

    let mut found = Vec::new();
    walk(package, &mut found);
    found.sort();
    assert!(
        !found.is_empty(),
        "no .baml sources under {}",
        package.display()
    );
    found
}

/// The package's structural invariants: the dependency direction between its
/// namespaces, the ban on the stdlib's random generators, and the ban on
/// wildcard match arms outside the engine.
///
/// In CI rather than in a pre-commit hook because a hook gates a commit on
/// the one machine that has it installed, and this gates a merge. `lint.sh`
/// is the single definition either way.
#[test]
fn type_quiz_lint() {
    let script = workspace_root().join("tools/type_quiz/lint.sh");
    let status = std::process::Command::new(&script)
        .status()
        .unwrap_or_else(|e| panic!("run {}: {e}", script.display()));
    assert!(
        status.success(),
        "{} reported a violation",
        script.display()
    );
}

/// Every source of the package is already what `baml fmt` would write.
///
/// Checked rather than applied: CI must not rewrite the tree, so each file is
/// compared against `fmt --dry-run`, which writes the formatted text to
/// stdout instead of to the file. This matters beyond tidiness — `lint.sh`
/// reads how a match arm is laid out across lines, so an unformatted tree can
/// make the lint answer a different question than the one it was written to
/// ask.
///
/// `--dry-run` ends each file it writes with a separator newline, so its
/// output is one byte longer than the file it would have written. That byte
/// is dropped before comparing, and only that byte: trimming trailing
/// whitespace instead would stop this noticing a file that gained or lost a
/// final newline, which is a real formatting difference.
#[test]
fn type_quiz_formatted() {
    let workspace_root = workspace_root();
    let package = workspace_root.join("tools/type_quiz");
    let mut unformatted = Vec::new();
    for source in quiz_sources(&package) {
        let output = std::process::Command::new("cargo")
            .args(["run", "-q", "-p", "baml_cli", "--", "fmt", "--dry-run"])
            .arg(&source)
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .current_dir(&workspace_root)
            .output()
            .expect("baml_cli fmt --dry-run should run");
        assert!(
            output.status.success(),
            "baml fmt --dry-run failed on {}:\n{}",
            source.display(),
            String::from_utf8_lossy(&output.stderr),
        );
        let on_disk =
            std::fs::read(&source).unwrap_or_else(|e| panic!("read {}: {e}", source.display()));
        let written = output
            .stdout
            .strip_suffix(b"\n")
            .expect("fmt --dry-run ends every file with a separator newline");
        if written != on_disk {
            unformatted.push(
                source
                    .strip_prefix(&workspace_root)
                    .unwrap_or(&source)
                    .display()
                    .to_string(),
            );
        }
    }
    assert!(
        unformatted.is_empty(),
        "unformatted; run `mise run fmt-type-quiz`:\n  {}",
        unformatted.join("\n  "),
    );
}

#[test]
fn type_quiz_conformance() {
    // Same isolation as the corpus runner: the CLI's cache and home live
    // outside the source tree, and the bytecode cache is content-addressed
    // with the compiler fingerprint so a stale hit is impossible.
    let tmp = tempfile::tempdir().expect("tempdir for quiz cache");
    let workspace_root = workspace_root();
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
        // Most of the suite compiles programs through the real compiler, and on
        // a shared runner with the rest of `baml_tests` alongside, the CLI's
        // default five-minute cap per test is met by the slowest of them with
        // nothing wrong. Fifteen minutes is the ceiling for a stall, not a
        // budget the suite uses: the whole of it runs in five on an idle core.
        .env("BAML_TEST_TIMEOUT_MS", "900000")
        // Profiling ships default-ON, and this suite is the worst possible
        // shape for it: a hundred-odd tests whose work is millions of small
        // BAML calls, every one of them traced. Measured on one learner test,
        // tracing was 47% of the CPU and wrote 262 MB that nothing ever reads
        // — the whole suite wrote about 1.5 GB a run. The benchmarks pin it
        // off for the same reason (`benches/runtime_benchmark.rs`); a
        // conformance run wants the answers, not the timings.
        .env("BAML_PROFILE", "0")
        .env("BAML_HOME", &home)
        .env("BAML_CACHE_DIR", &cache_dir)
        .env("BAML_PROFILE_DIR", tmp.path().join("profiles-v1"))
        .status()
        .expect("baml_cli test should not fail");
    assert!(status.success());
}
