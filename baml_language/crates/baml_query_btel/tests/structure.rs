//! Thread, calling-context and statistics semantics on hand-built recording
//! files, where every tick and count is known.
use std::path::{Path, PathBuf};

use baml_query_btel::{Index, IndexOptions, QueryRequest, QueryResult};
use btel_recorder::proto::{self, span_event::Event};
use prost::Message as _;
use serde_json::{Value as Json, json};

const EPOCH: u64 = 5;
const ROOT: u64 = 1;
/// `btel_settings::encoding::FORMAT_MAJOR`.
const FORMAT_MAJOR: u32 = 2;

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
    fn hex(&self) -> String {
        btel_reader::discovery::hex(&self.id)
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
        std::fs::rename(part, self.dir.join(format!("{sequence:020}.btel"))).unwrap();
    }
}

/// One tick is one nanosecond.
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

fn function(id: u64, fqn: &str, params: &[&str]) -> proto::FunctionDefinition {
    proto::FunctionDefinition {
        function_id: id,
        resolution: Some(proto::function_definition::Resolution::Metadata(
            proto::FunctionMetadata {
                fqn: fqn.into(),
                display_name: fqn.rsplit('.').next().unwrap().into(),
                kind: proto::FunctionKind::Bytecode as i32,
                origin: proto::FunctionOrigin::UserDefined as i32,
                namespace: vec!["user".into()],
                argument_layout: Some(proto::ArgumentLayout {
                    slots: params
                        .iter()
                        .map(|name| proto::ArgumentSlot {
                            name: Some((*name).into()),
                            receiver: false,
                        })
                        .collect(),
                }),
                ..Default::default()
            },
        )),
    }
}

fn path(id: u32, thread: u64, parent: u32, callee: u64, spawn: bool) -> proto::CallPathDefinition {
    proto::CallPathDefinition {
        call_path_id: id,
        thread_id: thread,
        parent_call_path_id: parent,
        visible_caller_function_id: None,
        caller_pc: id * 4,
        callee_function_id: callee,
        edge: if spawn {
            proto::CallPathEdge::Spawn
        } else {
            proto::CallPathEdge::Synchronous
        } as i32,
    }
}

fn thread(id: u64, parent: Option<u64>, spawn_path: u32, started: u64) -> proto::ThreadDefinition {
    proto::ThreadDefinition {
        thread_id: id,
        parent_id: parent,
        spawn_call_path_id: spawn_path,
        started_at_ticks: started,
        clock_epoch_id: EPOCH,
    }
}

