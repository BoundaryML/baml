//! A recording's first index is merged in memory; files after it are
//! reconciled against indexed rows in bounded transactions. Both paths must
//! derive the same index from the same files, whatever order evidence
//! arrives in across files.
mod support;

use std::{num::NonZeroUsize, path::Path, time::Duration};

use baml_query_btel::{Index, IndexOptions, RefreshOptions};
use bex_engine::{BexExternalValue, FunctionCallContextBuilder};
use btel_recorder::RecordingConfig;
use rusqlite::types::Value;
use support::*;

/// Fact tables (with the compared columns), and the columns that identify
/// a row. The copies' file stamps differ; their contents do not.
const TABLES: &[(&str, &str)] = &[
    (
        "recording (rec, recording_id, source_snapshot_id, format_minor, indexed_sequence,
           observed_sequence, terminal_sequence, blocked_sequence, blocked_reason, partial_files,
           files_after_end, indexed_bytes, generation, process_id, baml_version, host, command,
           process_started_ns, source_cas, process_end_status, process_end_ns, folded)",
        "rec",
    ),
    (
        "ledger (rec, sequence, size, content_hash)",
        "rec, sequence",
    ),
    ("rejected", "rec, sequence"),
    ("event_context", "rec, node_id, slot"),
    ("context_snapshot", "cas"),
    ("function_def", "rec, function_id"),
    ("function_param", "rec, function_id, position"),
    ("call_path", "rec, call_path_id"),
    ("thread", "rec, thread_id"),
    ("epoch", "rec, epoch_id"),
    ("epoch_state", "rec, epoch_id"),
    ("aggregate", "rec, node"),
    ("sysop", "rec, call_path_id"),
    ("model_usage", "rec, sequence, position"),
    ("call", "rec, call_id"),
    ("network_span", "rec, span_id"),
    ("network_event", "rec, span_id, seq"),
    ("profile_part", "rec, node_id"),
    ("profile_node", "process_id, node_id"),
];

const ERRORS: &str = r#"
class Failure { code int }
function Leaf(n: int) -> int { n + 1 }
function Thrower(n: int) -> int throws Failure { throw Failure { code: n } }
function Middle(n: int) -> int throws Failure { Thrower(n) + 1 }
function Recur(n: int) -> int throws Failure {
    if (n == 0) { Thrower(n) } else { Recur(n - 1) + 1 }
}
function Deferred(n: int) -> int throws Failure {
    defer { Leaf(1) }
    Middle(n)
}
function main(n: int) -> int {
    let a = Middle(n) catch (e) { Failure => 1 };
    let b = { Middle(n) catch (e) { Failure => { throw e } } } catch (outer) { Failure => 2 };
    let c = Recur(3) catch (e) { Failure => 3 };
    let d = Deferred(n) catch (e) { Failure => 4 };
    let child = spawn { Middle(n) };
    let e = (await child) catch (x) { Failure => 5 };
    let f = Pick(Box { value: n }).map((v) -> string { "v" }).value;
    a + b + c + d + e + Pick(f.length())
}
function crash(n: int) -> int { Middle(n) }
class Box<T> {
    value T
    function map<U>(self, f: (T) -> U throws never) -> Box<U> { Box { value: f(self.value) } }
}
function Pick<T>(x: T) -> T { x }
"#;

fn small_files() -> RecordingConfig {
    RecordingConfig {
        target_bytes: NonZeroUsize::new(4096).unwrap(),
        ..RecordingConfig::default()
    }
}

