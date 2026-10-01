//! End-to-end: real engine recordings → incremental index → SQL.
mod support;

use std::{collections::BTreeMap, path::Path, time::Duration};

use baml_query_btel::{QueryRequest, Status};
use bex_engine::ProcessStatus;
use btel_recorder::RecordingConfig;
use serde_json::{Value as Json, json};
use support::*;

fn rows(result: &baml_query_btel::QueryResult) -> Vec<Vec<Json>> {
    result.rows.clone()
}

/// Every completed invocation per function, once the process ended. A
/// future's own node shares its function's name, so only functions count.
const STATS_SQL: &str = "SELECT function_name, SUM(invocation_count) AS n FROM profiler
     WHERE node_type = 'function' GROUP BY function_name ORDER BY n DESC";

/// Completed spans per name: retained calls and futures, while recording.
const SPANS_SQL: &str =
    "SELECT span_name, COUNT(*) AS n FROM spans GROUP BY span_name ORDER BY n DESC";

fn stats(index: &mut baml_query_btel::Index) -> BTreeMap<String, i64> {
    counts_of(&sql(index, STATS_SQL))
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

fn counts_of(result: &baml_query_btel::QueryResult) -> BTreeMap<String, i64> {
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
        // `spawn` builds its plan through this call, which is recorded like
        // any other BAML call.
        ("baml.spawn.Plan.new".to_owned(), mains),
    ]);
    if crashes > 0 {
        expected.insert("user.crash".to_owned(), crashes);
    }
    expected
}

