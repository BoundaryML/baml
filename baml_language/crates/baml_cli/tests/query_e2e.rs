//! `baml query` in fresh processes over recordings made by `baml run`.
use std::{io::Write as _, path::Path, process::Command};

use serde_json::Value;

const PROGRAM: &str = r"
function Leaf(n: int) -> int { n + 1 }
function Work(n: int) -> int {
    let i = 0;
    let sum = 0;
    while (i < n) {
        sum = sum + Leaf(i);
        i = i + 1;
    }
    sum
}
function main() -> int {
    let child = spawn { Work(5) };
    Work(20) + (await child)
}
";

fn cli(project: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(args)
        .current_dir(project)
        .env("BAML_AGENT_SKILL_CHECK", "off")
        .env_remove("BAML_TELEMETRY")
        .envs(envs.iter().copied())
        .output()
        .expect("run CLI")
}

fn query(project: &Path, sql: &str) -> (i32, Value) {
    let output = cli(project, &["query", "--format", "json", sql], &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "{sql}: {e}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap_or(-1), json)
}

fn leaf_calls(json: &Value) -> i64 {
    json["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == "user.Leaf")
        .map_or(0, |row| row[1].as_i64().unwrap())
}

const STATS: &str = "SELECT fqn, SUM(call_count) FROM function_stats GROUP BY fqn";

#[test]
fn fresh_query_processes_index_once_and_then_only_new_files() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(project.path().join("baml_src/main.baml"), PROGRAM).unwrap();

    // Before anything ran: an empty answer, and nothing is created.
    let (code, empty) = query(project.path(), "SELECT COUNT(*) FROM executions");
    assert_eq!(code, 0);
    assert_eq!(empty["rows"], serde_json::json!([[0]]));
    assert_eq!(empty["outcome"]["source_missing"], true);
    assert!(!project.path().join(".baml/btel/query.sqlite").exists());

    for mode in ["medium", "high"] {
        let run = cli(
            project.path(),
            &["run", "main"],
            &[("BAML_TELEMETRY", mode)],
        );
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    let (code, first) = query(project.path(), STATS);
    assert_eq!(code, 0, "{first}");
    let refresh = &first["outcome"]["refresh"];
    assert_eq!(
        refresh["schema_rebuilt"], true,
        "the first query creates the index"
    );
    let files = refresh["files_decoded"].as_u64().unwrap();
    assert!(files >= 2);
    assert_eq!(leaf_calls(&first), 2 * 25);
    assert_eq!(first["outcome"]["unsealed"], false);

    // Each `baml run` shut down normally: its recording ended after every run
    // settled, so every recorded clock is final.
    let (code, ended) = query(
        project.path(),
        "SELECT COUNT(*), SUM(state = 'sealed'), SUM(terminal_sequence = indexed_sequence)
         FROM recordings",
    );
    assert_eq!(code, 0, "{ended}");
    assert_eq!(ended["rows"], serde_json::json!([[2, 2, 2]]));
    let (_, clocks) = query(
        project.path(),
        "SELECT COUNT(*) > 0, COUNT(*) = SUM(is_final) FROM clocks",
    );
    assert_eq!(clocks["rows"], serde_json::json!([[1, 1]]));

    for _ in 0..2 {
        let (_, again) = query(project.path(), STATS);
        let refresh = &again["outcome"]["refresh"];
        assert_eq!(refresh["files_decoded"], 0, "{refresh}");
        assert_eq!(refresh["files_unchanged"].as_u64(), Some(files));
        assert_eq!(leaf_calls(&again), 2 * 25);
    }

    let run = cli(project.path(), &["run", "main"], &[]);
    assert!(run.status.success());
    let (_, grown) = query(project.path(), STATS);
    let refresh = &grown["outcome"]["refresh"];
    assert_eq!(refresh["files_unchanged"].as_u64(), Some(files));
    assert_eq!(refresh["files_applied"], refresh["files_decoded"]);
    assert_eq!(
        leaf_calls(&grown),
        3 * 25,
        "new evidence counted exactly once"
    );

    // Population outcomes count timing-only and retained calls once each.
    let (code, outcomes) = query(
        project.path(),
        "SELECT SUM(completed_calls), SUM(ok_calls), SUM(errored_calls),
           SUM(cancelled_calls), MIN(outcome_state), MAX(outcome_state)
         FROM function_stats WHERE fqn = 'user.Leaf'",
    );
    assert_eq!(code, 0);
    assert_eq!(
        outcomes["rows"],
        serde_json::json!([[75, 75, 0, 0, "recorded", "recorded"]])
    );
    assert_eq!(outcomes["outcome"]["query"]["cas_loads"], 0);

    // Retained calls exist only for the high-mode run; values render as NULL
    // because nothing captured them, and that is not unavailability.
    let (code, calls) = query(
        project.path(),
        "SELECT fqn, output['x'] FROM calls WHERE fqn = 'user.Work' AND execution_id IS NOT NULL",
    );
    assert_eq!(code, 0);
    assert_eq!(calls["rows"].as_array().unwrap().len(), 2);
    assert_eq!(calls["outcome"]["status"], "complete");
}

#[test]
fn schema_invalid_sql_and_stdin() {
    let project = tempfile::tempdir().unwrap();
    let schema = cli(
        project.path(),
        &["query", "--schema", "--table", "calls"],
        &[],
    );
    assert!(schema.status.success());
    let text = String::from_utf8_lossy(&schema.stdout);
    assert!(
        text.contains("args") && text.contains("baml_value"),
        "{text}"
    );

    let invalid = cli(project.path(), &["query", "DELETE FROM calls"], &[]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("read-only"));

    let mut child = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["query", "--format", "jsonl", "-"])
        .current_dir(project.path())
        .env("BAML_AGENT_SKILL_CHECK", "off")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"SELECT COUNT(*) AS n FROM calls")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let lines: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap()["n"], 0);
    assert!(serde_json::from_str::<Value>(lines[1]).unwrap()["outcome"].is_object());
}