fn valid() -> proto::ClockStateBatch {
    state(proto::TimingStatus::Valid)
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

fn delta(path: u32, reentry: bool, count: u64, duration: u64, wait: u64) -> proto::AggregateDelta {
    proto::AggregateDelta {
        node: (u64::from(path) << 1) | u64::from(reentry),
        count,
        total_duration_ticks: duration,
        total_self_await_ticks: wait,
        outcomes: None,
    }
}

fn outcomes(
    path: u32,
    reentry: bool,
    count: u64,
    errored: u64,
    cancelled: u64,
) -> proto::AggregateDelta {
    proto::AggregateDelta {
        outcomes: Some(proto::AggregateOutcomes { errored, cancelled }),
        ..delta(path, reentry, count, 10, 0)
    }
}

fn outcome_file(entries: Vec<proto::AggregateDelta>) -> proto::RecordingFile {
    proto::RecordingFile {
        definitions: Some(proto::Definitions {
            functions: vec![function(10, "user.A", &[]), function(11, "user.B", &[])],
            call_paths: vec![
                path(1, ROOT, 0, 10, false),
                path(2, ROOT, 1, 10, false),
                path(3, ROOT, 1, 11, false),
                path(4, ROOT, 1, 11, false),
            ],
            threads: vec![thread(ROOT, None, 0, 0)],
            clock_epochs: vec![epoch()],
        }),
        clock_states: Some(valid()),
        aggregates: Some(proto::AggregateBatch { entries }),
        ..Default::default()
    }
}

const OUTCOMES: &str = "SELECT call_path_id, completed_calls, ok_calls, errored_calls,
    cancelled_calls, outcome_state FROM call_path_stats ORDER BY call_path_id";
const FUNCTION_OUTCOMES: &str = "SELECT fqn, completed_calls, ok_calls, errored_calls,
    cancelled_calls, outcome_state FROM function_stats ORDER BY fqn";

#[test]
fn outcomes_cover_recursive_completions_and_ignore_unobserved_contexts() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    let id = |n| json!(format!("{}:p{n}", recording.hex()));
    recording.write(
        1,
        outcome_file(vec![
            outcomes(1, false, 10, 2, 1),
            outcomes(1, true, 5, 1, 2),
            outcomes(2, false, 3, 0, 0),
            outcomes(3, false, 1, 1, 0),
        ]),
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, OUTCOMES),
        vec![
            vec![
                id(1),
                json!(15),
                json!(9),
                json!(3),
                json!(3),
                json!("recorded")
            ],
            vec![
                id(2),
                json!(3),
                json!(3),
                json!(0),
                json!(0),
                json!("recorded")
            ],
            vec![
                id(3),
                json!(1),
                json!(0),
                json!(1),
                json!(0),
                json!("recorded")
            ],
            vec![
                id(4),
                json!(0),
                json!(0),
                json!(0),
                json!(0),
                json!("none_observed")
            ],
        ]
    );
    assert_eq!(
        rows(&mut index, FUNCTION_OUTCOMES),
        vec![
            vec![
                json!("user.A"),
                json!(18),
                json!(12),
                json!(3),
                json!(3),
                json!("recorded")
            ],
            vec![
                json!("user.B"),
                json!(1),
                json!(0),
                json!(1),
                json!(0),
                json!("recorded")
            ],
        ]
    );
    assert_eq!(
        rows(&mut index, "SELECT COUNT(*) FROM calls"),
        vec![vec![json!(0)]]
    );
    // Invalid clock evidence only invalidates timing, never population counts.
    recording.write(
        2,
        proto::RecordingFile {
            clock_states: Some(state(proto::TimingStatus::Discontinuity)),
            ..Default::default()
        },
    );
    let result = run(&mut index, FUNCTION_OUTCOMES);
    assert_eq!(
        result.rows[0][2..],
        [json!(12), json!(3), json!(3), json!("recorded")]
    );
    assert_eq!(result.outcome.query.values.cas_loads, 0);
}

