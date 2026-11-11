//! `root_span_id`: the root future a span, a span announcement or a log
//! event runs under, however many spawns it is from that root.
mod support;

use bex_engine::BexExternalValue;
use serde_json::{Value, json};
use support::*;

const SOURCE: &str = r#"
function Work(n: int) -> int {
    log.info("work", event_name = "work");
    n + 1
}

function nested(n: int) -> int {
    let grandchild = spawn { Work(n) };
    await grandchild
}

function main(n: int) -> int {
    log.info("root", event_name = "root");
    let direct = Work(n);
    let child = spawn { Work(n) + nested(n) };
    direct + (await child)
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_row_of_a_run_names_its_root_future() {
    let project = tempfile::tempdir().unwrap();
    // Two runs in one process: each root call is a root future.
    let results = record_program(
        project.path(),
        SOURCE,
        &["Work"],
        &[("main", 1), ("main", 5)],
    )
    .await;
    assert_eq!(
        results,
        vec![Ok(BexExternalValue::Int(6)), Ok(BexExternalValue::Int(18))]
    );
    let mut index = index(project.path());
    let roots = sql(
        &mut index,
        "SELECT span_id, root_span_id FROM spans
         WHERE span_type = 'future' AND parent_span_id IS NULL AND span_name = 'user.main'
         ORDER BY start_time",
    )
    .rows;
    assert_eq!(roots.len(), 2, "{roots:?}");
    assert_ne!(roots[0][0], roots[1][0]);
    for root in &roots {
        // A root future names itself.
        assert!(root[0].is_string());
        assert_eq!(root[0], root[1]);
        let id = root[0].as_str().unwrap();
        for relation in ["spans", "span_announcements"] {
            // The root, the future it spawned and the future that one
            // spawned; `Work` on each of the three.
            let run = sql(
                &mut index,
                &format!(
                    "SELECT span_type, span_name, future_id = root_span_id AS on_root
                     FROM {relation} WHERE root_span_id = '{id}'
                     ORDER BY 1, 3"
                ),
            )
            .rows;
            let kinds: Vec<(&str, i64)> = run
                .iter()
                .map(|row| (row[0].as_str().unwrap(), row[2].as_i64().unwrap_or(-1)))
                .collect();
            assert_eq!(
                kinds,
                vec![
                    // `Work` in the grandchild, in the child and in the root.
                    ("function", 0),
                    ("function", 0),
                    ("function", 1),
                    // The root has no future of its own, the grandchild was
                    // spawned from the child, and the child from the root.
                    ("future", -1),
                    ("future", 0),
                    ("future", 1),
                ],
                "{relation}: {run:?}"
            );
            assert!(
                run.iter()
                    .filter(|row| row[0] == json!("function"))
                    .all(|row| row[1] == json!("user.Work")),
                "{run:?}"
            );
        }
        // The root's own log and the three `Work` logs, whichever future
        // logged them.
        let logs = sql(
            &mut index,
            &format!(
                "SELECT event_name, COUNT(*), COUNT(DISTINCT future_id) FROM logs
                 WHERE root_span_id = '{id}' GROUP BY event_name ORDER BY event_name"
            ),
        )
        .rows;
        assert_eq!(
            logs,
            vec![
                vec![json!("root"), json!(1), json!(1)],
                vec![json!("work"), json!(3), json!(3)],
            ]
        );
    }
    // No row of either run is left without a root, and none is shared.
    for (relation, column) in [
        ("spans", "span_id"),
        ("span_announcements", "span_id"),
        ("logs", "log_id"),
    ] {
        let grouped = sql(
            &mut index,
            &format!(
                "SELECT root_span_id, COUNT(DISTINCT {column}) FROM {relation}
                 WHERE root_span_id IN ('{}', '{}') OR root_span_id IS NULL
                 GROUP BY root_span_id ORDER BY 2 DESC",
                roots[0][0].as_str().unwrap(),
                roots[1][0].as_str().unwrap()
            ),
        )
        .rows;
        assert_eq!(grouped.len(), 2, "{relation}: {grouped:?}");
        assert_eq!(grouped[0][1], grouped[1][1], "{relation}");
        assert!(
            grouped.iter().all(|row| row[0] != Value::Null),
            "{relation}"
        );
    }
}
