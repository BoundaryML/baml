//! Network spans on hand-built recording files: their rows, their events,
//! payloads read lazily, and evidence that arrives out of order or in
//! conflict. One tick is one nanosecond, from 2026-09-21T14:13:20Z.
use std::path::{Path, PathBuf};

use baml_query_btel::{Index, IndexOptions, QueryRequest, QueryResult, Status};
use btel_recorder::proto::{self, span_event::Event};
use btel_snapshot::{BexStr, OwnedType, SnapshotValue};
use prost::Message as _;
use serde_json::{Value as Json, json};

const EPOCH: u64 = 5;
const ROOT: u64 = 1;
const CHILD: u64 = 2;
const A: u64 = 10;
const B: u64 = 11;
/// A retained call of `user.A` on the root thread.
const CALL: u64 = 50;
const REQUEST: u64 = 100;
const OTHER: u64 = 101;
const URL: &str = "https://api.example.com/v1/messages?key=sha256:0123456789abcdef";

struct Recording {
    project: PathBuf,
    dir: PathBuf,
    id: [u8; 16],
    /// 7 records network spans; 3 is a recording from before them.
    minor: u32,
}

impl Recording {
    fn new(project: &Path) -> Self {
        Self::with(project, [3; 16], 7)
    }

    /// A recording from before network spans, in the same process.
    fn before_network(project: &Path) -> Self {
        Self::with(project, [4; 16], 3)
    }

    fn with(project: &Path, id: [u8; 16], minor: u32) -> Self {
        let dir = project
            .join(".baml/btel/recordings")
            .join(btel_reader::discovery::hex(&id));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            project: project.to_owned(),
            dir,
            id,
            minor,
        }
    }

    fn write(&self, sequence: u64, mut file: proto::RecordingFile) {
        file.header = Some(proto::RecordingHeader {
            format_major: 2,
            format_minor: self.minor,
            recording_id: self.id.to_vec(),
            process_id: Some(vec![9; 16]),
            ..Default::default()
        });
        file.sequence = sequence;
        let part = self.dir.join(format!("{sequence:020}.btel.part"));
        std::fs::write(&part, file.encode_to_vec()).unwrap();
        std::fs::rename(part, self.dir.join(format!("{sequence:020}.btel"))).unwrap();
    }

    /// Store `value` in the CAS, as a recording does, and return its ID.
    fn cas(&self, value: &Json) -> proto::CasId {
        let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
        let mut builder = pool.try_acquire().unwrap();
        let root = build(&mut builder, value);
        let snapshot = builder.finish(root, &mut btel_snapshot::Shaper::default());
        let mut scratch = btel_snapshot::BlobScratch::default();
        for blob in snapshot.blobs() {
            let mut bytes = Vec::new();
            blob.write(&mut scratch, &mut bytes).unwrap();
            let path = self.blob(blob.id());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let bytes = *snapshot.root_id().as_bytes();
        proto::CasId {
            low: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            high: u64::from_le_bytes(bytes[8..].try_into().unwrap()),
        }
    }

    fn blob(&self, id: btel_snapshot::CasId) -> PathBuf {
        btel_reader::layout::SourceLayout::for_project(&self.project).blob_path(id)
    }

    fn remove(&self, id: &proto::CasId) {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&id.low.to_le_bytes());
        bytes[8..].copy_from_slice(&id.high.to_le_bytes());
        std::fs::remove_file(self.blob(btel_snapshot::CasId::from_bytes(bytes))).unwrap();
    }

    fn span(&self, n: u64) -> Json {
        json!(format!("{}:{n}", btel_reader::discovery::hex(&self.id)))
    }
}

/// Strings, integers, null and string-keyed maps.
fn build(b: &mut btel_snapshot::Builder, value: &Json) -> SnapshotValue {
    match value {
        Json::Null => SnapshotValue::Null,
        Json::Number(n) => SnapshotValue::Int(n.as_i64().unwrap()),
        Json::String(text) => b.leaves().string_value(&BexStr::from(text.as_str())),
        Json::Object(entries) => {
            let values: Vec<SnapshotValue> = entries.values().map(|v| build(b, v)).collect();
            let key_type = b.leaves().ty(OwnedType::string());
            let value_type = b.leaves().ty(OwnedType::unknown());
            let map = b.map(
                key_type,
                value_type,
                entries.keys().zip(values),
                |leaves, (key, value)| (leaves.string_value(&BexStr::from(key.as_str())), value),
            );
            SnapshotValue::Object(b.leaves().object(map).unwrap())
        }
        other => panic!("unsupported fixture value {other}"),
    }
}

fn function(id: u64, fqn: &str) -> proto::FunctionDefinition {
    proto::FunctionDefinition {
        function_id: id,
        resolution: Some(proto::function_definition::Resolution::Metadata(
            proto::FunctionMetadata {
                fqn: fqn.into(),
                display_name: fqn.into(),
                kind: proto::FunctionKind::Bytecode as i32,
                argument_layout: Some(proto::ArgumentLayout { slots: vec![] }),
                ..Default::default()
            },
        )),
    }
}

fn path(id: u32, parent: u32, callee: u64) -> proto::CallPathDefinition {
    proto::CallPathDefinition {
        call_path_id: id,
        thread_id: ROOT,
        parent_call_path_id: parent,
        visible_caller_function_id: None,
        caller_pc: id * 4,
        callee_function_id: callee,
        edge: proto::CallPathEdge::Synchronous as i32,
    }
}

fn thread(id: u64, parent: Option<u64>) -> proto::ThreadDefinition {
    proto::ThreadDefinition {
        thread_id: id,
        parent_id: parent,
        spawn_call_path_id: 0,
        started_at_ticks: 0,
        clock_epoch_id: EPOCH,
        ..Default::default()
    }
}

/// The root thread runs `user.A` (path 1), which calls `user.B` (path 2);
/// a child thread shares the clock.
fn definitions() -> proto::Definitions {
    proto::Definitions {
        functions: vec![function(A, "user.A"), function(B, "user.B")],
        call_paths: vec![path(1, 0, A), path(2, 1, B)],
        threads: vec![thread(ROOT, None), thread(CHILD, Some(ROOT))],
        clock_epochs: vec![proto::ClockEpochDefinition {
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
        }],
        clock_anchors: Vec::new(),
    }
}

