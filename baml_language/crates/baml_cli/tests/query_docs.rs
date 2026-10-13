//! Follow the published query guide with the real CLI and real local recordings.
use std::{path::Path, process::Command};

use baml_query_btel::catalog;
use serde_json::{Value, json};

fn cli(project: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    cmd.args(["--agent-skill-check", "off", "--color", "never"])
        .current_dir(project)
        .env("BAML_HOME", project.join("home"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DO_NOT_TRACK", "1")
        .env("BOUNDARY_API_KEY", "local")
        .env_remove("BOUNDARY_PROJECT")
        .env_remove("BOUNDARY_API_URL")
        .env_remove("BAML_TELEMETRY");
    cmd
}

fn run(project: &Path, args: &[&str]) -> String {
    let output = cli(project).args(args).output().unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn json_output(project: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&run(project, args)).unwrap()
}

#[test]
fn catalog_discovery_is_offline_and_matches_query_schema() {
    let project = tempfile::tempdir().unwrap();
    // Documentation remains available even when the user's project cannot load.
    std::fs::write(project.path().join("baml.toml"), "[invalid").unwrap();
    let schema = json_output(project.path(), &["query", "--schema", "--format", "json"]);
    let described = json_output(project.path(), &["describe", "query.tables", "--json"]);
    assert_eq!(described["catalog"], "local");
    assert_eq!(described["tables"], schema);
    for table in catalog::RELATIONS {
        let topic = format!("query.tables.{}", table.name);
        let described = json_output(project.path(), &["describe", &topic, "--json"]);
        assert_eq!(described["table"], json!(table));
        let rendered = run(project.path(), &["describe", &topic]);
        assert!(rendered.contains("not live cloud metadata"));
        for column in table.columns {
            assert!(rendered.contains(column.name));
        }
    }
    let output = cli(project.path())
        .args(["describe", "query.cloud"])
        .env("BOUNDARY_API_KEY", "invalid-credential")
        .env("BOUNDARY_API_URL", "not even a URL")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!project.path().join(".baml").exists());
    let unknown = cli(project.path())
        .args(["describe", "query.tables.not_a_table"])
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("baml describe query.tables"));
}

#[test]
fn search_finds_guides_and_generated_table_documentation() {
    let project = tempfile::tempdir().unwrap();
    for (term, expected) in [
        ("query cloud", "query.cloud"),
        ("input_args", "query.tables.spans.input_args"),
        ("profiler", "query.tables.profiler"),
    ] {
        let hits = json_output(project.path(), &["describe", "--search", term, "--json"]);
        let hits = hits.as_array().unwrap();
        assert!(hits.iter().any(|hit| hit["path"] == expected), "{hits:?}");
        for hit in hits.iter().filter(|hit| hit["kind"] == "topic") {
            run(project.path(), &["describe", hit["path"].as_str().unwrap()]);
        }
    }
    let hits = json_output(
        project.path(),
        &[
            "describe",
            "--search",
            "query cloud",
            "--limit",
            "1",
            "--json",
        ],
    );
    assert_eq!(hits.as_array().unwrap().len(), 1);
}

const PROGRAM: &str = r#"
class Customer { age int }
class Item { name string }
class Answer { items Item[] }
function Inspect(customer: Customer) -> Answer {
    Answer { items: [Item { name: "sample" }] }
}
function main() -> int {
    Inspect(Customer { age: 42 }, $trace = trace.span(inputs = true, output = true));
    0
}
"#;

#[test]
fn published_local_examples_execute_over_real_recordings() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("baml_src")).unwrap();
    std::fs::write(project.path().join("baml_src/main.baml"), PROGRAM).unwrap();
    run(project.path(), &["run", "main"]);

    let mut examples = 0;
    for (topic, doc) in baml_builtins2::language_topics() {
        if topic != "query" && !topic.starts_with("query.") {
            continue;
        }
        for line in doc.syntax.as_deref().unwrap_or_default().lines() {
            // The guide spells each executable query as flags followed by one quoted SQL
            // argument. Read those arguments directly rather than maintaining copied SQL.
            let Some(command) = line.trim().strip_prefix("baml query ") else {
                continue;
            };
            let Some((flags, quoted)) = command.split_once('"') else {
                continue;
            };
            if !flags.split_whitespace().any(|flag| flag == "--local") {
                continue;
            }
            let sql = quoted.strip_suffix('"').expect("SQL is the last argument");
            let flags: Vec<_> = flags
                .split_whitespace()
                .map(|flag| {
                    if flag == "/path/to/project" {
                        project.path().to_str().unwrap()
                    } else {
                        flag
                    }
                })
                .collect();
            let mut cmd = cli(project.path());
            cmd.arg("query").args(flags).arg(sql);
            let output = cmd.output().unwrap();
            assert!(output.status.success(), "{topic}: {line}\n{output:?}");
            examples += 1;
        }
    }
    assert!(examples >= 10, "guide examples were not exercised");
    let values = json_output(
        project.path(),
        &[
            "query",
            "--local",
            "--format",
            "json",
            "SELECT input_args['customer']['age'], output_value['items'][0]['name'] FROM spans WHERE span_name = 'user.Inspect'",
        ],
    );
    assert_eq!(values["rows"], json!([[42, "sample"]]));
    // Every documented column must actually bind in the local engine.
    for table in catalog::RELATIONS {
        let columns = table
            .columns
            .iter()
            .map(|column| column.name)
            .collect::<Vec<_>>()
            .join(", ");
        run(
            project.path(),
            &[
                "query",
                "--local",
                &format!("SELECT {columns} FROM {} LIMIT 0", table.name),
            ],
        );
    }
}
