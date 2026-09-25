//! Sites and error evidence under old, late, partial and malformed files.
use std::path::{Path, PathBuf};

use baml_query_btel::{Index, IndexOptions, QueryRequest};
use btel_recorder::proto::{self, function_definition::Resolution, span_event::Event};
use prost::Message as _;
use serde_json::{Value as Json, json};

const THREAD: u64 = 1;
const EPOCH: u64 = 5;
const CALLER: u64 = 10;
const CALLEE: u64 = 11;
const FILE: u32 = 3;

struct Recording {
    dir: PathBuf,
    id: [u8; 16],
    minor: u32,
}

impl Recording {
    fn new(project: &Path, tag: u8, minor: u32) -> Self {
        let id = [tag; 16];
        let dir = project
            .join(".baml/btel/recordings")
            .join(btel_reader::discovery::hex(&id));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir, id, minor }
    }
    fn write(&self, sequence: u64, mut file: proto::RecordingFile) {
        file.header = Some(proto::RecordingHeader {
            format_major: 2,
            format_minor: self.minor,
            recording_id: self.id.to_vec(),
            source_snapshot_id: None,
        });
        file.sequence = sequence;
        let part = self.dir.join(format!("{sequence:020}.btel.part"));
        std::fs::write(&part, file.encode_to_vec()).unwrap();
        std::fs::rename(part, self.dir.join(format!("{sequence:020}.btel"))).unwrap();
    }
}

fn metadata(function: u64, fqn: &str, map: Option<proto::SourceMap>) -> proto::FunctionDefinition {
    proto::FunctionDefinition {
        function_id: function,
        resolution: Some(Resolution::Metadata(proto::FunctionMetadata {
            fqn: fqn.into(),
            display_name: fqn.into(),
            source_file: Some("main.baml".into()),
            source_span: Some(proto::SourceSpan {
                file_id: FILE,
                start: 0,
                end: 500,
            }),
            kind: proto::FunctionKind::Bytecode as i32,
            source_map: map,
            ..Default::default()
        })),
    }
}

/// Entries at PCs 2, 10 and 20 of a 40-byte function; the last one lies in
/// `last_file`.
fn map(last_file: u32) -> proto::SourceMap {
    proto::SourceMap {
        coordinate: proto::PcCoordinate::CompactByteOffset as i32,
        code_bytes: 40,
        pc: vec![2, 10, 20],
        file_id: vec![FILE, FILE, last_file],
        start: vec![100, 120, 140],
        end: vec![110, 130, 150],
        line: vec![4, 5, 6],
    }
}

fn path(id: u32, caller: Option<u64>, pc: u32) -> proto::CallPathDefinition {
    proto::CallPathDefinition {
        call_path_id: id,
        thread_id: THREAD,
        parent_call_path_id: 0,
        visible_caller_function_id: caller,
        caller_pc: pc,
        callee_function_id: CALLEE,
        edge: proto::CallPathEdge::Synchronous as i32,
    }
}

fn base() -> proto::Definitions {
    proto::Definitions {
        threads: vec![proto::ThreadDefinition {
            thread_id: THREAD,
            parent_id: None,
            spawn_call_path_id: 0,
            started_at_ticks: 10,
            clock_epoch_id: EPOCH,
        }],
        clock_epochs: vec![proto::ClockEpochDefinition {
            epoch_id: EPOCH,
            domain_id: 1,
            source: proto::ClockSource::OsMonotonic as i32,
            multiplier: 1,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn failed_call(id: u64, path: u32) -> proto::SpanBatch {
    proto::SpanBatch {
        sections: vec![proto::ThreadSection {
            thread_id: THREAD,
            events: vec![proto::SpanEvent {
                event: Some(Event::FunctionCompletion(proto::FunctionCompletion {
                    id,
                    parent_id: THREAD,
                    node: u64::from(path) << 1,
                    entered_at_ticks: 20,
                    exited_at_ticks: 30,
                    completion_flags: 2,
                    ..Default::default()
                })),
            }],
        }],
    }
}

fn rows(index: &mut Index, sql: &str) -> Vec<Vec<Json>> {
    index
        .refresh_and_query(&QueryRequest {
            sql: sql.into(),
            ..QueryRequest::default()
        })
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .rows
}

fn open(project: &Path) -> Index {
    Index::for_project(project, IndexOptions::default()).unwrap()
}

#[test]
fn older_recordings_keep_failed_calls_and_say_what_they_lack() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, 1);
    let mut definitions = base();
    definitions.functions = vec![
        metadata(CALLER, "user.Caller", None),
        metadata(CALLEE, "user.Callee", None),
    ];
    definitions.call_paths = vec![path(1, None, 0), path(2, Some(CALLER), 12)];
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions),
            spans: Some(failed_call(100, 2)),
            ..Default::default()
        },
    );
    let mut index = open(project.path());
    assert_eq!(
        rows(
            &mut index,
            "SELECT local_call_path_id, call_site_state, call_site_line FROM call_paths ORDER BY 1"
        ),
        [
            vec![json!(1), json!("no_caller"), Json::Null],
            vec![json!(2), json!("no_source_map"), Json::Null]
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT status, call_site_state, error_raise_id, error_link_state FROM error_calls"
        ),
        [vec![
            json!("errored"),
            json!("no_source_map"),
            Json::Null,
            json!("not_recorded")
        ]]
    );
    assert!(rows(&mut index, "SELECT * FROM error_raises").is_empty());
    assert!(rows(&mut index, "SELECT * FROM error_occurrences").is_empty());
}

