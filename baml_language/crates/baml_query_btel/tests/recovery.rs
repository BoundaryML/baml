//! Reconciliation under adverse evidence, with hand-built recording files.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use baml_query_btel::{
    Error, Index, IndexOptions, QueryRequest, QueryResult, RefreshOptions, Status, store,
};
use btel_recorder::proto::{self, span_event::Event};
use prost::Message as _;
use serde_json::{Value as Json, json};

const THREAD: u64 = 1;
const EPOCH: u64 = 5;
const FUNCTION: u64 = 10;
const PATH: u32 = 3;
const CALL: u64 = 100;

struct Recording {
    dir: PathBuf,
    id: [u8; 16],
}

impl Recording {
    fn new(project: &Path, tag: u8) -> Self {
        let id = [tag; 16];
        let dir = project
            .join(".baml/btel/recordings")
            .join(btel_reader::discovery::hex(&id));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir, id }
    }
    fn path(&self, sequence: u64) -> PathBuf {
        self.dir.join(format!("{sequence:020}.btel"))
    }
    fn write(&self, sequence: u64, mut file: proto::RecordingFile) {
        file.header = Some(proto::RecordingHeader {
            format_major: FORMAT_MAJOR,
            format_minor: 1,
            recording_id: self.id.to_vec(),
            source_snapshot_id: None,
        });
        file.sequence = sequence;
        let part = self.dir.join(format!("{sequence:020}.btel.part"));
        std::fs::write(&part, file.encode_to_vec()).unwrap();
        std::fs::rename(part, self.path(sequence)).unwrap();
    }
}

/// `btel_settings::encoding::FORMAT_MAJOR`.
const FORMAT_MAJOR: u32 = 2;

fn epoch() -> proto::ClockEpochDefinition {
    proto::ClockEpochDefinition {
        epoch_id: EPOCH,
        domain_id: 1,
        source: proto::ClockSource::OsMonotonic as i32,
        multiplier: 1,
        shift: 0,
        utc: Some(proto::UtcAnchor {
            ticks: 0,
            unix_nanos: Some(proto::UnixNanos {
                low: 1_790_000_000_000_000_000,
                high: 0,
            }),
            uncertainty_ns: 0,
        }),
        ..Default::default()
    }
}

fn definitions(fqn: &str) -> proto::Definitions {
    proto::Definitions {
        functions: vec![proto::FunctionDefinition {
            function_id: FUNCTION,
            resolution: Some(proto::function_definition::Resolution::Metadata(
                proto::FunctionMetadata {
                    fqn: fqn.into(),
                    display_name: fqn.rsplit('.').next().unwrap().into(),
                    argument_layout: Some(proto::ArgumentLayout { slots: vec![] }),
                    ..Default::default()
                },
            )),
        }],
        call_paths: vec![proto::CallPathDefinition {
            call_path_id: PATH,
            thread_id: THREAD,
            parent_call_path_id: 0,
            visible_caller_function_id: None,
            caller_pc: 0,
            callee_function_id: FUNCTION,
            edge: proto::CallPathEdge::Synchronous as i32,
        }],
        threads: vec![proto::ThreadDefinition {
            thread_id: THREAD,
            parent_id: None,
            spawn_call_path_id: 0,
            started_at_ticks: 900,
            clock_epoch_id: EPOCH,
        }],
        clock_epochs: vec![epoch()],
    }
}

fn state(status: proto::TimingStatus) -> proto::ClockStateBatch {
    proto::ClockStateBatch {
        states: vec![proto::ClockEpochState {
            epoch_id: EPOCH,
            status: status as i32,
            r#final: false,
        }],
    }
}

fn aggregate(count: u64, duration: u64) -> proto::AggregateBatch {
    proto::AggregateBatch {
        entries: vec![proto::AggregateDelta {
            node: u64::from(PATH) << 1,
            count,
            total_duration_ticks: duration,
            total_self_await_ticks: 0,
            outcomes: None,
        }],
    }
}

fn spans(events: Vec<Event>) -> proto::SpanBatch {
    proto::SpanBatch {
        sections: vec![proto::ThreadSection {
            thread_id: THREAD,
            events: events
                .into_iter()
                .map(|event| proto::SpanEvent { event: Some(event) })
                .collect(),
        }],
    }
}

fn announce(inputs: Option<[u8; 16]>) -> Event {
    Event::FunctionAnnouncement(proto::FunctionAnnouncement {
        id: CALL,
        parent_id: THREAD,
        call_path_id: PATH,
        entered_at_ticks: 1000,
        inputs_cas_id: inputs.map(|b| proto::SnapshotId {
            low: u64::from_le_bytes(b[..8].try_into().unwrap()),
            high: u64::from_le_bytes(b[8..].try_into().unwrap()),
        }),
    })
}

