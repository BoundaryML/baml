//! The output contract every command shares: stdout carries the result and
//! nothing else. A command with a result prints exactly one JSON document
//! under `--output-format json`; a command with none prints nothing in any
//! format. Status, logs and diagnostics stay on stderr.

mod common;

use std::{
    path::Path,
    process::{Command, Output},
};

fn baml(dir: &Path, args: &[&str]) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    common::share_build_cache(&home);
    Command::new(common::baml_cli())
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env("BAML_HOME", &home)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BOUNDARY_API_KEY", "local")
        .env("DO_NOT_TRACK", "1")
        .env_remove("BAML_LOG")
        .output()
        .expect("spawn baml-cli")
}

fn project(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("baml.toml"),
        "[package]\nname = \"contract\"\n\n\
         [generator.py]\n\
         output_type = \"python/pydantic\"\n\
         output_dir = \"generated\"\n\
         naming_convention = \"preserve-case\"\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("baml_src")).unwrap();
    std::fs::write(dir.path().join("baml_src/main.baml"), source).unwrap();
    dir
}

const SOURCE: &str = "function main() -> string {\n    \"hello\"\n}\n\n\
test \"logs\" {\n    log.info(\"noise\");\n    assert.equal(main(), \"hello\");\n}\n";

fn json(dir: &Path, args: &[&str]) -> (Output, serde_json::Value) {
    let mut argv = args.to_vec();
    argv.extend(["--output-format", "json"]);
    let output = baml(dir, &argv);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!(
            "`baml {}` stdout is not one JSON document: {error}\nstdout: {stdout}\nstderr: {}",
            argv.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output, value)
}

#[test]
fn json_mode_writes_exactly_one_document_for_every_result() {
    let dir = project(SOURCE);
    let dir = dir.path();
    for args in [
        &["fmt", "--check"][..],
        &["fmt", "--dry-run"],
        &["test"],
        &["test", "--list"],
        &["run", "main"],
        &["run", "--list"],
        &["describe"],
        &["describe", "main"],
        &["describe", "baml"],
        &["describe", "match"],
        &["describe", "--search", "read a file"],
        &["codequality"],
        &["codequality", "--functions"],
        &["local", "recordings", "schema"],
        &["local", "recordings", "query", "SELECT COUNT(*) FROM spans"],
        &["query", "SELECT COUNT(*) FROM spans"],
        &["telemetry"],
        &["help", "check"],
    ] {
        json(dir, args);
    }

    let scratch = tempfile::tempdir().unwrap();
    assert!(baml(scratch.path(), &["new", "created"]).status.success());
    json(
        scratch.path(),
        &["agent", "install", "--project-dir", "created"],
    );
}

/// `baml run` logs share stdout with the program as text, but a structured
/// stdout holds only the program's output and the returned value.
#[test]
fn run_logs_stay_off_a_structured_stdout() {
    let dir = project("function main() -> string {\n    log.info(\"noise\");\n    \"hello\"\n}\n");
    let (output, value) = json(dir.path(), &["run", "main"]);
    assert_eq!(value, "hello", "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[INFO] noise"),
        "{output:?}"
    );

    let text = baml(dir.path(), &["run", "main"]);
    assert!(
        String::from_utf8_lossy(&text.stdout).contains("[INFO] noise"),
        "{text:?}"
    );
}

