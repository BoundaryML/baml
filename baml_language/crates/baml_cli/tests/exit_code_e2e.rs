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
///
/// `BAML_HOME` is pointed at an empty directory inside the project (with the
/// freshness auto-check disabled) so the passive skill check never reads the
/// developer's real `~/.baml` state or touches the network.
fn run_baml_cli(built: &BuiltPaths, dir: &Path, args: &[&str]) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut cmd = Command::new(&built.baml_cli);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(dir);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    cmd.env("BAML_HOME", &home);
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

/// Create a project with a `[generator.py]` section in `baml.toml` for
/// testing the generate command.
fn create_project_with_generator(dir: &Path, source: &str) {
    create_project(dir, source);
    std::fs::write(
        dir.join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.py]\n\
         output_type = \"python/pydantic\"\n\
         output_dir = \"..\"\n\
         naming_convention = \"preserve-case\"\n",
    )
    .unwrap();
}

fn create_project_with_go_generator(dir: &Path, source: &str) {
    create_project(dir, source);
    std::fs::write(
        dir.join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.go_client]\n\
         output_type = \"go\"\n\
         output_dir = \".\"\n\
         naming_convention = \"language\"\n\
         sdk_import_path = \"example.com/test-project/baml_sdk\"\n",
    )
    .unwrap();
}

// ============================================================================
// Tests for `baml check` exit codes
// ============================================================================

/// A valid project should return exit code 0 from `baml check`.
#[test]
fn check_valid_project_returns_zero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["check", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit code 0 for valid project, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// `baml check` with no `--from` is sugar for `baml check --from .`.
#[test]
fn check_defaults_from_to_current_directory() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["check"]);

    assert!(
        output.status.success(),
        "Expected exit code 0 for `baml check` defaulting to cwd, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Compilation errors must result in a non-zero exit code for `baml check`.
#[test]
fn check_compilation_error_returns_nonzero_exit_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        "function broken() -> MissingType {\n  \"never\"\n}\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["check", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for compilation error in `baml check`",
    );
    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit code 4 for compilation error",
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MissingType") || stderr.contains("unresolved"),
        "Expected error message to mention the unresolved type, got: {stderr}",
    );
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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "Generating clients with CLI version: {}",
            baml_version::CANONICAL_VERSION
        )),
        "Expected generate output to include CLI version, got: {stderr}",
    );
    assert!(
        stderr.contains("Compiling 1 file(s)"),
        "`baml generate` should keep compile progress, got: {stderr}",
    );
}