fn valid() -> proto::ClockStateBatch {
    proto::ClockStateBatch {
        states: vec![proto::ClockEpochState {
            epoch_id: EPOCH,
            status: proto::TimingStatus::Valid as i32,
            r#final: false,
            ..Default::default()
        }],
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

fn spans(sections: Vec<proto::ThreadSection>) -> proto::SpanBatch {
    proto::SpanBatch { sections }
}

fn announce(
    id: u64,
    method: &str,
    url: &str,
    started: u64,
    request: Option<proto::CasId>,
) -> Event {
    Event::NetworkAnnouncement(proto::NetworkAnnouncement {
        id,
        parent_id: CALL,
        call_path_id: 2,
        started_at_ticks: started,
        method: method.into(),
        url: url.into(),
        request_cas_id: request,
    })
}

fn event(span: u64, name: &str, at: u64, payload: Option<proto::CasId>) -> Event {
    Event::NetworkEvent(proto::NetworkEvent {
        span_id: span,
        name: name.into(),
        at_ticks: at,
        payload_cas_id: payload,
    })
}

fn complete(
    span: u64,
    at: u64,
    outcome: proto::InvocationOutcome,
    error: Option<proto::CasId>,
) -> Event {
    Event::NetworkCompletion(proto::NetworkCompletion {
        span_id: span,
        completed_at_ticks: at,
        outcome: outcome as i32,
        panicked: false,
        error_cas_id: error,
    })
}

fn call() -> Event {
    Event::FunctionAnnouncement(proto::FunctionAnnouncement {
        id: CALL,
        parent_id: ROOT,
        call_path_id: 1,
        entered_at_ticks: 5_000,
        inputs_cas_id: None,
        type_args_cas_id: None,
    })
}

/// `2026-09-21T14:13:20Z` plus `ticks` nanoseconds, as timestamps render.
fn at(ticks: u64) -> Json {
    json!(format!("2026-09-21T14:13:20.{:06}Z", ticks / 1_000))
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

fn index(project: &Path) -> Index {
    Index::for_project(project, IndexOptions::default()).unwrap()
}

#[test]
fn network_spans_are_spans_with_their_request_and_events() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let request = recording.cas(&json!({"request": {
        "method": "POST", "url": URL, "headers": {"content-type": "application/json"},
        "body": "{\"model\":\"m\"}",
    }}));
    let connection = recording.cas(&json!({"status": 200, "headers": {"request-id": "r1"}}));
    let body = recording.cas(&json!("{\"ok\":true}"));
    let error = recording.cas(&json!({"error": "connection refused"}));
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            aggregates: Some(proto::AggregateBatch {
                entries: [1, 2]
                    .map(|path| proto::AggregateDelta {
                        node: path << 1,
                        count: 1,
                        total_duration_ticks: 50_000,
                        total_self_await_ticks: 0,
                        outcomes: Some(proto::AggregateOutcomes::default()),
                    })
                    .to_vec(),
                sysop_times: vec![],
            }),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    call(),
                    announce(REQUEST, "POST", URL, 10_000, Some(request)),
                    event(REQUEST, "connection", 20_000, Some(connection)),
                    event(REQUEST, "data", 30_000, Some(body)),
                    event(REQUEST, "await", 31_000, None),
                    complete(REQUEST, 31_000, proto::InvocationOutcome::Ok, None),
                    announce(OTHER, "GET", "https://example.com/health", 40_000, None),
                    complete(
                        OTHER,
                        45_000,
                        proto::InvocationOutcome::Errored,
                        Some(error),
                    ),
                    Event::ThreadCompletion(proto::ThreadCompletion {
                        completed_at_ticks: 60_000,
                        outcome: proto::InvocationOutcome::Ok as i32,
                        panicked: false,
                    }),
                ],
            )])),
            // The recording is folded: only the paths spans read remain.
            end: Some(proto::RecordingEnd {
                process_end: Some(proto::ProcessEnd {
                    status: proto::ProcessStatus::Success as i32,
                    at_unix_ns: 1_790_000_000_000_100_000,
                }),
            }),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_id, span_name, parent_span_id, future_id, status, scheduled_time,
               start_time, end_time, duration, input_args['request']['url'], output_value,
               error_value['error'], temporary_projections, span_reason
             FROM spans WHERE span_type = 'network_span' ORDER BY span_id"
        ),
        vec![
            vec![
                recording.span(REQUEST),
                json!(format!("POST {URL}")),
                recording.span(CALL),
                recording.span(ROOT),
                json!("return"),
                Json::Null,
                at(10_000),
                at(31_000),
                json!(21_000),
                json!(URL),
                Json::Null,
                Json::Null,
                Json::Null,
                json!("other"),
            ],
            vec![
                recording.span(OTHER),
                json!("GET https://example.com/health"),
                recording.span(CALL),
                recording.span(ROOT),
                json!("user_error"),
                Json::Null,
                at(40_000),
                at(45_000),
                json!(5_000),
                Json::Null,
                Json::Null,
                json!("connection refused"),
                Json::Null,
                json!("other"),
            ],
        ]
    );
    // The node of the function that made the request, kept by the fold.
    assert_eq!(
        rows(
            &mut index,
            "SELECT p.function_name, COUNT(*) FROM spans s
             JOIN profiler p ON p.profiler_node_id = s.profiler_node_id
             WHERE s.span_type = 'network_span' GROUP BY 1"
        ),
        vec![vec![json!("user.B"), json!(2)]]
    );
    assert_eq!(
        rows(
            &mut index,
            &format!(
                "SELECT network_event_values FROM spans WHERE span_id = '{}'",
                recording.span(REQUEST).as_str().unwrap()
            )
        ),
        vec![vec![json!([
            {
                "event_name": "connection",
                "payload": {"status": 200, "headers": {"request-id": "r1"}},
                "timestamp": at(20_000),
            },
            {"event_name": "data", "payload": "{\"ok\":true}", "timestamp": at(30_000)},
            {"event_name": "await", "payload": null, "timestamp": at(31_000)},
        ])]]
    );
    // Function and future spans have no events; a request without any has
    // an empty list.
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_type, COUNT(*), COUNT(network_event_values),
               SUM(network_event_values = baml_value_json('[]'))
             FROM span_announcements GROUP BY span_type ORDER BY span_type"
        ),
        vec![
            vec![json!("function"), json!(1), json!(0), Json::Null],
            vec![json!("future"), json!(2), json!(0), Json::Null],
            vec![json!("network_span"), json!(2), json!(2), json!(1)],
        ]
    );
    let result = run(
        &mut index,
        "SELECT network_event_values[0]['event_name'], network_event_values[-1]['event_name'],
           network_event_values[1]['payload'], network_event_values[0]['payload']['status'],
           network_event_values[2]['payload'], network_event_values[3], baml_kind(network_event_values)
         FROM spans WHERE span_type = 'network_span' AND span_name LIKE 'POST %'",
    );
    assert_eq!(
        result.rows,
        vec![vec![
            json!("connection"),
            json!("await"),
            json!("{\"ok\":true}"),
            json!(200),
            Json::Null,
            Json::Null,
            json!("json"),
        ]]
    );
    assert_eq!(result.outcome.status, Status::Complete);
}

