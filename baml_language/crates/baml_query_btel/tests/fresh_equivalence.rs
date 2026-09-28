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
    ("recording", "rec"),
    (
        "ledger (rec, sequence, size, content_hash)",
        "rec, sequence",
    ),
    ("rejected", "rec, sequence"),
    ("function_def", "rec, function_id"),
    ("function_param", "rec, function_id, position"),
    ("call_path", "rec, call_path_id"),
    ("thread", "rec, thread_id"),
    ("epoch", "rec, epoch_id"),
    ("epoch_state", "rec, epoch_id"),
    ("aggregate", "rec, node"),
    ("call", "rec, call_id"),
    ("error_raise", "rec, raise_id"),
    ("error_frame", "rec, raise_id, position"),
    ("error_link", "rec, raise_id, call_id, role"),
    ("raise_path", "rec, call_path_id"),
    ("raise_path_frame", "rec, call_path_id, position"),
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
    a + b + c + d + e
}
function crash(n: int) -> int { Middle(n) }
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
    let results =
        record_program_with(project, ERRORS, &["Middle", "Recur"], &calls, small_files()).await;
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
                .prepare(&format!("SELECT {columns} FROM {table} ORDER BY {key}"))
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
    assert_same(&dump(&fresh), &dump(&incremental));
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
                format_minor: 2,
                recording_id: id.to_vec(),
                source_snapshot_id: None,
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
            }),
        }
    }

    fn raise(id: u64, path: Option<u32>, pc: u32) -> proto::ErrorRaise {
        proto::ErrorRaise {
            raise_id: id,
            thread_id: T2,
            raised_at_ticks: 25,
            kind: proto::RaiseKind::Throw as i32,
            function_id: Some(CALLEE),
            pc: Some(pc),
            origin_state: proto::OriginState::Fresh as i32,
            frame_count: 3,
            call_path_id: path,
            ..Default::default()
        }
    }

    fn end(raise: u64, pc: u32) -> proto::ErrorUnwindEnd {
        proto::ErrorUnwindEnd {
            raise_id: raise,
            result: proto::UnwindResult::Caught as i32,
            handler_function_id: Some(CALLER),
            handler_pc: Some(pc),
            unwound_frames: 1,
        }
    }

    fn link(raise: u64, call: u64) -> proto::ErrorCallLink {
        proto::ErrorCallLink {
            raise_id: raise,
            call_id: call,
            role: proto::ErrorLinkRole::Unwound as i32,
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
                sections: vec![section(
                    T2,
                    vec![
                        Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
                        completion(100, T2, 2, 20),
                    ],
                )],
            }),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![aggregate(2, 3, 30, 1), aggregate(4, 1, 7, 0)],
            }),
            errors: Some(proto::ErrorBatch {
                raises: vec![raise(301, Some(2), 2)],
                call_links: vec![link(301, 100)],
                unwind_ends: vec![end(300, 20)],
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
                            }),
                            Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                                id: 100,
                                parent_id: T2,
                                call_path_id: 2,
                                entered_at_ticks: 21,
                                inputs_cas_id: None,
                            }),
                        ],
                    ),
                    section(
                        T3,
                        vec![completion(101, T3, 4, 30), completion(101, T3, 4, 31)],
                    ),
                ],
            }),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![aggregate(2, 2, 20, 0), aggregate(4, 1, 5, 3)],
            }),
            errors: Some(proto::ErrorBatch {
                raises: vec![
                    raise(300, None, 10),
                    raise(301, Some(2), 10),
                    raise(0, None, 2),
                ],
                call_links: vec![link(302, 100), link(301, 100)],
                unwind_ends: vec![end(301, 20), end(301, 30)],
            }),
            ..Default::default()
        };
        let third = proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![metadata(CALLEE, "user.Callee", 7)],
                call_paths: vec![
                    path(1, 0, None, 0, CALLER, 1),
                    path(4, 1, Some(CALLER), 2, CALLEE, 1),
                ],
                threads: vec![thread(T1, None, 0, 10)],
                clock_epochs: vec![],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![section(
                    T2,
                    vec![Event::ThreadCompletion(proto::ThreadCompletion {
                        completed_at_ticks: 95,
                        outcome: 2,
                    })],
                )],
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
        "error_raise_conflict",
        "error_end_conflict",
        "error_link_conflict",
        "error_evidence_invalid",
        "aggregate_outcome_invalid",
    ] {
        assert!(
            issues.iter().any(|issue| issue.contains(code)),
            "{code}: {issues:?}"
        );
    }
    assert_same(&fresh, &incremental);
}