/// Retained calls of the captured functions, and each run's two futures.
fn expected_spans(mains: i64) -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("user.main".to_owned(), mains),
        ("user.Extract".to_owned(), mains),
        ("user.Classify".to_owned(), mains),
        ("user.Validate".to_owned(), mains),
        ("user.Link".to_owned(), mains),
        (".<lambda(main, 0)>".to_owned(), mains),
    ])
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
        "SELECT function_name, SUM(invocation_count), SUM(return_count), SUM(error_count),
           SUM(nonpanic_error_count), SUM(panic_error_count), SUM(future_cancel_count),
           SUM(missing_count)
         FROM profiler GROUP BY function_name ORDER BY function_name",
    );
    let counts = |name, n, ok, errors| {
        vec![
            json!(name),
            json!(n),
            json!(ok),
            json!(errors),
            json!(errors),
            json!(0),
            json!(0),
            json!(0),
        ]
    };
    assert_eq!(
        result.rows,
        vec![
            counts("user.Recur", 4, 0, 4),
            counts("user.Risky", 6, 2, 4),
            counts("user.main", 1, 1, 0),
        ]
    );
    assert_eq!(result.outcome.query.values.cas_loads, 0);
    let retained = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Risky'",
    );
    assert!(
        retained.rows[0][0].as_u64().unwrap() < 6,
        "the profiler includes timing-only calls"
    );
    // Direct recursion folds into one node: 1 call and 3 re-entries.
    let recursive = sql(
        &mut index,
        "SELECT invocation_count, error_count FROM profiler WHERE function_name = 'user.Recur'",
    );
    assert_eq!(recursive.rows, vec![vec![json!(4), json!(4)]]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acceptance_queries_answer_from_real_recordings() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob"), (1, "ann")]).await;
    let mut index = index(project.path());

    let processes = sql(
        &mut index,
        "SELECT status, host, baml_version IS NOT NULL, status_history[1]['status']
         FROM processes",
    );
    assert_eq!(
        processes.rows,
        vec![vec![
            json!("error"),
            json!("unknown"),
            json!(1),
            json!("error")
        ]],
        "the host recorded how the process ended"
    );
    assert!(
        !processes.outcome.unsealed,
        "shutdown writes the end marker"
    );
    assert_eq!(processes.outcome.status, Status::Complete);

    let roots = sql(
        &mut index,
        "SELECT span_name, status, duration > 0, start_time < end_time FROM spans
         WHERE span_type = 'future' AND parent_span_id IS NULL ORDER BY start_time",
    );
    let run = |name, status| vec![json!(name), json!(status), json!(1), json!(1)];
    assert_eq!(
        rows(&roots),
        vec![
            run("user.main", "return"),
            run("user.main", "return"),
            run("user.main", "return"),
            run("user.crash", "user_error"),
        ]
    );
    assert_eq!(stats(&mut index), expected_stats(3, 1));

    let names = sql(
        &mut index,
        "SELECT span_id, output_value['items'][0]['name'] AS name FROM spans
         WHERE status = 'return' AND span_name = 'user.Extract' ORDER BY start_time",
    );
    assert_eq!(
        column(&names, "name"),
        vec![&json!("ann-item"), &json!("bob-item"), &json!("ann-item")]
    );
    assert_eq!(names.columns[1].column_type, "baml_value");

    let adults = sql(
        &mut index,
        "SELECT c.span_name FROM spans c WHERE c.input_args['customer']['age'] >= 25
         ORDER BY c.start_time",
    );
    assert_eq!(
        column(&adults, "span_name"),
        vec![
            &json!("user.Extract"),
            &json!("user.Classify"),
            &json!("user.Validate")
        ]
    );

    // Named inputs, enum and int outputs, captured errors, missing paths.
    let typed = sql(
        &mut index,
        "SELECT span_name, input_args['customer']['name'] AS who, input_args['count'] AS count,
           output_value AS result, error_value['error']['reason'] AS reason,
           output_value['nope'] AS missing
         FROM spans WHERE input_args['customer']['name'] = 'bob' ORDER BY start_time",
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
        "SELECT error_value['error']['code'] AS code, error_value['error']['reason'] AS reason,
           error_value['stack_trace']['frames'][0]['function_name'] AS thrown_in
         FROM spans WHERE status = 'user_error' AND span_type = 'function'",
    );
    assert_eq!(errors.rows.len(), 3);
    assert!(
        errors
            .rows
            .iter()
            .all(|r| r[0] == json!(42) && r[1] == json!("too young")),
        "{:?}",
        errors.rows
    );

    // Aliases, a CTE, and a name reused across scopes.
    let cte = sql(
        &mut index,
        "WITH x AS (SELECT c.span_id AS id, c.input_args AS a, c.span_name FROM spans c
           WHERE c.span_name = 'user.Extract')
         SELECT a['customer']['name'] AS name, a['count'] AS n FROM x
         WHERE EXISTS (SELECT 1 FROM spans x WHERE x.output_value = 'Premium')
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
        "SELECT COUNT(*) AS n, SUM(output_value = 27) AS as_int, SUM(output_value = '27') AS as_text
         FROM spans WHERE span_name = 'user.Validate' AND status = 'return'",
    );
    assert_eq!(rows(&coercion), vec![vec![json!(1), json!(1), json!(0)]]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metadata_queries_read_no_cas_and_repeated_paths_share_hydration() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob"), (1, "ann")]).await;
    let mut index = index(project.path());
    for query in [
        "SELECT span_id, span_name, status, duration, span_reason FROM spans",
        "SELECT process_id, status, command FROM processes",
        STATS_SQL,
        "SELECT COUNT(output_value) FROM spans",
    ] {
        let result = sql(&mut index, query);
        assert_eq!(result.outcome.query.values.cas_loads, 0, "{query}");
        assert_eq!(result.outcome.query.values.value_evaluations, 0, "{query}");
    }
    let repeated = sql(
        &mut index,
        "SELECT input_args['customer']['name'], input_args['customer']['age'], input_args['count']
         FROM spans WHERE span_name = 'user.Extract' AND input_args['customer']['age'] > 0",
    );
    let values = &repeated.outcome.query.values;
    assert_eq!(repeated.rows.len(), 3);
    // ann's repeated inputs share one blob: each of the two decoded once.
    assert_eq!(values.cas_loads, 2, "each blob decoded once per query");
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
        "SELECT input_args['count'] FROM spans WHERE span_name = 'user.Extract'",
    );
    assert_eq!(again.outcome.query.values.cas_loads, 2);
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
    engine.record_process_exit(ProcessStatus::Success);
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
        // The deadline flush may publish in more than one file; wait until
        // this round's spans are all indexed.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let metrics = index.refresh().unwrap();
            applied += metrics.files_applied;
            assert_eq!(
                metrics.files_decoded,
                metrics.files_applied + metrics.files_rejected,
                "only new files are decoded"
            );
            let totals = counts_of(&indexed_sql(&mut index, SPANS_SQL));
            if totals == expected_spans(round) {
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
        // The process is still running: no profiler yet.
        let running = indexed_sql(
            &mut index,
            "SELECT p.status, (SELECT COUNT(*) FROM profiler) FROM processes p",
        );
        assert_eq!(running.rows, vec![vec![json!("running"), json!(0)]]);
    }
    engine.record_process_exit(ProcessStatus::Success);
    engine.shutdown().await;
    applied += index.refresh().unwrap().files_applied;
    let process = sql(&mut index, "SELECT status FROM processes");
    assert_eq!(process.rows[0][0], json!("success"));
    assert!(!process.outcome.unsealed);
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
    engine.record_process_exit(ProcessStatus::Success);
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
                        sql: STATS_SQL.into(),
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

