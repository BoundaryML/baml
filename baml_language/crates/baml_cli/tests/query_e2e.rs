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
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env_remove("BAML_TELEMETRY")
        .envs(envs.iter().copied())
        .output()
        .expect("run CLI")
}

fn query(project: &Path, sql: &str) -> (i32, Value) {
    let output = cli(project, &["query", "--local", "--format", "json", sql], &[]);
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

/// Every completed invocation per function, over each ended `baml run`.
const STATS: &str =
    "SELECT function_name, SUM(invocation_count) FROM profiler GROUP BY function_name";

#[test]
fn launch_context_reaches_processes_roots_and_futures_without_nested_overrides() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(
        project.path().join("baml_src/main.baml"),
        include_str!("../../baml_tests/baml_src/ns_trace_context/query.baml"),
    )
    .unwrap();
    let context = r#"{"distinct_id":"launch-user","metadata":{"root":"launch","count":42,"ratio":1.5,"enabled":true}}"#;
    std::fs::write(project.path().join("context.json"), context).unwrap();
    for source in [context, "@context.json"] {
        let output = cli(
            project.path(),
            &["run", "launch_context_main", "--context", source],
            &[("BAML_TELEMETRY", "high")],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "16");
    }
    let (code, processes) = query(
        project.path(),
        "SELECT context_distinct_id, context_metadata['root'], context_metadata['count'],
          context_metadata['ratio'], context_metadata['enabled'] FROM processes
         WHERE context_distinct_id = 'launch-user'",
    );
    assert_eq!(code, 0, "{processes}");
    assert_eq!(
        processes["rows"],
        serde_json::json!([
            ["launch-user", "launch", 42, 1.5, true],
            ["launch-user", "launch", 42, 1.5, true]
        ])
    );
    let (code, spans) = query(
        project.path(),
        "SELECT context_distinct_id, COUNT(*) FROM spans
         WHERE span_name IN ('user.launch_context_leaf', 'user.query_context_leaf')
         GROUP BY context_distinct_id ORDER BY context_distinct_id",
    );
    assert_eq!(code, 0, "{spans}");
    assert_eq!(
        spans["rows"],
        serde_json::json!([["launch-user", 4], ["query-user", 4]])
    );
    let (code, future) = query(
        project.path(),
        "SELECT COUNT(*) FROM spans WHERE span_type = 'future'
         AND context_distinct_id = 'launch-user' AND context_metadata['root'] = 'launch'",
    );
    assert_eq!(code, 0, "{future}");
    assert_eq!(future["rows"], serde_json::json!([[4]]));
}

#[test]
fn fresh_query_processes_index_once_and_then_only_new_files() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(project.path().join("baml_src/main.baml"), PROGRAM).unwrap();

    // Before anything ran: an empty answer, and nothing is created.
    let (code, empty) = query(project.path(), "SELECT COUNT(*) FROM spans");
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

    // Each `baml run` is its own process, which said how it ended and which
    // sources it ran.
    let (code, ended) = query(
        project.path(),
        "SELECT COUNT(*), SUM(status = 'success'), SUM(host = 'baml'),
           SUM(baml_source_code_content_id IS NOT NULL), SUM(command LIKE '% run main')
         FROM processes",
    );
    assert_eq!(code, 0, "{ended}");
    assert_eq!(ended["rows"], serde_json::json!([[2, 2, 2, 2, 2]]));

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

    // The profiler counts timing-only and retained calls once each.
    let (code, outcomes) = query(
        project.path(),
        "SELECT SUM(invocation_count), SUM(return_count), SUM(error_count),
           SUM(panic_error_count), SUM(future_cancel_count), SUM(missing_count)
         FROM profiler WHERE function_name = 'user.Leaf'",
    );
    assert_eq!(code, 0);
    assert_eq!(outcomes["rows"], serde_json::json!([[75, 75, 0, 0, 0, 0]]));
    assert_eq!(outcomes["outcome"]["query"]["cas_loads"], 0);

    // Retained calls exist only for the high-mode run; values render as NULL
    // because nothing captured them, and that is not unavailability.
    let (code, calls) = query(
        project.path(),
        "SELECT span_name, output_value['x'] FROM spans WHERE span_name = 'user.Work'",
    );
    assert_eq!(code, 0);
    assert_eq!(calls["rows"].as_array().unwrap().len(), 2);
    assert_eq!(calls["outcome"]["status"], "complete");
}

const LOGGING: &str = r#"function helper(n: int) -> int {
    log.warn({ "n": n }, event_name = "helper");
    n + 1
}

function main() -> int {
    log.info("start");
    helper(2)
}
"#;

