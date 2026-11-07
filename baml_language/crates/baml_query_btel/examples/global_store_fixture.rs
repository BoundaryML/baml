//! Generate isolated multi-project query benchmark data, never a user's store.
//! Usage: `global_store_fixture NEW_DIRECTORY RECORDINGS CALLS_PER_RECORDING [VALUE_BYTES] [shuffled|ordered]`.
//! Follow with `query_bench NEW_DIRECTORY ITERATIONS SQL...` or CLI queries.
//! Each run has a distinct process/recording ID, one root future, N completed
//! functions, one network span and two context slots per span. Twenty project
//! identities share the store. Optional captured outputs are distinct per run
//! and shared by its calls. No model or network is used.
#![allow(clippy::print_stdout)]
use std::path::Path;

use btel_reader::layout::SourceLayout;
use btel_recorder::proto::{self, span_event::Event};
use btel_snapshot::{BlobScratch, Limits, SnapshotPool, host};
use prost::Message as _;

/// Publish deterministic multi-project evidence beneath a new, explicit path,
/// with optional captured outputs and independently shuffled recording IDs.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(
        (4..=6).contains(&args.len()),
        "NEW_DIRECTORY RECORDINGS CALLS_PER_RECORDING [VALUE_BYTES] [shuffled|ordered]"
    );
    let project = Path::new(&args[1]);
    // Refuse existing directories, so reruns cannot overwrite recordings or
    // derived state. All artifacts are beneath this explicitly supplied path.
    std::fs::create_dir(project).expect("fixture directory must be new");
    let recordings: u64 = args[2].parse().expect("RECORDINGS");
    let calls: u64 = args[3].parse().expect("CALLS_PER_RECORDING");
    let value_bytes: usize = args
        .get(4)
        .map_or(0, |arg| arg.parse().expect("VALUE_BYTES"));
    let order = args.get(5).map_or("shuffled", String::as_str);
    assert!(matches!(order, "shuffled" | "ordered"));
    assert!(recordings > 0 && calls > 0);
    assert!(value_bytes <= 32 * 1024);
    let layout = SourceLayout::for_project(project);
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut bytes = 0;
    let mut cas_bytes = 0;
    let mut cas_blobs = 0;
    for run in 1..=recordings {
        // Real recording IDs are random. An odd multiplier permutes IDs
        // deterministically, separating physical and chronological order.
        // Ordered IDs exercise the sorter's worst case: each next function
        // row displaces an older row from the top-N heap.
        let id = if order == "ordered" {
            u128::from(run)
        } else {
            u128::from(run).wrapping_mul(0x9e37_79b9_7f4a_7c15_d1b5_4a32_d192_ed03)
        }
        .to_be_bytes();
        let group = run % 20;
        let ns = 1_700_000_000_000_000_000 + run * 1_000_000;
        let output = (value_bytes > 0).then(|| {
            let snapshot = host::capture(
                pool.try_acquire().unwrap(),
                &host::HostValue::String(format!("run-{run}:{}", "x".repeat(value_bytes))),
            );
            for blob in snapshot.blobs() {
                let path = layout.blob_path(blob.id());
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                let mut file = std::fs::File::create(path).unwrap();
                blob.write(&mut BlobScratch::default(), &mut file).unwrap();
                cas_bytes += file.metadata().unwrap().len();
                cas_blobs += 1;
            }
            let id = *snapshot.root_id().as_bytes();
            proto::CasId {
                low: u64::from_le_bytes(id[..8].try_into().unwrap()),
                high: u64::from_le_bytes(id[8..].try_into().unwrap()),
            }
        });
        let mut events = vec![Event::ThreadAnnouncement(proto::ThreadAnnouncement {})];
        for call in 0..calls {
            events.push(Event::FunctionCompletion(proto::FunctionCompletion {
                id: call + 2,
                parent_id: 1,
                node: 2,
                entered_at_ticks: call * 100,
                exited_at_ticks: call * 100 + 50,
                completion_flags: 1,
                value_cas_id: output,
                ..Default::default()
            }));
        }
        events.extend([
            Event::NetworkAnnouncement(proto::NetworkAnnouncement {
                id: calls + 2,
                parent_id: 1,
                call_path_id: 1,
                method: "GET".into(),
                url: format!("https://project-{group}.invalid/fixture"),
                ..Default::default()
            }),
            Event::NetworkCompletion(proto::NetworkCompletion {
                span_id: calls + 2,
                completed_at_ticks: calls * 100,
                outcome: 1,
                ..Default::default()
            }),
            Event::ThreadCompletion(proto::ThreadCompletion {
                completed_at_ticks: calls * 100,
                outcome: 1,
                ..Default::default()
            }),
        ]);
        let file = proto::RecordingFile {
            header: Some(proto::RecordingHeader {
                format_major: 2,
                format_minor: 13,
                recording_id: id.to_vec(),
                process_id: Some(id.to_vec()),
                source_snapshot_id: Some(vec![u8::try_from(group).unwrap(); 32]),
                host: Some("python".into()),
                command: vec!["python".into(), format!("project-{group}/main.py")],
                process_started_at_unix_ns: Some(i64::try_from(ns).unwrap()),
                ..Default::default()
            }),
            sequence: 1,
            definitions: Some(proto::Definitions {
                functions: vec![proto::FunctionDefinition {
                    function_id: 1,
                    resolution: Some(proto::function_definition::Resolution::Metadata(
                        proto::FunctionMetadata {
                            fqn: format!("project{group}.Main"),
                            display_name: "Main".into(),
                            ..Default::default()
                        },
                    )),
                }],
                call_paths: vec![proto::CallPathDefinition {
                    call_path_id: 1,
                    thread_id: 1,
                    callee_function_id: 1,
                    edge: proto::CallPathEdge::Synchronous as i32,
                    ..Default::default()
                }],
                threads: vec![proto::ThreadDefinition {
                    thread_id: 1,
                    clock_epoch_id: 1,
                    ..Default::default()
                }],
                clock_epochs: vec![proto::ClockEpochDefinition {
                    epoch_id: 1,
                    domain_id: 1,
                    source: proto::ClockSource::Mock as i32,
                    multiplier: 1,
                    utc: Some(proto::UtcAnchor {
                        unix_nanos: Some(proto::UnixNanos { low: ns, high: 0 }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
            }),
            spans: Some(proto::SpanBatch {
                sections: vec![proto::ThreadSection {
                    thread_id: 1,
                    context: Some(proto::thread_section::Context::EmptyContext(true)),
                    events: events
                        .into_iter()
                        .map(|event| proto::SpanEvent { event: Some(event) })
                        .collect(),
                }],
            }),
            clock_states: Some(proto::ClockStateBatch {
                states: vec![proto::ClockEpochState {
                    epoch_id: 1,
                    status: proto::TimingStatus::Valid as i32,
                    r#final: true,
                }],
            }),
            end: Some(proto::RecordingEnd {
                process_end: Some(proto::ProcessEnd {
                    status: proto::ProcessStatus::Success as i32,
                    at_unix_ns: i64::try_from(ns + calls * 100).unwrap(),
                }),
            }),
            ..Default::default()
        };
        let directory = layout.recordings.join(btel_reader::discovery::hex(&id));
        std::fs::create_dir_all(&directory).unwrap();
        let encoded = file.encode_to_vec();
        bytes += encoded.len();
        std::fs::write(directory.join("00000000000000000001.btel"), encoded).unwrap();
    }
    println!(
        "{}",
        serde_json::json!({
            "project": project,
            "projects": recordings.min(20),
            "recordings": recordings,
            "files": recordings,
            "span_rows": recordings * (calls + 2),
            "recording_bytes": bytes,
            "cas_bytes": cas_bytes,
            "cas_blobs": cas_blobs,
            "id_order": order,
        })
    );
}