async fn record_corpus(project: &Path) {
    let engine = engine(project, small_files());
    for i in 0..48 {
        let name = format!("c{i}");
        assert_eq!(
            run_main(&engine, i, &name).await,
            BexExternalValue::Int(expected_main(i))
        );
    }
    let failed = engine
        .call_function(
            "crash",
            vec![BexExternalValue::Int(3)],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(failed.is_err());
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .expect("shutdown");
    let calls: Vec<(&str, i64)> = (0..24).map(|n| ("main", n)).chain([("crash", 1)]).collect();
    let results = record_program_with(
        project,
        ERRORS,
        &["Middle", "Recur", "Pick", "map"],
        &calls,
        small_files(),
    )
    .await;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 24);
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

type Dump = Vec<(String, Vec<Vec<Value>>)>;

fn dump(index: &Index) -> Dump {
    let conn = index.connection();
    let mut tables: Dump = TABLES
        .iter()
        .map(|(table, key)| {
            let (table, columns) = match table.split_once(' ') {
                Some((table, columns)) => (table, columns.trim_matches(['(', ')'])),
                None => (*table, "*"),
            };
            let mut statement = conn
                .prepare(&format!(
                    "SELECT {} FROM {table} ORDER BY {key}",
                    columns.split_whitespace().collect::<Vec<_>>().join(" ")
                ))
                .unwrap();
            let width = statement.column_count();
            let rows = statement
                .query_map([], |r| (0..width).map(|i| r.get::<_, Value>(i)).collect())
                .unwrap()
                .collect::<Result<Vec<Vec<Value>>, _>>()
                .unwrap();
            (table.to_owned(), rows)
        })
        .collect();
    // Issues are compared as a multiset: their ids follow batch boundaries.
    let mut issues: Vec<Vec<Value>> = conn
        .prepare("SELECT rec, sequence, code, subject, detail FROM issue")
        .unwrap()
        .query_map([], |r| (0..5).map(|i| r.get::<_, Value>(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    issues.sort_by_key(|row| format!("{row:?}"));
    tables.push(("issue".to_owned(), issues));
    tables
}

fn assert_same(fresh: &Dump, incremental: &Dump) {
    for ((table, a), (_, b)) in fresh.iter().zip(incremental) {
        assert_eq!(a.len(), b.len(), "{table}: row counts differ");
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x, y, "{table}: fresh row vs incremental row");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_and_incremental_indexes_are_identical() {
    let root = tempfile::tempdir().unwrap();
    let recorded = root.path().join("recorded");
    record_corpus(&recorded).await;
    let (fresh_project, incremental_project) = (root.path().join("fresh"), root.path().join("inc"));
    copy_dir(&recorded, &fresh_project);
    copy_dir(&recorded, &incremental_project);

    let mut fresh = Index::for_project(&fresh_project, IndexOptions::default()).unwrap();
    let metrics = fresh.refresh().unwrap();
    assert_eq!(metrics.transactions, 2, "one transaction per recording");
    assert!(metrics.files_applied > 20, "{metrics:?}");

    let options = IndexOptions {
        refresh: RefreshOptions {
            fresh_bytes: 0,
            batch_files: 1,
            ..RefreshOptions::default()
        },
        ..IndexOptions::default()
    };
    let mut incremental = Index::for_project(&incremental_project, options).unwrap();
    let metrics = incremental.refresh().unwrap();
    assert_eq!(metrics.transactions, metrics.files_applied);
    let fresh = dump(&fresh);
    // Pick<T> and Box<T>.map<U> recorded their type arguments.
    let calls = &fresh.iter().find(|(table, _)| table == "call").unwrap().1;
    assert_eq!(
        calls.iter().filter(|row| row[9] != Value::Null).count(),
        24 * 3,
        "type_args_cas"
    );
    assert_same(&fresh, &dump(&incremental));
}

mod crafted {
    //! Evidence out of order and in conflict, written as recording files.
    use btel_recorder::proto::{self, function_definition::Resolution, span_event::Event};
    use prost::Message as _;

    const T1: u64 = 11;
    const T2: u64 = 12;
    const T3: u64 = 13;
    const EPOCH: u64 = 5;
    const CALLER: u64 = 21;
    const CALLEE: u64 = 22;

    pub(super) fn write(project: &std::path::Path, files: Vec<proto::RecordingFile>) {
        let id = [7_u8; 16];
        let dir = project
            .join(".baml/btel/recordings")
            .join(btel_reader::discovery::hex(&id));
        std::fs::create_dir_all(&dir).unwrap();
        for (index, mut file) in files.into_iter().enumerate() {
            let sequence = index as u64 + 1;
            file.header = Some(proto::RecordingHeader {
                format_major: 2,
                format_minor: 3,
                recording_id: id.to_vec(),
                source_snapshot_id: None,
                process_id: Some(vec![9; 16]),
                baml_version: Some("0.20.1".into()),
                host: Some("baml".into()),
                command: vec!["baml".into(), "run".into(), "main".into()],
                process_started_at_unix_ns: Some(1_000),
                source_cas_id: None,
            });
            file.sequence = sequence;
            std::fs::write(
                dir.join(format!("{sequence:020}.btel")),
                file.encode_to_vec(),
            )
            .unwrap();
        }
    }

    fn metadata(function: u64, fqn: &str, line: u32) -> proto::FunctionDefinition {
        proto::FunctionDefinition {
            function_id: function,
            resolution: Some(Resolution::Metadata(proto::FunctionMetadata {
                fqn: fqn.into(),
                display_name: fqn.into(),
                source_file: Some("main.baml".into()),
                source_span: Some(proto::SourceSpan {
                    file_id: 3,
                    start: 0,
                    end: 500,
                }),
                kind: proto::FunctionKind::Bytecode as i32,
                source_map: Some(proto::SourceMap {
                    coordinate: proto::PcCoordinate::CompactByteOffset as i32,
                    code_bytes: 40,
                    pc: vec![2, 10, 20],
                    file_id: vec![3, 3, 3],
                    start: vec![100, 120, 140],
                    end: vec![110, 130, 150],
                    line: vec![line, line + 1, line + 2],
                }),
                ..Default::default()
            })),
        }
    }

    fn path(
        id: u32,
        parent: u32,
        caller: Option<u64>,
        pc: u32,
        callee: u64,
        edge: i32,
    ) -> proto::CallPathDefinition {
        proto::CallPathDefinition {
            call_path_id: id,
            thread_id: T1,
            parent_call_path_id: parent,
            visible_caller_function_id: caller,
            caller_pc: pc,
            callee_function_id: callee,
            edge,
        }
    }

    fn thread(id: u64, parent: Option<u64>, spawn: u32, started: u64) -> proto::ThreadDefinition {
        proto::ThreadDefinition {
            thread_id: id,
            parent_id: parent,
            spawn_call_path_id: spawn,
            started_at_ticks: started,
            clock_epoch_id: EPOCH,
            ..Default::default()
        }
    }

    fn epoch(multiplier: u64) -> proto::ClockEpochDefinition {
        proto::ClockEpochDefinition {
            epoch_id: EPOCH,
            domain_id: 1,
            source: proto::ClockSource::OsMonotonic as i32,
            multiplier,
            ..Default::default()
        }
    }

    fn completion(id: u64, parent: u64, path: u32, entered: u64) -> Event {
        Event::FunctionCompletion(proto::FunctionCompletion {
            id,
            parent_id: parent,
            node: u64::from(path) << 1,
            entered_at_ticks: entered,
            exited_at_ticks: entered + 10,
            completion_flags: 2,
            ..Default::default()
        })
    }

    fn section(thread: u64, events: Vec<Event>) -> proto::ThreadSection {
        proto::ThreadSection {
            thread_id: thread,
            context: None,
            events: events
                .into_iter()
                .map(|event| proto::SpanEvent { event: Some(event) })
                .collect(),
        }
    }

    fn aggregate(path: u32, count: u64, duration: u64, errored: u64) -> proto::AggregateDelta {
        proto::AggregateDelta {
            node: u64::from(path) << 1,
            count,
            total_duration_ticks: duration,
            total_self_await_ticks: 0,
            outcomes: Some(proto::AggregateOutcomes {
                errored,
                cancelled: 0,
                ..Default::default()
            }),
        }
    }

    fn snapshot(n: u64) -> proto::SnapshotId {
        proto::SnapshotId { low: n, high: 0 }
    }

    fn announce(id: u64, path: u32, method: &str, started: u64) -> Event {
        Event::NetworkAnnouncement(proto::NetworkAnnouncement {
            id,
            parent_id: 100,
            call_path_id: path,
            started_at_ticks: started,
            method: method.into(),
            url: "https://api.example.com/v1/messages".into(),
            request_cas_id: Some(snapshot(id)),
        })
    }

    fn network_event(span: u64, name: &str, at: u64, payload: Option<u64>) -> Event {
        Event::NetworkEvent(proto::NetworkEvent {
            span_id: span,
            name: name.into(),
            at_ticks: at,
            payload_cas_id: payload.map(snapshot),
        })
    }

    fn network_done(span: u64, at: u64, outcome: proto::InvocationOutcome) -> Event {
        Event::NetworkCompletion(proto::NetworkCompletion {
            span_id: span,
            completed_at_ticks: at,
            outcome: outcome as i32,
            panicked: false,
            error_cas_id: None,
        })
    }

    fn usage(node: u64, model: &str, input: u64) -> proto::ModelUsage {
        proto::ModelUsage {
            node_id: node,
            thread_id: T2,
            model: Some(model.into()),
            input_tokens: input,
            output_tokens: 10,
            cache_read_tokens: Some(5),
            cache_write_tokens: None,
            reasoning_tokens: None,
        }
    }

    pub(super) fn files() -> Vec<proto::RecordingFile> {
        let first = proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![metadata(CALLER, "user.Caller", 4)],
                call_paths: vec![path(2, 1, Some(CALLER), 12, CALLEE, 1)],
                threads: vec![thread(T2, Some(T1), 3, 12)],
                clock_epochs: vec![epoch(1)],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![
                    section(
                        T2,
                        vec![
                            Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
                            completion(100, T2, 2, 20),
                        ],
                    ),
                    // A request's events and completion before its announcement.
                    section(
                        T3,
                        vec![
                            network_event(200, "data", 30, Some(7)),
                            network_event(200, "connection", 25, Some(8)),
                            network_event(0, "data", 26, None),
                            network_done(200, 40, proto::InvocationOutcome::Ok),
                            network_event(201, "connection", 24, None),
                        ],
                    ),
                ],
            }),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![aggregate(2, 3, 30, 1), aggregate(4, 1, 7, 0)],
                sysop_times: vec![proto::SysOpTime {
                    node: 2 << 1,
                    sysops: 2,
                    total_ticks: 6,
                }],
            }),
            usage: Some(proto::UsageBatch {
                entries: vec![usage(100, "claude-opus-5-5", 1000)],
            }),
            ..Default::default()
        };
        let second = proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![
                    metadata(CALLER, "user.Caller", 9),
                    proto::FunctionDefinition {
                        function_id: CALLEE,
                        resolution: Some(Resolution::Unavailable(proto::MetadataUnavailable {})),
                    },
                ],
                call_paths: vec![path(2, 1, Some(CALLER), 13, CALLEE, 1)],
                threads: vec![thread(T2, Some(T1), 3, 99)],
                clock_epochs: vec![epoch(2)],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![
                    section(
                        T2,
                        vec![
                            Event::ThreadCompletion(proto::ThreadCompletion {
                                completed_at_ticks: 90,
                                outcome: 1,
                                ..Default::default()
                            }),
                            // Late: its completion was in the first file.
                            Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                                id: 100,
                                parent_id: T2,
                                call_path_id: 2,
                                entered_at_ticks: 21,
                                inputs_cas_id: None,
                                type_args_cas_id: Some(proto::SnapshotId { low: 3, high: 4 }),
                            }),
                        ],
                    ),
                    section(
                        T3,
                        vec![completion(101, T3, 4, 30), completion(101, T3, 4, 31)],
                    ),
                    section(
                        T2,
                        vec![
                            announce(200, 4, "GET", 22),
                            network_event(200, "end", 30, None),
                            network_done(200, 41, proto::InvocationOutcome::Ok),
                            // Never completed, made from a path only it uses.
                            announce(201, 6, "POST", 23),
                            network_done(202, 50, proto::InvocationOutcome::Unspecified),
                        ],
                    ),
                ],
            }),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![aggregate(2, 2, 20, 0), aggregate(4, 1, 5, 3)],
                sysop_times: vec![proto::SysOpTime {
                    node: 2 << 1,
                    sysops: 1,
                    total_ticks: 4,
                }],
            }),
            usage: Some(proto::UsageBatch {
                entries: vec![usage(100, "jev-1.13.0", 50), usage(T2, "unpriced-model", 7)],
            }),
            ..Default::default()
        };
        let third = proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![metadata(CALLEE, "user.Callee", 7)],
                call_paths: vec![
                    path(1, 0, None, 0, CALLER, 1),
                    path(4, 1, Some(CALLER), 2, CALLEE, 1),
                    path(6, 1, Some(CALLER), 6, CALLEE, 1),
                ],
                threads: vec![
                    thread(T1, None, 0, 10),
                    proto::ThreadDefinition {
                        name: Some("worker".into()),
                        ..thread(T3, Some(T1), 4, 28)
                    },
                ],
                clock_epochs: vec![],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![section(
                    T2,
                    vec![
                        Event::ThreadCompletion(proto::ThreadCompletion {
                            completed_at_ticks: 95,
                            outcome: 2,
                            panicked: true,
                        }),
                        announce(200, 4, "POST", 22),
                        network_event(200, "await", 41, None),
                        network_event(201, "data", 60, Some(9)),
                    ],
                )],
            }),
            end: Some(proto::RecordingEnd {
                process_end: Some(proto::ProcessEnd {
                    status: proto::ProcessStatus::Panicked as i32,
                    at_unix_ns: 5_000,
                }),
            }),
            clock_states: Some(proto::ClockStateBatch {
                states: vec![proto::ClockEpochState {
                    epoch_id: EPOCH,
                    status: 1,
                    r#final: true,
                }],
            }),
            ..Default::default()
        };
        vec![first, second, third]
    }
}

