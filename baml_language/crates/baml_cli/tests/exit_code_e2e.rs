// End-to-end tests for CLI exit codes on compilation errors.
//
// These tests verify that `baml-cli` commands return non-zero exit codes
// when compilation errors occur. This is critical for CI/CD pipelines and
// scripting use cases where the exit code is used to determine success or
// failure.

mod common;

use std::{
    path::Path,
    process::{Command, Output},
};

use common::BuiltPaths;

// ============================================================================
// Helpers
// ============================================================================

/// Run a baml-cli command and return the output (stdout, stderr, exit code).
fn run_baml_cli(built: &BuiltPaths, dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(&built.baml_cli);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(dir);
    cmd.output().expect("spawn baml-cli")
}

/// Create a minimal project structure with the given source code.
fn create_project(dir: &Path, source: &str) {
    std::fs::write(
        dir.join("baml.toml"),
        "[package]\nname = \"test-project\"\n",
    )
    .unwrap();
    let src = dir.join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("main.baml"), source).unwrap();
}

/// Create a project with a generator block for testing the generate command.
fn create_project_with_generator(dir: &Path, source: &str) {
    let generator_block = r#"
generator py {
    output_type "python/pydantic"
    output_dir ".."
    naming_convention "preserve-case"
}
"#;
    let full_source = format!("{source}\n{generator_block}");
    create_project(dir, &full_source);
}

// ============================================================================
// Tests for `baml generate` exit codes
// ============================================================================

/// Compilation errors must result in a non-zero exit code for `baml generate`.
/// This is critical for CI/CD pipelines that use exit codes to gate deployments.
#[test]
fn generate_compilation_error_returns_nonzero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    // Create a project with an unresolved type error
    create_project_with_generator(
        tmp.path(),
        "function test_func() -> UndefinedType {\n  \"never called\"\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for compilation error, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Verify the exit code is specifically 4 (ExitCode::Other)
    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit code 4 for compilation error, got: {:?}",
        output.status.code(),
    );

    // Verify the error message mentions the unresolved type
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("UndefinedType") || stderr.contains("unresolved"),
        "Expected error message to mention the unresolved type, got: {stderr}",
    );
}

/// Multiple compilation errors should still result in a non-zero exit code.
#[test]
fn generate_multiple_compilation_errors_returns_nonzero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    // Create a project with multiple errors
    create_project_with_generator(
        tmp.path(),
        r#"
function bad_func1() -> UnknownType1 {
  "never"
}
function bad_func2() -> UnknownType2 {
  "called"
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for multiple compilation errors",
    );
    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit code 4 for compilation errors",
    );
}

/// A valid project should return exit code 0 from `baml generate`.
#[test]
fn generate_valid_project_returns_zero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    // Create a valid project
    create_project_with_generator(
        tmp.path(),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit code 0 for valid project, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ============================================================================
// Tests for `baml run` exit codes
// ============================================================================

/// Compilation errors must result in a non-zero exit code for `baml run --list`.
#[test]
fn run_list_compilation_error_returns_nonzero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        "function broken() -> MissingType {\n  \"never\"\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["run", "--list", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for compilation error in `baml run --list`",
    );
    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit code 4 for compilation error",
    );
}

// ============================================================================
// Tests for `baml test` exit codes
// ============================================================================

/// Compilation errors must result in a non-zero exit code for `baml test`.
#[test]
fn test_compilation_error_returns_nonzero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        "function broken() -> MissingType {\n  \"never\"\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for compilation error in `baml test`",
    );
    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit code 4 for compilation error",
    );
}

/// `baml test` with no tests should return exit code 5 (`NoTestsRun`), not 0.
#[test]
fn test_no_tests_returns_specific_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    // Valid project with no tests
    create_project(
        tmp.path(),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);

    // NoTestsRun maps to exit code 5
    assert_eq!(
        output.status.code(),
        Some(5),
        "Expected exit code 5 for no tests found, got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}
