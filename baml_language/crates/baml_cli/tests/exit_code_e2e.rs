// End-to-end tests for CLI exit codes on compilation errors.
//
// These tests verify that `baml-cli` commands return non-zero exit codes
// when compilation errors occur. This is critical for CI/CD pipelines and
// scripting use cases where the exit code is used to determine success or
// failure.

mod common;

use std::{
    io::{BufRead as _, BufReader, Read as _},
    path::Path,
    process::{Command, Output, Stdio},
};

// ============================================================================
// Helpers
// ============================================================================

/// Run a baml-cli command and return the output (stdout, stderr, exit code).
///
/// `BAML_HOME` and `HOME` are pointed inside the project so tests never read
/// developer state.
///
/// Tests here take the CLI from `common::baml_cli()`, never `ensure_built()`:
/// nothing in this suite runs `baml pack`, and `ensure_built`'s in-test
/// `cargo build -p baml_pack_host` freshness check costs ~10s per test
/// process under nextest even when fully fresh.
fn run_baml_cli(built: &Path, dir: &Path, args: &[&str]) -> Output {
    run_baml_cli_with_env(built, dir, args, &[])
}

fn run_baml_cli_with_env(built: &Path, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut cmd = Command::new(built);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(dir);
    cmd.env("HOME", dir);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    // Pin the human output preset: under a coding agent the inherited
    // CLAUDECODE/AI_AGENT/… environment flips `--output-preset auto` to
    // `agent`, which disables the progress lines some assertions read.
    cmd.env("BAML_OUTPUT_PRESET", "human");
    cmd.env("BAML_HOME", &home);
    // Tests are quiet unless they explicitly exercise the inherited log level.
    cmd.env_remove("BAML_LOG");
    cmd.envs(env.iter().copied());
    // Share the bytecode cache across the suite so only the first invocation
    // pays the stdlib compile; see `common::shared_cache_dir`.
    cmd.env("BAML_CACHE_DIR", common::shared_cache_dir());
    cmd.output().expect("spawn baml-cli")
}

fn gofmt_is_available() -> bool {
    Command::new("gofmt").arg("-h").output().is_ok()
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
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
fn generate_reports_identifier_renames_in_normal_verbose_and_quiet_modes() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        "enum Choice {\n  None\n}\n\nfunction pick() -> Choice { Choice.None }\n",
    );
    std::fs::write(
        tmp.path().join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.py]\n\
         output_type = \"python/pydantic\"\n\
         output_dir = \"generated\"\n\
         naming_convention = \"preserve-case\"\n",
    )
    .unwrap();

    let normal = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert!(
        normal.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&normal.stderr)
    );
    let normal_stderr = String::from_utf8_lossy(&normal.stderr);
    assert!(
        normal_stderr.contains("1 identifier rename →"),
        "{normal_stderr}"
    );
    assert!(
        !normal_stderr.contains("Renamed enum variant"),
        "{normal_stderr}"
    );

    let verbose = run_baml_cli(built, tmp.path(), &["generate", "--from", ".", "--verbose"]);
    assert!(
        verbose.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&verbose.stderr)
    );
    let verbose_stderr = String::from_utf8_lossy(&verbose.stderr);
    assert!(
        verbose_stderr.contains("1 identifier rename →"),
        "{verbose_stderr}"
    );
    assert!(
        verbose_stderr
            .contains("Renamed enum variant `user.Choice.None`: `None` → `None_` (Python keyword)"),
        "{verbose_stderr}"
    );

    let quiet = run_baml_cli(built, tmp.path(), &["generate", "--from", ".", "--quiet"]);
    assert!(
        quiet.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&quiet.stderr)
    );
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_stderr.contains("identifier rename"),
        "{quiet_stderr}"
    );
    assert!(
        !quiet_stderr.contains("Renamed enum variant"),
        "{quiet_stderr}"
    );
}

#[test]
fn generate_rust_language_naming_convention_returns_diagnostic() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        "function greet(name: string) -> string {\n  \"Hello, \" + name\n}\n",
    );
    std::fs::write(
        tmp.path().join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.rust_client]\n\
         output_type = \"rust\"\n\
         output_dir = \".\"\n\
         naming_convention = \"language\"\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert_eq!(
        output.status.code(),
        Some(4),
        "Expected exit code 4, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|line| line.trim() == "E0019"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "generator `rust_client` with `output_type = \"rust\"` requires `naming_convention = \"preserve-case\"`"
        ),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("panicked at"), "stderr: {stderr}");
}

