//! End-to-end: real engine recordings → incremental index → SQL.
mod support;

use std::{collections::BTreeMap, path::Path, time::Duration};

use baml_query_btel::{QueryRequest, Status};
use btel_recorder::RecordingConfig;
use serde_json::{Value as Json, json};
use support::*;

fn rows(result: &baml_query_btel::QueryResult) -> Vec<Vec<Json>> {
    result.rows.clone()
}

const STATS_SQL: &str =
    "SELECT fqn, SUM(call_count) AS n FROM function_stats GROUP BY fqn ORDER BY n DESC";

fn stats(index: &mut baml_query_btel::Index) -> BTreeMap<String, i64> {
    stats_of(&sql(index, STATS_SQL))
}

/// Query what is already indexed, without applying new files: a test that
/// counts every applied file must see each refresh's metrics.
fn indexed_sql(index: &mut baml_query_btel::Index, text: &str) -> baml_query_btel::QueryResult {
    let request = baml_query_btel::QueryRequest {
        sql: text.into(),
        ..baml_query_btel::QueryRequest::default()
    };
    index.query(&request).expect("indexed query")
}

fn indexed_stats(index: &mut baml_query_btel::Index) -> BTreeMap<String, i64> {
    stats_of(&indexed_sql(index, STATS_SQL))
}

fn stats_of(result: &baml_query_btel::QueryResult) -> BTreeMap<String, i64> {
    result
        .rows
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap_or("?").to_owned(),
                row[1].as_i64().unwrap(),
            )
        })
        .collect()
}

