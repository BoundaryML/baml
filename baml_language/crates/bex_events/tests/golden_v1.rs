//! Golden-fixture verifier for `testdata/golden/v1/` (design §6.9).
//!
//! Each fixture has a deterministic builder in this file. Normal runs assert
//! the builder's bytes equal the committed file byte-for-byte (encoder drift
//! fails CI) and that readers honor the recovery contract on torn tails.
//! `BAML_GOLDEN_WRITE=1` regenerates the files — legitimate exactly once per
//! new fixture; changing an existing file means "mint v2/", not "regenerate".

use bex_events::{
    ids::BoundaryId,
    prof::{
        clock::TickConverter,
        encode::{encode_disk_event, encode_length_delimited_message},
        pb,
        read::read_bamlprof_from_bytes,
    },
    run::{ProjectGeneration, ProjectId, RunRequestSummary, RunTarget, RunTimeAnchor},
    value::{
        ByteValueArtifactSink, CaptureLossKind, CaptureLossReason, CaptureLossRecord,
        RunStartedRecord, ValueCodec, ValueFileRecord, ValueWriter,
        read::read_bamlvalue_from_bytes,
    },
};

fn golden_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/golden/v1")
}

/// Byte-compare (or regenerate under BAML_GOLDEN_WRITE=1).
fn assert_golden(name: &str, built: &[u8]) {
    let path = golden_dir().join(name);
    if std::env::var("BAML_GOLDEN_WRITE").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, built).unwrap();
        return;
    }
    let committed = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "missing golden fixture {} ({err}); generate once with \
             BAML_GOLDEN_WRITE=1 cargo test -p bex_events --test golden_v1",
            path.display()
        )
    });
    assert!(
        committed == built,
        "{name}: builder output differs from the FROZEN v1 fixture \
         ({} committed vs {} built bytes). v1 bytes never change — if an \
         encoder legitimately changed, mint testdata/golden/v2/ instead.",
        committed.len(),
        built.len()
    );
}

// ---------------------------------------------------------------------------
// .bamlprof
// ---------------------------------------------------------------------------