#[test]
fn generate_csharp_removes_stale_owned_files_and_refuses_user_edits() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        "function root_echo(value: string) -> string {\n  value\n}\n",
    );
    std::fs::write(
        tmp.path().join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.csharp]\n\
         output_type = \"csharp\"\n\
         output_dir = \".\"\n\
         naming_convention = \"language\"\n",
    )
    .unwrap();
    let stale_source = tmp.path().join("baml_src/ns_stale/extra.baml");
    std::fs::create_dir_all(stale_source.parent().unwrap()).unwrap();
    std::fs::write(
        &stale_source,
        "function stale_echo(value: int) -> int {\n  value\n}\n",
    )
    .unwrap();
    let sdk = tmp.path().join("baml_sdk");
    std::fs::create_dir(&sdk).unwrap();
    std::fs::write(sdk.join("User.cs"), "public sealed class UserOwned {}\n").unwrap();

    let first = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert!(
        first.status.success(),
        "initial C# generation failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(sdk.join("Stale/Functions.g.cs").is_file());
    assert!(sdk.join(".baml-generated-files.json").is_file());

    std::fs::remove_file(stale_source).unwrap();
    let second = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert!(
        second.status.success(),
        "second C# generation failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(!sdk.join("Stale").exists());
    assert_eq!(
        std::fs::read_to_string(sdk.join("User.cs")).unwrap(),
        "public sealed class UserOwned {}\n"
    );

    let manifest_before = std::fs::read(sdk.join(".baml-generated-files.json")).unwrap();
    let program_before = std::fs::read(sdk.join("BamlGeneratedProgram.g.cs")).unwrap();
    let functions = sdk.join("Functions.g.cs");
    let mut edited = std::fs::read_to_string(&functions).unwrap();
    edited.push_str("// intentional user edit\n");
    std::fs::write(&functions, &edited).unwrap();

    let rejected = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert_eq!(rejected.status.code(), Some(4));
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("was modified; refusing to overwrite")
    );
    assert_eq!(std::fs::read_to_string(functions).unwrap(), edited);
    assert_eq!(
        std::fs::read(sdk.join(".baml-generated-files.json")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(sdk.join("BamlGeneratedProgram.g.cs")).unwrap(),
        program_before
    );
    assert_eq!(
        std::fs::read_to_string(sdk.join("User.cs")).unwrap(),
        "public sealed class UserOwned {}\n"
    );
}

#[test]
fn generate_rejects_duplicate_output_directory_before_writing_files() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        "function root_echo(value: string) -> string {\n  value\n}\n",
    );
    std::fs::write(
        tmp.path().join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.first]\n\
         output_type = \"csharp\"\n\
         output_dir = \".\"\n\
         naming_convention = \"language\"\n\n\
         [generator.second]\n\
         output_type = \"csharp\"\n\
         output_dir = \".\"\n\
         naming_convention = \"language\"\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resolve to the same output directory"),
        "unexpected duplicate-output diagnostic: {stderr}"
    );
    assert!(
        std::fs::read_dir(tmp.path().join("baml_sdk"))
            .unwrap()
            .next()
            .is_none(),
        "duplicate output ownership must fail before generated files are written"
    );
}

#[test]
fn generate_go_writes_sdk_through_cli() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    create_project_with_go_generator(
        tmp.path(),
        "function echo(value: string) -> string { value }\n",
    );

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert!(
        output.status.success(),
        "Go generation failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let functions = std::fs::read_to_string(tmp.path().join("baml_sdk/functions.go"))
        .expect("Go functions.go should be generated");
    assert!(
        functions.contains("func Echo("),
        "generated Go:\n{functions}"
    );
    let bootstrap =
        std::fs::read_to_string(tmp.path().join("baml_sdk/internal/bootstrap/bootstrap.go"))
            .expect("Go bootstrap should be generated");
    assert!(bootstrap.contains("func Ensure() error"));
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

/// `baml run` should only emit the program output for a formatted project.
/// Compile progress remains reserved for `baml check` and `baml generate`.
#[test]
fn run_valid_project_outputs_only_program_output() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(tmp.path(), "function answer() -> int {\n    42\n}\n");
    // Installed skills keep the passive skill check quiet, so stderr stays
    // exactly the program's own output.
    let skill_dir = tmp.path().join(".agents/skills/baml-core");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "---\nname: baml-core\n---\n").unwrap();

    let output = run_baml_cli(built, tmp.path(), &["run", "answer", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit code 0 for valid run, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "Expected run result, got:\n{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.trim().is_empty(),
        "Expected empty stderr, got:\n{stderr}"
    );
}

/// The formatter advisory is the allowed `baml run` stderr exception.
#[test]
fn run_unformatted_project_keeps_format_warning() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(tmp.path(), "function answer()->int {\n42\n}\n");

    let output = run_baml_cli(built, tmp.path(), &["run", "answer", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit code 0 for valid run, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "Expected run result, got:\n{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Code is unformatted"),
        "Expected format warning, got:\n{stderr}"
    );
    common::assert_no_compile_file_status(&stderr);
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

