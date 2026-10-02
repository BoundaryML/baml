//! Profiler, span and process semantics on hand-built recording files,
//! where every tick and count is known. One tick is one nanosecond.
use std::path::{Path, PathBuf};

use baml_query_btel::{Index, IndexOptions, QueryRequest, QueryResult};
use btel_recorder::proto::{self, span_event::Event};
use prost::Message as _;
use serde_json::{Value as Json, json};

const EPOCH: u64 = 5;
const ROOT: u64 = 1;
const CHILD: u64 = 2;
const A: u64 = 10;
const B: u64 = 11;
/// `btel_settings::encoding::FORMAT_MAJOR`.
const FORMAT_MAJOR: u32 = 2;

struct Recording {
    dir: PathBuf,
    id: [u8; 16],
    /// `None` writes a recording from before process identities.
    process: Option<[u8; 16]>,
}

impl Recording {
    fn new(project: &Path, tag: u8, process: Option<[u8; 16]>) -> Self {
        let id = [tag; 16];
        let dir = project
            .join(".baml/btel/recordings")
            .join(btel_reader::discovery::hex(&id));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir, id, process }
    }
    fn write(&self, sequence: u64, mut file: proto::RecordingFile) {
        file.header = Some(proto::RecordingHeader {
            format_major: FORMAT_MAJOR,
            format_minor: if self.process.is_some() { 3 } else { 2 },
            recording_id: self.id.to_vec(),
            source_snapshot_id: None,
            process_id: self.process.map(|p| p.to_vec()),
            baml_version: self.process.map(|_| "0.20.1".into()),
            host: self.process.map(|_| "baml".into()),
            command: vec![],
            process_started_at_unix_ns: self.process.map(|_| 1_790_000_000_000_000_000),
            source_cas_id: None,
        });
        file.sequence = sequence;
        let part = self.dir.join(format!("{sequence:020}.btel.part"));
        std::fs::write(&part, file.encode_to_vec()).unwrap();
        std::fs::rename(part, self.dir.join(format!("{sequence:020}.btel"))).unwrap();
    }
}

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

fn function(id: u64, fqn: &str) -> proto::FunctionDefinition {
    proto::FunctionDefinition {
        function_id: id,
        resolution: Some(proto::function_definition::Resolution::Metadata(
            proto::FunctionMetadata {
                fqn: fqn.into(),
                display_name: fqn.rsplit('.').next().unwrap().into(),
                kind: proto::FunctionKind::Bytecode as i32,
                argument_layout: Some(proto::ArgumentLayout { slots: vec![] }),
                ..Default::default()
            },
        )),
    }
}