#[test]
fn old_and_mixed_outcomes_remain_unknown_across_refresh_and_rebuild() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    let id = |n| json!(format!("{}:p{n}", recording.hex()));
    // No outcome message means old evidence, not successful completion.
    recording.write(1, outcome_file(vec![delta(1, false, 4, 10, 0)]));
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, OUTCOMES)[0],
        vec![
            id(1),
            json!(4),
            Json::Null,
            Json::Null,
            Json::Null,
            json!("not_recorded")
        ]
    );
    recording.write(
        2,
        proto::RecordingFile {
            aggregates: Some(proto::AggregateBatch {
                entries: vec![
                    outcomes(1, false, 2, 1, 0),
                    outcomes(2, false, 3, 0, 1),
                    // A completion whose context definition arrives in the next file.
                    outcomes(5, false, 2, 0, 1),
                ],
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        rows(&mut index, OUTCOMES)[0],
        vec![
            id(1),
            json!(6),
            Json::Null,
            Json::Null,
            Json::Null,
            json!("partial")
        ]
    );
    recording.write(
        3,
        proto::RecordingFile {
            definitions: Some(proto::Definitions {
                call_paths: vec![path(5, ROOT, 1, 11, false)],
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let expected = vec![
        vec![
            json!("user.A"),
            json!(9),
            Json::Null,
            Json::Null,
            Json::Null,
            json!("partial"),
        ],
        vec![
            json!("user.B"),
            json!(2),
            json!(1),
            json!(0),
            json!(1),
            json!("recorded"),
        ],
    ];
    assert_eq!(rows(&mut index, FUNCTION_OUTCOMES), expected);
    let repeated = run(&mut index, FUNCTION_OUTCOMES);
    assert_eq!(repeated.rows, expected);
    assert_eq!(repeated.outcome.refresh.as_ref().unwrap().files_applied, 0);

    // A v4 cache has no outcome evidence and must be rebuilt from BTEL.
    let db = baml_query_btel::store::database_path(index.layout());
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();
    drop(conn);
    let mut rebuilt = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let rebuilt_result = run(&mut rebuilt, FUNCTION_OUTCOMES);
    assert!(
        rebuilt_result
            .outcome
            .refresh
            .as_ref()
            .unwrap()
            .schema_rebuilt
    );
    assert_eq!(rebuilt_result.rows, expected);
    // Removing the newest definition rebuilds only the trustworthy prefix.
    std::fs::remove_file(recording.dir.join("00000000000000000003.btel")).unwrap();
    assert_eq!(rows(&mut rebuilt, OUTCOMES)[0][5], json!("partial"));
}

#[test]
fn missing_outcomes_poison_rollups_across_contexts_and_reentry_nodes() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    recording.write(
        1,
        outcome_file(vec![
            outcomes(1, false, 4, 1, 1),
            delta(1, true, 2, 10, 0),
            outcomes(3, false, 2, 0, 0),
            delta(4, false, 1, 10, 0),
        ]),
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, FUNCTION_OUTCOMES),
        vec![
            vec![
                json!("user.A"),
                json!(6),
                Json::Null,
                Json::Null,
                Json::Null,
                json!("partial")
            ],
            vec![
                json!("user.B"),
                json!(3),
                Json::Null,
                Json::Null,
                Json::Null,
                json!("partial")
            ],
        ]
    );
}

#[test]
fn invalid_outcome_counts_preserve_other_facts_and_report_an_issue() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    let id = |n| json!(format!("{}:p{n}", recording.hex()));
    recording.write(
        1,
        outcome_file(vec![
            outcomes(1, false, 2, 2, 1),
            // The validation addition itself must not wrap at u64::MAX.
            outcomes(2, false, u64::MAX, u64::MAX, 1),
        ]),
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, OUTCOMES)[0],
        vec![
            id(1),
            json!(2),
            Json::Null,
            Json::Null,
            Json::Null,
            json!("invalid")
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT completed_calls, inclusive_ns FROM call_path_stats WHERE call_path_id LIKE '%:p1'"
        ),
        vec![vec![json!(2), json!(10)]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT code, subject_kind FROM issues WHERE code = 'aggregate_outcome_invalid'"
        ),
        vec![vec![json!("aggregate_outcome_invalid"), json!("call_path_node")]; 2]
    );
}

#[test]
fn outcome_counts_do_not_wrap_when_nodes_or_function_rollups_overflow() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    let big = 3 << 61;
    recording.write(
        1,
        outcome_file(vec![
            outcomes(1, false, big, big, 0),
            outcomes(1, false, big, big, 0),
            outcomes(3, false, big, 0, big),
            outcomes(4, false, big, 0, big),
        ]),
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, FUNCTION_OUTCOMES),
        vec![
            vec![
                json!("user.A"),
                Json::Null,
                json!(0),
                Json::Null,
                json!(0),
                json!("overflow")
            ],
            vec![
                json!("user.B"),
                Json::Null,
                json!(0),
                json!(0),
                Json::Null,
                json!("overflow")
            ],
        ]
    );
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

fn thread_done(ticks: u64, outcome: proto::InvocationOutcome) -> Event {
    Event::ThreadCompletion(proto::ThreadCompletion {
        completed_at_ticks: ticks,
        outcome: outcome as i32,
    })
}

fn run(index: &mut Index, sql: &str) -> QueryResult {
    index
        .refresh_and_query(&QueryRequest {
            sql: sql.into(),
            ..QueryRequest::default()
        })
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn rows(index: &mut Index, sql: &str) -> Vec<Vec<Json>> {
    run(index, sql).rows
}

/// A calling-context tree with known aggregates: A (root, recursive) calls B
/// synchronously and spawns C; ticks are nanoseconds.
fn stats_recording(recording: &Recording, b_duration: u64, finished: bool) {
    let mut events = Vec::new();
    if finished {
        events.push(thread_done(1_000, proto::InvocationOutcome::Ok));
    }
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![
                    function(10, "user.A", &["n"]),
                    function(11, "user.B", &[]),
                    function(12, "user.C", &["x", "y"]),
                ],
                call_paths: vec![
                    path(1, ROOT, 0, 10, false),
                    path(2, ROOT, 1, 11, false),
                    path(3, ROOT, 1, 12, true),
                    // Defined, never completed.
                    path(4, ROOT, 2, 11, false),
                ],
                threads: vec![thread(ROOT, None, 0, 0)],
                clock_epochs: vec![epoch()],
            }),
            clock_states: Some(valid()),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![
                    // A: one outermost call (100) and two recursive
                    // re-entries (60 in total, inside the 100).
                    delta(1, false, 1, 100, 10),
                    delta(1, true, 2, 60, 5),
                    delta(2, false, 3, b_duration, 0),
                    // Spawned work is not A's child time.
                    delta(3, false, 1, 500, 0),
                ],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![section(ROOT, events)],
            }),
            ..Default::default()
        },
    );
}