/// A selector that matches no test must NOT print a green `PASS testing::*`
/// line: the aggregate of zero tests is a vacuous pass, but stdout that says
/// PASS while the command exits 5 (`NoTestsRun`) misleads anything parsing it.
/// Regression for B-628.
#[test]
fn test_no_match_selector_does_not_print_pass() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    // A project that DOES have tests, so discovery yields a registry — the
    // empty selection has to come from the filter, not an empty project.
    create_project(
        tmp.path(),
        r#"
testset "suite" {
  test "one" { assert.is_true(true) }
  test "two" { assert.is_true(true) }
}
"#,
    );

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["test", "--from", ".", "-i", "totally-bogus-selector-xyz"],
    );

    // Exit-code semantics are preserved: no tests selected is exit 5.
    assert_eq!(
        output.status.code(),
        Some(5),
        "Expected NoTestsRun (5) for a no-match selector, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        !combined.contains("PASS"),
        "A no-match selector must not print a PASS line, got:\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert!(
        combined.contains("no tests selected"),
        "Expected a `no tests selected` message, got:\nstdout: {stdout}\nstderr: {stderr}",
    );
}

/// `baml test` should not emit the compile file-count status pair.
#[test]
fn test_valid_project_omits_compile_file_status() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
test "passes" {
  assert.equal(1, 1)
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit code 0 for valid test, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PASS"),
        "Expected passing test output, got:\n{stdout}"
    );
    common::assert_no_compile_file_status(&String::from_utf8_lossy(&output.stderr));
}

/// Failing `assert.equal` should surface both operand values and keep stack
/// traces user-facing (no internal `Span`/`FileId` debug structs).
#[test]
fn test_assert_equal_failure_shows_values_without_internal_span_debug() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
test "assert-equal-failure" {
  assert.equal(4611686018427387903, -4611686018427387904)
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected test failure exit code for failing assert.equal, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .contains("assertion failed: left = 4611686018427387903, right = -4611686018427387904"),
        "Expected assert.equal failure message to include left/right values, got: {stderr}",
    );
    assert!(
        !stderr.contains("Span {"),
        "User-facing test output should not include internal Span debug data: {stderr}",
    );
    assert!(
        !stderr.contains("FileId("),
        "User-facing test output should not include internal FileId debug data: {stderr}",
    );
}

/// `assert.approx_equal` lets float assertions pass with a tolerance so normal
/// floating-point rounding artifacts do not fail tests.
///
/// Returns:
/// - Nothing; this test passes when `baml test` exits successfully.
///
/// Panics:
/// - Panics if `baml test` fails or does not report the passing test case.
#[test]
fn test_assert_approx_equal_accepts_float_tolerance() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
test "assert-approx-equal-passes" {
  let total = 9.99 + 5.50 + 2.00
  assert.approx_equal(total, 17.49, 0.000001)
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "Expected assert.approx_equal to tolerate float rounding, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout,
        stderr,
    );
    assert!(
        combined.contains("1 passed, 0 failed, 1 total"),
        "Expected a passing aggregate summary, got:\n{combined}",
    );
}

#[test]
fn test_unfiltered_testset_run_honors_pass_rate_runner() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
testset "suite" with testing.PassRate(0.6) {
  test "one" { assert.is_true(true) }
  test "two" { assert.is_true(true) }
  test "three" { assert.is_true(false) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "Expected unfiltered `baml test` to honor PassRate and pass, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout,
        stderr,
    );
    assert!(
        combined.contains("PASS testing::* [outcome=pass; 1 tolerated failure]"),
        "Expected unfiltered aggregate output to identify tolerated failures, got:\n{combined}"
    );
    assert!(
        combined.contains("aggregate passed — 2 passed, 1 tolerated failure, 3 total"),
        "Expected unfiltered aggregate summary to report tolerated leaf totals, got:\n{combined}"
    );
}