fn expected_stats(mains: i64, crashes: i64) -> BTreeMap<String, i64> {
    let mut expected = BTreeMap::from([
        ("user.main".to_owned(), mains),
        ("user.Leaf".to_owned(), mains * 11),
        ("user.Extract".to_owned(), mains),
        ("user.Classify".to_owned(), mains),
        ("user.Validate".to_owned(), mains + crashes),
        ("user.Link".to_owned(), mains),
        (".<lambda(main, 0)>".to_owned(), mains),
    ]);
    if crashes > 0 {
        expected.insert("user.crash".to_owned(), crashes);
    }
    expected
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn population_outcomes_include_timing_only_calls_and_recursive_errors() {
    let project = tempfile::tempdir().unwrap();
    let source = r#"
        class Failure { code int }
        function Risky(n: int) -> int throws Failure {
            if (n % 2 == 0) { throw Failure { code: n } }
            n
        }
        function Recur(n: int) -> int {
            if (n == 0) { Risky(0) } else { Recur(n - 1) }
        }
        function main(n: int) -> int {
            let i = 0;
            let sum = 0;
            while (i < n) {
                sum = sum + (Risky(i) catch (e) { Failure => 0 });
                i = i + 1;
            }
            sum + (Recur(3) catch (e) { Failure => 0 })
        }
    "#;
    let results = record_program(project.path(), source, &[], &[("main", 5)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(4))]);
    let mut index = index(project.path());
    let result = sql(
        &mut index,
        "SELECT fqn, completed_calls, ok_calls, errored_calls, cancelled_calls, outcome_state FROM function_stats ORDER BY fqn",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![
                json!("user.Recur"),
                json!(4),
                json!(0),
                json!(4),
                json!(0),
                json!("recorded")
            ],
            vec![
                json!("user.Risky"),
                json!(6),
                json!(2),
                json!(4),
                json!(0),
                json!("recorded")
            ],
            vec![
                json!("user.main"),
                json!(1),
                json!(1),
                json!(0),
                json!(0),
                json!("recorded")
            ],
        ]
    );
    assert_eq!(result.outcome.query.values.cas_loads, 0);
    let retained = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Risky'",
    );
    assert!(
        retained.rows[0][0].as_u64().unwrap() < 6,
        "population includes timing-only successes"
    );
    let recursive = sql(
        &mut index,
        "SELECT reentry_completed_calls, errored_calls FROM call_path_stats WHERE fqn = 'user.Recur' ORDER BY call_path_id",
    );
    assert!(recursive.rows.iter().any(|r| r[0].as_u64().unwrap() > 0));
    assert_eq!(
        recursive
            .rows
            .iter()
            .map(|r| r[1].as_u64().unwrap())
            .sum::<u64>(),
        4
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acceptance_queries_answer_from_real_recordings() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob"), (1, "ann")]).await;
    let mut index = index(project.path());

    let recordings = sql(
        &mut index,
        "SELECT recording_id, indexed_sequence, state, terminal_sequence FROM recordings",
    );
    assert_eq!(recordings.rows.len(), 1);
    assert_eq!(
        recordings.rows[0][2],
        json!("sealed"),
        "normal shutdown writes the end marker"
    );
    assert_eq!(recordings.rows[0][3], recordings.rows[0][1]);
    assert!(!recordings.outcome.unsealed);
    assert_eq!(recordings.outcome.status, Status::Complete);
    let clocks = sql(
        &mut index,
        "SELECT COUNT(*), SUM(is_final), SUM(timing_state = 'valid') FROM clocks",
    );
    let counts = &clocks.rows[0];
    assert!(counts[0].as_u64().unwrap() > 0);
    assert_eq!(counts[1], counts[0], "every recorded run settled");
    assert_eq!(counts[2], counts[0]);

    let executions = sql(
        &mut index,
        "SELECT entry_fqn, status, timing_state, threads, retained_calls, duration_ns > 0
         FROM executions ORDER BY started_at",
    );
    assert_eq!(
        rows(&executions),
        vec![
            vec![
                json!("user.main"),
                json!("ok"),
                json!("valid"),
                json!(2),
                json!(4),
                json!(1)
            ],
            vec![
                json!("user.main"),
                json!("ok"),
                json!("valid"),
                json!(2),
                json!(4),
                json!(1)
            ],
            vec![
                json!("user.main"),
                json!("ok"),
                json!("valid"),
                json!(2),
                json!(4),
                json!(1)
            ],
            vec![
                json!("user.crash"),
                json!("errored"),
                json!("valid"),
                json!(1),
                json!(1),
                json!(1)
            ],
        ]
    );
    assert_eq!(stats(&mut index), expected_stats(3, 1));

    let names = sql(
        &mut index,
        "SELECT call_id, output['items'][0]['name'] AS name FROM calls
         WHERE status = 'ok' AND fqn = 'user.Extract' ORDER BY started_at",
    );
    assert_eq!(
        column(&names, "name"),
        vec![&json!("ann-item"), &json!("bob-item"), &json!("ann-item")]
    );
    assert_eq!(names.columns[1].column_type, "baml_value");

    let adults = sql(
        &mut index,
        "SELECT c.fqn FROM calls c WHERE c.args['customer']['age'] >= 25 ORDER BY c.started_at",
    );
    assert_eq!(
        column(&adults, "fqn"),
        vec![
            &json!("user.Extract"),
            &json!("user.Classify"),
            &json!("user.Validate")
        ]
    );

    // Named inputs, enum and int outputs, captured errors, missing paths.
    let typed = sql(
        &mut index,
        "SELECT fqn, args['customer']['name'] AS who, args['count'] AS count,
           output AS result, error['reason'] AS reason, output['nope'] AS missing
         FROM calls WHERE args['customer']['name'] = 'bob' ORDER BY started_at",
    );
    assert_eq!(column(&typed, "who"), vec![&json!("bob"); 3]);
    assert_eq!(column(&typed, "count")[0], &json!(8));
    let results = column(&typed, "result");
    assert_eq!(results[0]["$class"], json!("Order"));
    assert_eq!(results[0]["items"].as_array().map(Vec::len), Some(8));
    assert_eq!(results[1], &json!("Premium"));
    assert_eq!(results[2], &json!(27));
    assert_eq!(column(&typed, "missing"), vec![&Json::Null; 3]);
    assert_eq!(
        typed.outcome.status,
        Status::Complete,
        "missing paths are not unavailability"
    );
    let errors = sql(
        &mut index,
        "SELECT error['code'] AS code, error['reason'] AS reason FROM calls WHERE status = 'errored'",
    );
    assert_eq!(errors.rows.len(), 3);
    assert!(
        errors
            .rows
            .iter()
            .all(|r| r[0] == json!(42) && r[1] == json!("too young"))
    );

    // Aliases, a CTE, and a name reused across scopes.
    let cte = sql(
        &mut index,
        "WITH x AS (SELECT c.call_id AS id, c.args AS a, c.fqn FROM calls c WHERE c.fqn = 'user.Extract')
         SELECT a['customer']['name'] AS name, a['count'] AS n FROM x
         WHERE EXISTS (SELECT 1 FROM calls x WHERE x.output = 'Premium')
         ORDER BY n DESC",
    );
    assert_eq!(
        rows(&cte),
        vec![
            vec![json!("bob"), json!(8)],
            vec![json!("ann"), json!(2)],
            vec![json!("ann"), json!(2)]
        ]
    );
    // Comparisons do not coerce: the text '27' is not the int 27.
    let coercion = sql(
        &mut index,
        "SELECT COUNT(*) AS n, SUM(output = 27) AS as_int, SUM(output = '27') AS as_text
         FROM calls WHERE fqn = 'user.Validate' AND status = 'ok'",
    );
    assert_eq!(rows(&coercion), vec![vec![json!(1), json!(1), json!(0)]]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metadata_queries_read_no_cas_and_repeated_paths_share_hydration() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob"), (1, "ann")]).await;
    let mut index = index(project.path());
    for query in [
        "SELECT call_id, fqn, status, duration_ns, args_state, args_cas_id FROM calls",
        "SELECT * FROM executions",
        "SELECT fqn, SUM(call_count) FROM function_stats GROUP BY fqn",
        "SELECT COUNT(output) FROM calls",
    ] {
        let result = sql(&mut index, query);
        assert_eq!(result.outcome.query.values.cas_loads, 0, "{query}");
        assert_eq!(result.outcome.query.values.value_evaluations, 0, "{query}");
    }
    let distinct = sql(
        &mut index,
        "SELECT COUNT(DISTINCT args_cas_id) FROM calls WHERE fqn = 'user.Extract'",
    );
    let blobs = distinct.rows[0][0].as_u64().unwrap();
    assert_eq!(blobs, 2, "ann's repeated inputs share one blob");
    let repeated = sql(
        &mut index,
        "SELECT args['customer']['name'], args['customer']['age'], args['count']
         FROM calls WHERE fqn = 'user.Extract' AND args['customer']['age'] > 0",
    );
    let values = &repeated.outcome.query.values;
    assert_eq!(repeated.rows.len(), 3);
    assert_eq!(values.cas_loads, blobs, "each blob decoded once per query");
    // Every other evaluation (the WHERE path, each projected path and its
    // kind column) is served from the blob cache or, for an identical
    // handle, the result cache.
    assert!(values.value_evaluations > 3 * 4);
    assert_eq!(
        values.value_evaluations,
        values.cas_loads + values.cas_cache_hits + values.result_cache_hits
    );
    // A fresh query starts with an empty cache.
    let again = sql(
        &mut index,
        "SELECT args['count'] FROM calls WHERE fqn = 'user.Extract'",
    );
    assert_eq!(again.outcome.query.values.cas_loads, blobs);
}

fn btel_files(project: &Path) -> usize {
    let recordings = project.join(".baml/btel/recordings");
    std::fs::read_dir(recordings)
        .map(|dirs| {
            dirs.flatten()
                .flat_map(|dir| {
                    std::fs::read_dir(dir.path())
                        .into_iter()
                        .flatten()
                        .flatten()
                })
                .filter(|f| f.path().extension().is_some_and(|e| e == "btel"))
                .count()
        })
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reopening_over_unchanged_recordings_decodes_no_files() {
    let project = tempfile::tempdir().unwrap();
    // Many small files: seal after every processed batch.
    let engine = engine(
        project.path(),
        RecordingConfig {
            target_bytes: std::num::NonZeroUsize::new(1).unwrap(),
            ..RecordingConfig::default()
        },
    );
    for i in 0..4 {
        run_main(&engine, i, "ann").await;
    }
    engine.shutdown().await;
    let files = btel_files(project.path());
    assert!(files > 4, "expected many files, found {files}");

    let mut first = index(project.path());
    let created = first.refresh().unwrap();
    assert!(created.schema_rebuilt, "the first query creates the index");
    assert_eq!(created.files_decoded, files);
    assert_eq!(created.files_applied, files);
    drop(first);
    for _ in 0..3 {
        let mut reopened = index(project.path());
        let metrics = reopened.refresh().unwrap();
        assert!(!metrics.schema_rebuilt);
        assert_eq!(metrics.files_decoded, 0);
        assert_eq!(metrics.files_unchanged, files);
        assert_eq!(
            metrics.transactions, 0,
            "an unchanged source writes nothing"
        );
        assert_eq!(stats(&mut reopened), expected_stats(4, 0));
    }
}

async fn wait_for_files(project: &Path, more_than: usize) -> usize {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let count = btel_files(project);
            if count > more_than {
                return count;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("recording publishes a new file")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_recording_applies_each_new_file_exactly_once() {
    let project = tempfile::tempdir().unwrap();
    let engine = engine(
        project.path(),
        RecordingConfig {
            flush_interval_duration: Duration::from_millis(10),
            ..RecordingConfig::default()
        },
    );
    let mut index = index(project.path());
    let mut applied = 0;
    for round in 1..=4_i64 {
        let before = btel_files(project.path());
        run_main(&engine, round, "live").await;
        wait_for_files(project.path(), before).await;
        // The deadline flush may publish in more than one file; wait until the
        // aggregates of this round are all indexed.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let metrics = index.refresh().unwrap();
            applied += metrics.files_applied;
            assert_eq!(
                metrics.files_decoded,
                metrics.files_applied + metrics.files_rejected,
                "only new files are decoded"
            );
            let totals = indexed_stats(&mut index);
            if totals.get("user.main") == Some(&round)
                && totals.get("user.Leaf") == Some(&(round * 11))
            {
                assert_eq!(totals, expected_stats(round, 0), "round {round}");
                break;
            }
            assert!(
                totals.get("user.main").copied().unwrap_or(0) <= round,
                "counts never exceed the work done: {totals:?}"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "round {round} never indexed"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let recording = indexed_sql(&mut index, "SELECT state, indexed_sequence FROM recordings");
        assert_eq!(recording.rows[0][0], json!("unsealed"));
    }
    engine.shutdown().await;
    applied += index.refresh().unwrap().files_applied;
    let recording = sql(&mut index, "SELECT state FROM recordings");
    assert_eq!(recording.rows[0][0], json!("sealed"));
    assert_eq!(applied, btel_files(project.path()));
    assert_eq!(stats(&mut index), expected_stats(4, 0));
    let ledger: i64 = index
        .connection()
        .query_row("SELECT COUNT(*) FROM ledger", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        usize::try_from(ledger).unwrap(),
        applied,
        "one ledger row per applied file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_first_queries_share_one_index_without_duplicates() {
    let project = tempfile::tempdir().unwrap();
    let engine = engine(
        project.path(),
        RecordingConfig {
            target_bytes: std::num::NonZeroUsize::new(1).unwrap(),
            ..RecordingConfig::default()
        },
    );
    for i in 0..3 {
        run_main(&engine, i, "ann").await;
    }
    engine.shutdown().await;
    let files = btel_files(project.path());
    let path = project.path().to_owned();
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut index = index(&path);
                let result = index
                    .refresh_and_query(&QueryRequest {
                        sql: "SELECT fqn, SUM(call_count) FROM function_stats GROUP BY fqn".into(),
                        ..QueryRequest::default()
                    })
                    .unwrap();
                (result.outcome.refresh.unwrap(), result.rows)
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    let applied: usize = results.iter().map(|(m, _)| m.files_applied).sum();
    assert_eq!(applied, files, "each file applied by exactly one process");
    let rebuilt = results.iter().filter(|(m, _)| m.schema_rebuilt).count();
    assert_eq!(rebuilt, 1, "exactly one process creates the index");
    let first = &results[0].1;
    assert!(
        results.iter().all(|(_, rows)| rows == first),
        "every reader sees the full answer"
    );
    assert_eq!(stats(&mut index(&path)), expected_stats(3, 0));
}
