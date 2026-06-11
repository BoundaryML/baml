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
}

#[test]
fn generate_python_pydantic_language_naming_convention_returns_diagnostic() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"generator target {
  output_type "python/pydantic"
  output_dir "../"
  default_client_mode "sync"
  naming_convention "language"
}

function main() -> string {
  "hello"
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for unsupported python/pydantic naming_convention, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(output.status.code(), Some(4));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid value `language` for `naming_convention`"),
        "Expected structured invalid-value diagnostic, got: {stderr}",
    );
    assert!(
        stderr.contains("expected \"preserve-case\""),
        "Expected diagnostic to list only the supported value, got: {stderr}",
    );
    assert!(
        !stderr.contains("panicked at"),
        "Diagnostic path should not panic, got: {stderr}",
    );
}

#[test]
fn generate_python_pydantic_missing_naming_convention_lists_supported_value() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"generator target {
  output_type "python/pydantic"
  output_dir "../"
}

function main() -> string {
  "hello"
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for missing naming_convention, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(output.status.code(), Some(4));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .contains("missing required property `naming_convention` (expected \"preserve-case\")"),
        "Expected pydantic-specific naming_convention guidance, got: {stderr}",
    );
    assert!(
        !stderr.contains("\"language\""),
        "python/pydantic guidance should not advertise unsupported `language`, got: {stderr}",
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
  key: Key
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
  return account.as<PublicIdentity>.key
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
  key: Key
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
  function find(self) -> Self.Record
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
  function get(self) -> Self.Item
}

function get_public_key(account: AccountRecord) -> (AccountRecord as PublicIdentity).Key {
  return account.as<PublicIdentity>.key
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
        "read_item<T extends BoxLike>(box: T) -> T.Item",
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
    assert_eq!(read_item["return_type"].as_str(), Some("T.Item"));

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

/// `baml generate` runs a generator on a manifest-less `baml_src/` project.
#[test]
fn generate_without_baml_toml_using_baml_src_succeeds() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n\n\
         generator py {\n  output_type \"python/pydantic\"\n  output_dir \"..\"\n  \
         naming_convention \"preserve-case\"\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected exit 0 for `baml generate` on a baml_src-only project, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
