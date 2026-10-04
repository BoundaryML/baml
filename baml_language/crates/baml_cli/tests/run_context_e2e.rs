//! Launch input ownership is tested with real pipes, including pipes held open.
mod common;

use std::{
    io::{Read as _, Write as _},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const CONTEXT: &str = r#"{"distinct_id":"pipe-user","metadata":{"phase":"launch"}}"#;

fn project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    common::write_project(
        project.path(),
        include_str!("fixtures/launch_context/baml_src/main.baml"),
    );
    std::fs::write(
        project.path().join("baml.toml"),
        r#"[package]
name = "run_context"
[scripts]
json = ["--function", "Check", "--", "--json-args", "-"]
dash = ["--function", "Check", "--", "--value", "-"]
"#,
    )
    .unwrap();
    project
}

fn piped_cli(project: &Path, args: &[&str], input: Option<&str>) -> Output {
    let mut child = Command::new(common::baml_cli())
        .args(args)
        .current_dir(project)
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_TELEMETRY", "high")
        .env("BAML_HOME", common::shared_baml_home())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    // With no input, retain the writer until exit so any stdin read blocks.
    if let Some(input) = input {
        stdin.write_all(input.as_bytes()).unwrap();
        drop(stdin);
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "CLI blocked with args {args:?}\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&stdout.join().unwrap()),
                String::from_utf8_lossy(&stderr.join().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

fn assert_success(output: &Output, expected: &str) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
}

#[test]
fn nominal_parameters_preserve_cli_flags_and_json_coercion() {
    let project = tempfile::tempdir().unwrap();
    common::write_project(
        project.path(),
        include_str!("fixtures/catalog_types/baml_src/main.baml"),
    );
    for target in ["EnumArg", "OptionalEnum"] {
        assert_success(
            &piped_cli(
                project.path(),
                &[
                    "run",
                    target,
                    "--output-format",
                    "json",
                    "--",
                    "--status",
                    "Active",
                ],
                None,
            ),
            r#""Active""#,
        );
    }
    assert_success(
        &piped_cli(
            project.path(),
            &[
                "run",
                "-f",
                "EnumArg",
                "--context",
                "-",
                "--output-format",
                "json",
                "--",
                "EnumArg",
                "--status",
                "Active",
            ],
            Some(CONTEXT),
        ),
        r#""Active""#,
    );
    for (target, input, expected) in [
        (
            "ClassArg",
            r#"{"customer":{"name":"Ada","status":"Active"}}"#,
            serde_json::json!({"name": "Ada", "status": "Active"}),
        ),
        (
            "Nested",
            r#"{"values":{"items":[{"value":"Inactive"}]}}"#,
            serde_json::json!({"items": [{"value": "Inactive"}]}),
        ),
        (
            "UnionArg",
            r#"{"value":"Inactive"}"#,
            serde_json::json!("Inactive"),
        ),
        (
            "RecursiveArg",
            r#"{"value":[1,[2,3]]}"#,
            serde_json::json!([1, [2, 3]]),
        ),
    ] {
        let output = piped_cli(
            project.path(),
            &[
                "run",
                target,
                "--output-format",
                "json",
                "--",
                "--json-args",
                input,
            ],
            None,
        );
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            expected,
            "{target}"
        );
    }
}

#[test]
fn context_stdin_reaches_target_and_spawn_on_cold_and_cached_runs() {
    let project = project();
    for _ in 0..2 {
        assert_success(
            &piped_cli(
                project.path(),
                &["run", "Check", "--context", "-", "--", "--value", "-"],
                Some(CONTEXT),
            ),
            r#""-""#,
        );
    }
    assert_success(
        &piped_cli(
            project.path(),
            &["run", "dash", "--context", "-"],
            Some(CONTEXT),
        ),
        r#""-""#,
    );
    let output = piped_cli(
        project.path(),
        &[
            "query",
            "--format",
            "json",
            "SELECT context_distinct_id, context_metadata['phase'] FROM processes",
        ],
        None,
    );
    assert!(output.status.success());
    let query: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        query["rows"],
        serde_json::json!([
            ["pipe-user", "launch"],
            ["pipe-user", "launch"],
            ["pipe-user", "launch"]
        ])
    );
}