#[test]
fn an_event_payload_loads_only_its_own_blob() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let connection = recording.cas(&json!({"status": 200, "headers": {}}));
    let first = recording.cas(&json!({"event": "message_start", "data": "{}", "id": null}));
    let second = recording.cas(&json!({"event": "message_stop", "data": "{}", "id": null}));
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    call(),
                    announce(REQUEST, "POST", URL, 10_000, None),
                    event(REQUEST, "connection", 20_000, Some(connection)),
                    event(REQUEST, "data", 21_000, Some(first)),
                    event(REQUEST, "data", 22_000, Some(second)),
                    event(REQUEST, "end", 23_000, None),
                    event(REQUEST, "await", 23_000, None),
                    complete(REQUEST, 23_000, proto::InvocationOutcome::Ok, None),
                ],
            )])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let network = "FROM spans WHERE span_type = 'network_span'";
    // Rows, and the blobs the query read.
    let loads = |index: &mut Index, select: &str| {
        let result = run(index, &format!("{select} {network}"));
        (result.rows, result.outcome.query.values.cas_loads)
    };
    assert_eq!(
        loads(
            &mut index,
            "SELECT network_event_values[1]['payload']['event']"
        ),
        (vec![vec![json!("message_start")]], 1)
    );
    assert_eq!(
        loads(
            &mut index,
            "SELECT network_event_values[2]['event_name'], network_event_values[-2]['event_name'],
               network_event_values[1]['timestamp']"
        ),
        (vec![vec![json!("data"), json!("end"), at(21_000)]], 0),
        "names and times read no payload"
    );
    assert_eq!(
        loads(&mut index, "SELECT network_event_values[2]"),
        (
            vec![vec![json!({
                "event_name": "data",
                "payload": {"event": "message_stop", "data": "{}", "id": null},
                "timestamp": at(22_000),
            })]],
            1
        )
    );
    let filtered = run(
        &mut index,
        &format!(
            "SELECT COUNT(*) {network} AND network_event_values[0]['payload']['status'] = 200"
        ),
    );
    assert_eq!(
        (filtered.rows, filtered.outcome.query.values.cas_loads),
        (vec![vec![json!(1)]], 1)
    );
    // COUNT of a value inside counts where it IS NOT NULL.
    assert_eq!(
        loads(
            &mut index,
            "SELECT COUNT(network_event_values), COUNT(network_event_values[1]['payload']),
               COUNT(network_event_values[4]['payload']), COUNT(network_event_values[9])"
        ),
        (vec![vec![json!(1), json!(1), json!(0), json!(0)]], 1)
    );
    let (whole, whole_loads) = loads(&mut index, "SELECT network_event_values");
    assert_eq!(whole_loads, 3, "the whole list reads every payload");
    assert_eq!(whole[0][0].as_array().unwrap().len(), 5);

    // A payload whose blob is gone: that event's payload, and any value
    // holding it, cannot be read; the others still can.
    recording.remove(&second);
    let result = run(
        &mut index,
        &format!(
            "SELECT network_event_values[1]['payload']['event'], network_event_values[2]['payload'],
               network_event_values[3]['event_name'], network_event_values {network}"
        ),
    );
    assert_eq!(
        result.rows,
        vec![vec![
            json!("message_start"),
            Json::Null,
            json!("end"),
            Json::Null
        ]]
    );
    assert_eq!(result.outcome.status, Status::Incomplete);
    assert!(
        result
            .outcome
            .diagnostics
            .iter()
            .any(|d| d.code == "cas_missing"),
        "{:?}",
        result.outcome.diagnostics
    );
}

#[test]
fn events_with_the_same_payload_each_show_it() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    // One blob, named by both events.
    let ping = recording.cas(&json!({"event": "ping", "data": "{}", "id": null}));
    let again = recording.cas(&json!({"event": "ping", "data": "{}", "id": null}));
    assert_eq!(ping, again);
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    call(),
                    announce(REQUEST, "POST", URL, 10_000, None),
                    event(REQUEST, "data", 21_000, Some(ping)),
                    event(REQUEST, "data", 22_000, Some(again)),
                    complete(REQUEST, 23_000, proto::InvocationOutcome::Ok, None),
                ],
            )])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let result = run(
        &mut index,
        "SELECT network_event_values FROM spans WHERE span_type = 'network_span'",
    );
    let payload = json!({"event": "ping", "data": "{}", "id": null});
    let payloads: Vec<_> = result.rows[0][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["payload"].clone())
        .collect();
    assert_eq!(payloads, [payload.clone(), payload]);
    assert_eq!(result.outcome.query.values.cas_loads, 1);
    assert_eq!(result.outcome.status, Status::Complete);
}

#[test]
fn a_body_stored_as_a_blob_of_its_own_is_read_through_its_payload() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    // Long enough for the capture to store each as its own blob: the request
    // body inside its payload, and one event's data inside its.
    let long = "x".repeat(64 * 1024);
    let body = json!({"model": "claude-opus-5-5", "prompt": long}).to_string();
    let request = recording.cas(&json!({"request": {
        "method": "POST", "url": URL, "headers": {}, "body": body,
    }}));
    let usage = json!({"usage": {"input_tokens": 900, "output_tokens": 30}, "text": long});
    let data = recording.cas(&json!({
        "event": "message_delta", "data": usage.to_string(), "id": null,
    }));
    let cas = btel_reader::layout::SourceLayout::for_project(project.path());
    let blobs = |id: &proto::CasId| {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&id.low.to_le_bytes());
        bytes[8..].copy_from_slice(&id.high.to_le_bytes());
        let path = cas.blob_path(btel_snapshot::CasId::from_bytes(bytes));
        std::fs::metadata(path).unwrap().len()
    };
    assert!(blobs(&request) < 1024 && blobs(&data) < 1024);
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    call(),
                    announce(
                        REQUEST,
                        "POST",
                        "https://api.anthropic.com/v1/messages",
                        10_000,
                        Some(request),
                    ),
                    event(REQUEST, "data", 21_000, Some(data)),
                    complete(REQUEST, 23_000, proto::InvocationOutcome::Ok, None),
                ],
            )])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let result = run(
        &mut index,
        "SELECT input_args['request']['body'], network_event_values[0]['payload']['data'],
           network_event_values[0]['payload'], temporary_projections['model_name'],
           temporary_projections['input_tokens']
         FROM spans WHERE span_type = 'network_span'",
    );
    let [body_read, data_read, payload, model, input] = &result.rows[0][..] else {
        panic!("five columns")
    };
    assert_eq!(*body_read, json!(body));
    assert_eq!(*data_read, json!(usage.to_string()));
    assert_eq!(
        *payload,
        json!({"event": "message_delta", "data": usage.to_string(), "id": null})
    );
    assert_eq!(
        (model, input),
        (&json!("claude-opus-5-5"), &json!(900)),
        "{:?}",
        result.outcome.diagnostics
    );
    assert_eq!(result.outcome.status, Status::Complete);
}