#[test]
fn test_filtered_testset_run_honors_pass_rate_runner_for_selected_set() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
testset "suite" with testing.PassRate(0.6) {
  test "one" { assert.is_true(true) }
  test "two" { assert.is_true(true) }
  test "three" { assert.is_true(false) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", ".", "-i", "suite::"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "Expected filtered testset run to honor PassRate and pass, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout,
        stderr,
    );
    assert!(
        combined.contains("PASS testing::* [outcome=pass; 1 tolerated failure]"),
        "Expected filtered aggregate output to identify tolerated failures, got:\n{combined}"
    );
    assert!(
        combined.contains("aggregate passed — 2 passed, 1 tolerated failure, 3 total"),
        "Expected filtered aggregate summary to report selected leaf totals, got:\n{combined}"
    );
}

#[test]
fn test_filtered_testset_leaf_runs_under_parent_runner() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
testset "suite" with testing.PassRate(0.0) {
  test "failing leaf" { assert.is_true(false) }
}
"#,
    );

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["test", "--from", ".", "-i", "suite::failing leaf"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "Expected filtered leaf to run under parent PassRate and pass, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout,
        stderr,
    );
    assert!(
        combined.contains("PASS testing::* [outcome=pass; 1 tolerated failure]"),
        "Expected filtered leaf output to identify tolerated failures, got:\n{combined}"
    );
    assert!(
        combined.contains("aggregate passed — 0 passed, 1 tolerated failure, 1 total"),
        "Expected filtered leaf output to report selected leaf totals, got:\n{combined}"
    );
}

#[test]
fn test_mixed_testset_run_keeps_tolerated_failures_out_of_failed_total() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
testset "tolerant" with testing.PassRate(0.0) {
  test "tolerated failure" { assert.is_true(false) }
}

testset "hard" {
  test "passes" { assert.is_true(true) }
  test "fails" { assert.is_true(false) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected hard failing sibling testset to fail the command, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout,
        stderr,
    );
    assert!(
        combined.contains("1 passed, 1 failed, 1 tolerated failure, 3 total"),
        "Expected tolerated leaf to stay out of hard failure count, got:\n{combined}"
    );
}

#[test]
fn test_unfiltered_testset_run_reports_failed_child_name() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
testset "suite" {
  test "one" { assert.is_true(true) }
  test "two" { assert.is_true(false) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected unfiltered failing testset to fail, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout,
        stderr,
    );
    assert!(
        stdout.contains("failed: suite/two"),
        "Expected aggregate output to include the failed child name, got:\n{stdout}"
    );
}

#[test]
fn test_unfiltered_testset_run_fails_when_aggregate_outcome_fails() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
function AlwaysFail(children: testing.TestSetChild[]) -> testing.TestSetReport {
  let report = testing.Sequential()(children)
  testing.TestSetReport {
    outcome: "fail",
    passed: report.passed,
    failed: 0,
    total: report.total,
    failed_names: report.failed_names,
    results: report.results,
  }
}

testset "suite" with AlwaysFail {
  test "one" { assert.is_true(true) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected failing aggregate outcome to fail even with zero failed children, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ============================================================================
// Tests for project-less introspection (`baml describe` / `baml grep` /
// `baml fmt` without a `baml.toml`). The most expensive thing an agent can
// do is fail fast and burn a turn, so these read-only commands fall back to
// a stdlib-only "default state" instead of erroring.
// ============================================================================

/// `baml describe baml.String` from a directory with no `baml.toml` must
/// succeed against the stdlib — the headline use case for the default
/// state. Regression for the old "doesn't look like a BAML project" bail.
#[test]
fn describe_stdlib_without_baml_toml_succeeds() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["describe", "baml.String", "--from", "."],
    );

    assert!(
        output.status.success(),
        "Expected exit 0 describing stdlib with no baml.toml, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String"),
        "Expected stdout to describe `String`, got:\n{stdout}",
    );
    // The old failure message must not appear.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("doesn't look like a BAML project"),
        "Default state should not emit the no-project error, got stderr:\n{stderr}",
    );
}