#[test]
fn generate_rust_default_output_stays_inside_project() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_project(
        &project,
        "function echo(value: string) -> string { value }\n",
    );
    std::fs::write(
        project.join("baml.toml"),
        "[package]\nname = \"test-project\"\n\n\
         [generator.rust]\n\
         output_type = \"rust\"\n\
         naming_convention = \"preserve-case\"\n",
    )
    .unwrap();

    let output = run_baml_cli(built, &project, &["generate", "--from", "."]);

    assert!(
        output.status.success(),
        "Rust generation failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        project.join("baml_sdk/Cargo.toml").is_file(),
        "the default Rust SDK should be generated inside the project"
    );
    assert!(
        !tmp.path().join("baml_sdk").exists(),
        "generation must not create a sibling directory outside the project"
    );
}

#[test]
fn generate_go_writes_sdk_through_cli() {
    if !gofmt_is_available() {
        return;
    }
    let built = &common::baml_cli();
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
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("baml_sdk/.gitignore")).unwrap(),
        baml_codegen_types::GENERATED_GITIGNORE
    );
}

#[test]
fn generate_go_first_run_preserves_preexisting_user_files() {
    if !gofmt_is_available() {
        return;
    }
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project_with_go_generator(
        tmp.path(),
        "function echo(value: string) -> string { value }\n",
    );
    let sdk = tmp.path().join("baml_sdk");
    std::fs::create_dir(&sdk).unwrap();
    std::fs::write(sdk.join("user-notes.txt"), "keep me").unwrap();

    let output = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);

    assert!(
        output.status.success(),
        "Go generation failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(
        std::fs::read_to_string(sdk.join("user-notes.txt")).unwrap(),
        "keep me"
    );
    assert!(sdk.join("functions.go").is_file());
}

#[test]
fn generate_go_removes_stale_owned_files_and_preserves_unknown_files() {
    if !gofmt_is_available() {
        return;
    }
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project_with_go_generator(
        tmp.path(),
        "class FormerType {\n  value: string\n}\n\nfunction echo(value: string) -> string { value }\n",
    );

    let first = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert!(
        first.status.success(),
        "initial Go generation failed:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let sdk = tmp.path().join("baml_sdk");
    assert!(sdk.join("types.go").is_file());
    std::fs::write(sdk.join("user-notes.txt"), "keep me").unwrap();

    std::fs::write(
        tmp.path().join("baml_src/main.baml"),
        "function echo(value: string) -> string { value }\n",
    )
    .unwrap();
    let second = run_baml_cli(built, tmp.path(), &["generate", "--from", "."]);
    assert!(
        second.status.success(),
        "second Go generation failed:\n{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(
        !sdk.join("types.go").exists(),
        "types.go from the removed class must not survive regeneration"
    );
    assert_eq!(
        std::fs::read_to_string(sdk.join("user-notes.txt")).unwrap(),
        "keep me"
    );
}

// ============================================================================
// Tests for `baml run` exit codes
// ============================================================================

/// Compilation errors must result in a non-zero exit code for `baml run --list`.
#[test]
fn run_list_compilation_error_returns_nonzero_exit_code() {
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(tmp.path(), "function answer() -> int {\n    42\n}\n");
    // Installed skills keep the passive skill check quiet, so stderr stays
    // exactly the program's own output.
    let skill_dir = tmp.path().join(".agents/skills/baml-core");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        common::installed_skill_content(),
    )
    .unwrap();

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

/// `baml run` keeps logs silent by default and streams the selected levels
/// before printing the target's return value when `--log` or `BAML_LOG` enables them.
#[test]
fn run_log_sources_surface_filtered_logs_for_targets_and_expressions() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
function logged() -> string {
    log.debug("debug-detail");
    log.info("info-detail");
    log.warn({"user": "ada", "attempts": [1, 2]});
    log.error("error-detail");
    "target-result"
}

class LoggedConversion {
    value string

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            log.warn("from-json-detail");
            LoggedConversion {
                value: baml.json.from_json<string>(baml.json.field(j, "value"))
            }
        }
    }

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            log.error("to-json-detail");
            self.value
        }
    }
}