const STATS: &str = "SELECT fqn, normal_completed_calls, reentry_completed_calls, completed_calls,
  inclusive_ns, reentry_duration_ns, invocation_duration_sum_ns, direct_child_ns, await_ns,
  self_ns, self_time_state, aggregate_state
  FROM call_path_stats ORDER BY call_path_id";

#[test]
fn child_time_tracks_late_definitions_deltas_invalidations_and_rebuilds() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    stats_recording(&recording, 30, true);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let sql = "SELECT direct_child_ns, self_ns FROM call_path_stats WHERE fqn = 'user.A'";
    assert_eq!(rows(&mut index, sql), vec![vec![json!(30), json!(55)]]);

    // A completed child arrives before the definition linking it to A.
    recording.write(
        2,
        proto::RecordingFile {
            aggregates: Some(proto::AggregateBatch {
                entries: vec![delta(5, false, 1, 7, 0)],
            }),
            ..Default::default()
        },
    );
    assert_eq!(rows(&mut index, sql), vec![vec![json!(30), json!(55)]]);
    recording.write(
        3,
        proto::RecordingFile {
            definitions: Some(proto::Definitions {
                call_paths: vec![path(5, ROOT, 1, 11, false)],
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert_eq!(rows(&mut index, sql), vec![vec![json!(37), json!(48)]]);
    recording.write(
        4,
        proto::RecordingFile {
            aggregates: Some(proto::AggregateBatch {
                entries: vec![
                    delta(5, false, 1, 5, 0),
                    // Recursion and spawned work must not be subtracted again.
                    delta(5, true, 1, 9_000, 0),
                    delta(3, false, 1, 9_000, 0),
                ],
            }),
            ..Default::default()
        },
    );
    assert_eq!(rows(&mut index, sql), vec![vec![json!(42), json!(43)]]);

    recording.write(
        5,
        proto::RecordingFile {
            clock_states: Some(state(proto::TimingStatus::Discontinuity)),
            ..Default::default()
        },
    );
    assert_eq!(rows(&mut index, sql), vec![vec![Json::Null, Json::Null]]);
    std::fs::remove_file(recording.dir.join(format!("{:020}.btel", 5))).unwrap();
    assert_eq!(rows(&mut index, sql), vec![vec![json!(42), json!(43)]]);
    std::fs::remove_file(recording.dir.join(format!("{:020}.btel", 4))).unwrap();
    assert_eq!(rows(&mut index, sql), vec![vec![json!(37), json!(48)]]);

    // Overflowed evidence makes the derived time unavailable, not zero.
    recording.write(
        4,
        proto::RecordingFile {
            aggregates: Some(proto::AggregateBatch {
                entries: vec![delta(5, false, 1, u64::MAX, 0)],
            }),
            ..Default::default()
        },
    );
    assert_eq!(rows(&mut index, sql), vec![vec![Json::Null, Json::Null]]);
}

#[test]
fn recursion_aware_path_statistics_have_exact_values() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    stats_recording(&recording, 30, true);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, STATS),
        vec![
            // 100 - 30 (B) - 15 (await, outer and recursive) = 55.
            vec![
                json!("user.A"),
                json!(1),
                json!(2),
                json!(3),
                json!(100),
                json!(60),
                json!(160),
                json!(30),
                json!(15),
                json!(55),
                json!("valid"),
                json!("observed_prefix")
            ],
            vec![
                json!("user.B"),
                json!(3),
                json!(0),
                json!(3),
                json!(30),
                json!(0),
                json!(30),
                json!(0),
                json!(0),
                json!(30),
                json!("valid"),
                json!("observed_prefix")
            ],
            vec![
                json!("user.C"),
                json!(1),
                json!(0),
                json!(1),
                json!(500),
                json!(0),
                json!(500),
                json!(0),
                json!(0),
                json!(500),
                json!("valid"),
                json!("observed_prefix")
            ],
            // Defined but not completed: zero observed, not a verified zero
            // time.
            vec![
                json!("user.B"),
                json!(0),
                json!(0),
                json!(0),
                Json::Null,
                json!(0),
                json!(0),
                json!(0),
                json!(0),
                Json::Null,
                json!("incomplete"),
                json!("none_observed")
            ],
        ]
    );
    // Structure: depths, edges, the spawn edge excluded from child time.
    assert_eq!(
        rows(
            &mut index,
            "SELECT fqn, depth, edge_kind, recorded_edge, caller_pc, structure_state
             FROM call_paths ORDER BY call_path_id"
        ),
        vec![
            vec![
                json!("user.A"),
                json!(0),
                json!("root"),
                json!("synchronous"),
                json!(4),
                json!("resolved")
            ],
            vec![
                json!("user.B"),
                json!(1),
                json!("call"),
                json!("synchronous"),
                json!(8),
                json!("resolved")
            ],
            vec![
                json!("user.C"),
                json!(1),
                json!("spawn"),
                json!("spawn"),
                json!(12),
                json!("resolved")
            ],
            vec![
                json!("user.B"),
                json!(2),
                json!("call"),
                json!("synchronous"),
                json!(16),
                json!("resolved")
            ],
        ]
    );
    // Function totals sum contexts; B's never-completed context has no
    // supported self time, so B's total is unavailable rather than partial.
    assert_eq!(
        rows(
            &mut index,
            "SELECT fqn, completed_calls, invocation_duration_sum_ns, await_ns, self_ns,
               self_time_state
             FROM function_stats ORDER BY fqn"
        ),
        vec![
            vec![
                json!("user.A"),
                json!(3),
                json!(160),
                json!(15),
                json!(55),
                json!("valid")
            ],
            vec![
                json!("user.B"),
                json!(3),
                json!(30),
                json!(0),
                Json::Null,
                json!("incomplete")
            ],
            vec![
                json!("user.C"),
                json!(1),
                json!(500),
                json!(0),
                json!(500),
                json!("valid")
            ],
        ]
    );
    // The raw nodes, exactly as recorded.
    assert_eq!(
        rows(
            &mut index,
            "SELECT fqn, reentry, completed_calls, completed_calls_exact, duration_sum_ns,
               duration_ticks_exact
             FROM call_path_nodes WHERE fqn = 'user.A' ORDER BY reentry"
        ),
        vec![
            vec![
                json!("user.A"),
                json!(0),
                json!(1),
                json!("1"),
                json!(100),
                json!("100")
            ],
            vec![
                json!("user.A"),
                json!(1),
                json!(2),
                json!("2"),
                json!(60),
                json!("60")
            ],
        ]
    );
    let hot = rows(
        &mut index,
        "SELECT fqn, self_ns FROM hot_call_paths ORDER BY self_ns DESC",
    );
    assert_eq!(
        hot,
        vec![
            vec![json!("user.C"), json!(500)],
            vec![json!("user.A"), json!(55)],
            vec![json!("user.B"), json!(30)],
        ]
    );
    // Execution totals come from the same aggregates.
    assert_eq!(
        rows(
            &mut index,
            "SELECT entry_fqn, entry_state, status, completed_calls, calls_retained, threads_total
             FROM executions"
        ),
        vec![vec![
            json!("user.A"),
            json!("resolved"),
            json!("ok"),
            json!(7),
            json!(0),
            json!(1)
        ]]
    );
    // Recorded metadata and parameter slots.
    assert_eq!(
        rows(
            &mut index,
            "SELECT fqn, definition_state, kind, origin, namespace, namespace_components,
               parameter_count, argument_layout_state
             FROM function_definitions WHERE fqn = 'user.C'"
        ),
        vec![vec![
            json!("user.C"),
            json!("resolved"),
            json!("bytecode"),
            json!("user"),
            json!("user"),
            json!("[\"user\"]"),
            json!(2),
            json!("known")
        ]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT position, name, is_receiver FROM function_parameters WHERE fqn = 'user.C'
             ORDER BY position"
        ),
        vec![
            vec![json!(0), json!("x"), json!(0)],
            vec![json!(1), json!("y"), json!(0)],
        ]
    );
}