/// `baml describe` walks up to an ancestor `baml.toml`, so introspection
/// from a project subdirectory resolves a user-defined symbol.
#[test]
fn describe_walks_up_to_ancestor_project() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    );
    let nested = tmp.path().join("baml_src").join("nested");
    std::fs::create_dir_all(&nested).unwrap();

    // Invoke from the nested subdir (default --from is ".").
    let output = run_baml_cli(built, &nested, &["describe", "greet"]);

    assert!(
        output.status.success(),
        "Expected exit 0 resolving a user symbol from a subdir, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greet"),
        "Expected stdout to describe `greet`, got:\n{stdout}",
    );
}

/// `baml fmt` in a directory with no project is a no-op success, not an
/// error — nothing to format is not a failure.
#[test]
fn fmt_without_project_is_noop_success() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    let output = run_baml_cli(built, tmp.path(), &["fmt", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit 0 for `baml fmt` with no project, got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// `baml grep` with no `baml.toml` must not hard-fail on the missing
/// manifest. The user-file set is empty in the default state (grep only
/// searches user files, not the stdlib), so a "no match" result is correct
/// — what matters is that the old "doesn't look like a BAML project" bail
/// is gone and the process doesn't crash.
#[test]
fn grep_without_baml_toml_does_not_fail_on_missing_manifest() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    let output = run_baml_cli(built, tmp.path(), &["grep", "Foo", "--from", "."]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("doesn't look like a BAML project"),
        "grep should not emit the no-project error in the default state, got:\n{stderr}",
    );
    // Exited normally (Some(code)) rather than crashing on a signal (None).
    assert!(
        output.status.code().is_some(),
        "grep should exit cleanly in the default state, not crash",
    );
}

/// `baml describe` walks up to an ancestor with a **malformed** `baml.toml`
/// (no `[package].name`) and still succeeds — introspection tolerates a bad
/// manifest, unlike the strict build/execute path.
#[test]
fn describe_walks_up_to_ancestor_with_invalid_manifest() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    // Malformed manifest: no [package] table.
    std::fs::write(tmp.path().join("baml.toml"), "# no package table\n").unwrap();
    let nested = tmp.path().join("sub");
    std::fs::create_dir_all(&nested).unwrap();

    let output = run_baml_cli(built, &nested, &["describe", "baml.String"]);

    assert!(
        output.status.success(),
        "Expected exit 0 describing stdlib above an invalid manifest, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ============================================================================
// Tests for manifest-less execution: `baml.toml` is opt-in, a `baml_src/`
// directory alone is a valid project for run/test/generate/pack.
// ============================================================================

/// `baml run --list` works on a `baml_src/`-only project (no `baml.toml`).
#[test]
fn run_list_without_baml_toml_using_baml_src_succeeds() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    // No baml.toml — just a baml_src/ directory with a function.
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["run", "--list", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit 0 for `baml run --list` on a baml_src-only project, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greet"),
        "Expected the function list to include `greet`, got:\n{stdout}",
    );
}

/// A directory with neither a `baml.toml` nor a `baml_src/` is still
/// rejected — but the error points at `baml_src/`, `baml init`, and
/// `--file`, not just "missing baml.toml".
#[test]
fn run_without_any_project_marker_errors_with_hint() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    // A loose .baml at the root, but no baml.toml and no baml_src/.
    std::fs::write(
        tmp.path().join("loose.baml"),
        "function f() -> int {\n  1\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["run", "--list", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit when neither project marker is present",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The hint names all three escape hatches.
    for needle in ["baml_src/", "baml init", "--file"] {
        assert!(
            stderr.contains(needle),
            "Expected the error to mention `{needle}`, got:\n{stderr}",
        );
    }
}

/// `baml run <fn>` actually *executes* a function in a manifest-less
/// `baml_src/` project — the headline of this change, proven end-to-end
/// (not just `--list`). `answer` is pure (no LLM), so it runs hermetically.
#[test]
fn run_execute_function_without_baml_toml_succeeds() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function answer() -> int {\n  42\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["run", "answer", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit 0 executing a function on a baml_src-only project, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("42"),
        "Expected the result `42`, got:\n{stdout}"
    );
}

/// `baml describe --from` uses the same manifest-less `baml_src/` project
/// marker as `run`; agents should be able to inspect symbols in scratch
/// projects created without a `baml.toml`.
#[test]
fn describe_from_baml_src_only_project_finds_user_symbols() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        r#"
interface Named {
  function label(self) -> string
}

class Ticket {
  id: string

  implements Named {
    function label(self) -> string {
      return self.id
    }
  }
}
"#,
    )
    .unwrap();

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["describe", "Ticket", "--from", ".", "--budget", "120"],
    );

    assert!(
        output.status.success(),
        "Expected describe to find Ticket in a baml_src-only project, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("class Ticket"),
        "Expected class description, got:\n{stdout}"
    );
    assert!(
        stdout.contains("implements Named"),
        "Expected implements summary, got:\n{stdout}"
    );
}

