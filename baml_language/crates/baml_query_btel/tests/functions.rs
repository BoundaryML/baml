//! The `functions` relation: the definitions a recording carried, per
//! source revision.
mod support;

use bex_engine::{BexCallArg, BexExternalValue, TelemetryRecording};
use btel_recorder::RecordingConfig;
use serde_json::{Value, json};
use support::*;

const SOURCE: &str = include_str!("../../baml_tests/baml_src/ns_trace_logs/logs.baml");

/// One recording of `log_sequence` that carries `sources`.
async fn record_with(project: &std::path::Path, sources: Vec<(String, String)>) {
    let results = record_program_into(
        TelemetryRecording::local_files(project, RecordingConfig::default()).with_sources(sources),
        SOURCE,
        &[],
        vec![(
            "log_sequence",
            vec![BexCallArg::Provided(Box::new(BexExternalValue::String(
                "long-name".into(),
            )))],
        )],
    )
    .await;
    assert_eq!(results, vec![Ok(BexExternalValue::Int(14))]);
}

fn sources(extra: &str) -> Vec<(String, String)> {
    vec![
        ("test.baml".to_owned(), SOURCE.to_owned()),
        ("lib/extra.baml".to_owned(), extra.to_owned()),
    ]
}

fn is_content_id(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_without_recorded_sources_has_a_null_revision() {
    let project = tempfile::tempdir().unwrap();
    record_with(project.path(), Vec::new()).await;
    let mut index = index(project.path());
    assert_eq!(
        sql(
            &mut index,
            "SELECT baml_source_code_content_id, baml_source_code_content_id IS NULL
             FROM processes"
        )
        .rows,
        vec![vec![Value::Null, json!(1)]]
    );
    // An unrecorded id is NULL, never the empty string.
    assert_eq!(
        sql(
            &mut index,
            "SELECT COUNT(*) FROM processes WHERE baml_source_code_content_id = ''"
        )
        .rows,
        vec![vec![json!(0)]]
    );
    // Its definitions are still listed, under no revision.
    assert_eq!(
        sql(
            &mut index,
            "SELECT COUNT(*), COUNT(baml_source_code_content_id) FROM functions
             WHERE function_name = 'user.span_log'"
        )
        .rows,
        vec![vec![json!(1), json!(0)]]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn functions_list_each_definition_once_per_source_revision() {
    let project = tempfile::tempdir().unwrap();
    // Two recordings of one revision, one of another, and two without
    // sources, which list each function once.
    record_with(project.path(), sources("// one\n")).await;
    record_with(project.path(), sources("// one\n")).await;
    record_with(project.path(), sources("// two\n")).await;
    record_with(project.path(), Vec::new()).await;
    record_with(project.path(), Vec::new()).await;
    let mut index = index(project.path());

    let revisions = sql(
        &mut index,
        "SELECT baml_source_code_content_id, function_name, display_name, kind, origin, package,
           namespace, source_file, source_start, source_end
         FROM functions WHERE function_name = 'user.span_log'
         ORDER BY baml_source_code_content_id",
    );
    assert_eq!(revisions.rows.len(), 3, "{:?}", revisions.rows);
    assert_eq!(revisions.rows[0][0], Value::Null);
    assert!(is_content_id(&revisions.rows[1][0]));
    assert!(is_content_id(&revisions.rows[2][0]));
    assert_ne!(revisions.rows[1][0], revisions.rows[2][0]);
    for row in &revisions.rows {
        assert_eq!(
            row[1..8],
            [
                json!("user.span_log"),
                json!("span_log"),
                json!("bytecode"),
                json!("user"),
                json!("user"),
                Value::Null,
                json!("test.baml"),
            ]
        );
        // Half-open byte offsets of the definition in its file.
        let (start, end) = (row[8].as_u64().unwrap(), row[9].as_u64().unwrap());
        let text = &SOURCE[usize::try_from(start).unwrap()..usize::try_from(end).unwrap()];
        assert!(text.starts_with("function span_log()"), "{text}");
        assert!(text.ends_with('}'), "{text}");
    }

    // A revision's id is the one `processes` reports for its sources, so a
    // definition's text can be cut from the recorded file.
    let located = sql(
        &mut index,
        "SELECT f.source_file, f.source_start, f.source_end
         FROM functions f
         JOIN processes p ON p.baml_source_code_content_id = f.baml_source_code_content_id
         WHERE f.function_name = 'user.hidden_log'",
    );
    assert_eq!(located.rows.len(), 1, "{:?}", located.rows);
    let row = &located.rows[0];
    assert_eq!(row[0], json!("test.baml"));
    let (start, end) = (row[1].as_u64().unwrap(), row[2].as_u64().unwrap());
    assert!(
        SOURCE[usize::try_from(start).unwrap()..usize::try_from(end).unwrap()]
            .starts_with("function hidden_log()"),
        "{row:?}"
    );

    // One row per revision and name; only definitions the recording resolved.
    let shape = sql(
        &mut index,
        "SELECT COUNT(*), COUNT(function_name),
           COUNT(DISTINCT COALESCE(baml_source_code_content_id, '') || ' ' || function_name)
         FROM functions",
    );
    let total = shape.rows[0][0].as_i64().unwrap();
    assert!(total >= 9, "{:?}", shape.rows);
    assert_eq!(
        shape.rows[0],
        vec![json!(total), json!(total), json!(total)]
    );
    // Builtins a recording referenced are definitions too, with their origin.
    assert_eq!(
        sql(
            &mut index,
            "SELECT DISTINCT origin, package FROM functions WHERE function_name = 'assert.equal'"
        )
        .rows,
        vec![vec![json!("builtin"), json!("assert")]]
    );
}