#[test]
fn two_event_lists_are_compared_each_by_its_own_content() {
    const THIRD: u64 = 102;
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    // Each request has one event, whose data is long enough to be a blob of
    // its own: comparing the lists reads through both.
    let payload = |letter: &str| {
        recording.cas(&json!({
            "event": "message_delta", "data": letter.repeat(64 * 1024), "id": null,
        }))
    };
    let (first, second, same) = (payload("a"), payload("b"), payload("a"));
    let request = |id, payload| {
        vec![
            announce(id, "POST", URL, 10_000, None),
            event(id, "data", 21_000, Some(payload)),
            complete(id, 23_000, proto::InvocationOutcome::Ok, None),
        ]
    };
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                [
                    vec![call()],
                    request(REQUEST, first),
                    request(OTHER, second),
                    request(THIRD, same),
                ]
                .concat(),
            )])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let result = run(
        &mut index,
        "SELECT a.span_id, b.span_id, a.network_event_values = b.network_event_values
         FROM spans a JOIN spans b ON a.span_id < b.span_id
         WHERE a.span_type = 'network_span' AND b.span_type = 'network_span'
         ORDER BY a.span_id, b.span_id",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![recording.span(REQUEST), recording.span(OTHER), json!(0)],
            vec![recording.span(REQUEST), recording.span(THIRD), json!(1)],
            vec![recording.span(OTHER), recording.span(THIRD), json!(0)],
        ],
        "{:?}",
        result.outcome.diagnostics
    );
    assert_eq!(result.outcome.status, Status::Complete);
}

#[test]
fn an_unfinished_request_shows_its_events_so_far() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let connection = recording.cas(&json!({"status": 200, "headers": {}}));
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    call(),
                    announce(REQUEST, "POST", URL, 10_000, None),
                    event(REQUEST, "connection", 20_000, Some(connection)),
                ],
            )])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let announced =
        "SELECT span_name, start_time, is_complete, network_event_values[-1]['event_name'],
        network_event_values[0]['payload']['status']
      FROM span_announcements WHERE span_type = 'network_span'";
    assert_eq!(
        rows(&mut index, announced),
        vec![vec![
            json!(format!("POST {URL}")),
            at(10_000),
            json!(0),
            json!("connection"),
            json!(200)
        ]]
    );
    let completed = "SELECT COUNT(*) FROM spans WHERE span_type = 'network_span'";
    assert_eq!(rows(&mut index, completed), vec![vec![json!(0)]]);

    // The rest arrives in a later file, applied row by row.
    recording.write(
        2,
        proto::RecordingFile {
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    event(REQUEST, "data", 30_000, Some(recording.cas(&json!("done")))),
                    event(REQUEST, "await", 31_000, None),
                    complete(REQUEST, 31_000, proto::InvocationOutcome::Ok, None),
                ],
            )])),
            ..Default::default()
        },
    );
    assert_eq!(
        rows(&mut index, announced),
        vec![vec![
            json!(format!("POST {URL}")),
            at(10_000),
            json!(1),
            json!("await"),
            json!(200)
        ]]
    );
    assert_eq!(rows(&mut index, completed), vec![vec![json!(1)]]);
}

/// A network span has the context of the frame that sent its request: the
/// context its announcement was recorded in. Its events and its completion,
/// which can be read under another context, do not change it. Both ingest
/// paths agree.
#[test]
fn network_spans_have_the_context_they_were_sent_in() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let sent = recording.cas(&json!({"distinct_id": "user-7", "metadata": {"phase": "send"}}));
    let read = recording.cas(&json!({"distinct_id": "other", "metadata": {"phase": "read"}}));
    let mut announced = section(
        ROOT,
        vec![call(), announce(REQUEST, "POST", URL, 10_000, None)],
    );
    announced.context = Some(proto::thread_section::Context::ContextCasId(sent));
    let mut finished = section(
        CHILD,
        vec![
            event(REQUEST, "await", 20_000, None),
            complete(REQUEST, 20_000, proto::InvocationOutcome::Ok, None),
        ],
    );
    finished.context = Some(proto::thread_section::Context::ContextCasId(read));
    // No context recorded with this one: unavailable, not empty.
    let unknown = section(
        ROOT,
        vec![
            announce(OTHER, "GET", URL, 30_000, None),
            complete(OTHER, 31_000, proto::InvocationOutcome::Ok, None),
        ],
    );
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![announced, finished, unknown])),
            ..Default::default()
        },
    );
    let expected = vec![
        vec![
            recording.span(REQUEST),
            json!("user-7"),
            json!("send"),
            json!("json"),
        ],
        vec![
            recording.span(OTHER),
            Json::Null,
            Json::Null,
            json!("unavailable"),
        ],
    ];
    let query = |relation: &str| {
        format!(
            "SELECT span_id, context_distinct_id, context_metadata['phase'],
               baml_kind(context_metadata)
             FROM {relation} WHERE span_type = 'network_span' ORDER BY start_time"
        )
    };
    let mut fresh = index(project.path());
    for relation in ["spans", "span_announcements"] {
        assert_eq!(rows(&mut fresh, &query(relation)), expected, "{relation}");
    }
    drop(fresh);
    let layout = btel_reader::layout::SourceLayout::for_project(project.path());
    std::fs::remove_file(layout.root.join("query.sqlite")).unwrap();
    let mut options = IndexOptions::default();
    options.refresh.fresh_bytes = 0;
    options.refresh.batch_files = 1;
    let mut incremental = Index::open(layout, options).unwrap();
    for relation in ["spans", "span_announcements"] {
        assert_eq!(
            rows(&mut incremental, &query(relation)),
            expected,
            "{relation}"
        );
    }
}