fn complete(parent: u64, flags: u32) -> Event {
    Event::FunctionCompletion(proto::FunctionCompletion {
        id: CALL,
        parent_id: parent,
        node: u64::from(PATH) << 1,
        entered_at_ticks: 1000,
        exited_at_ticks: 1500,
        self_await_ticks: 100,
        completion_flags: flags,
        value_cas_id: None,
    })
}

fn only_aggregate(count: u64) -> proto::RecordingFile {
    proto::RecordingFile {
        aggregates: Some(aggregate(count, count * 10)),
        ..Default::default()
    }
}

fn options() -> IndexOptions {
    IndexOptions::default()
}

fn run(index: &mut Index, sql: &str) -> QueryResult {
    index
        .query(&QueryRequest {
            sql: sql.into(),
            ..QueryRequest::default()
        })
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn one(index: &mut Index, sql: &str) -> Vec<Json> {
    let result = run(index, sql);
    assert_eq!(result.rows.len(), 1, "{sql}: {:?}", result.rows);
    result.rows[0].clone()
}

fn count(index: &mut Index) -> Json {
    one(index, "SELECT SUM(call_count) FROM function_stats")[0].clone()
}

#[test]
fn late_definitions_completions_and_clock_invalidation_repair_derived_fields() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    // File 1: an announcement and aggregates; every definition arrives later.
    recording.write(
        1,
        proto::RecordingFile {
            aggregates: Some(aggregate(1, 500)),
            spans: Some(spans(vec![announce(None)])),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    assert_eq!(
        one(
            &mut index,
            "SELECT status, fqn, execution_id, timing_state, duration_ns FROM calls"
        ),
        vec![
            json!("incomplete"),
            Json::Null,
            Json::Null,
            json!("incomplete"),
            Json::Null
        ]
    );
    assert_eq!(
        one(
            &mut index,
            "SELECT fqn, call_count, timing_state, total_duration_ns FROM function_stats"
        ),
        vec![Json::Null, json!(1), json!("unknown_clock"), Json::Null]
    );
    let issues = run(&mut index, "SELECT code FROM issues ORDER BY code");
    assert!(
        issues.rows.contains(&vec![json!("call_path_undefined")]),
        "{:?}",
        issues.rows
    );

    // File 2: definitions, a valid clock state, the completion.
    recording.write(
        2,
        proto::RecordingFile {
            definitions: Some(definitions("user.Ask")),
            clock_states: Some(state(proto::TimingStatus::Valid)),
            spans: Some(spans(vec![
                complete(THREAD, 1),
                Event::ThreadCompletion(proto::ThreadCompletion {
                    completed_at_ticks: 2000,
                    outcome: proto::InvocationOutcome::Ok as i32,
                }),
            ])),
            ..Default::default()
        },
    );
    let metrics = index.refresh().unwrap();
    assert_eq!((metrics.files_decoded, metrics.files_unchanged), (1, 1));
    assert_eq!(
        one(
            &mut index,
            "SELECT status, fqn, execution_id IS NOT NULL, timing_state, duration_ns, self_await_ns FROM calls"
        ),
        vec![
            json!("ok"),
            json!("user.Ask"),
            json!(1),
            json!("valid"),
            json!(500),
            json!(100)
        ]
    );
    assert_eq!(
        one(
            &mut index,
            "SELECT entry_fqn, status, duration_ns, started_at FROM executions"
        ),
        vec![
            json!("user.Ask"),
            json!("ok"),
            json!(1100),
            json!("2026-09-21T14:13:20.000000Z")
        ]
    );
    assert_eq!(
        one(
            &mut index,
            "SELECT fqn, call_count, total_duration_ns, timing_state FROM function_stats"
        ),
        vec![json!("user.Ask"), json!(1), json!(500), json!("valid")]
    );
    assert!(run(&mut index, "SELECT * FROM issues").rows.is_empty());

    // File 3: the epoch is invalidated; durations become unavailable.
    recording.write(
        3,
        proto::RecordingFile {
            clock_states: Some(state(proto::TimingStatus::Discontinuity)),
            ..Default::default()
        },
    );
    index.refresh().unwrap();
    assert_eq!(
        one(&mut index, "SELECT duration_ns, timing_state FROM calls"),
        vec![Json::Null, json!("invalidated_discontinuity")]
    );
    assert_eq!(
        one(
            &mut index,
            "SELECT duration_ns, timing_state FROM executions"
        ),
        vec![Json::Null, json!("invalidated_discontinuity")]
    );
    assert_eq!(
        one(
            &mut index,
            "SELECT total_duration_ns, timing_state, call_count FROM function_stats"
        ),
        vec![Json::Null, json!("invalidated_discontinuity"), json!(1)]
    );
}

#[test]
fn completion_before_its_announcement_waits_for_inputs() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 2);
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions("user.Ask")),
            clock_states: Some(state(proto::TimingStatus::Valid)),
            // Bit 3: the inputs live in an announcement not yet published.
            spans: Some(spans(vec![complete(THREAD, 9)])),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    assert_eq!(
        one(&mut index, "SELECT status, args_state FROM calls"),
        vec![json!("ok"), json!("pending")]
    );
    assert_eq!(
        one(&mut index, "SELECT code FROM issues")[0],
        json!("announcement_pending")
    );
    // Reading the inputs now is unavailable evidence, not a non-match.
    let pending = run(
        &mut index,
        "SELECT args[0], baml_value_state(args[0]) FROM calls",
    );
    assert_eq!(
        pending.rows,
        vec![vec![Json::Null, json!("capture_pending")]]
    );
    assert_eq!(pending.outcome.status, Status::Incomplete);
    assert_eq!(pending.outcome.diagnostics[0].code, "capture_pending");
    recording.write(
        2,
        proto::RecordingFile {
            spans: Some(spans(vec![announce(Some([7; 16]))])),
            ..Default::default()
        },
    );
    index.refresh().unwrap();
    assert_eq!(
        one(
            &mut index,
            "SELECT status, args_state, args_cas_id FROM calls"
        ),
        vec![json!("ok"), json!("reference"), json!("07".repeat(16))]
    );
    assert!(run(&mut index, "SELECT * FROM issues").rows.is_empty());
    // The inputs blob was never delivered: an explicit unavailable outcome.
    let result = run(&mut index, "SELECT args[0] FROM calls");
    assert_eq!(result.rows, vec![vec![Json::Null]]);
    assert_eq!(result.outcome.status, Status::Incomplete);
    assert_eq!(result.outcome.diagnostics[0].code, "cas_missing");
}