/// Associated type projections that resolve to concrete value types must still
/// produce stdout through `baml run`. This catches a real boundary bug where the
/// VM metadata erased `(Class as Interface).Assoc` to `void`; dispatch treats
/// `void` as "do not print", so a value-returning function silently produced no
/// output.
#[test]
fn run_prints_concrete_associated_type_projection_return() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        r#"
interface PublicIdentity {
  type Key
  key: Self.Key
}

class AccountRecord {
  public_key: string

  implements PublicIdentity {
    type Key = string
    key as public_key
  }
}

function get_public_key() -> (AccountRecord as PublicIdentity).Key {
  let account = AccountRecord { public_key: "visible-key" }
  return account.as<PublicIdentity<Key = string>>.key
}
"#,
    );

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["run", "get_public_key", "--from", ".", "--features", "beta"],
    );

    assert!(
        output.status.success(),
        "Expected exit 0 for projected associated type return, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("visible-key"),
        "Expected projected string return to be printed, got:\n{stdout}"
    );
}

/// `baml run --list` reads VM function metadata, so concrete associated
/// projections must resolve while generic signatures stay visible for users and
/// agents instead of inheriting runtime erasure.
#[test]
fn run_list_prints_resolved_associated_projection_metadata() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        r#"
interface PublicIdentity {
  type Key
  key: Self.Key
}

class AccountRecord {
  public_key: string

  implements PublicIdentity {
    type Key = string
    key as public_key
  }
}

interface Repository {
  type Record
  function find(self) -> Self.Record throws never
}

class UserRecord {
  name: string
}

class UserRepository {
  value: UserRecord

  implements Repository {
    type Record = UserRecord

    function find(self) -> Self.Record {
      return self.value
    }
  }
}

class GenericBox<T> {
  value: T

  function get(self) -> T {
    return self.value
  }
}

interface BoxLike {
  type Item
  function get(self) -> Self.Item throws never
}

function get_public_key(account: AccountRecord) -> (AccountRecord as PublicIdentity).Key {
  return account.as<PublicIdentity<Key = string>>.key
}

