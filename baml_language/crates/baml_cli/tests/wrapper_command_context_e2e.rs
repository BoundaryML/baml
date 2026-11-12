//! The packed wrapper's actual local recordings, queried in a fresh process.
//! BAML cannot inspect the persisted process outcome or span announcements itself.
mod common;

use std::{
    path::Path,
    process::{Command, Output},
};

use serde_json::{Value, json};

fn succeeded(output: &Output) {
    assert!(output.status.success(), "{output:?}");
}

fn local_command(binary: &Path, home: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env("BAML_HOME", home)
        .env("BOUNDARY_API_KEY", "local")
        .env_remove("BOUNDARY_PROJECT")
        .env_remove("BOUNDARY_API_URL")
        .env_remove("BAML_CONTEXT")
        .env("BAML_TELEMETRY", "off")
        .env("DO_NOT_TRACK", "1")
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
    command
}

#[test]
fn canonical_commands_join_local_process_outcomes_with_posthog_disabled() {
    let built = common::ensure_built();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    common::write_project(&project, "function main() -> int { 7 }\n");
    let home = temp.path().join("home/.baml");
    let binary = temp.path().join(if cfg!(windows) {
        "wrapper.exe"
    } else {
        "wrapper"
    });
    let python = Path::new(if cfg!(windows) { "python" } else { "python3" });
    succeeded(
        &local_command(python, &home)
            .arg(repo.join("baml_language/wrapper/build"))
            .arg("--cli")
            .arg(&built.baml_cli)
            .arg("--pack-host")
            .arg(&built.baml_pack_host)
            .arg("--output")
            .arg(&binary)
            .output()
            .unwrap(),
    );
    let project_path = project.to_str().unwrap();
    // The launch command intentionally conflicts with our computed label: the
    // wrapper owns this key in dispatch, while unrelated launch keys survive.
    let cases = [
        ("empty", vec![], "baml", 0, false),
        ("help", vec!["--help"], "baml", 0, false),
        (
            "help-stop",
            vec!["--help", "toolchain", "update"],
            "baml",
            0,
            false,
        ),
        ("version", vec!["--version"], "baml", 0, false),
        ("check", vec!["check"], "baml check", 0, false),
        (
            "delegated-error",
            vec!["check", "--private-marker-secret"],
            "baml check",
            2,
            false,
        ),
        (
            "global-project",
            vec!["--project", project_path, "check"],
            "baml check",
            0,
            false,
        ),
        (
            "option-value",
            vec!["--color", "never", "check"],
            "baml check",
            0,
            false,
        ),
        (
            "query",
            vec!["query", "--project", "private-marker-secret", "--help"],
            "baml query",
            0,
            false,
        ),
        ("unknown", vec!["private-marker-secret"], "baml", 2, false),
        (
            "terminator",
            vec!["--", "toolchain", "update"],
            "baml",
            2,
            false,
        ),
        (
            "toolchain-help",
            vec!["toolchain", "--help"],
            "baml toolchain",
            0,
            false,
        ),
        (
            "nested-help",
            vec!["help", "toolchain", "update"],
            "baml toolchain update",
            0,
            false,
        ),
        (
            "use-help",
            vec!["toolchain", "use", "--help"],
            "baml toolchain use",
            0,
            false,
        ),
        (
            "status",
            vec!["toolchain", "status", "--json"],
            "baml toolchain status",
            0,
            false,
        ),
        (
            "install-help",
            vec!["toolchain", "install", "--help"],
            "baml toolchain install",
            0,
            false,
        ),
        (
            "update-help",
            vec!["toolchain", "update", "--help"],
            "baml toolchain update",
            0,
            false,
        ),
        (
            "list",
            vec!["toolchain", "list"],
            "baml toolchain list",
            0,
            false,
        ),
        (
            "uninstall-help",
            vec!["toolchain", "uninstall", "--help"],
            "baml toolchain uninstall",
            0,
            false,
        ),
        ("parse-error", vec!["--project"], "baml", 1, true),
        ("selection-error", vec!["check"], "baml check", 1, true),
        (
            "project-selection-error",
            vec!["toolchain", "use", "./private-marker-secret"],
            "baml toolchain use",
            1,
            true,
        ),
    ];
    for (scenario, args, _, code, _) in &cases {
        let output = local_command(&binary, &home)
            .args(args)
            .current_dir(&project)
            .env("BAML_TELEMETRY", "medium")
            .env(
                "BAML_CONTEXT",
                if *scenario == "check" {
                    json!({"metadata": {"scenario": scenario, "command": "caller-command"}})
                } else {
                    json!({"metadata": {"scenario": scenario}})
                }
                .to_string(),
            )
            .env(
                "BAML_TOOLCHAIN",
                if *scenario == "selection-error" {
                    Path::new("invalid-version")
                } else {
                    &built.baml_cli
                },
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(*code), "{scenario}: {output:?}");
    }
    // These helper invocations must clear an inherited foreground command.
    for (scenario, args, code) in [
        ("flush-helper", vec!["__flush-wrapper-telemetry"], 0),
        ("replace-helper", vec!["--replace"], 1),
        ("recording-off", vec!["--help"], 0),
    ] {
        let output = local_command(&binary, &home)
            .args(args)
            .current_dir(&project)
            .env(
                "BAML_TELEMETRY",
                if scenario == "recording-off" {
                    "off"
                } else {
                    "medium"
                },
            )
            .env(
                "BAML_CONTEXT",
                json!({"metadata": {"scenario": scenario, "command": "caller-command"}})
                    .to_string(),
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code), "{scenario}: {output:?}");
    }
    let query = |sql: &str| {
        let output = local_command(&built.baml_cli, &home)
            .args(["query", "--local", "--format", "json", "--from"])
            .arg(temp.path().join("home"))
            .arg(sql)
            .output()
            .unwrap();
        succeeded(&output);
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["rows"].take()
    };
    // Exactly one dispatch per foreground process: no multiply-counting the
    // inherited parser/management spans. sys.exit cancels delegated dispatch,
    // so process status, not dispatch status, supplies the invocation outcome.
    let rows = query(
        "SELECT s.context_metadata['scenario'], s.context_metadata['command'],
                p.status, s.status, COUNT(*)
         FROM spans s JOIN processes p ON s.process_id = p.process_id
         WHERE s.span_name = 'user.dispatch'
         GROUP BY s.context_metadata['scenario'], s.context_metadata['command'], p.status, s.status
         ORDER BY s.context_metadata['scenario']",
    );
    let mut expected = cases
        .iter()
        .map(|(scenario, _, command, code, wrapper_error)| {
            json!([
                scenario,
                command,
                if *code == 0 { "success" } else { "error" },
                if *wrapper_error {
                    "user_error"
                } else if [
                    "empty",
                    "help",
                    "toolchain-help",
                    "nested-help",
                    "use-help",
                    "status",
                    "install-help",
                    "update-help",
                    "list",
                    "uninstall-help"
                ]
                .contains(scenario)
                {
                    "return"
                } else {
                    "cancel_error"
                },
                1
            ])
        })
        .collect::<Vec<_>>();
    expected.push(json!(["flush-helper", null, "success", "return", 1]));
    expected.push(json!(["replace-helper", null, "error", "user_error", 1]));
    expected.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
    assert_eq!(rows, json!(expected));

    for relation in ["spans", "span_announcements"] {
        assert_eq!(
            query(&format!(
                "SELECT COUNT(*) FROM {relation} WHERE span_name = 'user.WrapperInvocation.parse'"
            )),
            json!([[cases.len()]]),
            "{relation}: each foreground invocation reaches parsing, including parse failures"
        );
        assert_eq!(
            query(&format!(
                "SELECT COUNT(*) FROM {relation} WHERE span_name = 'user.dispatch'"
            )),
            json!([[cases.len() + 2]]),
            "{relation}: exactly one dispatch per recorded wrapper invocation"
        );
        assert_eq!(
            query(&format!(
                "SELECT COUNT(*) FROM {relation} s JOIN spans d ON s.process_id = d.process_id
             WHERE d.span_name = 'user.dispatch' AND s.span_name = 'user.WrapperInvocation.parse'
               AND (s.context_metadata['command'] != d.context_metadata['command']
                 OR baml_value_state(s.context_metadata['command']) != 'present')"
            )),
            json!([[0]]),
            "{relation}: parser inherits dispatch context"
        );
        assert_eq!(
            query(&format!(
                "SELECT COUNT(*) FROM {relation} WHERE span_name = 'user.dispatch'
             AND context_metadata['command'] LIKE '%private-marker-secret%'"
            )),
            json!([[0]])
        );
        assert_eq!(
            query(&format!(
                "SELECT COUNT(*) FROM {relation} WHERE span_name = 'user.dispatch'
             AND context_metadata['command'] LIKE '%--%'"
            )),
            json!([[0]])
        );
    }
    assert_eq!(
        query(
            "SELECT COUNT(*) FROM processes WHERE context_metadata['scenario'] = 'recording-off'"
        ),
        json!([[0]])
    );
    assert_eq!(
        query(
            "SELECT COUNT(*) FROM processes WHERE context_metadata['command'] = 'caller-command'"
        ),
        json!([[3]])
    );
    assert_eq!(
        query(
            "SELECT COUNT(*) FROM processes WHERE baml_value_state(context_metadata['command']) = 'present'"
        ),
        json!([[3]]),
        "dispatch context never replaces or adds process launch context"
    );
    assert!(
        !home.join("wrapper-telemetry.toml").exists(),
        "PostHog collection stayed disabled"
    );
    assert!(
        !home.join("wrapper-telemetry").exists(),
        "PostHog delivery stayed disabled"
    );
}