#[test]
fn conflicting_evidence_is_reported_and_first_definitions_kept() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 3);
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions("user.Ask")),
            clock_states: Some(state(proto::TimingStatus::Valid)),
            spans: Some(spans(vec![complete(THREAD, 1)])),
            ..Default::default()
        },
    );
    let mut conflicting = definitions("user.Other");
    conflicting.clock_epochs[0].multiplier = 2;
    recording.write(
        2,
        proto::RecordingFile {
            definitions: Some(conflicting),
            spans: Some(spans(vec![complete(99, 1)])),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    let codes: Vec<Json> = run(&mut index, "SELECT code FROM issues ORDER BY code")
        .rows
        .into_iter()
        .map(|mut row| row.remove(0))
        .collect();
    assert_eq!(
        codes,
        vec![
            json!("call_evidence_conflict"),
            json!("clock_epoch_conflict"),
            json!("function_metadata_conflict")
        ]
    );
    assert_eq!(
        one(&mut index, "SELECT fqn, timing_state FROM calls"),
        vec![json!("user.Ask"), json!("conflicted")],
        "first metadata kept; a conflicting clock makes timing unavailable"
    );
}

#[test]
fn gaps_and_invalid_files_stop_indexing_until_repaired() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 4);
    for sequence in [1, 2, 4, 5] {
        recording.write(sequence, only_aggregate(1));
    }
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    assert_eq!(
        one(
            &mut index,
            "SELECT state, indexed_sequence, observed_sequence, blocked_sequence FROM recordings"
        ),
        vec![json!("gap"), json!(2), json!(5), json!(3)]
    );
    assert_eq!(
        count(&mut index),
        json!(2),
        "files beyond a gap are not facts"
    );
    let gap = run(&mut index, "SELECT COUNT(*) FROM calls");
    assert_eq!(gap.outcome.status, Status::Incomplete);
    assert_eq!(gap.outcome.diagnostics[0].code, "recording_gap");

    recording.write(3, only_aggregate(1));
    let metrics = index.refresh().unwrap();
    assert_eq!(metrics.files_applied, 3);
    assert_eq!(metrics.recordings_rebuilt, 0);
    assert_eq!(count(&mut index), json!(5));
    assert_eq!(
        one(&mut index, "SELECT state FROM recordings")[0],
        json!("unsealed")
    );

    std::fs::write(recording.path(6), b"not a recording file").unwrap();
    recording.write(7, only_aggregate(1));
    index.refresh().unwrap();
    assert_eq!(
        one(
            &mut index,
            "SELECT state, indexed_sequence, blocked_sequence FROM recordings"
        ),
        vec![json!("invalid_file"), json!(5), json!(6)]
    );
    assert_eq!(count(&mut index), json!(5));
    // Unchanged invalid file: neither decoded again nor written about.
    let again = index.refresh().unwrap();
    assert_eq!((again.files_decoded, again.transactions), (0, 0));
    recording.write(6, only_aggregate(1));
    let repaired = index.refresh().unwrap();
    assert_eq!(repaired.files_applied, 2);
    assert_eq!(count(&mut index), json!(7));
}