function logged_conversion(input: LoggedConversion) -> LoggedConversion {
    log.info("conversion-target-detail");
    input
}
"#,
    );

    let quiet = run_baml_cli(built, tmp.path(), &["run", "logged", "--from", "."]);
    assert!(
        quiet.status.success(),
        "default run failed; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&quiet.stdout),
        String::from_utf8_lossy(&quiet.stderr),
    );
    let quiet_stdout = String::from_utf8_lossy(&quiet.stdout);
    assert!(
        quiet_stdout.contains("target-result"),
        "stdout: {quiet_stdout}"
    );
    assert!(!quiet_stdout.contains("detail"), "stdout: {quiet_stdout}");

    let info = run_baml_cli_with_env(
        built,
        tmp.path(),
        &["run", "logged", "--from", ".", "--log", "INFO"],
        &[("BAML_LOG", "ERROR")],
    );
    assert!(
        info.status.success(),
        "--log INFO run failed; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&info.stdout),
        String::from_utf8_lossy(&info.stderr),
    );
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("[INFO] info-detail"), "stdout: {stdout}");
    assert!(
        stdout.contains(r#"[WARN] {"user": "ada", "attempts": [1, 2]}"#),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("[ERROR] error-detail"), "stdout: {stdout}");
    assert!(!stdout.contains("debug-detail"), "stdout: {stdout}");
    assert!(
        stdout.find("[ERROR] error-detail") < stdout.find("target-result"),
        "captured logs must be flushed before the return value: {stdout}"
    );

    let from_env = run_baml_cli_with_env(
        built,
        tmp.path(),
        &["run", "logged", "--from", "."],
        &[("BAML_LOG", "WARN")],
    );
    assert!(
        from_env.status.success(),
        "BAML_LOG=WARN run failed; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&from_env.stdout),
        String::from_utf8_lossy(&from_env.stderr),
    );
    let stdout = String::from_utf8_lossy(&from_env.stdout);
    assert!(stdout.contains("[WARN]"), "stdout: {stdout}");
    assert!(stdout.contains("[ERROR] error-detail"), "stdout: {stdout}");
    assert!(!stdout.contains("info-detail"), "stdout: {stdout}");
    assert!(!stdout.contains("debug-detail"), "stdout: {stdout}");

    let expression = run_baml_cli(
        built,
        tmp.path(),
        &[
            "run",
            "--from",
            ".",
            "--log",
            "INFO",
            "-e",
            r#"log.info("expression-detail"); 7"#,
        ],
    );
    assert!(
        expression.status.success(),
        "logged expression failed; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&expression.stdout),
        String::from_utf8_lossy(&expression.stderr),
    );
    let stdout = String::from_utf8_lossy(&expression.stdout);
    assert!(
        stdout.contains("[INFO] expression-detail"),
        "stdout: {stdout}"
    );
    let lines: Vec<_> = stdout.lines().collect();
    let log_line = lines
        .iter()
        .position(|line| line.contains("[INFO] expression-detail"))
        .expect("expression log");
    let result_line = lines
        .iter()
        .position(|line| line.trim() == "7")
        .expect("expression return value");
    assert!(
        log_line < result_line,
        "expression logs must be flushed before the return value: {stdout}"
    );

    let conversion = run_baml_cli(
        built,
        tmp.path(),
        &[
            "run",
            "logged_conversion",
            "--from",
            ".",
            "--log",
            "INFO",
            "--output-format",
            "json",
            "--",
            "--json-args",
            r#"{"input":{"value":"hook-result"}}"#,
        ],
    );
    assert!(
        conversion.status.success(),
        "logged conversion failed; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&conversion.stdout),
        String::from_utf8_lossy(&conversion.stderr),
    );
    let stdout = String::from_utf8_lossy(&conversion.stdout);
    for expected in [
        "[WARN] from-json-detail",
        "[INFO] conversion-target-detail",
        "[ERROR] to-json-detail",
    ] {
        assert!(stdout.contains(expected), "stdout: {stdout}");
    }
    let result_pos = stdout.find(r#""hook-result""#).expect("serialized result");
    assert!(
        stdout.find("[ERROR] to-json-detail") < Some(result_pos),
        "conversion logs must be flushed before the serialized result: {stdout}"
    );
}

#[test]
fn run_expression_serialization_failure_returns_target_error() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
class BrokenConversion {
    value int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            throw baml.json.JsonSerializationError {
                message: "serialize-boom",
                path: "",
                reason: "serialize-boom"
            }
        }
    }
}
"#,
    );

    let output = run_baml_cli(
        built,
        tmp.path(),
        &[
            "run",
            "--from",
            ".",
            "--output-format",
            "json",
            "-e",
            "BrokenConversion { value: 1 }",
        ],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("failed to serialize output"), "{stderr}");
    assert!(stderr.contains("serialize-boom"), "{stderr}");
}