function read_item<T extends BoxLike>(box: T) -> T.Item {
  return box.get()
}
"#,
    );

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["run", "--list", "--from", ".", "--features", "beta"],
    );

    assert!(
        output.status.success(),
        "Expected exit 0 listing projected associated type metadata, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "get_public_key(account: AccountRecord) -> string",
        "UserRepository.Repository.find(self: UserRepository) -> UserRecord",
        "GenericBox.get<T>(self: GenericBox<T>) -> T",
        // The projection renders fully determined — lowering resolves the
        // declaring interface, so `T.Item` prints as its canonical
        // `(T as BoxLike).Item` triple.
        "read_item<T extends BoxLike>(box: T) -> (T as BoxLike).Item",
    ] {
        assert!(
            stdout.contains(expected),
            "Expected `baml run --list` output to contain `{expected}`, got:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("read_item(box: unknown) -> unknown"),
        "Generic associated projection signatures must not be erased in list output:\n{stdout}"
    );
    assert!(
        !stdout.contains("-> void"),
        "Projected value-returning functions must not be listed as void:\n{stdout}"
    );

    let json_output = run_baml_cli(
        built,
        tmp.path(),
        &[
            "run",
            "--list",
            "--output-format",
            "json",
            "--from",
            ".",
            "--features",
            "beta",
        ],
    );
    assert!(
        json_output.status.success(),
        "Expected JSON list exit 0, got: {:?}\nstdout: {}\nstderr: {}",
        json_output.status.code(),
        String::from_utf8_lossy(&json_output.stdout),
        String::from_utf8_lossy(&json_output.stderr),
    );
    let value: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("parse run --list JSON output");
    let functions = value["functions"]
        .as_array()
        .expect("JSON list has functions array");
    let read_item = functions
        .iter()
        .find(|f| f["name"].as_str() == Some("read_item"))
        .expect("read_item listed in JSON");
    let generic_params: Vec<&str> = read_item["generic_params"]
        .as_array()
        .expect("read_item has generic_params")
        .iter()
        .map(|v| v.as_str().expect("generic param is string"))
        .collect();
    assert_eq!(generic_params, vec!["T extends BoxLike"]);
    assert_eq!(read_item["params"][0]["type"].as_str(), Some("T"));
    assert_eq!(
        read_item["return_type"].as_str(),
        Some("(T as BoxLike).Item")
    );

    let generic_box_get = functions
        .iter()
        .find(|f| f["name"].as_str() == Some("GenericBox.get"))
        .expect("GenericBox.get listed in JSON");
    let generic_box_params: Vec<&str> = generic_box_get["generic_params"]
        .as_array()
        .expect("GenericBox.get has generic_params")
        .iter()
        .map(|v| v.as_str().expect("generic param is string"))
        .collect();
    assert_eq!(generic_box_params, vec!["T"]);
    assert_eq!(
        generic_box_get["params"][0]["type"].as_str(),
        Some("GenericBox<T>")
    );
    assert_eq!(generic_box_get["return_type"].as_str(), Some("T"));
}

/// `baml run -e <expr>` picks up a manifest-less `baml_src/` project's
/// definitions (the `has_explicit_project` marker now accepts `baml_src/`).
#[test]
fn run_expr_without_baml_toml_picks_up_baml_src_context() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function answer() -> int {\n  42\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["run", "-e", "answer()", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit 0 for `run -e` referencing a baml_src-only project fn, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("42"),
        "Expected the result `42`, got:\n{stdout}"
    );
}