fn path(id: u32, parent: u32, callee: u64, spawn: bool) -> proto::CallPathDefinition {
    proto::CallPathDefinition {
        call_path_id: id,
        thread_id: ROOT,
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
        ..Default::default()
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

/// `(path, reentry, count, duration, errored, cancelled, panicked)`.
fn delta(
    path: u32,
    reentry: bool,
    count: u64,
    duration: u64,
    (errored, cancelled, panicked): (u64, u64, Option<u64>),
) -> proto::AggregateDelta {
    proto::AggregateDelta {
        node: (u64::from(path) << 1) | u64::from(reentry),
        count,
        total_duration_ticks: duration,
        total_self_await_ticks: 0,
        outcomes: Some(proto::AggregateOutcomes {
            errored,
            cancelled,
            panicked,
        }),
    }
}

fn ok() -> (u64, u64, Option<u64>) {
    (0, 0, Some(0))
}

fn sysop(path: u32, ticks: u64) -> proto::SysOpTime {
    proto::SysOpTime {
        node: u64::from(path) << 1,
        sysops: 1,
        total_ticks: ticks,
    }
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

fn thread_done(ticks: u64, outcome: proto::InvocationOutcome, panicked: bool) -> Event {
    Event::ThreadCompletion(proto::ThreadCompletion {
        completed_at_ticks: ticks,
        outcome: outcome as i32,
        panicked,
    })
}

fn process_end(status: proto::ProcessStatus) -> proto::RecordingEnd {
    proto::RecordingEnd {
        process_end: Some(proto::ProcessEnd {
            status: status as i32,
            at_unix_ns: 1_790_000_000_000_001_000,
        }),
    }
}

/// A calls itself at one site, A at a second site, and B at two sites; A
/// also spawns B on a child thread. Paths: 1 root→A, 2 A→A, 3 and 4 A→B,
/// 5 A spawns B.
fn tree(
    aggregates: Vec<proto::AggregateDelta>,
    sysops: Vec<proto::SysOpTime>,
) -> proto::RecordingFile {
    proto::RecordingFile {
        definitions: Some(proto::Definitions {
            functions: vec![function(A, "user.A"), function(B, "user.B")],
            call_paths: vec![
                path(1, 0, A, false),
                path(2, 1, A, false),
                path(3, 1, B, false),
                path(4, 1, B, false),
                path(5, 1, B, true),
            ],
            threads: vec![thread(ROOT, None, 0, 0), thread(CHILD, Some(ROOT), 5, 20)],
            clock_epochs: vec![epoch()],
        }),
        clock_states: Some(valid()),
        aggregates: Some(proto::AggregateBatch {
            entries: aggregates,
            sysop_times: sysops,
        }),
        ..Default::default()
    }
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

#[test]
fn context_evidence_is_section_local_and_announcement_wins_in_both_ingesters() {
    use proto::thread_section::Context;
    let announcement = |id| {
        Event::FunctionAnnouncement(proto::FunctionAnnouncement {
            id,
            parent_id: ROOT,
            call_path_id: 3,
            entered_at_ticks: 10,
            inputs_cas_id: None,
            type_args_cas_id: None,
        })
    };
    let completion = |id, late| {
        let done = proto::FunctionCompletion {
            id,
            parent_id: ROOT,
            node: 3 << 1,
            entered_at_ticks: 10,
            exited_at_ticks: 40,
            completion_flags: 1,
            ..Default::default()
        };
        if late {
            Event::LateFunctionCompletion(done)
        } else {
            Event::FunctionCompletion(done)
        }
    };
    let context_section = |thread, context, events| {
        let mut section = section(thread, events);
        section.context = context;
        section
    };
    for incremental in [false, true] {
        let project = tempfile::tempdir().unwrap();
        let recording = Recording::new(project.path(), 55, None);
        let mut first = tree(vec![], vec![]);
        first.spans = Some(proto::SpanBatch {
            sections: vec![
                context_section(ROOT, None, vec![completion(30, false)]),
                context_section(
                    ROOT,
                    Some(Context::EmptyContext(true)),
                    vec![completion(31, false), completion(32, true)],
                ),
                context_section(ROOT, None, vec![completion(33, true)]),
                context_section(
                    CHILD,
                    Some(Context::EmptyContext(true)),
                    vec![Event::ThreadAnnouncement(proto::ThreadAnnouncement {})],
                ),
                context_section(
                    3,
                    Some(Context::EmptyContext(true)),
                    vec![Event::ThreadAnnouncement(proto::ThreadAnnouncement {})],
                ),
            ],
        });
        recording.write(1, first);
        let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
        if incremental {
            index.refresh().unwrap();
        }
        let second = proto::RecordingFile {
            spans: Some(proto::SpanBatch {
                sections: vec![
                    context_section(
                        ROOT,
                        Some(Context::EmptyContext(true)),
                        vec![announcement(30)],
                    ),
                    context_section(ROOT, None, vec![announcement(31)]),
                    context_section(
                        CHILD,
                        None,
                        vec![
                            Event::ThreadRunning(proto::ThreadRunning { at_ticks: 20 }),
                            thread_done(20, proto::InvocationOutcome::Cancelled, false),
                        ],
                    ),
                    context_section(
                        3,
                        None,
                        vec![thread_done(20, proto::InvocationOutcome::Cancelled, false)],
                    ),
                ],
            }),
            ..Default::default()
        };
        recording.write(2, second);
        let query = "SELECT span_id, baml_kind(context_metadata),
             context_distinct_id FROM spans ORDER BY span_id";
        let id = |node| json!(format!("{}:{node}", "37".repeat(16)));
        assert_eq!(
            rows(&mut index, query),
            vec![
                vec![id(2), json!("json"), Json::Null],
                vec![id(3), json!("json"), Json::Null],
                vec![id(30), json!("json"), Json::Null],
                vec![id(31), json!("unavailable"), Json::Null],
                vec![id(32), json!("json"), Json::Null],
                vec![id(33), json!("unavailable"), Json::Null],
            ]
        );
        assert_eq!(
            rows(
                &mut index,
                "SELECT baml_kind(context_metadata) FROM span_announcements
             WHERE span_id LIKE '%:32'"
            ),
            vec![vec![json!("unavailable")]]
        );
    }
}

/// Each node by its path of function names and edges, for readable rows.
const PROFILE: &str = "SELECT n.function_name, p.function_name, n.invocation_count,
    n.return_count, n.error_count, n.panic_error_count, n.nonpanic_error_count,
    n.future_cancel_count, n.missing_count, n.total_time, n.self_time, n.io_self_time,
    n.io_total_time
  FROM profiler n LEFT JOIN profiler p ON p.profiler_node_id = n.parent_profiler_node_id
  ORDER BY n.total_time DESC, n.invocation_count DESC";

#[test]
fn the_profiler_merges_call_sites_and_counts_every_outcome() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, Some([9; 16]));
    recording.write(
        1,
        tree(
            vec![
                delta(1, false, 10, 100, (2, 1, Some(1))),
                // Direct recursion: time is inside the outer invocation's.
                delta(1, true, 5, 40, (1, 2, Some(0))),
                delta(2, false, 3, 10, ok()),
                delta(3, false, 1, 12, (1, 0, Some(1))),
                delta(4, false, 2, 18, ok()),
                delta(5, false, 1, 50, ok()),
            ],
            vec![sysop(1, 7), sysop(3, 5)],
        ),
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert!(
        rows(&mut index, PROFILE).is_empty(),
        "no profiler while the process runs"
    );
    let status = rows(&mut index, "SELECT status FROM processes");
    assert_eq!(status, vec![vec![json!("running")]]);

    recording.write(
        2,
        proto::RecordingFile {
            end: Some(process_end(proto::ProcessStatus::Success)),
            ..Default::default()
        },
    );
    let a = |name, parent: Json, n, ok, err, panic, nonpanic, missing, total, own, io, io_all| {
        vec![
            json!(name),
            parent,
            json!(n),
            json!(ok),
            json!(err),
            json!(panic),
            json!(nonpanic),
            json!(0),
            json!(missing),
            json!(total),
            json!(own),
            json!(io),
            json!(io_all),
        ]
    };
    assert_eq!(
        rows(&mut index, PROFILE),
        vec![
            // 100 minus its synchronous callees (10 + 30); the spawned B's 50
            // runs beside it.
            // Its 3 cancelled invocations never finished: missing, not errors.
            a("user.A", Json::Null, 15, 9, 3, 1, 2, 3, 100, 60, 7, 12),
            a("user.B", json!("user.A"), 1, 1, 0, 0, 0, 0, 50, 50, 0, 0),
            // Two call sites, one node.
            a("user.B", json!("user.A"), 3, 2, 1, 1, 0, 0, 30, 30, 5, 5),
            a("user.A", json!("user.A"), 3, 3, 0, 0, 0, 0, 10, 10, 0, 0),
        ]
    );
    let process = rows(
        &mut index,
        "SELECT process_id, status, host, status_history[1]['status'] FROM processes",
    );
    assert_eq!(
        process,
        vec![vec![
            json!("09".repeat(16)),
            json!("success"),
            json!("baml"),
            json!("success")
        ]]
    );
}

#[test]
fn processes_merge_engines_and_rebuild_on_new_evidence() {
    let project = tempfile::tempdir().unwrap();
    let first = Recording::new(project.path(), 1, Some([9; 16]));
    first.write(1, tree(vec![delta(1, false, 2, 20, ok())], vec![]));
    first.write(
        2,
        proto::RecordingFile {
            end: Some(proto::RecordingEnd::default()),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    // An engine ended without saying how the process did.
    assert_eq!(
        rows(&mut index, "SELECT status FROM processes"),
        vec![vec![json!("unknown")]]
    );
    let second = Recording::new(project.path(), 2, Some([9; 16]));
    second.write(1, tree(vec![delta(1, false, 3, 30, ok())], vec![]));
    second.write(
        2,
        proto::RecordingFile {
            end: Some(process_end(proto::ProcessStatus::Panicked)),
            ..Default::default()
        },
    );
    // Two engines, one process: the node sums both.
    let merged = "SELECT function_name, invocation_count, total_time FROM profiler
        WHERE function_name = 'user.A' AND parent_profiler_node_id IS NULL";
    assert_eq!(
        rows(&mut index, merged),
        vec![vec![json!("user.A"), json!(5), json!(50)]]
    );
    assert_eq!(
        rows(&mut index, "SELECT COUNT(*), MAX(status) FROM processes"),
        vec![vec![json!(1), json!("panicked")]]
    );
    // A recording without a process ID is its own process, and without a
    // process end it has no profiler.
    let old = Recording::new(project.path(), 3, None);
    old.write(1, tree(vec![delta(1, false, 1, 10, (0, 0, None))], vec![]));
    assert_eq!(
        rows(&mut index, "SELECT status FROM processes ORDER BY status"),
        vec![vec![json!("panicked")], vec![json!("running")]]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT COUNT(DISTINCT process_id) FROM profiler"
        ),
        vec![vec![json!(1)]]
    );
}

#[test]
fn unknown_counts_and_times_are_null_never_guessed() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, Some([9; 16]));
    let mut old = delta(1, false, 4, 40, (1, 0, None));
    let mut missing = delta(3, false, 2, 20, ok());
    missing.outcomes = None;
    old.total_self_await_ticks = 0;
    recording.write(1, tree(vec![old, missing], vec![]));
    recording.write(
        2,
        proto::RecordingFile {
            clock_states: Some(state(proto::TimingStatus::Discontinuity)),
            end: Some(process_end(proto::ProcessStatus::Error)),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(
            &mut index,
            "SELECT function_name, invocation_count, return_count, error_count,
               panic_error_count, nonpanic_error_count, total_time
             FROM profiler ORDER BY function_name"
        ),
        vec![
            // Panics were not counted: the split is unknown, the errors are not.
            vec![
                json!("user.A"),
                json!(4),
                json!(3),
                json!(1),
                Json::Null,
                Json::Null,
                Json::Null
            ],
            // No outcomes at all: only the count is known.
            vec![
                json!("user.B"),
                json!(2),
                Json::Null,
                Json::Null,
                Json::Null,
                Json::Null,
                Json::Null
            ],
        ],
        "an invalidated clock leaves the counts"
    );
}

#[test]
fn nodes_wait_for_function_names_and_follow_late_metadata() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, Some([9; 16]));
    // B's name is unavailable at first; its node takes its ID.
    let mut file = tree(vec![delta(3, false, 1, 5, ok())], vec![]);
    let definitions = file.definitions.as_mut().unwrap();
    definitions.functions[1] = proto::FunctionDefinition {
        function_id: B,
        resolution: Some(proto::function_definition::Resolution::Unavailable(
            proto::MetadataUnavailable {},
        )),
    };
    // A path whose callee is only referenced, never described: no node.
    definitions.call_paths.push(path(6, 1, 99, false));
    file.aggregates
        .as_mut()
        .unwrap()
        .entries
        .push(delta(6, false, 1, 1, ok()));
    file.end = Some(process_end(proto::ProcessStatus::Success));
    recording.write(1, file);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let names = "SELECT function_name FROM profiler ORDER BY 1";
    assert_eq!(
        rows(&mut index, names),
        vec![vec![Json::Null], vec![json!("user.A")]],
        "an unavailable name is unknown; an undescribed callee has no node"
    );
    let before = rows(
        &mut index,
        "SELECT profiler_node_id FROM profiler ORDER BY 1",
    );
    // Changed evidence rebuilds the recording: with B's name, a new node ID.
    std::fs::remove_file(recording.dir.join(format!("{:020}.btel", 1))).unwrap();
    let mut file = tree(vec![delta(3, false, 1, 5, ok())], vec![]);
    file.end = Some(process_end(proto::ProcessStatus::Success));
    recording.write(1, file);
    assert_eq!(
        rows(&mut index, names),
        vec![vec![json!("user.A")], vec![json!("user.B")]]
    );
    let after = rows(
        &mut index,
        "SELECT profiler_node_id FROM profiler ORDER BY 1",
    );
    assert_eq!(before.len(), after.len());
    assert_ne!(before, after);
}

#[test]
fn totals_beyond_i64_are_null_not_wrapped() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, Some([9; 16]));
    let huge = u64::MAX / 2;
    recording.write(
        1,
        tree(
            vec![
                delta(1, false, 1, huge, ok()),
                delta(1, false, 1, huge, ok()),
            ],
            vec![],
        ),
    );
    recording.write(
        2,
        proto::RecordingFile {
            end: Some(process_end(proto::ProcessStatus::Success)),
            ..Default::default()
        },
    );
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        rows(
            &mut index,
            "SELECT invocation_count, total_time, self_time FROM profiler"
        ),
        vec![vec![json!(2), Json::Null, Json::Null]]
    );
}