/// Deterministic `.bamlprof`: fixed ids, identity clock, two functions, one
/// thread with two calls (one errored), a `$id` override, one heartbeat.
fn build_events_bamlprof() -> Vec<u8> {
    let process_id = *b"\x42BAMLGOLDENV1\x00\x00\x01";
    let meta = bex_events::prof::EngineProfileMetadata {
        program_id: "golden-program".to_string(),
        source_snapshot_id: Some("golden-snapshot".to_string()),
        revision_id: Some("golden-revision".to_string()),
        functions: vec![
            bex_events::prof::FunctionMetaEntry {
                function_id: 1,
                fqn: "user.Main".to_string(),
                source_file: "main.baml".to_string(),
                span_start: 0,
                span_end: 42,
                kind: "bytecode".to_string(),
                definition_key: Some("function:user.Main".to_string()),
                owner_type: None,
                parent_function: None,
                lambda_path: None,
                package_name: Some("user".to_string()),
                namespace: vec![],
            },
            bex_events::prof::FunctionMetaEntry {
                function_id: 2,
                fqn: "user.Helper".to_string(),
                source_file: "main.baml".to_string(),
                span_start: 44,
                span_end: 80,
                kind: "bytecode".to_string(),
                definition_key: Some("function:user.Helper".to_string()),
                owner_type: None,
                parent_function: None,
                lambda_path: None,
                package_name: Some("user".to_string()),
                namespace: vec![],
            },
        ],
        dictionary: None,
    };
    let conv = TickConverter::identity();
    let header = bex_events::prof::encode::build_header(
        process_id,
        7,
        1_700_000_000_000_000_000u128,
        Some(&meta),
        &conv,
    );
    let mut out = Vec::new();
    encode_length_delimited_message(&mut out, &header).expect("header encodes");

    let events = [
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::StartThread(pb::StartThread {
                thread_id: 1,
                parent_thread_id: None,
                parent_call_id: None,
                name: None,
                timestamp_ns: 1_000,
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::CallFunction(pb::CallFunction {
                thread_id: 1,
                call_id: 1,
                parent_call_id: None,
                function_id: 1,
                timestamp_ns: 1_100,
                call_site_file_id: None,
                call_site_start_offset: None,
                call_site_end_offset: None,
                call_site_line: None,
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::SetFunctionId(pb::SetFunctionId {
                thread_id: 1,
                call_id: 1,
                id: b"GOLDENID12345678".to_vec(),
                timestamp_ns: 1_150,
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::CallFunction(pb::CallFunction {
                thread_id: 1,
                call_id: 2,
                parent_call_id: Some(1),
                function_id: 2,
                timestamp_ns: 1_200,
                call_site_file_id: Some(0),
                call_site_start_offset: Some(10),
                call_site_end_offset: Some(20),
                call_site_line: Some(3),
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::EndFunction(pb::EndFunction {
                thread_id: 1,
                call_id: 2,
                status: pb::FunctionEndStatus::Errored as i32,
                timestamp_ns: 2_200,
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::Heartbeat(pb::Heartbeat {
                timestamp_ns: 2_500,
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::EndFunction(pb::EndFunction {
                thread_id: 1,
                call_id: 1,
                status: pb::FunctionEndStatus::Ok as i32,
                timestamp_ns: 3_000,
            })),
        },
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::EndThread(pb::EndThread {
                thread_id: 1,
                status: pb::ThreadEndStatus::Completed as i32,
                timestamp_ns: 3_100,
            })),
        },
    ];
    for event in &events {
        encode_disk_event(&mut out, event);
    }
    out
}

#[test]
fn bamlprof_fixture_is_frozen_and_parses() {
    let built = build_events_bamlprof();
    assert_golden("events.bamlprof", &built);

    let contents = read_bamlprof_from_bytes(&built).expect("golden bamlprof parses");
    assert!(!contents.truncated);
    assert_eq!(contents.events.len(), 8);
    assert_eq!(contents.header.engine_id, 7);
    let table = contents.header.function_table.as_ref().unwrap();
    assert_eq!(table.functions.len(), 2);
    assert_eq!(table.functions[0].fqn, "user.Main");
}

#[test]
fn bamlprof_torn_tails_honor_the_recovery_contract() {
    let built = build_events_bamlprof();
    let contents = read_bamlprof_from_bytes(&built).unwrap();
    assert_eq!(contents.events.len(), 8);

    // Interesting offsets (§6.9): inside the header varint/body, exactly at
    // header end, inside an event frame, between frames, last byte missing.
    // Header length: find it by re-encoding just the header.
    let header_len = {
        let header = read_bamlprof_from_bytes(&built).unwrap().header;
        let mut buf = Vec::new();
        encode_length_delimited_message(&mut buf, &header).expect("header encodes");
        buf.len()
    };

    // Mid-header: reader must fail loudly (InvalidData), never fabricate.
    for cut in [1, header_len / 2, header_len.saturating_sub(1)] {
        assert!(
            read_bamlprof_from_bytes(&built[..cut]).is_err(),
            "cut at {cut} (inside header) must be an explicit error"
        );
    }

    // Exactly at header end: valid file with zero events.
    let at_header = read_bamlprof_from_bytes(&built[..header_len]).unwrap();
    assert_eq!(at_header.events.len(), 0);
    assert!(!at_header.truncated);

    // Every offset strictly inside the event stream: whole-message prefix +
    // truncated flag, never an error, never a partial event.
    for cut in header_len + 1..built.len() {
        let torn = read_bamlprof_from_bytes(&built[..cut])
            .unwrap_or_else(|err| panic!("cut at {cut}: reader must tolerate torn tails: {err}"));
        assert!(
            torn.truncated || torn.events.len() < 8,
            "cut at {cut}: full stream can't survive a truncation"
        );
        assert!(torn.events.len() < 8, "cut at {cut} dropped no event");
    }
}

// ---------------------------------------------------------------------------
// .bamlvalue
// ---------------------------------------------------------------------------

/// Deterministic `.bamlvalue`: fixed boundary id; RunStarted, one captured
/// value, one capture-loss marker.
fn build_values_bamlvalue() -> Vec<u8> {
    let boundary = BoundaryId::from_bytes(*b"GOLDENBOUNDARY01");
    let sink = ByteValueArtifactSink::new();
    let mut writer = ValueWriter::new(sink, boundary).expect("in-memory writer");
    writer
        .append_run_started(&RunStartedRecord {
            request: RunRequestSummary {
                project_id: ProjectId("golden-project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "user.Main".into(),
                },
                args_summary: Some("()".to_string()),
                options_summary: None,
            },
            created_at_ms: 1_700_000_000_000,
            time_anchor: RunTimeAnchor {
                epoch_created_at_ms: 1_700_000_000_000,
                trace_zero_ns: 0,
            },
        })
        .unwrap();
    writer
        .append_body(ValueCodec::BamlOutboundValue, b"{\"answer\":42}".to_vec())
        .unwrap();
    writer
        .append_capture_loss(&CaptureLossRecord {
            kind: CaptureLossKind::Value,
            reason: CaptureLossReason::QueueFull,
            skipped_count: 3,
            call: None,
            message: Some("golden loss marker".to_string()),
            timestamp_ms: 1_700_000_000_500,
        })
        .unwrap();
    writer.flush().unwrap();
    writer.sink().bytes().to_vec()
}

#[test]
fn bamlvalue_fixture_is_frozen_and_parses() {
    let built = build_values_bamlvalue();
    assert_golden("values.bamlvalue", &built);

    let contents = read_bamlvalue_from_bytes(&built).expect("golden bamlvalue parses");
    assert!(!contents.truncated);
    assert_eq!(contents.records.len(), 3);
    assert!(matches!(
        contents.records[0],
        ValueFileRecord::RunStarted(_)
    ));
    match &contents.records[1] {
        ValueFileRecord::CapturedValue(v) => {
            assert_eq!(v.body, b"{\"answer\":42}");
            assert_eq!(v.value_ref.id, "value_1");
        }
        other => panic!("expected captured value, got {other:?}"),
    }
    match &contents.records[2] {
        ValueFileRecord::CaptureLoss(loss) => assert_eq!(loss.skipped_count, 3),
        other => panic!("expected capture loss, got {other:?}"),
    }
}

#[test]
fn bamlvalue_torn_tails_honor_the_recovery_contract() {
    let built = build_values_bamlvalue();
    // Find the header length the same way the reader does.
    let header_len = {
        // The header is a length-delimited ValueFileHeaderV1 at offset 0;
        // reuse the reader on a header-only prefix search: smallest prefix
        // that parses with zero records.
        (1..built.len())
            .find(|&len| {
                read_bamlvalue_from_bytes(&built[..len])
                    .map(|c| c.records.is_empty() && !c.truncated)
                    .unwrap_or(false)
            })
            .expect("header-only prefix exists")
    };
    for cut in [1, header_len / 2] {
        assert!(
            read_bamlvalue_from_bytes(&built[..cut]).is_err(),
            "cut at {cut} (inside header) must be an explicit error"
        );
    }
    for cut in header_len + 1..built.len() {
        let torn = read_bamlvalue_from_bytes(&built[..cut])
            .unwrap_or_else(|err| panic!("cut at {cut}: reader must tolerate torn tails: {err}"));
        assert!(torn.records.len() < 3, "cut at {cut} dropped no record");
    }
}