/// Each model request is a span of its own, so two calls made on one thread
/// from different steps are two rows, each under the step that made it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_model_request_is_its_own_span() {
    let project = tempfile::tempdir().unwrap();
    let source = r##"
        class Canned {
            server: baml.http.Server,
            body: string,

            function handle(self, req: baml.http.Request) -> baml.http.Response {
                baml.http.Response.new(200, { "content-type": "application/json" }, self.body.to_utf8())
            }
        }

        function Summarize(text: string) -> string {
            client: "anthropic/claude-opus-5-5"
            prompt: `Summarize ${text}`
        }

        function Ask(base_url: string, text: string) -> string {
            let cl = anthropic.Client.new(
                model = "claude-opus-5-5",
                api_key = "test-key",
                base_url = base_url,
            );
            ai.Agent.new(client = cl).run(Summarize@spec(text)).value
        }

        // Two steps on one thread, neither a span of its own.
        function Group(base_url: string) -> string {
            Ask(base_url, "facts")
        }

        function Draft(base_url: string) -> string {
            Ask(base_url, "a draft")
        }

        function main(n: int) -> int {
            let server = Canned {
                server: baml.http.Server.bind("127.0.0.1:0"),
                body: `{"id":"m","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"short"}],"stop_reason":"end_turn","usage":{"input_tokens":900,"output_tokens":30,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}`,
            };
            let task = spawn { server.server.serve(server.handle) };
            let url = "http://" + server.server.addr;
            let facts = Group(url);
            let draft = Draft(url);
            task.cancel();
            facts.length() + draft.length()
        }
    "##;
    let results = record_program(project.path(), source, &[], &[("main", 0)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(10))]);
    let mut index = index(project.path());
    let requests = sql(
        &mut index,
        "SELECT span_name, temporary_projections['model_calls'],
           round(temporary_projections['cost'], 6)
         FROM spans WHERE temporary_projections IS NOT NULL",
    );
    assert_eq!(
        requests.rows,
        vec![
            vec![json!("baml.http.send"), json!(1), json!(0.00474)],
            vec![json!("baml.http.send"), json!(1), json!(0.00474)],
        ]
    );
    // Each request's call path leads up to the step that made it: a step's
    // cost is the sum over the requests below it.
    let steps = sql(
        &mut index,
        "WITH RECURSIVE up(span, cost, node) AS (
           SELECT span_id, temporary_projections['cost'], profiler_node_id
           FROM spans WHERE temporary_projections IS NOT NULL
           UNION ALL
           SELECT u.span, u.cost, p.parent_profiler_node_id
           FROM up u JOIN profiler p ON p.profiler_node_id = u.node
         )
         SELECT p.function_name, count(*), round(sum(u.cost), 6)
         FROM up u JOIN profiler p ON p.profiler_node_id = u.node
         WHERE p.function_name IN ('user.Group', 'user.Draft')
         GROUP BY p.function_name ORDER BY p.function_name",
    );
    assert_eq!(
        steps.rows,
        vec![
            vec![json!("user.Draft"), json!(1), json!(0.00474)],
            vec![json!("user.Group"), json!(1), json!(0.00474)],
        ]
    );
}