/// The formatter advisory is the allowed `baml run` stderr exception.
#[test]
fn run_unformatted_project_keeps_format_warning() {
    let built = &common::baml_cli();
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
        stderr.contains("code is unformatted"),
        "Expected format warning, got:\n{stderr}"
    );
    common::assert_no_compile_file_status(&stderr);
}

// ============================================================================
// Tests for `baml test` exit codes
// ============================================================================

/// The no-project diagnostic must recommend the public source-path option.
#[test]
fn test_no_project_error_recommends_project() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    let output = run_baml_cli(built, tmp.path(), &["test"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "unexpected success: {stderr}");
    assert!(
        stderr.contains("--project <DIR>"),
        "unexpected error: {stderr}"
    );
    assert!(
        !stderr.contains("--file <PATH>"),
        "unexpected error: {stderr}"
    );
}

/// Compilation errors must result in a non-zero exit code for `baml test`.
#[test]
fn test_compilation_error_returns_nonzero_exit_code() {
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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

/// The `--project <DIR>` invocation recommended by project discovery accepts
/// an explicit source directory outside a marked project.
#[test]
fn test_accepts_explicit_source_directory_as_project() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    let source_dir = tmp.path().join("sources");
    std::fs::create_dir(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("standalone.baml"),
        r#"
function add(a: int, b: int) -> int { a + b }

test "adds" {
  assert.equal(add(2, 3), 5)
}
"#,
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["test", "--project", "sources"]);

    assert!(
        output.status.success(),
        "Expected explicit source directory to succeed, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("1 passed, 0 failed, 1 total"),
        "Expected passing test report, got stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn test_unhandled_spawn_error_uses_host_default() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
function bad() -> int throws string { throw "boom" }

test "passes" {
  spawn { bad() };
  baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
  assert.is_true(true)
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected TestFailure (2), got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("FAIL testing::unhandled_spawn_error"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

/// A selector that matches no test must NOT print a green aggregate pass
/// line: the aggregate of zero tests is a vacuous pass, but stdout that says
/// PASS while the command exits 5 (`NoTestsRun`) misleads anything parsing it.
/// Regression for B-628.
#[test]
fn test_no_match_selector_does_not_print_pass() {
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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

/// BAML log events stay silent by default and become stdout lines only when
/// the caller opts into a level threshold with `--log` or `BAML_LOG`.
#[test]
fn test_log_sources_route_filtered_baml_logs_to_stdout_without_changing_exit_codes() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
test "logs" {
  log.debug("debug-detail");
  log.info("info-detail");
  log.warn("warn-detail");
  log.warn({"user": "ada", "attempts": [1, 2]});
  log.error("error-detail");
  assert.is_true(true)
}

test "fails" {
  log.error("failure-detail");
  assert.is_true(false)
}
"#,
    );

    let quiet = run_baml_cli(built, tmp.path(), &["test", "--from", ".", "-i", "::logs"]);
    assert!(
        quiet.status.success(),
        "expected default log mode to pass; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&quiet.stdout),
        String::from_utf8_lossy(&quiet.stderr),
    );
    let quiet_stdout = String::from_utf8_lossy(&quiet.stdout);
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(quiet_stdout.contains("PASS"), "stdout: {quiet_stdout}");
    assert!(
        format!("{quiet_stdout}{quiet_stderr}").contains("1 passed, 0 failed, 1 total"),
        "stdout: {quiet_stdout}\nstderr: {quiet_stderr}"
    );
    assert!(!quiet_stdout.contains("detail"), "stdout: {quiet_stdout}");

    // Uppercase is intentional: this is the documented shell spelling and
    // guards clap's case-insensitive value parsing.
    let info = run_baml_cli_with_env(
        built,
        tmp.path(),
        &["test", "--from", ".", "-i", "::logs"],
        &[("BAML_LOG", "INFO")],
    );
    assert!(
        info.status.success(),
        "expected BAML_LOG=INFO to pass; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&info.stdout),
        String::from_utf8_lossy(&info.stderr),
    );
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("[INFO] info-detail"), "stdout: {stdout}");
    assert!(stdout.contains("[WARN] warn-detail"), "stdout: {stdout}");
    assert!(
        stdout.contains(r#"[WARN] {"user": "ada", "attempts": [1, 2]}"#),
        "stdout: {stdout}"
    );
    for implementation_detail in ["BamlOutboundValue", "MapValue", "ListValue", "Some("] {
        assert!(
            !stdout.contains(implementation_detail),
            "stdout leaked `{implementation_detail}`: {stdout}"
        );
    }
    assert!(stdout.contains("[ERROR] error-detail"), "stdout: {stdout}");
    assert!(!stdout.contains("debug-detail"), "stdout: {stdout}");
    assert!(
        stdout.find("[ERROR] error-detail") < stdout.find("PASS"),
        "the final captured log must be printed before the test report: {stdout}"
    );

    let failure = run_baml_cli(
        built,
        tmp.path(),
        &["test", "--from", ".", "-i", "::fails", "--log", "ERROR"],
    );
    assert_eq!(
        failure.status.code(),
        Some(2),
        "--log must preserve the test-failure exit code; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&failure.stdout),
        String::from_utf8_lossy(&failure.stderr),
    );
    assert!(
        String::from_utf8_lossy(&failure.stdout).contains("[ERROR] failure-detail"),
        "stdout: {}",
        String::from_utf8_lossy(&failure.stdout),
    );
}

/// A redirected stdout stream is explicitly flushed while a test is still
/// running, rather than releasing its logs only with the final report.
#[test]
fn test_logs_flag_flushes_stdout_during_long_running_test() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
test "streams" {
  log.info("stream-start");
  baml.sys.sleep(baml.time.Duration.from_milliseconds(750n));
  log.info("stream-end");
  assert.is_true(true)
}
"#,
    );

    // Warm the compile/discovery cache without executing the sleeping test.
    let listed = run_baml_cli(built, tmp.path(), &["test", "--from", ".", "--list"]);
    assert!(
        listed.status.success(),
        "failed to prepare streaming test: {}",
        String::from_utf8_lossy(&listed.stderr),
    );

    let home = tmp.path().join(".baml-home");
    let mut child = Command::new(built)
        .args(["test", "--from", ".", "--log", "INFO"])
        .current_dir(tmp.path())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        // Pin the human preset so inherited agent env (CLAUDECODE/AI_AGENT/…)
        // cannot flip `--output-preset auto` to `agent` and hide progress lines.
        .env("BAML_OUTPUT_PRESET", "human")
        .env("BAML_HOME", &home)
        .env("BAML_CACHE_DIR", common::shared_cache_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn baml-cli with piped stdout");

    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout is piped"));
    let mut captured = String::new();
    loop {
        let mut line = String::new();
        let read = stdout.read_line(&mut line).expect("read streamed log line");
        assert_ne!(read, 0, "stdout ended before the first log: {captured}");
        captured.push_str(&line);
        if line.contains("[INFO] stream-start") {
            break;
        }
    }
    assert!(
        child.try_wait().expect("query child status").is_none(),
        "the first log was buffered until the test process exited: {captured}"
    );

    stdout
        .read_to_string(&mut captured)
        .expect("read remaining stdout");
    let output = child.wait_with_output().expect("wait for baml-cli");
    assert!(
        output.status.success(),
        "streaming test failed; stdout: {captured}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(captured.contains("[INFO] stream-end"), "stdout: {captured}");
    assert!(
        captured.find("[INFO] stream-end") < captured.find("PASS"),
        "the final log must be flushed before the test report: {captured}"
    );
}

/// Failing `assert.equal` should surface both operand values and keep stack
/// traces user-facing (no internal `Span`/`FileId` debug structs).
#[test]
fn test_assert_equal_failure_shows_values_without_internal_span_debug() {
    let built = &common::baml_cli();
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

/// Non-assertion errors should retain their type, fields, and BAML stack
/// context instead of producing a bare `FAIL` line.
#[test]
fn test_thrown_error_prints_rendered_error_context() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
class ProviderFailure {
  message string
  status int
}

function fail_request() -> void {
  throw ProviderFailure {
    message: "provider rejected request",
    status: 429,
  }
}

test "provider-failure" {
  fail_request()
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected test failure exit code for thrown error, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("user.ProviderFailure"),
        "Expected the thrown error type in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains(r#"message: "provider rejected request""#),
        "Expected the thrown error message in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("status: 429"),
        "Expected the thrown error fields in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("main.baml"),
        "Expected BAML source context in stderr, got: {stderr}",
    );
    assert!(
        !stderr.contains("Span {") && !stderr.contains("FileId("),
        "User-facing test output should not include internal source debug data: {stderr}",
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
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
        combined.contains("AGGREGATE PASS [outcome=pass; 1 tolerated failure]"),
        "Expected unfiltered aggregate output to identify tolerated failures, got:\n{combined}"
    );
    assert!(combined.contains("PASS root::suite::one"), "{combined}");
    assert!(
        combined.contains("TOLERATED root::suite::three"),
        "{combined}"
    );
    assert!(
        combined.contains("aggregate passed — 2 passed, 1 tolerated failure, 3 total"),
        "Expected unfiltered aggregate summary to report tolerated leaf totals, got:\n{combined}"
    );
}

#[test]
fn test_filtered_testset_run_honors_pass_rate_runner_for_selected_set() {
    let built = &common::baml_cli();
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

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["test", "--from", ".", "-i", "root::suite::*"],
    );
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
        combined.contains("AGGREGATE PASS [outcome=pass; 1 tolerated failure]"),
        "Expected filtered aggregate output to identify tolerated failures, got:\n{combined}"
    );
    assert!(combined.contains("PASS root::suite::two"), "{combined}");
    assert!(
        combined.contains("TOLERATED root::suite::three"),
        "{combined}"
    );
    assert!(
        combined.contains("aggregate passed — 2 passed, 1 tolerated failure, 3 total"),
        "Expected filtered aggregate summary to report selected leaf totals, got:\n{combined}"
    );
}

#[test]
fn test_filtered_testset_leaf_runs_under_parent_runner() {
    let built = &common::baml_cli();
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
        &["test", "--from", ".", "-i", "root::suite::failing leaf"],
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
        combined.contains("AGGREGATE PASS [outcome=pass; 1 tolerated failure]"),
        "Expected filtered leaf output to identify tolerated failures, got:\n{combined}"
    );
    assert!(
        combined.contains("TOLERATED root::suite::failing leaf"),
        "{combined}"
    );
    assert!(
        combined.contains("aggregate passed — 0 passed, 1 tolerated failure, 1 total"),
        "Expected filtered leaf output to report selected leaf totals, got:\n{combined}"
    );
}