/// A help path that names no command is a usage error.
#[test]
fn help_for_an_unknown_command_is_a_usage_error() {
    let dir = project(SOURCE);
    let output = baml(dir.path(), &["help", "cloud", "auth", "missing"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

/// A failing command that has a result still reports it.
#[test]
fn json_mode_reports_failures_as_json() {
    let dir = project("test \"fails\" {\n    assert.is_true(false)\n}\n");
    let (output, report) = json(dir.path(), &["test"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(report["summary"]["failed"], 1, "{report}");
}

/// Runs `args` as text and as JSON and requires an empty stdout from both.
fn assert_silent(dir: &Path, args: &[&str]) -> Output {
    let mut last = None;
    for format in ["text", "json"] {
        let mut argv = args.to_vec();
        argv.extend(["--output-format", format]);
        let output = baml(dir, &argv);
        assert!(
            output.stdout.is_empty(),
            "`baml {}` wrote to stdout: {}",
            argv.join(" "),
            String::from_utf8_lossy(&output.stdout)
        );
        last = Some(output);
    }
    last.unwrap()
}

/// A command that prints nothing as text prints nothing as JSON either.
#[test]
fn commands_without_a_result_are_silent_in_every_format() {
    let dir = project(SOURCE);
    for args in [&["check"][..], &["fmt"], &["generate"]] {
        let output = assert_silent(dir.path(), args);
        assert!(output.status.success(), "{output:?}");
    }

    // Scaffolding refuses an existing destination, so each format gets its own.
    let scratch = tempfile::tempdir().unwrap();
    for (format, new, init) in [
        ("text", "created_text", "initialized_text"),
        ("json", "created_json", "initialized_json"),
    ] {
        for args in [["new", new], ["init", init]] {
            let output = baml(
                scratch.path(),
                &[args[0], args[1], "--output-format", format],
            );
            assert!(output.status.success(), "{args:?} as {format}: {output:?}");
            assert!(output.stdout.is_empty(), "{args:?} as {format}: {output:?}");
        }
    }
}

/// A failure with nothing to show leaves stdout empty; the reason is on
/// stderr.
#[test]
fn failures_without_a_result_are_silent_in_every_format() {
    let broken = project("function main() -> Missing {\n    1\n}\n");
    let output = assert_silent(broken.path(), &["check"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Missing"),
        "{output:?}"
    );

    let dir = project(SOURCE);
    for args in [
        &["describe", "no_such_symbol"][..],
        &["local", "recordings", "query", "SELECT nonsense FROM spans"],
    ] {
        let output = assert_silent(dir.path(), args);
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
    }
}

/// `jsonl` is for results that are sequences; everything else rejects it as a
/// usage error instead of guessing a shape.
#[test]
fn jsonl_is_only_for_sequences() {
    let dir = project(SOURCE);
    let output = baml(dir.path(), &["test", "--output-format", "jsonl"]);
    assert!(output.status.success(), "{output:?}");
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is JSON"))
        .collect();
    assert_eq!(lines[0]["status"], "pass", "{lines:?}");
    assert_eq!(lines.last().unwrap()["summary"]["passed"], 1, "{lines:?}");

    let output = baml(dir.path(), &["check", "--output-format", "jsonl"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

/// Usage errors exit 2; a command that ran and failed exits 1.
#[test]
fn exit_codes_separate_usage_from_failure() {
    let dir = project("function main() -> Missing {\n    1\n}\n");
    assert_eq!(baml(dir.path(), &["check"]).status.code(), Some(1));
    assert_eq!(
        baml(dir.path(), &["check", "--nope"]).status.code(),
        Some(2)
    );
    assert_eq!(baml(dir.path(), &["nope"]).status.code(), Some(2));
}

/// Argument mistakes that only the command can detect exit like the ones
/// clap rejects.
#[test]
fn command_level_argument_errors_are_usage_errors() {
    let dir = project("function main() -> int {\n    1\n}\n");
    for args in [
        &["run", "--file", "x.baml", "--project-dir", "."][..],
        &["run", "main", "-e", "1"],
        &["run", "-e", "1", "--list"],
        &["pack", "main", "-f", "main"],
    ] {
        let output = baml(dir.path(), args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    }
}

/// Failing tests are an ordinary failure, and running none is not a failure.
#[test]
fn test_exit_codes_are_the_common_ones() {
    let failing = project("test \"fails\" {\n    assert.is_true(false)\n}\n");
    assert_eq!(baml(failing.path(), &["test"]).status.code(), Some(1));

    let empty = project("function main() -> int {\n    1\n}\n");
    assert_eq!(baml(empty.path(), &["test"]).status.code(), Some(0));
    assert_eq!(
        baml(empty.path(), &["test", "--list"]).status.code(),
        Some(0)
    );
}