/// A streamed turn is priced like a non-streamed one with the same tokens:
/// its cache reads and writes reach `temporary_projections`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streamed_usage_keeps_the_cache_split() {
    let project = tempfile::tempdir().unwrap();
    let source = r##"
        class Sse {
            server: baml.http.Server,
            events: string[],

            function handle(self, req: baml.http.Request) -> baml.http.Response {
                let resp = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
                let events = self.events;
                spawn {
                    for (let event in events) {
                        resp.write((event + "\n\n").to_utf8());
                    }
                    resp.end();
                    null
                };
                resp
            }
        }

        function sse(name: string, data: string) -> string {
            "event: " + name + "\ndata: " + data
        }

        function Summarize(text: string) -> string {
            client: "anthropic/claude-opus-5-5"
            prompt: `Summarize ${text}`
        }

        function Stream(base_url: string) -> string {
            let cl = anthropic.Client.new(
                model = "claude-opus-5-5",
                api_key = "test-key",
                base_url = base_url,
            );
            ai.stream.from_spec<string>(Summarize@spec("hi"), client = cl).final()
        }

        function main(n: int) -> int {
            let server = Sse {
                server: baml.http.Server.bind("127.0.0.1:0"),
                events: [
                    sse("message_start", `{"type":"message_start","message":{"id":"m","model":"claude-opus-5-5","usage":{"input_tokens":900,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}`),
                    sse("content_block_start", `{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}`),
                    sse("content_block_delta", `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"short"}}`),
                    sse("message_delta", `{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":30}}`),
                    sse("message_stop", `{"type":"message_stop"}`),
                ],
            };
            let task = spawn { server.server.serve(server.handle) };
            let text = Stream("http://" + server.server.addr, $trace = trace.span());
            task.cancel();
            text.length()
        }
    "##;
    let results = record_program(project.path(), source, &[], &[("main", 0)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(5))]);
    let mut index = index(project.path());
    let usage = sql(
        &mut index,
        "SELECT s.span_name, parent.span_name, s.temporary_projections['model_name'],
           s.temporary_projections['model_calls'], s.temporary_projections['input_tokens'],
           s.temporary_projections['output_tokens'], s.temporary_projections['cache_read_tokens'],
           s.temporary_projections['cache_write_tokens'], round(s.temporary_projections['cost'], 6)
         FROM spans s JOIN spans parent ON parent.span_id = s.parent_span_id
         WHERE s.temporary_projections IS NOT NULL",
    );
    // The request's own span, inside the caller's; the same tokens and price
    // as the non-streamed turn below.
    assert_eq!(
        usage.rows,
        vec![vec![
            json!("baml.http.fetch_sse"),
            json!("user.Stream"),
            json!("claude-opus-5-5"),
            json!(1),
            json!(900),
            json!(30),
            json!(200),
            json!(100),
            json!(0.00474)
        ]]
    );
}