#[test]
fn self_time_is_never_clamped() {
    // B's completed time exceeds A's: while the thread is unfinished, A's
    // own completion is simply not indexed yet.
    let project = tempfile::tempdir().unwrap();
    stats_recording(&Recording::new(project.path(), 1), 120, false);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let a = "SELECT self_ns, self_time_state FROM call_path_stats WHERE fqn = 'user.A'";
    assert_eq!(
        rows(&mut index, a),
        vec![vec![Json::Null, json!("incomplete")]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT self_time_state FROM call_path_stats WHERE fqn = 'user.C'"
        ),
        vec![vec![json!("provisional")]],
        "an unfinished thread's supported self time is provisional"
    );

    // On a finished thread the same numbers are inconsistent evidence.
    let project = tempfile::tempdir().unwrap();
    stats_recording(&Recording::new(project.path(), 1), 120, true);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, a),
        vec![vec![Json::Null, json!("underflow")]]
    );
}

#[test]
fn clock_invalidation_keeps_counts_and_explains_missing_time() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    stats_recording(&recording, 30, true);
    recording.write(
        2,
        proto::RecordingFile {
            clock_states: Some(state(proto::TimingStatus::Discontinuity)),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(
            &mut index,
            "SELECT completed_calls, inclusive_ns, self_ns, timing_state, self_time_state
             FROM call_path_stats WHERE fqn = 'user.A'"
        ),
        vec![vec![
            json!(3),
            Json::Null,
            Json::Null,
            json!("invalidated_discontinuity"),
            json!("invalidated_discontinuity")
        ]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT observed_status, is_final, timing_state, source, multiplier, utc_anchor_at
             FROM clocks"
        ),
        vec![vec![
            json!("discontinuity"),
            json!(0),
            json!("invalidated_discontinuity"),
            json!("os_monotonic"),
            json!("1"),
            json!("2026-09-21T14:13:20.000000Z")
        ]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT DISTINCT completed_calls IS NOT NULL, self_ns, timing_state FROM function_stats"
        ),
        vec![vec![
            json!(1),
            Json::Null,
            json!("invalidated_discontinuity")
        ]],
        "a function's timing is never reported valid over an invalid clock"
    );
    let issues = rows(&mut index, "SELECT code, subject_kind FROM issues");
    assert_eq!(
        issues,
        vec![vec![json!("clock_invalidated"), json!("clock")]]
    );
}