#[test]
fn sites_resolve_when_definitions_arrive_later_and_bad_pcs_stay_explicit() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 2, 2);
    let mut first = base();
    first.call_paths = vec![
        path(1, Some(CALLER), 12),
        path(2, Some(CALLER), 40),
        path(3, Some(CALLER), u32::MAX),
        path(4, Some(CALLER), 1),
        path(5, Some(CALLER), 25),
    ];
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(first),
            ..Default::default()
        },
    );
    let mut index = open(project.path());
    let states = |index: &mut Index| {
        rows(
            index,
            "SELECT call_site_state, call_site_line, call_site_start, call_site_end
             FROM call_paths ORDER BY local_call_path_id",
        )
    };
    assert!(states(&mut index).iter().all(|row| row
        == &vec![
            json!("function_unresolved"),
            Json::Null,
            Json::Null,
            Json::Null
        ]));

    // The caller's map arrives in a later file; the last entry is in another
    // compiler file than the function's own.
    let second = proto::Definitions {
        functions: vec![metadata(CALLER, "user.Caller", Some(map(FILE + 1)))],
        ..Default::default()
    };
    recording.write(
        2,
        proto::RecordingFile {
            definitions: Some(second),
            ..Default::default()
        },
    );
    assert_eq!(
        states(&mut index),
        [
            vec![json!("resolved"), json!(5), json!(120), json!(130)],
            vec![json!("pc_out_of_range"), Json::Null, Json::Null, Json::Null],
            vec![json!("sentinel_pc"), Json::Null, Json::Null, Json::Null],
            vec![json!("unmapped_pc"), Json::Null, Json::Null, Json::Null],
            vec![json!("foreign_file"), Json::Null, Json::Null, Json::Null],
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT call_site_file FROM call_paths WHERE local_call_path_id = 1"
        ),
        [vec![json!("main.baml")]]
    );

    // A malformed map is reported and its sites say so.
    let recording = Recording::new(project.path(), 3, 2);
    let mut bad = map(FILE);
    bad.pc = vec![10, 2, 20];
    let mut definitions = base();
    definitions.functions = vec![metadata(CALLER, "user.Caller", Some(bad))];
    definitions.call_paths = vec![path(1, Some(CALLER), 12)];
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions),
            ..Default::default()
        },
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT p.call_site_state FROM call_paths p
             WHERE p.recording_id = '03030303030303030303030303030303'"
        ),
        [vec![json!("invalid_source_map")]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT code FROM issues WHERE code = 'source_map_invalid'"
        )
        .len(),
        1
    );
}

fn raise(id: u64, origin: Option<u64>) -> proto::ErrorRaise {
    proto::ErrorRaise {
        raise_id: id,
        thread_id: THREAD,
        raised_at_ticks: 25,
        kind: proto::RaiseKind::Throw as i32,
        function_id: Some(CALLER),
        pc: Some(12),
        origin_state: if origin.is_some() {
            proto::OriginState::Proven as i32
        } else {
            proto::OriginState::Fresh as i32
        },
        origin_raise_id: origin,
        origin_via: if origin.is_some() {
            proto::OriginVia::Rethrow as i32
        } else {
            0
        },
        frames: vec![
            proto::ErrorFrame {
                function_id: Some(CALLER),
                pc: Some(12),
                native: false,
            },
            proto::ErrorFrame {
                function_id: Some(CALLEE),
                pc: None,
                native: true,
            },
        ],
        frame_count: 2,
        ..Default::default()
    }
}

fn link(raise: u64, call: u64) -> proto::ErrorCallLink {
    proto::ErrorCallLink {
        raise_id: raise,
        call_id: call,
        role: proto::ErrorLinkRole::Unwound as i32,
    }
}