#[test]
fn futures_are_spans_named_by_their_spawn() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, Some([9; 16]));
    let mut file = tree(vec![], vec![]);
    let threads = &mut file.definitions.as_mut().unwrap().threads;
    threads.push(proto::ThreadDefinition {
        name: Some("fetcher".into()),
        ..thread(3, Some(ROOT), 5, 25)
    });
    file.spans = Some(proto::SpanBatch {
        sections: vec![
            section(
                CHILD,
                vec![thread_done(70, proto::InvocationOutcome::Ok, false)],
            ),
            section(
                3,
                vec![thread_done(90, proto::InvocationOutcome::Errored, true)],
            ),
            section(
                ROOT,
                vec![
                    thread_done(100, proto::InvocationOutcome::Ok, false),
                    // A second, different completion is a conflict: the first stays.
                    thread_done(120, proto::InvocationOutcome::Cancelled, false),
                ],
            ),
        ],
    });
    recording.write(1, file);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let hex = btel_reader::discovery::hex(&recording.id);
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_id, span_name, parent_span_id, future_id, status, duration, span_reason
             FROM spans ORDER BY span_id"
        ),
        vec![
            vec![
                json!(format!("{hex}:1")),
                json!("user.A"),
                Json::Null,
                Json::Null,
                json!("return"),
                json!(100),
                json!("other")
            ],
            vec![
                json!(format!("{hex}:2")),
                json!("user.B"),
                json!(format!("{hex}:1")),
                json!(format!("{hex}:1")),
                json!("return"),
                json!(50),
                json!("other")
            ],
            vec![
                json!(format!("{hex}:3")),
                json!("fetcher"),
                json!(format!("{hex}:1")),
                json!(format!("{hex}:1")),
                json!("panic_error"),
                json!(65),
                json!("other")
            ],
        ]
    );
    // Spawned futures share the spawn's node; the root call takes its entry's.
    assert_eq!(
        rows(
            &mut index,
            "SELECT COUNT(DISTINCT profiler_node_id), COUNT(profiler_node_id) FROM spans"
        ),
        vec![vec![json!(2), json!(3)]]
    );
}