/// Model usage from the runner, panics, future names and sysop time, as
/// the engine records them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recorded_usage_panics_names_and_sysop_time_reach_the_tables() {
    let project = tempfile::tempdir().unwrap();
    let source = r#"
        client DefaultClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        class Probe {
            implements ai.Client {
                function id(self) -> string {
                    "probe"
                }

                function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
                    baml.http.Request { method: "POST", url: "https://probe.invalid", headers: {}, body: "" }
                }

                function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
                    ai.ModelTurn {
                        calls: [],
                        content: [ai.content.Text { text: "\"short\"" }],
                        stop_reason: ai.content.StopReason.Complete,
                        usage: ai.events.Usage {
                            input_tokens: 1200,
                            output_tokens: 30,
                            cached_input_tokens: 200,
                            reasoning_tokens: null,
                            cache_write_input_tokens: 100,
                            uncached_input_tokens: 900,
                        },
                        model: "claude-opus-5-5",
                    }
                }
            }
        }

        function Summarize(text: string) -> string {
            client: DefaultClient
            prompt: `Summarize ${text}`
        }

        function Step() -> string {
            ai.Agent.new(client = Probe {}).run(Summarize@spec("hi")).value
        }

        function Boom() -> int {
            let xs: int[] = [];
            xs[1]
        }

        function Read() -> int {
            let text = baml.fs.read("/nonexistent/baml/query/test") catch_all (e) {
                _ => ""
            };
            text.length()
        }

        function main(n: int) -> int {
            let reader = spawn "reader" { Read() };
            let read = await reader;
            let step = Step($trace = trace.span());
            let boom = Boom($trace = trace.span()) catch_all_panics (e) {
                _ => 0
            };
            read + step.length() + boom
        }
    "#;
    let results = record_program(project.path(), source, &[], &[("main", 0)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(5))]);
    let mut index = index(project.path());
    let usage = sql(
        &mut index,
        "SELECT temporary_projections['model_name'], temporary_projections['input_tokens'],
           temporary_projections['output_tokens'], temporary_projections['cache_read_tokens'],
           temporary_projections['cache_write_tokens'], round(temporary_projections['cost'], 6)
         FROM spans WHERE span_name = 'user.Step'",
    );
    // 900 full-rate input at $4/M, 100 cache writes at 1.25x, 200 cache
    // reads at $0.20/M, 30 output at $20/M.
    assert_eq!(
        usage.rows,
        vec![vec![
            json!("claude-opus-5-5"),
            json!(900),
            json!(30),
            json!(200),
            json!(100),
            json!(0.00474)
        ]]
    );
    let statuses = sql(
        &mut index,
        "SELECT span_name, status FROM spans WHERE span_name IN ('user.Boom', 'reader')
         ORDER BY span_name",
    );
    assert_eq!(
        statuses.rows,
        vec![
            vec![json!("reader"), json!("return")],
            vec![json!("user.Boom"), json!("panic_error")],
        ]
    );
    let io = sql(
        &mut index,
        "SELECT io_self_time > 0, io_total_time >= io_self_time, panic_error_count
         FROM profiler WHERE function_name IN ('user.Read', 'user.Boom') ORDER BY function_name",
    );
    assert_eq!(
        io.rows,
        vec![
            vec![json!(0), json!(1), json!(1)],
            vec![json!(1), json!(1), json!(0)],
        ]
    );
}