#[test]
fn events_before_their_announcement_wait_for_it_and_keep_time_order() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let request = recording.cas(&json!({"request": {"method": "GET", "url": URL}}));
    // Another thread's section reads the stream; its events come first, and
    // the connection is recorded after the data it preceded.
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![
                section(CHILD, vec![event(REQUEST, "data", 30_000, None)]),
                section(ROOT, vec![event(REQUEST, "connection", 20_000, None)]),
            ])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let network = "SELECT COUNT(*) FROM span_announcements WHERE span_type = 'network_span'";
    assert_eq!(
        rows(&mut index, network),
        vec![vec![json!(0)]],
        "events alone say nothing about the span"
    );

    recording.write(
        2,
        proto::RecordingFile {
            spans: Some(spans(vec![section(
                CHILD,
                vec![complete(
                    REQUEST,
                    31_000,
                    proto::InvocationOutcome::Ok,
                    None,
                )],
            )])),
            ..Default::default()
        },
    );
    // Completed but not announced: the request is still due, and without
    // the announcing thread there is no clock.
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_name, future_id, status, end_time, baml_value_state(input_args),
               network_event_values[0]['event_name'], network_event_values[0]['timestamp']
             FROM spans WHERE span_type = 'network_span'"
        ),
        vec![vec![
            Json::Null,
            Json::Null,
            json!("return"),
            Json::Null,
            json!("capture_pending"),
            json!("connection"),
            Json::Null,
        ]]
    );

    recording.write(
        3,
        proto::RecordingFile {
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    announce(REQUEST, "GET", URL, 10_000, Some(request)),
                    // Same time as the data: recording order decides.
                    event(REQUEST, "end", 30_000, None),
                    event(REQUEST, "await", 31_000, None),
                ],
            )])),
            ..Default::default()
        },
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_name, future_id, start_time, duration, input_args['request']['method'],
               network_event_values
             FROM spans WHERE span_type = 'network_span'"
        ),
        vec![vec![
            json!(format!("GET {URL}")),
            recording.span(ROOT),
            at(10_000),
            json!(21_000),
            json!("GET"),
            json!([
                {"event_name": "connection", "payload": null, "timestamp": at(20_000)},
                {"event_name": "data", "payload": null, "timestamp": at(30_000)},
                {"event_name": "end", "payload": null, "timestamp": at(30_000)},
                {"event_name": "await", "payload": null, "timestamp": at(31_000)},
            ]),
        ]]
    );
}