#[test]
fn end_markers_and_final_clocks_are_evidence_and_older_recordings_stay_unsealed() {
    const RECORDINGS: &str = "SELECT state, seal_state, prefix_state, indexed_sequence,
      terminal_sequence FROM recordings ORDER BY recording_id";
    let project = tempfile::tempdir().unwrap();
    // The terminal file carries the settled clock and the end marker.
    let terminal = || proto::RecordingFile {
        clock_states: Some(proto::ClockStateBatch {
            states: vec![proto::ClockEpochState {
                epoch_id: EPOCH,
                status: proto::TimingStatus::Valid as i32,
                r#final: true,
            }],
        }),
        end: Some(proto::RecordingEnd {}),
        ..Default::default()
    };
    // Current producer after a normal shutdown: data, then the terminal file.
    let ended = Recording::new(project.path(), 1);
    stats_recording(&ended, 30, true);
    ended.write(2, terminal());
    // An older producer's recording of the same run: no end, no final state.
    let legacy = Recording::new(project.path(), 2);
    stats_recording(&legacy, 30, true);
    // An end marker beyond a missing file is not applied; the gap stays visible.
    let gapped = Recording::new(project.path(), 3);
    stats_recording(&gapped, 30, true);
    gapped.write(3, terminal());

    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(&mut index, RECORDINGS),
        vec![
            vec![
                json!("sealed"),
                json!("sealed"),
                json!("complete_prefix"),
                json!(2),
                json!(2)
            ],
            vec![
                json!("unsealed"),
                json!("unsealed"),
                json!("complete_prefix"),
                json!(1),
                Json::Null
            ],
            vec![
                json!("gap"),
                json!("unsealed"),
                json!("gap"),
                json!(1),
                Json::Null
            ],
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT observed_status, is_final, timing_state FROM clocks ORDER BY recording_id"
        ),
        vec![
            vec![json!("valid"), json!(1), json!("valid")],
            vec![json!("valid"), json!(0), json!("valid")],
            vec![json!("valid"), json!(0), json!("valid")],
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT recording_id, code FROM issues WHERE code = 'sequence_gap'"
        ),
        vec![vec![json!(gapped.hex()), json!("sequence_gap")]]
    );
    // Run completion is separate: the same executions answer in every recording.
    assert_eq!(
        rows(&mut index, "SELECT DISTINCT status FROM executions"),
        vec![vec![json!("ok")]]
    );
    // The lost file arrives: the end now applies and the recording is sealed.
    gapped.write(2, proto::RecordingFile::default());
    assert_eq!(
        rows(&mut index, RECORDINGS)[2],
        vec![
            json!("sealed"),
            json!("sealed"),
            json!("complete_prefix"),
            json!(3),
            json!(3)
        ]
    );
    assert!(
        run(&mut index, "SELECT 1").outcome.unsealed,
        "the older recording still has no end"
    );
}

