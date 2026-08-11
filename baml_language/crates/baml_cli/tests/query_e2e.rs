//! Q2 conformance anchors for `baml query` (09-delivery-plan §Q2 gate):
//! real artifacts from a real run, rebuild determinism, catalog rows,
//! value predicates through the canonical CAS, and typed exit codes.

mod common;

use std::path::Path;
use std::process::{Command, Output};

fn run_baml_cli(built: &Path, dir: &Path, args: &[&str]) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut cmd = Command::new(built);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(dir);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    cmd.env("BAML_HOME", &home);
    cmd.env("BAML_CACHE_DIR", common::shared_cache_dir());
    cmd.output().expect("spawn baml-cli")
}

fn create_project(dir: &Path) {
    std::fs::write(dir.join("baml.toml"), "[package]\nname = \"query-e2e\"\n").unwrap();
    let src = dir.join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function greet(name: string, age: int) -> string {\n    \"hi \" + name\n}\n\n\
         function answer() -> int {\n    greet(\"ada\", 36);\n    greet(\"bob\", 20);\n    42\n}\n",
    )
    .unwrap();
    // Installed skill keeps the passive check quiet.
    let skill = dir.join(".agents/skills/baml-core");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "---\nname: baml-core\n---\n").unwrap();
}

fn stdout_json(output: &Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!("stdout is not one JSON document: {e}\n{text}");
    })
}

#[test]
fn query_serves_catalog_rows_values_and_outcomes_from_a_real_run() {
    let built = &common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());

    let run = run_baml_cli(built, tmp.path(), &["run", "answer", "--from", "."]);
    assert!(run.status.success(), "run failed: {run:?}");

    // runs_v1: one complete run with fold-derived population totals.
    let runs = run_baml_cli(
        built,
        tmp.path(),
        &[
            "query",
            "SELECT run_id, status, entrypoint, total_calls, total_errors, \
             structure_state, value_state FROM runs_v1",
            "--format",
            "json",
        ],
    );
    assert!(runs.status.success(), "{runs:?}");
    let envelope = stdout_json(&runs);
    let rows = envelope["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["status"], "succeeded");
    assert_eq!(rows[0]["entrypoint"], "answer");
    assert_eq!(rows[0]["total_calls"], 3, "answer + two greets");
    assert_eq!(rows[0]["total_errors"], 0);
    assert_eq!(rows[0]["structure_state"], "complete");
    assert_eq!(rows[0]["value_state"], "complete");
    assert_eq!(envelope["queryOutcome"]["resultState"], "complete");
    let generation = envelope["queryOutcome"]["snapshot"]["generation"]
        .as_str()
        .unwrap()
        .to_string();

    // Rebuild determinism: there is no provider state to delete — a fresh
    // bind over the same artifacts yields the same generation and rows.
    let again = run_baml_cli(
        built,
        tmp.path(),
        &[
            "query",
            "SELECT run_id, status, entrypoint, total_calls, total_errors, \
             structure_state, value_state FROM runs_v1",
            "--format",
            "json",
        ],
    );
    let envelope_again = stdout_json(&again);
    assert_eq!(
        envelope_again["rows"], envelope["rows"],
        "rebinding the same artifacts must reproduce identical rows"
    );
    assert_eq!(
        envelope_again["queryOutcome"]["snapshot"]["generation"],
        serde_json::Value::String(generation),
        "same universe, same generation"
    );

    // Population grain via the version-pinned alias, dictionary-joined.
    let population = run_baml_cli(
        built,
        tmp.path(),
        &[
            "query",
            "SELECT fqn, definition_key, calls_started FROM cct_population \
             ORDER BY calls_started DESC",
            "--format",
            "json",
        ],
    );
    let rows = stdout_json(&population)["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["fqn"], "user.greet");
    assert_eq!(rows[0]["definition_key"], "function:user.greet");
    assert_eq!(rows[0]["calls_started"], 2);
    assert_eq!(rows[1]["fqn"], "user.answer");

    // Value predicate through the canonical CAS: named-args subscript.
    let filtered = run_baml_cli(
        built,
        tmp.path(),
        &[
            "query",
            "SELECT call_id FROM retained_calls_v1 WHERE \"return\" = baml_value_json('42')",
            "--format",
            "json",
        ],
    );
    assert!(filtered.status.success(), "{filtered:?}");
    let envelope = stdout_json(&filtered);
    assert_eq!(envelope["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        envelope["queryOutcome"]["valueEvaluations"]["unavailable"],
        0
    );

    // Frozen args-root remedy: numeric subscript is invalid SQL, exit 2.
    let invalid = run_baml_cli(
        built,
        tmp.path(),
        &[
            "query",
            "SELECT call_id FROM retained_calls_v1 WHERE args[0] = 1",
        ],
    );
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(
        stderr.contains("named-argument object"),
        "remedy present: {stderr}"
    );

    // --schema is machine-readable and versioned.
    let schema = run_baml_cli(built, tmp.path(), &["query", "--schema"]);
    assert!(schema.status.success());
    let schema = stdout_json(&schema);
    assert_eq!(schema["catalogVersion"], "v1");
    assert!(
        schema["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == "retained_calls_v1"),
    );
}