#[test]
fn the_first_network_evidence_wins_and_invalid_evidence_is_skipped() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    announce(REQUEST, "GET", URL, 10_000, None),
                    complete(REQUEST, 20_000, proto::InvocationOutcome::Ok, None),
                ],
            )])),
            ..Default::default()
        },
    );
    recording.write(
        2,
        proto::RecordingFile {
            spans: Some(spans(vec![section(
                CHILD,
                vec![
                    announce(REQUEST, "POST", URL, 10_000, None),
                    // A repeat of the same evidence is no conflict.
                    complete(REQUEST, 20_000, proto::InvocationOutcome::Ok, None),
                    complete(REQUEST, 25_000, proto::InvocationOutcome::Errored, None),
                    event(0, "data", 21_000, None),
                    complete(OTHER, 21_000, proto::InvocationOutcome::Unspecified, None),
                    Event::NetworkAnnouncement(proto::NetworkAnnouncement {
                        id: 102,
                        parent_id: 0,
                        call_path_id: 2,
                        started_at_ticks: 21_000,
                        method: "GET".into(),
                        url: URL.into(),
                        request_cas_id: None,
                    }),
                ],
            )])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_name, future_id, status, end_time FROM spans
             WHERE span_type = 'network_span'"
        ),
        vec![vec![
            json!(format!("GET {URL}")),
            recording.span(ROOT),
            json!("return"),
            at(20_000)
        ]]
    );
    let conn = index.connection();
    let issues: Vec<(String, String)> = conn
        .prepare(
            "SELECT code, subject FROM issue WHERE code LIKE 'network%' ORDER BY code, subject",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let issue = |code: &str, subject: &str| (code.to_owned(), subject.to_owned());
    assert_eq!(
        issues,
        vec![
            issue("network_completion_conflict", "100"),
            issue("network_evidence_invalid", "0"),
            issue("network_evidence_invalid", "101"),
            issue("network_evidence_invalid", "102"),
            issue("network_span_conflict", "100"),
        ]
    );
    let stored: Vec<(i64, i64)> = conn
        .prepare("SELECT defined, conflict FROM network_span")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(stored, vec![(1, 1)], "invalid evidence leaves no row");
}

/// A finished model request to `url` with `body`, answered by `data` events:
/// a whole body (a JSON string), or server-sent events (`sse`).
fn model_request(
    recording: &Recording,
    id: u64,
    url: &str,
    body: &Json,
    data: &[Json],
) -> Vec<Event> {
    let request = recording.cas(&json!({"request": {
        "method": "POST", "url": url, "headers": {}, "body": body.to_string(),
    }}));
    let mut events = vec![announce(id, "POST", url, 10_000, Some(request))];
    for (at, payload) in (20_000..).zip(data) {
        events.push(event(id, "data", at, Some(recording.cas(payload))));
    }
    events.push(event(id, "await", 30_000, None));
    events.push(complete(id, 30_000, proto::InvocationOutcome::Ok, None));
    events
}

fn body(value: &Json) -> Json {
    json!(value.to_string())
}

fn sse(event: &str, data: &Json) -> Json {
    json!({"event": event, "data": data.to_string(), "id": null})
}

/// Each network span's projection, by span ID: its model, model calls,
/// tokens and cost.
const PROJECTIONS: &str = "SELECT temporary_projections['model_name'],
    temporary_projections['model_calls'], temporary_projections['input_tokens'],
    temporary_projections['output_tokens'], temporary_projections['cache_read_tokens'],
    temporary_projections['cache_write_tokens'], temporary_projections['reasoning_tokens'],
    round(temporary_projections['cost'], 9)
  FROM spans WHERE span_type = 'network_span' ORDER BY span_id";

fn priced(model: &str, tokens: [Option<i64>; 5], cost: Option<f64>) -> Vec<Json> {
    let mut row = vec![json!(model), json!(1)];
    row.extend(tokens.map(|n| n.map_or(Json::Null, Json::from)));
    row.push(cost.map_or(Json::Null, Json::from));
    row
}

fn unpriced() -> Vec<Json> {
    vec![Json::Null; 8]
}

#[test]
fn model_requests_are_priced_from_their_responses() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let opus = "claude-opus-5-5-20260915";
    let sol = "gpt-6-sol-2026-08-01";
    let prompt = json!({"model": opus, "max_tokens": 64, "messages": []});
    // #5043's request: 900 uncached input, 200 cache reads, 100 cache
    // writes and 30 output tokens on Opus 5.5 cost $0.00474.
    let anthropic_usage = json!({"input_tokens": 900, "cache_creation_input_tokens": 100,
        "cache_read_input_tokens": 200, "output_tokens": 30});
    let anthropic = json!({"id": "msg_01", "type": "message", "role": "assistant", "model": opus,
        "content": [{"type": "text", "text": "Hi"}], "stop_reason": "end_turn",
        "usage": anthropic_usage});
    let anthropic_stream = [
        sse(
            "message_start",
            &json!({"type": "message_start", "message": {"id": "msg_02",
            "type": "message", "role": "assistant", "model": opus, "content": [],
            "usage": {"input_tokens": 900, "cache_creation_input_tokens": 100,
              "cache_read_input_tokens": 200, "output_tokens": 1}}}),
        ),
        sse(
            "content_block_delta",
            &json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": "Hi"}}),
        ),
        sse(
            "message_delta",
            &json!({"type": "message_delta",
            "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 30}}),
        ),
        sse("message_stop", &json!({"type": "message_stop"})),
    ];
    // OpenAI counts cache reads and writes inside the prompt: 900 of 1200
    // are fresh.
    let chat_usage = json!({"prompt_tokens": 1200, "completion_tokens": 300,
        "total_tokens": 1500,
        "prompt_tokens_details": {"cached_tokens": 200, "cache_write_tokens": 100},
        "completion_tokens_details": {"reasoning_tokens": 100}});
    let chat = json!({"id": "chatcmpl-1", "object": "chat.completion", "model": sol,
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hi"},
          "finish_reason": "stop"}],
        "usage": chat_usage});
    let chunk = |usage: Json| {
        json!({"id": "chatcmpl-2", "object": "chat.completion.chunk",
        "model": sol, "choices": [], "usage": usage})
    };
    let chat_stream = [
        sse("", &chunk(Json::Null)),
        sse("", &chunk(chat_usage)),
        json!({"event": null, "data": "[DONE]", "id": null}),
    ];
    let responses_usage = json!({"input_tokens": 1200,
        "input_tokens_details": {"cached_tokens": 200, "cache_write_tokens": 100},
        "output_tokens": 300,
        "output_tokens_details": {"reasoning_tokens": 100}, "total_tokens": 1500});
    let responses = json!({"id": "resp_1", "object": "response", "model": sol,
        "output": [], "usage": responses_usage});
    let responses_stream = [
        sse(
            "response.created",
            &json!({"type": "response.created",
            "response": {"id": "resp_2", "model": sol, "usage": null}}),
        ),
        sse(
            "response.output_text.delta",
            &json!({"type": "response.output_text.delta",
            "delta": "Hi"}),
        ),
        sse(
            "response.completed",
            &json!({"type": "response.completed",
            "response": {"id": "resp_2", "model": sol, "usage": responses_usage}}),
        ),
    ];
    let gemini = json!({"candidates": [], "modelVersion": "gemini-2.5-pro",
        "usageMetadata": {"promptTokenCount": 1200, "candidatesTokenCount": 300,
          "cachedContentTokenCount": 200, "thoughtsTokenCount": 50, "totalTokenCount": 1550}});
    // Vertex streams name no model: the URL does. Counts are cumulative.
    let gemini_stream = [
        sse(
            "",
            &json!({"candidates": [], "usageMetadata": {"promptTokenCount": 1200,
            "candidatesTokenCount": 10}}),
        ),
        sse(
            "",
            &json!({"candidates": [], "usageMetadata": {"promptTokenCount": 1200,
            "candidatesTokenCount": 300, "cachedContentTokenCount": 200,
            "thoughtsTokenCount": 50}}),
        ),
    ];
    let bedrock_usage = json!({"inputTokens": 900, "outputTokens": 30, "totalTokens": 1230,
        "cacheReadInputTokens": 200, "cacheWriteInputTokens": 100});
    let bedrock = json!({"output": {"message": {"role": "assistant", "content": [{"text": "Hi"}]}},
        "stopReason": "end_turn", "usage": bedrock_usage, "metrics": {"latencyMs": 100}});
    let bedrock_stream = [
        sse("messageStart", &json!({"role": "assistant"})),
        sse(
            "contentBlockDelta",
            &json!({"contentBlockIndex": 0, "delta": {"text": "Hi"}}),
        ),
        sse(
            "metadata",
            &json!({"usage": bedrock_usage, "metrics": {"latencyMs": 100}}),
        ),
    ];
    let mut no_model = anthropic.clone();
    no_model.as_object_mut().unwrap().remove("model");
    let jev = json!({"answers": [], "usage": {"input_tokens": 1_000_000, "output_tokens": 5}});
    let gemini_url =
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent";
    let vertex_url = "https://us-central1-aiplatform.googleapis.com/v1/projects/p/locations/us-central1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse";
    let bedrock_url = "https://bedrock-runtime.us-east-1.amazonaws.com/model/us.anthropic.claude-opus-5-5-20260915-v1%3A0";
    let (converse, converse_stream) = (
        format!("{bedrock_url}/converse"),
        format!("{bedrock_url}/converse-stream"),
    );
    let requests: Vec<(&str, Json, Vec<Json>)> = vec![
        (
            "https://api.anthropic.com/v1/messages",
            prompt.clone(),
            vec![body(&anthropic)],
        ),
        (
            "https://api.anthropic.com/v1/messages",
            prompt.clone(),
            anthropic_stream.to_vec(),
        ),
        (
            "https://api.openai.com/v1/chat/completions",
            json!({"model": "gpt-6-sol"}),
            vec![body(&chat)],
        ),
        (
            "https://api.openai.com/v1/chat/completions",
            json!({"model": "gpt-6-sol"}),
            chat_stream.to_vec(),
        ),
        (
            "https://api.openai.com/v1/responses",
            json!({"model": "gpt-6-sol"}),
            vec![body(&responses)],
        ),
        (
            "https://api.openai.com/v1/responses",
            json!({"model": "gpt-6-sol"}),
            responses_stream.to_vec(),
        ),
        (gemini_url, json!({"contents": []}), vec![body(&gemini)]),
        (vertex_url, json!({"contents": []}), gemini_stream.to_vec()),
        (&converse, json!({"messages": []}), vec![body(&bedrock)]),
        (
            &converse_stream,
            json!({"messages": []}),
            bedrock_stream.to_vec(),
        ),
        // The response names no model: the request does.
        (
            "https://api.anthropic.com/v1/messages",
            prompt.clone(),
            vec![body(&no_model)],
        ),
        (
            "https://api.typesafe.ai/v1/systemone",
            json!({"model": "jev-1.13.0"}),
            vec![body(&jev)],
        ),
        // A gateway path that holds both Anthropic's and OpenAI chat's:
        // OpenAI's route wins.
        (
            "https://gateway.example.com/v1/messages/chat/completions",
            json!({"model": "gpt-6-sol"}),
            vec![body(&chat)],
        ),
        // Not a model provider, though its body reports usage.
        (
            "https://example.com/api",
            prompt.clone(),
            vec![body(&anthropic)],
        ),
        // A provider's reply without usage, and one that is not JSON.
        (
            "https://api.anthropic.com/v1/messages",
            prompt.clone(),
            vec![body(
                &json!({"type": "error", "error": {"type": "overloaded_error"}}),
            )],
        ),
        (
            "https://api.anthropic.com/v1/messages",
            prompt,
            vec![json!("<html>502</html>")],
        ),
    ];
    let mut events = vec![call()];
    for (id, (url, request, data)) in (200..).zip(&requests) {
        events.extend(model_request(&recording, id, url, request, data));
    }
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(ROOT, events)])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let result = run(&mut index, PROJECTIONS);
    let anthropic = priced(
        opus,
        [Some(900), Some(30), Some(200), Some(100), None],
        Some(0.00474),
    );
    // 900 fresh at $2, 100 written at $2.5, 200 cached at $0.2, 300 out at
    // $10 per million.
    let openai = priced(
        sol,
        [Some(900), Some(300), Some(200), Some(100), Some(100)],
        Some(0.00509),
    );
    let gemini = |model| {
        priced(
            model,
            [Some(1000), Some(300), Some(200), None, Some(50)],
            None,
        )
    };
    let bedrock = |model| {
        priced(
            model,
            [Some(900), Some(30), Some(200), Some(100), None],
            None,
        )
    };
    assert_eq!(
        result.rows,
        vec![
            anthropic.clone(),
            anthropic.clone(),
            openai.clone(),
            openai.clone(),
            openai.clone(),
            openai.clone(),
            gemini("gemini-2.5-pro"),
            gemini("gemini-2.5-flash"),
            bedrock("us.anthropic.claude-opus-5-5-20260915-v1:0"),
            bedrock("us.anthropic.claude-opus-5-5-20260915-v1:0"),
            anthropic,
            priced(
                "jev-1.13.0",
                [Some(1_000_000), Some(5), None, None, None],
                Some(0.042)
            ),
            openai,
            unpriced(),
            unpriced(),
            unpriced(),
        ]
    );
    assert_eq!(result.outcome.status, Status::Complete);
}