#[test]
fn logs_functions_and_sources_of_a_run_are_queryable() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(project.path().join("baml_src/main.baml"), LOGGING).unwrap();
    let run = cli(project.path(), &["run", "main"], &[]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    // Each log names the function and line that logged, and the span (here
    // the root future) it ran under.
    let (code, logs) = query(
        project.path(),
        "SELECT l.level, l.event_name, l.data, l.function_name, l.source_file, l.line, s.span_type
         FROM logs l JOIN spans s ON s.span_id = l.span_id ORDER BY l.line",
    );
    assert_eq!(code, 0, "{logs}");
    assert_eq!(
        logs["rows"],
        serde_json::json!([
            ["warn", "helper", {"n": 2}, "user.helper", "baml_src/main.baml", 2, "future"],
            ["info", null, "start", "user.main", "baml_src/main.baml", 7, "future"],
        ])
    );

    // The process recorded its sources, keyed as `source_file` names them.
    let (code, source) = query(
        project.path(),
        "SELECT p.baml_source_code['baml_src/main.baml'], f.source_start, f.source_end
         FROM functions f
         JOIN processes p ON p.baml_source_code_content_id = f.baml_source_code_content_id
         WHERE f.function_name = 'user.helper' AND f.source_file = 'baml_src/main.baml'",
    );
    assert_eq!(code, 0, "{source}");
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{source}");
    assert_eq!(rows[0][0], LOGGING);
    let (start, end) = (
        usize::try_from(rows[0][1].as_u64().unwrap()).unwrap(),
        usize::try_from(rows[0][2].as_u64().unwrap()).unwrap(),
    );
    assert!(LOGGING[start..end].starts_with("function helper("));
    assert!(LOGGING[start..end].ends_with('}'));

    // Every profiler row names its process's source revision, so one
    // revision's code paths are read without joining `processes`.
    let (code, revisions) = query(
        project.path(),
        "SELECT COUNT(*), COUNT(n.baml_source_code_content_id),
           SUM(n.baml_source_code_content_id = p.baml_source_code_content_id)
         FROM profiler n JOIN processes p ON p.process_id = n.process_id",
    );
    assert_eq!(code, 0, "{revisions}");
    let row = &revisions["rows"][0];
    assert!(row[0].as_u64().unwrap() > 0, "{revisions}");
    assert_eq!((&row[1], &row[2]), (&row[0], &row[0]), "{revisions}");
    let profiler = cli(
        project.path(),
        &["query", "--schema", "--table", "profiler"],
        &[],
    );
    let text = String::from_utf8_lossy(&profiler.stdout);
    let columns: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("  ")?.split_whitespace().next())
        .collect();
    assert_eq!(
        columns[1..5],
        [
            "process_id",
            "process_start_time",
            "process_end_time",
            "baml_source_code_content_id"
        ],
        "{text}"
    );

    let schema = cli(project.path(), &["query", "--schema"], &[]);
    let text = String::from_utf8_lossy(&schema.stdout);
    for documented in [
        "logs: ",
        "functions: ",
        "baml_source_code ",
        "process_start_time ",
        "process_end_time ",
        // Media is described without its content, which a path reads.
        "v['base64']",
        // What a page shows for a stored value, and for a column.
        "Labels for people",
        "user_error     Failed",
        "context_distinct_id          User",
        "Showing results to people:",
    ] {
        assert!(text.contains(documented), "{documented}: {text}");
    }
}

/// A project's recorded sources hold its manifest beside its `.baml` files,
/// so a manifest change is a new source revision. A `--file` run records its
/// one file only.
#[test]
fn a_project_run_records_its_manifest_with_its_sources() {
    let manifest = |name: &str| format!("[package]\nname = \"{name}\"\n");
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(project.path().join("baml_src/main.baml"), LOGGING).unwrap();
    std::fs::write(project.path().join("script.baml"), LOGGING).unwrap();
    for (name, args) in [
        ("billing", &["run", "main"][..]),
        ("invoicing", &["run", "main"][..]),
        ("invoicing", &["run", "--file", "script.baml", "main"][..]),
    ] {
        std::fs::write(project.path().join("baml.toml"), manifest(name)).unwrap();
        let run = cli(project.path(), args, &[]);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    let (code, sources) = query(
        project.path(),
        "SELECT baml_source_code['baml.toml'], baml_source_code['baml_src/main.baml'],
           baml_source_code, baml_source_code_content_id
         FROM processes ORDER BY last_updated",
    );
    assert_eq!(code, 0, "{sources}");
    let rows = sources["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{sources}");
    let paths = |row: &Value| -> Vec<String> {
        let mut paths: Vec<String> = row[2].as_object().unwrap().keys().cloned().collect();
        paths.sort();
        paths
    };
    for (row, name) in rows.iter().zip(["billing", "invoicing"]) {
        assert_eq!(row[0], manifest(name));
        assert_eq!(row[1], LOGGING);
        assert_eq!(paths(row), ["baml.toml", "baml_src/main.baml"]);
    }
    // Only the manifest changed between the two project runs.
    assert_ne!(rows[0][3], rows[1][3]);
    assert_eq!(rows[2][0], Value::Null);
    assert_eq!(paths(&rows[2]), ["script.baml"]);
}

#[test]
fn schema_invalid_sql_and_stdin() {
    let project = tempfile::tempdir().unwrap();
    let schema = cli(
        project.path(),
        &["query", "--schema", "--table", "spans"],
        &[],
    );
    assert!(schema.status.success());
    let text = String::from_utf8_lossy(&schema.stdout);
    assert!(
        text.contains("input_args") && text.contains("baml_value"),
        "{text}"
    );

    let invalid = cli(
        project.path(),
        &["query", "--local", "DELETE FROM spans"],
        &[],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("read-only"));

    let mut child = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["query", "--local", "--format", "jsonl", "-"])
        .current_dir(project.path())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"SELECT COUNT(*) AS n FROM spans")
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