#[test]
fn unresolved_structure_stays_unresolved_until_evidence_arrives() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    // File 1: thread 2 is spawned by node 99, which nothing defines yet;
    // path 5's parent path 4 and path 6 are only referenced; thread 3 is
    // only referenced by its span section; the root is not defined yet.
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![function(10, "user.A", &[])],
                call_paths: vec![path(5, ROOT, 4, 10, false)],
                threads: vec![thread(2, Some(99), 0, 50)],
                clock_epochs: vec![epoch()],
            }),
            clock_states: Some(valid()),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![delta(5, false, 2, 20, 0), delta(6, false, 1, 5, 0)],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![section(
                    3,
                    vec![Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                        id: 200,
                        parent_id: 3,
                        call_path_id: 5,
                        entered_at_ticks: 60,
                        inputs_cas_id: None,
                    })],
                )],
            }),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert!(
        rows(&mut index, "SELECT * FROM executions").is_empty(),
        "no thread is known to be a root"
    );
    let threads = "SELECT thread_id, definition_state, structure_state, kind, parent_node_kind,
      execution_id FROM threads ORDER BY thread_id";
    let r = recording.hex();
    assert_eq!(
        rows(&mut index, threads),
        vec![
            vec![
                json!(format!("{r}:1")),
                json!("unresolved"),
                json!("unresolved"),
                Json::Null,
                Json::Null,
                Json::Null
            ],
            vec![
                json!(format!("{r}:2")),
                json!("resolved"),
                json!("unresolved"),
                json!("spawn"),
                Json::Null,
                Json::Null
            ],
            vec![
                json!(format!("{r}:3")),
                json!("unresolved"),
                json!("unresolved"),
                Json::Null,
                Json::Null,
                Json::Null
            ],
        ]
    );
    let paths = "SELECT local_call_path_id, definition_state, depth, structure_state
      FROM call_paths ORDER BY local_call_path_id";
    assert_eq!(
        rows(&mut index, paths),
        vec![
            vec![
                json!(4),
                json!("unresolved"),
                Json::Null,
                json!("unresolved")
            ],
            vec![json!(5), json!("resolved"), Json::Null, json!("unresolved")],
            vec![
                json!(6),
                json!("unresolved"),
                Json::Null,
                json!("unresolved")
            ],
        ]
    );
    let issues = "SELECT code, subject FROM issues ORDER BY code, subject";
    assert_eq!(
        rows(&mut index, issues),
        vec![
            vec![json!("call_path_undefined"), json!(format!("{r}:p4"))],
            vec![json!("call_path_undefined"), json!(format!("{r}:p6"))],
            vec![json!("thread_parent_unresolved"), json!(format!("{r}:2"))],
            vec![json!("thread_undefined"), json!(format!("{r}:1"))],
            vec![json!("thread_undefined"), json!(format!("{r}:3"))],
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT execution_id, parent_node_kind, structure_state FROM calls"
        ),
        vec![vec![Json::Null, json!("thread"), json!("unresolved")]],
        "thread 3 is known only as a thread id; its execution is unknown"
    );

    // File 2: the root, node 99 (a thread spawned by the root) and the
    // missing paths arrive; everything but thread 3 resolves.
    recording.write(
        2,
        proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![],
                call_paths: vec![
                    path(4, ROOT, 0, 10, false),
                    path(6, ROOT, 5, 10, false),
                    path(7, ROOT, 4, 10, true),
                ],
                threads: vec![thread(ROOT, None, 0, 0), thread(99, Some(ROOT), 7, 40)],
                clock_epochs: vec![epoch()],
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        rows(&mut index, "SELECT execution_id FROM executions"),
        vec![vec![json!(format!("{r}:1"))]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT thread_id, kind, parent_node_kind, parent_thread_id, execution_id,
               spawn_fqn, start_offset_ns
             FROM threads WHERE definition_state = 'resolved' ORDER BY start_offset_ns"
        ),
        vec![
            vec![
                json!(format!("{r}:1")),
                json!("root"),
                Json::Null,
                Json::Null,
                json!(format!("{r}:1")),
                Json::Null,
                json!(0)
            ],
            vec![
                json!(format!("{r}:99")),
                json!("spawn"),
                json!("thread"),
                json!(format!("{r}:1")),
                json!(format!("{r}:1")),
                json!("user.A"),
                json!(40)
            ],
            vec![
                json!(format!("{r}:2")),
                json!("spawn"),
                json!("thread"),
                json!(format!("{r}:99")),
                json!(format!("{r}:1")),
                Json::Null,
                json!(50)
            ],
        ]
    );
    assert_eq!(
        rows(&mut index, paths),
        vec![
            vec![json!(4), json!("resolved"), json!(0), json!("resolved")],
            vec![json!(5), json!("resolved"), json!(1), json!("resolved")],
            vec![json!(6), json!("resolved"), json!(2), json!("resolved")],
            vec![json!(7), json!("resolved"), json!(1), json!("resolved")],
        ]
    );
    assert_eq!(
        rows(&mut index, issues),
        vec![vec![json!("thread_undefined"), json!(format!("{r}:3"))]]
    );
}