#[test]
fn test_mixed_testset_run_keeps_tolerated_failures_out_of_failed_total() {
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
        stdout.contains("FAIL root::suite::two"),
        "Expected output to include the canonical failed child ID, got:\n{stdout}"
    );
}

#[test]
fn test_unfiltered_testset_run_fails_when_aggregate_outcome_fails() {
    let built = &common::baml_cli();
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

#[test]
fn test_fail_fast_does_not_report_skipped_leaf_as_passed() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
testset "suite" with testing.FailFast() {
  test "first fails" { assert.is_true(false) }
  test "never runs" { assert.is_true(true) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(2), "{combined}");
    assert!(
        combined.contains("FAIL root::suite::first fails"),
        "{combined}"
    );
    assert!(
        !combined.contains("PASS root::suite::never runs"),
        "{combined}"
    );
    assert!(
        combined.contains("0 passed, 1 failed, 1 total"),
        "{combined}"
    );
}

#[test]
fn test_legacy_custom_runner_does_not_invent_identity_for_skipped_leaf() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    create_project(
        tmp.path(),
        r#"
function FirstOnlyWithoutNames(children: testing.TestSetChild[]) -> testing.TestSetReport {
  let report = testing.Sequential()([children[0]])
  testing.TestSetReport {
    outcome: report.outcome,
    passed: report.passed,
    failed: report.failed,
    total: report.total,
    failed_names: report.failed_names,
    results: report.results,
  }
}

testset "suite" with FirstOnlyWithoutNames {
  test "first runs" { assert.is_true(true) }
  test "never runs" { assert.is_true(true) }
}
"#,
    );

    let output = run_baml_cli(built, tmp.path(), &["test", "--from", "."]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{combined}");
    assert!(
        !combined.contains("PASS root::suite::first runs"),
        "{combined}"
    );
    assert!(
        !combined.contains("PASS root::suite::never runs"),
        "{combined}"
    );
    assert!(
        combined.contains("1 passed, 0 failed, 1 total"),
        "{combined}"
    );
}

// ============================================================================
// Tests for project-less introspection (`baml fmt` without a `baml.toml`).
// The most expensive thing an agent can do is fail fast and burn a turn, so
// these read-only commands fall back to a no-op / stdlib-only "default state"
// instead of erroring.
// ============================================================================

/// `baml fmt` with no explicit source and no discoverable project is a no-op
/// success. An explicit `--from` is different: it opts into that source tree.
#[test]
fn fmt_without_from_or_project_is_noop_success() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();

    let output = run_baml_cli(built, tmp.path(), &["fmt"]);

    assert!(
        output.status.success(),
        "Expected exit 0 for `baml fmt` with no project, got: {:?}\nstderr: {}",
        output.status.code(),
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
    let built = &common::baml_cli();
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

/// An explicit source directory needs neither `baml.toml` nor a `baml_src/`
/// wrapper: `--from` itself is the opt-in to load that tree.
#[test]
fn run_list_accepts_explicit_unmarked_source_root() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    // A loose .baml at the root, with no baml.toml and no baml_src/.
    std::fs::write(
        tmp.path().join("loose.baml"),
        "function f() -> int {\n  1\n}\n",
    )
    .unwrap();

    let output = run_baml_cli(built, tmp.path(), &["run", "--list", "--from", "."]);

    assert!(
        output.status.success(),
        "Expected explicit unmarked source root to load, got {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains('f'),
        "Expected function list to contain `f`, got:\n{}",
        String::from_utf8_lossy(&output.stdout),
    );
}

/// `baml run <fn>` actually *executes* a function in a manifest-less
/// `baml_src/` project — the headline of this change, proven end-to-end
/// (not just `--list`). `answer` is pure (no LLM), so it runs hermetically.
#[test]
fn run_execute_function_without_baml_toml_succeeds() {
    let built = &common::baml_cli();
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

/// Associated type projections that resolve to concrete value types must still
/// produce stdout through `baml run`. This catches a real boundary bug where the
/// VM metadata erased `(Class as Interface).Assoc` to `void`; dispatch treats
/// `void` as "do not print", so a value-returning function silently produced no
/// output.
#[test]
fn run_prints_concrete_associated_type_projection_return() {
    let built = &common::baml_cli();
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
    let built = &common::baml_cli();
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
    // Interface-machinery bodies (impl-block methods, interface defaults) are
    // anonymous at runtime: they are not runnable entries, so the listing must
    // not offer them. Asserted on the bare `find(` fragment so the pin holds
    // whatever display spelling a leaked body would carry (`find` names
    // nothing else in this fixture).
    assert!(
        !stdout.contains("find("),
        "Impl-block method bodies must not be listed as runnable entries:\n{stdout}"
    );
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
    let built = &common::baml_cli();
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

/// B-359: an expression that only needs the standard library must not compile
/// or diagnose the surrounding project. Unrelated project errors should not
/// block `-e` from being used as an interactive probe.
#[test]
fn run_expr_ignores_unrelated_project_compile_errors() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project(
        tmp.path(),
        "function broken() -> int {\n  Int.parse(\"1\")\n}\n",
    );

    let output = run_baml_cli(
        built,
        tmp.path(),
        &["run", "-e", "int.parse(\"42\")", "--from", "."],
    );

    assert!(
        output.status.success(),
        "Expected an independent expression to ignore unrelated project errors, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unresolved name: Int"),
        "Unrelated project diagnostic leaked into expression evaluation:\n{stderr}"
    );
}

/// `baml test` reaches test discovery on a manifest-less `baml_src/`
/// project — a project with no test blocks returns the `NoTestsRun` code (5),
/// proving the loader accepted it rather than bailing on the missing manifest.
#[test]
fn test_without_baml_toml_using_baml_src_returns_no_tests_code() {
    let built = &common::baml_cli();
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

    let built = &common::baml_cli();
    let cli = built.display();

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
        .env("BAML_CACHE_DIR", common::shared_cache_dir())
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

    let built = &common::baml_cli();
    let cli = built.display();

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
        .env("BAML_CACHE_DIR", common::shared_cache_dir())
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

    let built = &common::baml_cli();
    let cli = built.display();

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
        .env("BAML_CACHE_DIR", common::shared_cache_dir())
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
    let built = &common::baml_cli();
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