#[test]
fn network_spans_replace_model_usage_and_older_recordings_keep_it() {
    let project = tempfile::tempdir().unwrap();
    let usage = |node: u64| proto::UsageBatch {
        entries: vec![proto::ModelUsage {
            node_id: node,
            thread_id: ROOT,
            model: Some("claude-opus-5-5".into()),
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: None,
            cache_write_tokens: None,
            reasoning_tokens: None,
        }],
    };
    // The same request, priced twice: as model usage on its call, and on
    // its network span.
    let current = Recording::new(project.path());
    let mut events = vec![call()];
    events.extend(model_request(
        &current,
        REQUEST,
        "https://api.anthropic.com/v1/messages",
        &json!({}),
        &[body(
            &json!({"model": "claude-opus-5-5", "usage": {"input_tokens": 1_000_000,
            "output_tokens": 0}}),
        )],
    ));
    events.push(Event::FunctionCompletion(proto::FunctionCompletion {
        id: CALL,
        parent_id: ROOT,
        node: 1 << 1,
        entered_at_ticks: 5_000,
        exited_at_ticks: 40_000,
        completion_flags: 1,
        ..Default::default()
    }));
    current.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(ROOT, events)])),
            usage: Some(usage(CALL)),
            ..Default::default()
        },
    );
    let older = Recording::before_network(project.path());
    older.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(
                ROOT,
                vec![
                    call(),
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: CALL,
                        parent_id: ROOT,
                        node: 1 << 1,
                        entered_at_ticks: 5_000,
                        exited_at_ticks: 40_000,
                        completion_flags: 1,
                        ..Default::default()
                    }),
                ],
            )])),
            usage: Some(usage(CALL)),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_id, span_type, temporary_projections['cost'] FROM spans
             WHERE temporary_projections IS NOT NULL ORDER BY span_id"
        ),
        vec![
            vec![current.span(REQUEST), json!("network_span"), json!(4.0)],
            vec![older.span(CALL), json!("function"), json!(4.0)],
        ]
    );
    assert_eq!(
        rows(
            &mut index,
            "SELECT sum(temporary_projections['cost']) FROM spans"
        ),
        vec![vec![json!(8.0)]]
    );
}