/// A future spawned behind a full `Limit` is scheduled when it is spawned and
/// starts when it is admitted: its span and its profiler node count only the
/// time it ran. A cancellation is the future's own outcome; a function it
/// interrupted never finished, so it is missing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn futures_record_when_they_were_scheduled_and_started() {
    let project = tempfile::tempdir().unwrap();
    let source = r##"
        function work(ms: int) -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
            ms
        }

        class Flag {
            set: bool,
        }

        // Sets `started` once it runs, then works until it is cancelled.
        function until_cancelled(started: Flag) -> int {
            started.set = true;
            work(5000)
        }

        function main(n: int) -> int {
            let one = baml.spawn.Limit.new(1);
            let first = spawn with one { work(200) };
            // Waits for `first` to finish.
            let second = spawn with one { work(100) };
            // Cancelled while it waits.
            let token = baml.spawn.CancelToken.new();
            let queued = spawn with one, token { work(1) };
            let _ = token.cancel();
            // Cancelled while it runs: only once it has started.
            let stop = baml.spawn.CancelToken.new();
            let started = Flag { set: false };
            let running = spawn with stop { until_cancelled(started) };
            while (!started.set) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
            }
            let _ = stop.cancel();
            (await baml.future.all_settled([first, second, queued, running])).length()
        }
    "##;
    let results = record_program(project.path(), source, &[], &[("main", 0)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(4))]);
    let mut index = index(project.path());
    let futures = sql(
        &mut index,
        "SELECT span_name, status, scheduled_time, start_time, end_time, duration
         FROM spans WHERE span_type = 'future' AND span_name LIKE '.<lambda(main, %'
         ORDER BY span_name",
    )
    .rows;
    let [first, second, queued, running] = futures.as_slice() else {
        panic!("four futures: {futures:?}");
    };
    let status: Vec<&Json> = futures.iter().map(|row| &row[1]).collect();
    assert_eq!(
        status,
        [
            &json!("return"),
            &json!("return"),
            &json!("cancel_error"),
            &json!("cancel_error")
        ]
    );
    let text = |row: &[Json], column: usize| row[column].as_str().unwrap().to_owned();
    let nanos = |row: &[Json]| row[5].as_i64().unwrap();
    // `second` was scheduled at once and started once `first` was done; its
    // duration is its own 100 ms run, not the 200 ms wait before it.
    assert!(text(second, 2) < text(second, 3));
    assert!(text(second, 3) >= text(first, 4));
    assert!(nanos(second) >= 100_000_000, "{second:?}");
    assert!(nanos(second) < nanos(first), "{first:?} {second:?}");
    // `queued` never ran: it starts and ends when it was cancelled.
    assert!(text(queued, 2) <= text(queued, 3));
    assert_eq!(text(queued, 3), text(queued, 4));
    assert_eq!(nanos(queued), 0);
    assert!(text(running, 2) <= text(running, 3));
    // A function span has no scheduled time.
    let functions = sql(
        &mut index,
        "SELECT count(*) FROM spans WHERE span_type = 'function' AND scheduled_time IS NOT NULL",
    );
    assert_eq!(functions.rows, vec![vec![json!(0)]]);

    let nodes = sql(
        &mut index,
        "SELECT node_type, function_name, invocation_count, return_count, future_cancel_count,
           error_count, missing_count
         FROM profiler WHERE function_name LIKE '.<lambda(main, %' AND node_type = 'future'
         ORDER BY function_name",
    );
    assert_eq!(
        nodes.rows,
        vec![
            vec![
                json!("future"),
                json!(".<lambda(main, 0)>"),
                json!(1),
                json!(1),
                json!(0),
                json!(0),
                json!(0)
            ],
            vec![
                json!("future"),
                json!(".<lambda(main, 1)>"),
                json!(1),
                json!(1),
                json!(0),
                json!(0),
                json!(0)
            ],
            vec![
                json!("future"),
                json!(".<lambda(main, 2)>"),
                json!(1),
                json!(0),
                json!(1),
                json!(0),
                json!(0)
            ],
            vec![
                json!("future"),
                json!(".<lambda(main, 3)>"),
                json!(1),
                json!(0),
                json!(1),
                json!(0),
                json!(0)
            ],
        ]
    );
    // The calls the cancellation interrupted never finished.
    let interrupted = sql(
        &mut index,
        "SELECT c.function_name, c.invocation_count, c.return_count, c.future_cancel_count,
           c.missing_count
         FROM profiler c
         JOIN profiler p ON p.profiler_node_id = c.parent_profiler_node_id
         WHERE c.function_name = 'user.until_cancelled'
            OR (c.function_name = 'user.work' AND p.function_name = 'user.until_cancelled')
         ORDER BY c.function_name",
    );
    assert_eq!(
        interrupted.rows,
        vec![
            vec![
                json!("user.until_cancelled"),
                json!(1),
                json!(0),
                json!(0),
                json!(1)
            ],
            vec![json!("user.work"), json!(1), json!(0), json!(0), json!(1)],
        ]
    );
    // Only the run counts: `second`'s node holds its 100 ms, not the wait.
    let times = sql(
        &mut index,
        "SELECT total_time FROM profiler
         WHERE node_type = 'future' AND function_name IN ('.<lambda(main, 0)>', '.<lambda(main, 1)>')
         ORDER BY function_name",
    );
    let first_ns = times.rows[0][0].as_i64().unwrap();
    let second_ns = times.rows[1][0].as_i64().unwrap();
    assert!(
        second_ns >= 100_000_000 && second_ns < first_ns,
        "{first_ns} vs {second_ns}"
    );
    // Every node adds up.
    let unbalanced = sql(
        &mut index,
        "SELECT count(*) FROM profiler
         WHERE invocation_count != return_count + error_count + future_cancel_count + missing_count
            OR error_count != panic_error_count + nonpanic_error_count
            OR (node_type = 'function' AND future_cancel_count != 0)
            OR (node_type = 'future' AND missing_count != 0)",
    );
    assert_eq!(unbalanced.rows, vec![vec![json!(0)]]);
}