#[test]
fn late_and_conflicting_evidence_indexes_identically() {
    let root = tempfile::tempdir().unwrap();
    let (fresh_project, incremental_project) = (root.path().join("fresh"), root.path().join("inc"));
    crafted::write(&fresh_project, crafted::files());
    crafted::write(&incremental_project, crafted::files());
    let mut fresh = Index::for_project(&fresh_project, IndexOptions::default()).unwrap();
    assert_eq!(fresh.refresh().unwrap().files_applied, 3);
    let options = IndexOptions {
        refresh: RefreshOptions {
            fresh_bytes: 0,
            batch_files: 1,
            ..RefreshOptions::default()
        },
        ..IndexOptions::default()
    };
    let mut incremental = Index::for_project(&incremental_project, options).unwrap();
    assert_eq!(incremental.refresh().unwrap().transactions, 3);
    let (fresh, incremental) = (dump(&fresh), dump(&incremental));
    // The scenario must produce every kind of conflict it was written for.
    let issues: Vec<String> = fresh
        .iter()
        .find(|(table, _)| table == "issue")
        .unwrap()
        .1
        .iter()
        .map(|row| format!("{:?}", row[2]))
        .collect();
    for code in [
        "function_metadata_conflict",
        "call_path_conflict",
        "thread_definition_conflict",
        "clock_epoch_conflict",
        "thread_completion_conflict",
        "call_evidence_conflict",
        "aggregate_outcome_invalid",
        "network_span_conflict",
        "network_completion_conflict",
        "network_evidence_invalid",
    ] {
        assert!(
            issues.iter().any(|issue| issue.contains(code)),
            "{code}: {issues:?}"
        );
    }
    // The process ended, so both indexes built its profiler, with the
    // callee's name only known from the last file.
    let profile = &fresh
        .iter()
        .find(|(table, _)| table == "profile_node")
        .unwrap()
        .1;
    assert!(
        profile
            .iter()
            .any(|row| row[4] == Value::Text("user.Callee".into())),
        "{profile:?}"
    );
    // The fold keeps the path only a network span reads.
    let paths = &fresh
        .iter()
        .find(|(table, _)| table == "call_path")
        .unwrap()
        .1;
    assert!(
        paths.iter().any(|row| row[1] == Value::Integer(6)),
        "{paths:?}"
    );
    let events = &fresh
        .iter()
        .find(|(table, _)| table == "network_event")
        .unwrap()
        .1;
    assert_eq!(events.len(), 6, "{events:?}");
    // The late announcement gives its call the type arguments.
    let type_args: Vec<i64> = fresh
        .iter()
        .find(|(table, _)| table == "call")
        .unwrap()
        .1
        .iter()
        .filter(|row| row[9] != Value::Null)
        .map(|row| match &row[1] {
            Value::Blob(id) => i64::from_be_bytes(id[..].try_into().unwrap()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(type_args, vec![100]);
    assert_same(&fresh, &incremental);
}