fn caught(raise: u64) -> proto::ErrorUnwindEnd {
    proto::ErrorUnwindEnd {
        raise_id: raise,
        result: proto::UnwindResult::Caught as i32,
        handler_function_id: Some(CALLER),
        handler_pc: Some(20),
        unwound_frames: 1,
    }
}

#[test]
fn error_evidence_arrives_in_pieces_and_bad_records_are_explicit() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 4, 2);
    let mut definitions = base();
    definitions.functions = vec![
        metadata(CALLER, "user.Caller", Some(map(FILE))),
        metadata(CALLEE, "user.Callee", None),
    ];
    definitions.call_paths = vec![path(1, Some(CALLER), 12)];
    // File 1, a live prefix: a raise and its link, before its end and before
    // the failed call itself is indexed.
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions),
            errors: Some(proto::ErrorBatch {
                raises: vec![raise(200, None)],
                call_links: vec![link(200, 100)],
                unwind_ends: vec![],
            }),
            ..Default::default()
        },
    );
    let mut index = open(project.path());
    let summary = "SELECT raise_id, occurrence_id, unwind_result, evidence_state, site_state,
        site_line, handler_site_line, failed_calls, stack_depth, stack_state
      FROM error_raises ORDER BY raise_id";
    let prefix = |id: u64| format!("04040404040404040404040404040404:{id}");
    assert_eq!(
        rows(&mut index, summary),
        [vec![
            json!(prefix(200)),
            json!(prefix(200)),
            json!("end_not_indexed"),
            json!("end_not_indexed"),
            json!("resolved"),
            json!(5),
            Json::Null,
            json!(1),
            json!(2),
            json!("complete")
        ]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT position, fqn, native, site_state, site_line FROM error_frames ORDER BY position"
        ),
        [
            vec![
                json!(0),
                json!("user.Caller"),
                json!(0),
                json!("resolved"),
                json!(5)
            ],
            vec![
                json!(1),
                json!("user.Callee"),
                json!(1),
                json!("no_pc"),
                Json::Null
            ]
        ]
    );

    // File 2: the failed call, the end, an end without its raise, a proven
    // rethrow whose origin is not indexed, a conflicting duplicate, a second
    // raise claiming the same call, and a malformed raise.
    let mut duplicate = raise(200, None);
    duplicate.pc = Some(20);
    recording.write(
        2,
        proto::RecordingFile {
            spans: Some(failed_call(100, 1)),
            errors: Some(proto::ErrorBatch {
                raises: vec![raise(201, Some(900)), duplicate, raise(0, None)],
                call_links: vec![link(201, 100)],
                unwind_ends: vec![caught(200), caught(202)],
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        rows(&mut index, summary),
        [
            vec![
                json!(prefix(200)),
                json!(prefix(200)),
                json!("caught"),
                json!("conflicted"),
                json!("resolved"),
                json!(5),
                json!(6),
                json!(1),
                json!(2),
                json!("complete")
            ],
            vec![
                json!(prefix(201)),
                json!(prefix(900)),
                json!("end_not_indexed"),
                json!("end_not_indexed"),
                json!("resolved"),
                json!(5),
                Json::Null,
                json!(0),
                json!(2),
                json!("complete")
            ],
            vec![
                json!(prefix(202)),
                Json::Null,
                json!("caught"),
                json!("raise_missing"),
                json!("raise_missing"),
                Json::Null,
                json!(6),
                json!(0),
                Json::Null,
                Json::Null
            ],
        ]
    );
    // The call stays with the first raise that claimed it.
    assert_eq!(
        rows(
            &mut index,
            "SELECT error_raise_id, error_occurrence_id, error_link_state FROM error_calls"
        ),
        [vec![
            json!(prefix(200)),
            json!(prefix(200)),
            json!("linked")
        ]]
    );
    let mut issues: Vec<String> = rows(&mut index, "SELECT code FROM issues")
        .into_iter()
        .map(|row| row[0].as_str().unwrap().to_owned())
        .collect();
    issues.sort();
    assert_eq!(
        issues,
        [
            "error_evidence_invalid",
            "error_link_conflict",
            "error_raise_conflict"
        ]
    );
    // Only the fresh raise is an occurrence; the rethrow of an unindexed
    // origin names it but does not invent a row for it.
    assert_eq!(
        rows(
            &mut index,
            "SELECT occurrence_id, raises, failed_calls FROM error_occurrences"
        ),
        [vec![json!(prefix(200)), json!(1), json!(1)]]
    );
}