#[test]
fn changed_or_deleted_applied_files_rebuild_from_the_remaining_prefix() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 5);
    for sequence in 1..=4 {
        recording.write(sequence, only_aggregate(1));
    }
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    assert_eq!(count(&mut index), json!(4));

    // Metadata-only change: contents hashed, found equal, nothing rebuilt.
    let file = std::fs::File::options()
        .write(true)
        .open(recording.path(2))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    drop(file);
    let touched = index.refresh().unwrap();
    assert_eq!(
        (
            touched.files_rehashed,
            touched.files_decoded,
            touched.recordings_rebuilt
        ),
        (1, 0, 0)
    );
    let quiet = index.refresh().unwrap();
    assert_eq!(
        (quiet.files_rehashed, quiet.transactions),
        (0, 0),
        "restamped"
    );

    // Changed contents: the old interpretation is discarded, never appended to.
    recording.write(2, only_aggregate(10));
    let rebuilt = index.refresh().unwrap();
    assert_eq!(rebuilt.recordings_rebuilt, 1);
    assert_eq!(rebuilt.files_applied, 4);
    assert_eq!(count(&mut index), json!(13));

    // A vanished applied file: rebuild from the prefix before it.
    std::fs::remove_file(recording.path(3)).unwrap();
    index.refresh().unwrap();
    assert_eq!(count(&mut index), json!(11));
    assert_eq!(
        one(
            &mut index,
            "SELECT state, indexed_sequence, blocked_sequence FROM recordings"
        ),
        vec![json!("gap"), json!(2), json!(3)]
    );
    std::fs::remove_dir_all(&recording.dir).unwrap();
    let removed = index.refresh().unwrap();
    assert_eq!(removed.recordings_removed, 1);
    assert!(run(&mut index, "SELECT * FROM recordings").rows.is_empty());
}

fn dump(index: &mut Index) -> Vec<Vec<Vec<Json>>> {
    [
        "SELECT * FROM recordings ORDER BY 1",
        "SELECT * FROM executions ORDER BY 1",
        "SELECT call_id, execution_id, fqn, status, duration_ns, timing_state, args, output, error FROM calls ORDER BY 1",
        "SELECT * FROM function_stats ORDER BY 1, 3",
        "SELECT * FROM issues ORDER BY 1, 2, 3, 4",
    ]
    .into_iter()
    .map(|sql| run(index, sql).rows)
    .collect()
}

#[test]
fn schema_changes_rebuild_and_equal_evidence_gives_equal_rows() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 6);
    recording.write(
        1,
        proto::RecordingFile {
            spans: Some(spans(vec![announce(None)])),
            aggregates: Some(aggregate(2, 7)),
            ..Default::default()
        },
    );
    recording.write(
        2,
        proto::RecordingFile {
            definitions: Some(definitions("user.Ask")),
            clock_states: Some(state(proto::TimingStatus::Valid)),
            spans: Some(spans(vec![complete(THREAD, 1)])),
            aggregates: Some(aggregate(1, 3)),
            ..Default::default()
        },
    );
    // Incremental: one file per refresh.
    let incremental = tempfile::tempdir().unwrap();
    let mut index = Index::for_project(project.path(), options()).unwrap();
    std::fs::rename(recording.path(2), incremental.path().join("2")).unwrap();
    index.refresh().unwrap();
    std::fs::rename(incremental.path().join("2"), recording.path(2)).unwrap();
    index.refresh().unwrap();
    let expected = dump(&mut index);
    assert_eq!(count(&mut index), json!(3));

    // A database from another schema version is never queried as-is.
    let db = store::database_path(index.layout());
    let other = rusqlite::Connection::open(&db).unwrap();
    other.pragma_update(None, "user_version", 999).unwrap();
    drop(other);
    let mut stale = Index::for_project(project.path(), options()).unwrap();
    let error = stale
        .query(&QueryRequest {
            sql: "SELECT 1".into(),
            ..QueryRequest::default()
        })
        .unwrap_err();
    assert!(matches!(error, Error::Unsupported(_)), "{error}");
    let rebuilt = stale.refresh().unwrap();
    assert!(rebuilt.schema_rebuilt);
    assert_eq!(rebuilt.files_applied, 2);
    assert_eq!(
        dump(&mut stale),
        expected,
        "rebuild from the same files gives the same rows"
    );
}