/// In a recording with network spans, the usage the stdlib recorded by hand
/// is dropped only on a span with a request under it that its response
/// prices; usage from a client that made no request stays where it was.
#[test]
fn hand_recorded_usage_is_dropped_only_where_a_request_prices_it() {
    const NO_HTTP: u64 = 51;
    const BODIES_OFF: u64 = 52;
    const UNREAD: u64 = 102;
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let usage = |node: u64, input: u64| proto::ModelUsage {
        node_id: node,
        thread_id: ROOT,
        model: Some("claude-opus-5-5".into()),
        input_tokens: input,
        output_tokens: 0,
        cache_read_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
    };
    let function = |id: u64| {
        [
            Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                id,
                parent_id: ROOT,
                call_path_id: 1,
                entered_at_ticks: 5_000,
                inputs_cas_id: None,
                type_args_cas_id: None,
            }),
            Event::FunctionCompletion(proto::FunctionCompletion {
                id,
                parent_id: ROOT,
                node: 1 << 1,
                entered_at_ticks: 5_000,
                exited_at_ticks: 40_000,
                completion_flags: 1,
                ..Default::default()
            }),
        ]
    };
    // CALL's request is priced from its response ($4), and was also
    // recorded by hand on CALL.
    let mut events = function(CALL).to_vec();
    events.extend(model_request(
        &recording,
        REQUEST,
        "https://api.anthropic.com/v1/messages",
        &json!({}),
        &[body(
            &json!({"model": "claude-opus-5-5", "usage": {"input_tokens": 1_000_000,
            "output_tokens": 0}}),
        )],
    ));
    // A client that made no request ($2).
    events.extend(function(NO_HTTP));
    // A request whose body was not recorded: only the hand-recorded usage
    // ($1) prices it.
    events.extend(function(BODIES_OFF));
    events.extend([
        Event::NetworkAnnouncement(proto::NetworkAnnouncement {
            id: UNREAD,
            parent_id: BODIES_OFF,
            call_path_id: 2,
            started_at_ticks: 10_000,
            method: "POST".into(),
            url: "https://api.anthropic.com/v1/messages".into(),
            request_cas_id: None,
        }),
        event(UNREAD, "data", 20_000, None),
        complete(UNREAD, 30_000, proto::InvocationOutcome::Ok, None),
        // The root future's own usage, from a client without a request ($0.5).
        Event::ThreadCompletion(proto::ThreadCompletion {
            completed_at_ticks: 60_000,
            outcome: proto::InvocationOutcome::Ok as i32,
            panicked: false,
        }),
    ]);
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(ROOT, events)])),
            usage: Some(proto::UsageBatch {
                entries: vec![
                    usage(CALL, 1_000_000),
                    usage(NO_HTTP, 500_000),
                    usage(BODIES_OFF, 250_000),
                    usage(ROOT, 125_000),
                ],
            }),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    assert_eq!(
        rows(
            &mut index,
            "SELECT span_id, span_type, temporary_projections['cost'] FROM spans
             WHERE temporary_projections IS NOT NULL ORDER BY span_type, 3"
        ),
        vec![
            vec![recording.span(BODIES_OFF), json!("function"), json!(1.0)],
            vec![recording.span(NO_HTTP), json!("function"), json!(2.0)],
            vec![recording.span(ROOT), json!("future"), json!(0.5)],
            vec![recording.span(REQUEST), json!("network_span"), json!(4.0)],
        ]
    );
    // CALL's request is counted once.
    assert_eq!(
        rows(
            &mut index,
            "SELECT sum(temporary_projections['cost']) FROM spans"
        ),
        vec![vec![json!(7.5)]]
    );
}

#[test]
fn pricing_reads_each_response_body_once() {
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let data: Vec<Json> = (0..4)
        .map(|n| {
            sse(
                "content_block_delta",
                &json!({"type": "content_block_delta", "index": n,
                "usage": {"output_tokens": n}}),
            )
        })
        .collect();
    let mut events = vec![call()];
    events.extend(model_request(
        &recording,
        REQUEST,
        "https://api.anthropic.com/v1/messages",
        &json!({"model": "claude-opus-5-5"}),
        &data,
    ));
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(ROOT, events)])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let result = run(
        &mut index,
        "SELECT temporary_projections['output_tokens'] FROM spans WHERE span_type = 'network_span'",
    );
    assert_eq!(result.rows, vec![vec![json!(3)]]);
    // Four bodies and the request (the responses name no model), each read
    // once: SQL must not evaluate a body's text again for every path.
    let values = &result.outcome.query.values;
    assert_eq!((values.cas_loads, values.cas_cache_hits), (5, 0));
}

/// Query-time pricing on a recording with many model requests. Run with
/// `--run-ignored only`; prints how long the sum took.
#[test]
#[ignore = "a timing measurement"]
#[expect(clippy::print_stderr, reason = "the timings are the result")]
fn pricing_many_requests_takes() {
    const REQUESTS: u64 = 3_000;
    const CHUNKS: u64 = 20;
    let project = tempfile::tempdir().unwrap();
    let recording = Recording::new(project.path());
    let mut events = vec![call()];
    for n in 0..REQUESTS {
        let id = 1_000 + n;
        let usage = json!({"input_tokens": 900 + n, "cache_creation_input_tokens": 100,
            "cache_read_input_tokens": 200, "output_tokens": 30});
        // Half whole bodies, half streams of CHUNKS deltas around the usage.
        let data: Vec<Json> = if n % 2 == 0 {
            vec![body(&json!({"id": format!("msg_{n}"), "type": "message",
                "model": "claude-opus-5-5-20260915", "content": [{"type": "text", "text": "Hi"}],
                "usage": usage}))]
        } else {
            let mut data = vec![sse(
                "message_start",
                &json!({"type": "message_start",
                "message": {"id": format!("msg_{n}"), "model": "claude-opus-5-5-20260915",
                  "usage": usage}}),
            )];
            data.extend((0..CHUNKS).map(|chunk| {
                sse(
                    "content_block_delta",
                    &json!({"type": "content_block_delta", "index": 0,
                  "delta": {"type": "text_delta", "text": format!("{n} {chunk}")}}),
                )
            }));
            data.push(sse(
                "message_delta",
                &json!({"type": "message_delta",
                "usage": {"output_tokens": 30}}),
            ));
            data
        };
        events.extend(model_request(
            &recording,
            id,
            "https://api.anthropic.com/v1/messages",
            &json!({}),
            &data,
        ));
    }
    recording.write(
        1,
        proto::RecordingFile {
            definitions: Some(definitions()),
            clock_states: Some(valid()),
            spans: Some(spans(vec![section(ROOT, events)])),
            ..Default::default()
        },
    );
    let mut index = index(project.path());
    let started = std::time::Instant::now();
    index.refresh().unwrap();
    let indexed = started.elapsed();
    let sum = "SELECT count(*), sum(temporary_projections['cost']) FROM spans
        WHERE span_type = 'network_span'";
    let started = std::time::Instant::now();
    let names = run(&mut index, "SELECT count(span_name) FROM spans");
    eprintln!(
        "without projections: {:?}, {} blobs read",
        started.elapsed(),
        names.outcome.query.values.cas_loads
    );
    for pass in 0..2 {
        let started = std::time::Instant::now();
        let result = run(&mut index, sum);
        eprintln!(
            "pass {pass}: {REQUESTS} requests ({} data events), indexed in {indexed:?}, summed in {:?}, {} blobs read: {:?}",
            REQUESTS / 2 * (CHUNKS + 2) + REQUESTS / 2,
            started.elapsed(),
            result.outcome.query.values.cas_loads,
            result.rows
        );
    }
}
