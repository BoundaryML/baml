//! The `logs` relation over a real recording of `log.*` calls: one row per
//! event, with its level, name, value, call site, context and the span it
//! ran under.
mod support;

use std::collections::{BTreeMap, BTreeSet};

use bex_engine::{BexCallArg, BexExternalValue, TelemetryRecording};
use btel_recorder::RecordingConfig;
use serde_json::{Value, json};
use support::*;

const SOURCE: &str = include_str!("../../baml_tests/baml_src/ns_trace_logs/logs.baml");

/// One recording of `log_sequence`, which logs 14 events.
async fn record_logs(project: &std::path::Path) {
    let results = record_program_into(
        TelemetryRecording::local_files(project, RecordingConfig::default()),
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

/// The one-based line of the only source line containing `text`.
fn line_of(text: &str) -> i64 {
    let mut lines = SOURCE
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(text));
    let (index, _) = lines.next().unwrap_or_else(|| panic!("no line has {text}"));
    assert!(lines.next().is_none(), "{text} is on several lines");
    i64::try_from(index).unwrap() + 1
}

fn single(index: &mut baml_query_btel::Index, text: &str) -> Value {
    let result = sql(index, text);
    assert_eq!(result.rows.len(), 1, "{text}: {:?}", result.rows);
    assert_eq!(result.rows[0].len(), 1, "{text}");
    result.rows[0][0].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_carry_level_name_value_site_and_context() {
    let project = tempfile::tempdir().unwrap();
    record_logs(project.path()).await;
    let mut index = index(project.path());

    let rows = sql(
        &mut index,
        "SELECT event_name, level, data, function_name, source_file, line,
           context_distinct_id, context_metadata['phase'], log_id, process_id, timestamp
         FROM logs",
    );
    assert_eq!(rows.rows.len(), 14);
    // Named logs by name; the two unnamed ones by level.
    let by_name: BTreeMap<String, &Vec<Value>> = rows
        .rows
        .iter()
        .map(|row| {
            let name = row[0]
                .as_str()
                .map_or_else(|| format!("<{}>", row[1].as_str().unwrap()), str::to_owned);
            (name, row)
        })
        .collect();
    assert_eq!(by_name.len(), 14, "{:?}", by_name.keys());

    // Level, value, the function whose code logged, and the call's line.
    let lambda = by_name["mutable"][3].as_str().unwrap();
    assert!(lambda.contains("log_sequence"), "{lambda}");
    for (name, level, data, function, line) in [
        (
            "mutable",
            "info",
            json!([1, 2]),
            None,
            "event_name = \"mutable\"",
        ),
        ("<debug>", "debug", json!(null), None, "log.debug(null)"),
        (
            "<warn>",
            "warn",
            json!([null, true, 1.5]),
            None,
            "log.warn([null, true, 1.5]",
        ),
        (
            "",
            "error",
            json!("empty name"),
            None,
            "log.error(\"empty name\"",
        ),
        (
            "enum",
            "info",
            json!("First"),
            None,
            "event_name = \"enum\"",
        ),
        (
            "hidden",
            "info",
            json!("hidden"),
            Some("user.hidden_log"),
            "event_name = \"hidden\"",
        ),
        (
            "span",
            "info",
            json!("span"),
            Some("user.span_log"),
            "event_name = \"span\"",
        ),
        (
            "ordered",
            "info",
            json!(99),
            None,
            "event_name = name_first(order)",
        ),
        (
            "long-name",
            "info",
            json!("long"),
            None,
            "event_name = long_name)",
        ),
        ("last", "debug", json!(7), None, "event_name = \"last\""),
    ] {
        let row = by_name[name];
        assert_eq!(row[1], json!(level), "{name}");
        assert_eq!(row[2], data, "{name}");
        if let Some(function) = function {
            assert_eq!(row[3], json!(function), "{name}");
        }
        assert_eq!(row[4], json!("test.baml"), "{name}");
        assert_eq!(row[5], json!(line_of(line)), "{name}");
    }
    // A call spread over several lines is placed where it starts.
    assert_eq!(
        by_name["structured"][5].as_i64().unwrap() + 1,
        line_of("\"event_name\": \"payload\"")
    );
    // A hidden call is not a span, but its code is still what logged.
    assert_ne!(by_name["hidden"][3], by_name["last"][3]);

    // The section's context: one identity, and the metadata in effect.
    for (name, row) in &by_name {
        let phase = match name.as_str() {
            "mutable" | "structured" => "before",
            "hidden" => "hidden",
            _ => "after",
        };
        assert_eq!(row[6], json!("log-user"), "{name}");
        assert_eq!(row[7], json!(phase), "{name}");
    }

    // Ids are unique, name their recording file, and share the process.
    let process = single(&mut index, "SELECT process_id FROM processes");
    let ids: BTreeSet<&str> = rows
        .rows
        .iter()
        .map(|row| row[8].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 14);
    for row in &rows.rows {
        let (recording, order) = row[8].as_str().unwrap().split_once(':').unwrap();
        assert_eq!(recording.len(), 32, "{row:?}");
        let (file, position) = order.split_once('.').unwrap();
        assert!(file.parse::<u64>().unwrap() >= 1, "{row:?}");
        position.parse::<u32>().unwrap();
        assert_eq!(row[9], process);
        let timestamp = row[10].as_str().unwrap();
        assert!(
            timestamp.starts_with("20") && timestamp.ends_with('Z'),
            "{timestamp}"
        );
    }

    // An unnamed log is NULL; an empty name is a name.
    assert_eq!(
        sql(
            &mut index,
            "SELECT level FROM logs WHERE event_name IS NULL ORDER BY level"
        )
        .rows,
        vec![vec![json!("debug")], vec![json!("warn")]]
    );
    assert_eq!(
        single(&mut index, "SELECT level FROM logs WHERE event_name = ''"),
        json!("error")
    );
    // The value navigates like any captured value; a logged null is a value.
    assert_eq!(
        sql(
            &mut index,
            "SELECT data['data'], data['level'], baml_kind(data) FROM logs
             WHERE event_name = 'structured'"
        )
        .rows,
        vec![vec![json!(42), json!("user"), json!("json")]]
    );
    assert_eq!(
        single(
            &mut index,
            "SELECT baml_value_state(data) FROM logs WHERE level = 'debug' AND event_name IS NULL"
        ),
        json!("null")
    );
    assert_eq!(
        single(
            &mut index,
            "SELECT data['value'] FROM logs WHERE event_name = 'class'"
        ),
        json!(7)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_link_to_the_span_and_future_they_ran_under() {
    let project = tempfile::tempdir().unwrap();
    record_logs(project.path()).await;
    let mut index = index(project.path());

    // Every log's span is a row of `spans`: the function span when one
    // encloses it, otherwise the future it ran on.
    let linked = sql(
        &mut index,
        "SELECT l.event_name, s.span_type, s.span_name, l.span_id = l.future_id
         FROM logs l JOIN spans s ON s.span_id = l.span_id",
    );
    assert_eq!(linked.rows.len(), 14);
    for row in &linked.rows {
        match row[0].as_str() {
            Some("span") => {
                assert_eq!(
                    row[1..],
                    [json!("function"), json!("user.span_log"), json!(0)]
                );
            }
            Some("spawned") => {
                assert_eq!(row[1], json!("future"));
                assert_ne!(row[2], json!("user.log_sequence"));
                assert_eq!(row[3], json!(1));
            }
            _ => assert_eq!(
                row[1..],
                [json!("future"), json!("user.log_sequence"), json!(1)],
                "{row:?}"
            ),
        }
    }
    // The spawned future logged on its own future; the span's log on its caller's.
    let root = single(
        &mut index,
        "SELECT span_id FROM spans WHERE span_type = 'future' AND span_name = 'user.log_sequence'",
    );
    let futures = sql(
        &mut index,
        "SELECT event_name, future_id FROM logs WHERE event_name IN ('span', 'spawned')
         ORDER BY event_name",
    );
    assert_eq!(futures.rows[0], vec![json!("span"), root.clone()]);
    assert_ne!(futures.rows[1][1], root);

    // Filtering by span finds a span's logs, through the index on it.
    let span = single(
        &mut index,
        "SELECT span_id FROM spans WHERE span_name = 'user.span_log'",
    );
    let span = span.as_str().unwrap();
    assert_eq!(
        single(
            &mut index,
            &format!("SELECT event_name FROM logs WHERE span_id = '{span}'")
        ),
        json!("span")
    );
    assert_eq!(
        single(
            &mut index,
            &format!(
                "SELECT COUNT(*) FROM logs WHERE span_id IN ('{span}', '{}')",
                root.as_str().unwrap()
            )
        ),
        json!(13)
    );
    let plan: Vec<String> = index
        .connection()
        .prepare(
            "EXPLAIN QUERY PLAN SELECT * FROM logs
             WHERE __span_id_rec = 1 AND __span_id_key = x'0000000000000001'",
        )
        .unwrap()
        .query_map([], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("log_by_parent")),
        "{plan:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_recording_takes_its_logs_and_their_contexts() {
    let project = tempfile::tempdir().unwrap();
    record_logs(project.path()).await;
    let mut index = index(project.path());
    assert_eq!(single(&mut index, "SELECT COUNT(*) FROM logs"), json!(14));
    let count = |index: &baml_query_btel::Index, table: &str| -> i64 {
        index
            .connection()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    assert_eq!(count(&index, "log"), 14);
    // Three contexts: before, after and the hidden call's.
    assert!(count(&index, "context_snapshot") >= 3);

    let recordings = index.layout().recordings.clone();
    for entry in std::fs::read_dir(&recordings).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            std::fs::remove_dir_all(entry.path()).unwrap();
        }
    }
    assert_eq!(single(&mut index, "SELECT COUNT(*) FROM logs"), json!(0));
    assert_eq!(count(&index, "log"), 0);
    assert_eq!(count(&index, "context_snapshot"), 0);
}