#[test]
fn parsed_stdin_conflicts_exit_without_reading() {
    let project = project();
    let cases: &[&[&str]] = &[
        &["run", "-e", "-", "--context", "-"],
        &["run", "Check", "--context", "-", "--", "--json-args", "-"],
        &["run", "json", "--context", "-"],
        &[
            "run",
            "-f",
            "Check",
            "--context",
            "-",
            "--",
            "Check",
            "--json-args=-",
        ],
        &[
            "run",
            "-f",
            "Args",
            "--context",
            "-",
            "--",
            "Args",
            "--json-args=-",
        ],
    ];
    for args in cases {
        let output = piped_cli(project.path(), args, None);
        assert!(!output.status.success(), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("only one stdin consumer"),
            "{args:?}: {stderr}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn expression_and_json_stdin_work_with_inline_context() {
    let project = project();
    assert_success(
        &piped_cli(
            project.path(),
            &["run", "-e", "-", "--context", CONTEXT],
            Some("Check(\"expression\")"),
        ),
        r#""expression""#,
    );
    for args in [
        vec!["run", "json", "--context", CONTEXT],
        vec![
            "run",
            "Check",
            "--context",
            CONTEXT,
            "--",
            "--json-args",
            "-",
        ],
        vec![
            "run",
            "-f",
            "Check",
            "--context",
            CONTEXT,
            "--",
            "Check",
            "--json-args",
            "-",
        ],
    ] {
        assert_success(
            &piped_cli(project.path(), &args, Some(r#"{"value":"json"}"#)),
            r#""json""#,
        );
    }
}

#[test]
fn context_stdin_works_with_file_and_project_expressions() {
    let project = project();
    for args in [
        vec![
            "run",
            "--file",
            "baml_src/main.baml",
            "Check",
            "--context",
            "-",
            "--",
            "--value",
            "file",
        ],
        vec![
            "run",
            "--file",
            "baml_src/main.baml",
            "-e",
            "Check(\"file\")",
            "--context",
            "-",
        ],
        vec!["run", "-e", "Check(\"file\")", "--context", "-"],
    ] {
        assert_success(
            &piped_cli(project.path(), &args, Some(CONTEXT)),
            r#""file""#,
        );
    }
    let empty = tempfile::tempdir().unwrap();
    assert_success(
        &piped_cli(
            empty.path(),
            &["run", "-e", "1 + 2", "--context", "-"],
            Some(CONTEXT),
        ),
        "3",
    );
}

#[test]
fn unparsed_parameterless_arguments_are_not_stdin_consumers() {
    let project = project();
    let output = piped_cli(
        project.path(),
        &["run", "Args", "--context", "-", "--", "--json-args", "-"],
        Some(CONTEXT),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let argv: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(&argv[1..], ["Args", "--json-args", "-"]);
}

#[test]
fn listing_help_and_invalid_arguments_do_not_read_context() {
    let project = project();
    for args in [
        vec!["run", "--list", "--context", "-"],
        vec!["run", "Check", "--context", "-", "--", "--help"],
    ] {
        let output = piped_cli(project.path(), &args, None);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = piped_cli(
        project.path(),
        &["run", "Check", "--context", "-", "--", "--unknown"],
        None,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
}

#[test]
fn context_stdin_composes_with_json_and_dash_like_argument_values() {
    let project = project();
    for args in [
        vec![
            "run",
            "Check",
            "--context",
            "-",
            "--",
            "--json-args",
            r#"{"value":"--json-args=-"}"#,
        ],
        vec![
            "run",
            "Check",
            "--context",
            "-",
            "--",
            "--value=--json-args=-",
        ],
    ] {
        assert_success(
            &piped_cli(project.path(), &args, Some(CONTEXT)),
            r#""--json-args=-""#,
        );
    }
}

#[test]
fn invalid_context_stdin_fails_before_execution() {
    let project = project();
    for input in ["not json", "[]", r#"{"metadata":{"nested":{}}}"#] {
        let output = piped_cli(
            project.path(),
            &["run", "Check", "--context", "-", "--", "--value", "unused"],
            Some(input),
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--context"));
    }
}