#[test]
fn totals_beyond_i64_stay_exact_and_are_never_wrapped() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    let big = 3 << 61;
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(proto::Definitions {
                functions: vec![function(10, "user.A", &[])],
                call_paths: vec![path(1, ROOT, 0, 10, false)],
                threads: vec![thread(ROOT, None, 0, 0)],
                clock_epochs: vec![epoch()],
            }),
            clock_states: Some(valid()),
            aggregates: Some(proto::AggregateBatch {
                entries: vec![delta(1, false, big, 1, 0), delta(1, false, big, 1, 0)],
            }),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(
            &mut index,
            "SELECT completed_calls, completed_calls_exact, aggregate_state FROM call_path_nodes"
        ),
        vec![vec![
            Json::Null,
            json!((2 * u128::from(big)).to_string()),
            json!("overflow")
        ]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT completed_calls, aggregate_state FROM call_path_stats"
        ),
        vec![vec![Json::Null, json!("overflow")]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT completed_calls, aggregate_state FROM executions"
        ),
        vec![vec![Json::Null, json!("overflow")]]
    );
}

#[test]
fn conflicting_thread_completions_are_conflicted_not_chosen() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1);
    stats_recording(&recording, 30, true);
    recording.write(
        2,
        proto::RecordingFile {
            spans: Some(proto::SpanBatch {
                sections: vec![section(
                    ROOT,
                    vec![thread_done(1_000, proto::InvocationOutcome::Errored)],
                )],
            }),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(
            &mut index,
            "SELECT status, completion_state FROM executions"
        ),
        vec![vec![json!("conflicted"), json!("conflicted")]]
    );
    assert_eq!(
        rows(&mut index, "SELECT code, subject_kind FROM issues"),
        vec![vec![json!("thread_completion_conflict"), json!("thread")]]
    );
}