/// `baml test` reaches test discovery on a manifest-less `baml_src/`
/// project — a project with no test blocks returns the `NoTestsRun` code (5),
/// proving the loader accepted it rather than bailing on the missing manifest.
#[test]
fn test_without_baml_toml_using_baml_src_returns_no_tests_code() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);

    assert_eq!(
        output.status.code(),
        Some(5),
        "Expected NoTestsRun (5) on a manifest-less project, got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ============================================================================
// `baml run --file` script mode + shebang execution
// ============================================================================

/// Executing an actual `#!` script through the OS kernel: the implicit
/// `main` entry point runs, and arguments passed after a `--` separator at
/// the call site (`./script.baml -- alpha …`) flow into `baml.sys.argv()`.
/// (There is no bare-shebang passthrough, so the `--` is required.)
#[cfg(unix)]
#[test]
fn run_file_script_mode_passes_args_after_separator_as_argv() {
    use std::os::unix::fs::PermissionsExt;

    let built = common::ensure_built();
    let cli = built.baml_cli.display();

    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("script.baml");
    std::fs::write(
        &script,
        format!(
            "#!/usr/bin/env -S BAML_CLI_ALLOW_DIRECT=1 {cli} run --file\n\
             function main() -> string[] {{\n    baml.sys.argv()\n}}\n"
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    // The kernel turns `./script.baml -- alpha --beta gamma` into
    // `… run --file <script> -- alpha --beta gamma`.
    let output = Command::new(&script)
        .args(["--", "alpha", "--beta", "gamma"])
        .current_dir(tmp.path())
        .output()
        .expect("execute the shebang script directly");

    assert!(
        output.status.success(),
        "Expected exit 0 running a shebang script, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The tokens after `--` land in argv verbatim — including the `--beta`
    // flag, which is NOT consumed as a run-level option.
    for needle in ["alpha", "--beta", "gamma", "main"] {
        assert!(
            stdout.contains(needle),
            "Expected argv to contain `{needle}`, got:\n{stdout}"
        );
    }
}

/// A shebang can name a *specific* function to run, not just `main`: the
/// function name goes in the shebang before `--file`
/// (`#! … run greet --file`), and the kernel appends the script path as the
/// `--file` value. The named function runs; `main` does not.
#[cfg(unix)]
#[test]
fn shebang_can_name_a_specific_function() {
    use std::os::unix::fs::PermissionsExt;

    let built = common::ensure_built();
    let cli = built.baml_cli.display();

    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("multi.baml");
    // `run greet --file` selects `greet`; the kernel appends this script's
    // path right after `--file`, so it becomes the `--file` value.
    std::fs::write(
        &script,
        format!(
            "#!/usr/bin/env -S BAML_CLI_ALLOW_DIRECT=1 {cli} run greet --file\n\
             function greet() -> string {{\n    \"greetings from the named function\"\n}}\n\
             function main() -> string {{\n    \"this is main, not greet\"\n}}\n"
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let output = Command::new(&script)
        .current_dir(tmp.path())
        .output()
        .expect("execute the shebang script directly");

    assert!(
        output.status.success(),
        "Expected exit 0 dispatching a named function from a shebang, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greetings from the named function"),
        "Expected `greet` output, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("this is main"),
        "Expected `greet` to run, not `main`, got:\n{stdout}"
    );
}

/// The real thing: make a `.baml` file executable with a `#!` line pointing
/// at the built CLI, then run it directly so the OS kernel drives the
/// shebang. Proves the feature end-to-end on Unix. The `env -S` line also
/// sets `BAML_CLI_ALLOW_DIRECT=1`, exactly as the committed demo does, to
/// silence the direct-invocation advisory.
#[cfg(unix)]
#[test]
fn executable_baml_script_runs_via_kernel_shebang() {
    use std::os::unix::fs::PermissionsExt;

    let built = common::ensure_built();
    let cli = built.baml_cli.display();

    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("greet.baml");
    std::fs::write(
        &script,
        format!(
            "#!/usr/bin/env -S BAML_CLI_ALLOW_DIRECT=1 {cli} run --file\n\
             function main() -> string {{\n    \"hello from a shebang script\"\n}}\n"
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    // No arguments: bare-shebang passthrough is unsupported, so the script
    // takes none. The kernel drives `#! … run --file <this script>`.
    let output = Command::new(&script)
        .current_dir(tmp.path())
        .output()
        .expect("execute the shebang script directly");

    assert!(
        output.status.success(),
        "Expected the executable .baml to run, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello from a shebang script"),
        "Expected the script's output, got:\n{stdout}"
    );
}

/// Generators are declared in `baml.toml`'s `[generator.<name>]` sections,
/// so a manifest-less `baml_src/`-only project has nowhere to declare one:
/// `baml generate` reports the missing-generator hint rather than producing
/// output.
#[test]
fn generate_without_baml_toml_reports_no_generators() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit 4 for a manifest-less project with no generators, got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[generator"),
        "Expected a missing-generator hint, got: {stderr}",
    );
}