#[test]
fn a_held_writer_lock_times_out_instead_of_answering_stale() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 7);
    recording.write(1, only_aggregate(1));
    let mut index = Index::for_project(
        project.path(),
        IndexOptions {
            refresh: RefreshOptions {
                lock_timeout: Duration::from_millis(50),
                ..RefreshOptions::default()
            },
            ..options()
        },
    )
    .unwrap();
    let (held, _) =
        store::WriterLock::acquire(&store::lock_path(index.layout()), Duration::from_secs(1))
            .unwrap();
    let error = index
        .refresh_and_query(&QueryRequest {
            sql: "SELECT COUNT(*) FROM recordings".into(),
            ..QueryRequest::default()
        })
        .unwrap_err();
    assert!(matches!(error, Error::LockTimeout(_)), "{error}");
    drop(held);
    index.refresh().unwrap();
    assert_eq!(count(&mut index), json!(1));
}

#[test]
fn read_transactions_keep_their_snapshot_while_files_are_applied() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 8);
    recording.write(1, only_aggregate(1));
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    let reader = rusqlite::Connection::open(store::database_path(index.layout())).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let before: i64 = reader
        .query_row("SELECT SUM(call_count) FROM aggregate", [], |r| r.get(0))
        .unwrap();
    recording.write(2, only_aggregate(1));
    index.refresh().unwrap();
    let during: i64 = reader
        .query_row("SELECT SUM(call_count) FROM aggregate", [], |r| r.get(0))
        .unwrap();
    assert_eq!((before, during), (1, 1), "the open read keeps its snapshot");
    reader.execute_batch("COMMIT").unwrap();
    let after: i64 = reader
        .query_row("SELECT SUM(call_count) FROM aggregate", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, 2);
}

/// Child mode: refresh one file per transaction under a fault spec.
#[test]
fn crash_child() {
    let Ok(project) = std::env::var("RECOVERY_CHILD_PROJECT") else {
        return;
    };
    let mut index = Index::for_project(
        Path::new(&project),
        IndexOptions {
            refresh: RefreshOptions {
                batch_files: 1,
                ..RefreshOptions::default()
            },
            ..options()
        },
    )
    .unwrap();
    index.refresh().unwrap();
}

fn crash(project: &Path, fault: &str) {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crash_child",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env("RECOVERY_CHILD_PROJECT", project)
        .env("BAML_QUERY_FAULT", fault)
        .status()
        .unwrap();
    assert!(!status.success(), "the child must crash at {fault}");
}

#[test]
fn crashes_around_commit_neither_lose_nor_double_apply_files() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 9);
    for sequence in 1..=3 {
        recording.write(sequence, only_aggregate(1));
    }
    // Killed before the first commit: nothing was applied.
    crash(project.path(), "before_commit:1");
    let mut index = Index::for_project(project.path(), options()).unwrap();
    let metrics = index.refresh().unwrap();
    assert_eq!(metrics.files_applied, 3);
    assert_eq!(count(&mut index), json!(3));

    for sequence in 4..=6 {
        recording.write(sequence, only_aggregate(1));
    }
    // Killed right after committing file 4: its ledger row came with it.
    crash(project.path(), "after_commit:1");
    let metrics = index.refresh().unwrap();
    assert_eq!((metrics.files_decoded, metrics.files_applied), (2, 2));
    assert_eq!(count(&mut index), json!(6));
    let ledger: i64 = index
        .connection()
        .query_row("SELECT COUNT(*) FROM ledger", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ledger, 6);
}

#[test]
fn value_callbacks_respect_the_time_budget() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 10);
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions("user.Ask")),
            spans: Some(spans(vec![announce(Some([1; 16]))])),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), options()).unwrap();
    index.refresh().unwrap();
    let error = index
        .query(&QueryRequest {
            sql: "SELECT args FROM calls".into(),
            budgets: baml_query_btel::Budgets {
                max_duration: Some(Duration::ZERO),
                ..baml_query_btel::Budgets::default()
            },
            ..QueryRequest::default()
        })
        .unwrap_err();
    assert!(matches!(error, Error::Budget(_)), "{error}");
}