#[test]
fn model_usage_is_priced_on_the_span_that_made_the_call() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path(), 1, Some([9; 16]));
    let call = 50;
    let usage = |node: u64, model: Option<&str>, input: u64, read, write| proto::ModelUsage {
        node_id: node,
        thread_id: ROOT,
        model: model.map(Into::into),
        input_tokens: input,
        output_tokens: 1_000,
        cache_read_tokens: read,
        cache_write_tokens: write,
        reasoning_tokens: None,
    };
    let mut file = tree(vec![], vec![]);
    file.spans = Some(proto::SpanBatch {
        sections: vec![section(
            ROOT,
            vec![
                Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                    id: call,
                    parent_id: ROOT,
                    call_path_id: 3,
                    entered_at_ticks: 10,
                    inputs_cas_id: None,
                    type_args_cas_id: None,
                }),
                Event::FunctionCompletion(proto::FunctionCompletion {
                    id: call,
                    parent_id: ROOT,
                    node: 3 << 1,
                    entered_at_ticks: 10,
                    exited_at_ticks: 40,
                    completion_flags: 1,
                    ..Default::default()
                }),
                thread_done(100, proto::InvocationOutcome::Ok, false),
            ],
        )],
    });
    file.usage = Some(proto::UsageBatch {
        entries: vec![
            // Two turns of one call: summed and priced turn by turn.
            usage(
                call,
                Some("claude-opus-5-5-20260915"),
                1_000_000,
                Some(1_000_000),
                Some(1_000_000),
            ),
            usage(call, Some("jev-1.13.0"), 1_000_000, None, None),
            // No function span open: the thread's future pays.
            usage(ROOT, Some("some-new-model"), 10, None, None),
        ],
    });
    recording.write(1, file);
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let result = run(
        &mut index,
        "SELECT span_type, temporary_projections['model_name'], temporary_projections['model_calls'],
           temporary_projections['input_tokens'], temporary_projections['output_tokens'],
           temporary_projections['cache_read_tokens'], temporary_projections['cache_write_tokens'],
           round(temporary_projections['cost'], 6)
         FROM spans ORDER BY span_type",
    );
    // Opus 5.5: 1M input $4, 1k output $0.02, 1M cache reads $0.20, 1M cache
    // writes $5; Jev: 1M input $0.042, output free.
    assert_eq!(
        result.rows,
        vec![
            vec![
                json!("function"),
                json!("claude-opus-5-5-20260915, jev-1.13.0"),
                json!(2),
                json!(2_000_000),
                json!(2_000),
                json!(1_000_000),
                json!(1_000_000),
                json!(9.262)
            ],
            vec![
                json!("future"),
                json!("some-new-model"),
                json!(1),
                json!(10),
                json!(1_000),
                Json::Null,
                Json::Null,
                Json::Null
            ],
        ],
        "an unpriced model has no cost, not a zero one"
    );
}
