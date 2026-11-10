//! The output contract every command shares: stdout carries the result and
//! nothing else, so `--output-format json` always yields exactly one JSON
//! document, and status, logs and diagnostics stay on stderr.

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
fn json_mode_writes_exactly_one_document_for_every_command() {
    let dir = project(SOURCE);
    let dir = dir.path();
    for args in [
        &["check"][..],
        &["fmt", "--check"],
        &["fmt", "--dry-run"],
        &["fmt"],
        &["test"],
        &["test", "--list"],
        &["run", "main"],
        &["run", "--list"],
        &["generate"],
        &["describe"],
        &["describe", "main"],
        &["describe", "baml"],
        &["describe", "match"],
        &["describe", "--search", "read a file"],
        &["describe", "no_such_symbol"],
        &["codequality"],
        &["codequality", "--functions"],
        &["local", "recordings", "schema"],
        &["local", "recordings", "query", "SELECT COUNT(*) FROM spans"],
        &["local", "recordings", "query", "SELECT nonsense FROM spans"],
        &["query", "SELECT COUNT(*) FROM spans"],
        &["telemetry"],
        &["help", "check"],
    ] {
        json(dir, args);
    }

    let scratch = tempfile::tempdir().unwrap();
    json(scratch.path(), &["new", "created"]);
    json(scratch.path(), &["init", "initialized"]);
    json(
        scratch.path(),
        &["agent", "install", "--project-dir", "created"],
    );
}

/// A failing command still reports what it found.
#[test]
fn json_mode_reports_failures_as_json() {
    let dir = project("function main() -> Missing {\n    1\n}\n");
    let (output, report) = json(dir.path(), &["check"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(report["errors"].as_u64().unwrap() >= 1, "{report}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Missing"),
        "diagnostics stay on stderr: {output:?}"
    );
}

/// Commands that only act print nothing to stdout in text mode.
#[test]
fn action_commands_are_silent_on_stdout_in_text_mode() {
    let dir = project(SOURCE);
    for args in [&["check"][..], &["fmt"], &["generate"]] {
        let output = baml(dir.path(), args);
        assert!(output.status.success(), "{output:?}");
        assert!(
            output.stdout.is_empty(),
            "`baml {}` wrote to stdout: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout)
        );
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
