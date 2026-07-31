use std::{collections::BTreeMap, sync::Arc};

use bex_events::prof::storage::{
    BcctHeader, BcctWriter, BlockRows, CctDeltaRow, ClockDescriptor, MarkerKind, MarkerRow,
    NodeBirthRow, SegmentState,
};
use bex_query::{
    BqfFrame, ByteBudgetCache, ByteSource, Counters, ExactTimelineCall, ExactTimelineTier, FileId,
    FoldedCct, FoldedNode, LeftHeavyRequest, ListRunsRequest, LiveMirrorSource, MemorySource,
    QueryEngine, QueryPoll, RangeCacheSource, RunState, SourceSnapshot, TimelineOverlay, Viewport,
    WindowDelta, left_heavy, scan_bcct, timeline, timeline_with_overlay,
};

fn header() -> BcctHeader {
    BcctHeader {
        process_euid: [1; 16],
        engine_id: 7,
        session_seg_seq: 1,
        started_epoch_ns: 10,
        clock: ClockDescriptor {
            kind: 1,
            quality: 1,
            tick_ns_numer: 1,
            tick_ns_denom: 1,
        },
        revision_id: [2; 32],
    }
}

fn sealed_fixture() -> Vec<u8> {
    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    writer
        .append(
            &BlockRows::NodeBirth(vec![
                NodeBirthRow {
                    node_id: 1,
                    parent_node_id: 0,
                    function_id: 10,
                    logical_thread_id: 100,
                    partition_id: 4,
                },
                NodeBirthRow {
                    node_id: 2,
                    parent_node_id: 1,
                    function_id: 20,
                    logical_thread_id: 100,
                    partition_id: 4,
                },
            ]),
            1,
            1,
        )
        .unwrap();
    writer
        .append(
            &BlockRows::CctDelta(vec![
                CctDeltaRow {
                    node_id: 1,
                    enters: 10,
                    ends_ok: 9,
                    ends_err: 1,
                    total_ns: 1_000,
                    self_ns: 700,
                    await_ns: 100,
                    ..CctDeltaRow::default()
                },
                CctDeltaRow {
                    node_id: 2,
                    enters: 5,
                    ends_ok: 5,
                    total_ns: 300,
                    self_ns: 300,
                    ..CctDeltaRow::default()
                },
            ]),
            10,
            20,
        )
        .unwrap();
    writer
        .append(
            &BlockRows::Marker(vec![MarkerRow {
                kind: MarkerKind::Loss,
                timestamp_ns: 20,
                node_id: Some(1),
                count: 3,
                message: "shed rows".to_owned(),
            }]),
            20,
            20,
        )
        .unwrap();
    writer.seal().unwrap();
    writer.into_inner()
}

#[test]
fn sans_io_scan_requests_exact_ranges_then_validates_sealed_bcct() {
    let bytes = sealed_fixture();
    let file = FileId(1);
    let source = RangeCacheSource::new(64 * 1024);
    source.set_snapshot(
        file,
        SourceSnapshot {
            committed_len: bytes.len() as u64,
            generation: 9,
        },
    );
    let scan = loop {
        match scan_bcct(&source, file).unwrap() {
            QueryPoll::Ready(scan) => break scan,
            QueryPoll::NeedData { ranges } => {
                assert!(!ranges.is_empty());
                for range in ranges {
                    assert!(range.end <= bytes.len() as u64);
                    let start = usize::try_from(range.start).unwrap();
                    let end = usize::try_from(range.end).unwrap();
                    source.insert(
                        file,
                        9,
                        range.start,
                        Arc::<[u8]>::from(bytes[start..end].to_vec()),
                    );
                }
            }
        }
    };
    assert!(matches!(scan.state, SegmentState::Sealed(_)));
    assert_eq!(scan.source.generation, 9);
    assert_eq!(scan.committed_len, bytes.len() as u64);
    assert!(source.retained_bytes() <= 64 * 1024);
}

#[test]
fn torn_tail_is_visible_and_fold_reports_capture_loss() {
    let bytes = sealed_fixture();
    let file = FileId(2);
    let memory = MemorySource::new();
    memory.insert(file, Arc::<[u8]>::from(bytes.clone()));
    let scan = match scan_bcct(&memory, file).unwrap() {
        QueryPoll::Ready(scan) => scan,
        QueryPoll::NeedData { .. } => panic!("memory source must be resident"),
    };
    let folded = bex_query::fold_bcct(&[scan], Some(4)).unwrap();
    assert_eq!(folded.nodes[&1].counters.enters, 10);
    assert_eq!(folded.nodes[&2].counters.total_ns, 300);
    assert_eq!(folded.meta.capture_loss.len(), 1);
    assert!(!folded.meta.complete);

    let torn_file = FileId(3);
    memory.insert(
        torn_file,
        Arc::<[u8]>::from(bytes[..bytes.len() - 17].to_vec()),
    );
    let torn = match scan_bcct(&memory, torn_file).unwrap() {
        QueryPoll::Ready(scan) => scan,
        QueryPoll::NeedData { .. } => panic!("memory source must be resident"),
    };
    assert_eq!(torn.state, SegmentState::Torn);
    assert!(torn.committed_len < memory.committed_len(torn_file));
}

#[test]
fn query_engine_cache_is_generation_keyed_and_byte_bounded() {
    let file = FileId(4);
    let memory = MemorySource::new();
    memory.insert(file, Arc::<[u8]>::from(sealed_fixture()));
    let engine = QueryEngine::with_cache_budget(memory, 4096);
    let first = match engine.open_run(&[file], Some(4)).unwrap() {
        QueryPoll::Ready(cct) => cct,
        QueryPoll::NeedData { .. } => panic!("memory source must be resident"),
    };
    let second = match engine.open_run(&[file], Some(4)).unwrap() {
        QueryPoll::Ready(cct) => cct,
        QueryPoll::NeedData { .. } => panic!("memory source must be resident"),
    };
    assert!(Arc::ptr_eq(&first, &second));
    assert!(engine.cache_retained_bytes() <= engine.cache_max_bytes());
}

#[test]
fn split_checkpoint_rows_are_combined_before_replacing_absolute_totals() {
    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    writer
        .append(
            &BlockRows::NodeBirth(vec![NodeBirthRow {
                node_id: 1,
                parent_node_id: 0,
                function_id: 10,
                logical_thread_id: 100,
                partition_id: 4,
            }]),
            1,
            1,
        )
        .unwrap();
    writer
        .append(
            &BlockRows::NodeTotal(vec![
                CctDeltaRow {
                    node_id: 1,
                    enters: u32::MAX,
                    total_ns: 100,
                    ..CctDeltaRow::default()
                },
                CctDeltaRow {
                    node_id: 1,
                    enters: 5,
                    ..CctDeltaRow::default()
                },
            ]),
            2,
            2,
        )
        .unwrap();
    writer.seal().unwrap();
    let source = MemorySource::new();
    let file = FileId(99);
    source.insert(file, Arc::<[u8]>::from(writer.into_inner()));
    let scan = match scan_bcct(&source, file).unwrap() {
        QueryPoll::Ready(scan) => scan,
        QueryPoll::NeedData { .. } => panic!("memory source must be resident"),
    };
    let cct = bex_query::fold_bcct(&[scan], Some(4)).unwrap();
    assert_eq!(cct.nodes[&1].counters.enters, u64::from(u32::MAX) + 5);
    assert_eq!(cct.nodes[&1].counters.total_ns, 100);
}

#[test]
fn byte_budget_cache_evicts_by_bytes_not_entry_count() {
    let mut cache = ByteBudgetCache::new(10);
    assert!(cache.insert(1_u8, "one", 6));
    assert!(cache.insert(2_u8, "two", 6));
    assert!(cache.get(&1).is_none());
    assert_eq!(cache.get(&2), Some(&"two"));
    assert_eq!(cache.retained_bytes(), 6);
    assert!(!cache.insert(3_u8, "oversize", 11));
    assert_eq!(cache.len(), 1);
}

#[test]
fn left_heavy_emits_visible_smaller_aggregation_and_bounded_bqf1() {
    let cct = direct_cct(64, 1_000);
    let response = left_heavy(
        &cct,
        LeftHeavyRequest {
            pixel_width: 1,
            max_bytes: 4096,
        },
    )
    .unwrap();
    assert!(response.nodes.iter().any(|node| node.synthetic_smaller));
    let frame = response.to_bqf(17, 4096).unwrap();
    assert!(frame.as_bytes().len() <= 4096);
    assert_eq!(&frame.as_bytes()[..4], b"BQF1");
    assert_eq!(BqfFrame::decode(frame.as_bytes()).unwrap(), frame);
}

#[test]
fn timeline_frame_size_is_invariant_to_aggregate_call_count() {
    let one_million = direct_cct(256, 1_000_000);
    let thirty_six_million = direct_cct(256, 36_000_000);
    let viewport = Viewport {
        start_ns: 0,
        end_ns: 256_000,
        pixel_width: 256,
        lanes: 1,
        max_bytes: 4096,
    };
    let small = timeline(&one_million, viewport).unwrap();
    let large = timeline(&thirty_six_million, viewport).unwrap();
    assert!(small.meta.lod_degraded);
    assert!(!small.tiers.exact_recency);
    assert!(small.tiers.aggregate_bands);
    assert!(!small.tiers.exact_evidence);
    let small_frame = small.to_bqf(1, viewport.max_bytes).unwrap();
    let large_frame = large.to_bqf(2, viewport.max_bytes).unwrap();
    assert_eq!(small_frame.as_bytes().len(), large_frame.as_bytes().len());
    assert!(small_frame.as_bytes().len() <= viewport.max_bytes);

    let mut corrupt = small_frame.as_bytes().to_vec();
    let payload_index = corrupt.len() - 5;
    corrupt[payload_index] ^= 1;
    assert!(BqfFrame::decode(&corrupt).is_err());

    let zoomed = timeline(
        &one_million,
        Viewport {
            start_ns: 0,
            end_ns: 1000,
            pixel_width: 256,
            lanes: 1,
            max_bytes: 64 * 1024,
        },
    )
    .unwrap();
    assert_eq!(zoomed.aggregate_resolution_ns, 1000);
    assert!(
        zoomed
            .meta
            .warnings
            .iter()
            .any(|warning| warning.contains("aggregate resolution limit"))
    );
}

#[test]
fn timeline_keeps_exact_recency_and_evidence_distinct_from_aggregate_bands() {
    let cct = direct_cct(64, 1_000_000);
    let viewport = Viewport {
        start_ns: 0,
        end_ns: 64_000,
        pixel_width: 256,
        lanes: 1,
        max_bytes: 64 * 1024,
    };
    let response = timeline_with_overlay(
        &cct,
        viewport,
        &TimelineOverlay {
            exact_calls: vec![
                ExactTimelineCall {
                    tier: ExactTimelineTier::Recency,
                    logical_thread_id: 100,
                    call_id: 41,
                    node_id: 1,
                    function_id: 10,
                    start_ns: 50_000,
                    end_ns: 51_000,
                    status: 0,
                },
                ExactTimelineCall {
                    tier: ExactTimelineTier::Evidence,
                    logical_thread_id: 100,
                    call_id: 42,
                    node_id: 1,
                    function_id: 10,
                    start_ns: 55_000,
                    end_ns: 0,
                    status: 0,
                },
            ],
            evicted_recent_calls: 9000,
        },
    )
    .unwrap();
    assert!(response.tiers.aggregate_bands);
    assert!(response.tiers.exact_recency);
    assert!(response.tiers.exact_evidence);
    assert_eq!(response.exact_rects.len(), 2);
    assert!(response.exact_rects[1].open);
    assert_eq!(response.evicted_recent_calls, 9000);
    assert!(response.meta.warnings.iter().any(|warning| {
        warning.contains("9000 older calls are represented only by aggregates")
    }));
    let frame = response.to_bqf(8, viewport.max_bytes).unwrap();
    assert_eq!(
        frame.header().unwrap().nrows as usize,
        response.bands.len() + response.exact_rects.len()
    );
    BqfFrame::decode(frame.as_bytes()).unwrap();
}

#[test]
fn live_mirror_generation_and_exact_sidecar_feed_one_query_engine() {
    let file = FileId(88);
    let source = LiveMirrorSource::new();
    source.insert(file, Arc::<[u8]>::from(sealed_fixture()));
    source.publish_timeline(
        file,
        TimelineOverlay {
            exact_calls: vec![ExactTimelineCall {
                tier: ExactTimelineTier::Recency,
                logical_thread_id: 100,
                call_id: 7,
                node_id: 1,
                function_id: 10,
                start_ns: 10,
                end_ns: 20,
                status: 0,
            }],
            evicted_recent_calls: 3,
        },
    );
    let generation = source.generation(file);
    let engine = QueryEngine::new(source);
    let response = match engine
        .timeline_live(
            &[file],
            Some(4),
            Viewport {
                start_ns: 1,
                end_ns: 100,
                pixel_width: 100,
                lanes: 1,
                max_bytes: 16 * 1024,
            },
        )
        .unwrap()
    {
        QueryPoll::Ready(response) => response,
        QueryPoll::NeedData { .. } => panic!("live mirror bytes are resident"),
    };
    assert_eq!(response.exact_rects.len(), 1);
    assert_eq!(response.evicted_recent_calls, 3);
    assert_eq!(engine.source().generation(file), generation);
}

#[cfg(feature = "native")]
#[test]
fn run_listing_scans_only_metadata_and_bounds_its_frame() {
    use std::{
        fs,
        io::Write as _,
        time::{SystemTime, UNIX_EPOCH},
    };

    use bex_events::{
        ids::BoundaryId,
        prof::storage::{
            BoundaryBeginMeta, BoundaryCompleteMeta, BoundaryCounts, BoundaryLossMeta, MetaWriter,
            TypedBoundaryMeta, encode_typed_boundary_meta,
        },
    };
    use bex_query::{FileSource, list_runs, open_run_meta, open_run_meta_pinned};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("bex-query-runs-{}-{nonce}", std::process::id()));
    let boundary_id = BoundaryId::from_bytes([7; 16]);
    let directory = root
        .join(".baml")
        .join("history")
        .join(format!("123-AgentRun-{}", boundary_id.to_wire_string()));
    fs::create_dir_all(&directory).unwrap();
    let mut writer = MetaWriter::new(Vec::new());
    for record in [
        TypedBoundaryMeta::Begin(BoundaryBeginMeta {
            boundary_id: boundary_id.as_bytes(),
            target: "AgentRunTyped".to_owned(),
            source: "test".to_owned(),
            created_ms: 456,
            project_id: "project".to_owned(),
            revision_id: [9; 32],
            capture_defaults: 0,
        }),
        TypedBoundaryMeta::Complete(BoundaryCompleteMeta {
            status: "ok".to_owned(),
            completed_ms: 789,
            last_seg_seq: 1,
            counts: BoundaryCounts::default(),
            diagnostics: Vec::new(),
            dump_refs: Vec::new(),
        }),
    ] {
        let (kind, payload) = encode_typed_boundary_meta(&record).unwrap();
        writer.append(kind, &payload).unwrap();
    }
    fs::write(directory.join("boundary.bamlmeta"), writer.into_inner()).unwrap();
    fs::write(directory.join("cct.bamlcct"), sealed_fixture()).unwrap();

    let listing = list_runs(
        std::slice::from_ref(&root),
        ListRunsRequest {
            limit: 10,
            max_bytes: 4096,
            cursor: None,
        },
    )
    .unwrap();
    assert_eq!(listing.runs.len(), 1);
    assert_eq!(listing.runs[0].state, RunState::Complete);
    assert_eq!(listing.runs[0].created_ms, 456);
    assert_eq!(listing.runs[0].target, "AgentRunTyped");
    assert!(listing.runs[0].has_snapshot);
    assert!(listing.meta.complete);
    let frame = listing.to_bqf(44, 4096).unwrap();
    assert!(frame.as_bytes().len() <= 4096);

    let meta = open_run_meta(&directory).unwrap();
    assert_eq!(meta.records.len(), 2);
    assert_eq!(meta.summary.boundary_id, [7; 16]);
    let pins = meta
        .meta
        .snapshot
        .iter()
        .map(|watermark| (watermark.file, watermark.source))
        .collect::<BTreeMap<_, _>>();
    let mut appended = MetaWriter::new(Vec::new());
    let (kind, payload) = encode_typed_boundary_meta(&TypedBoundaryMeta::Loss(BoundaryLossMeta {
        timestamp_ns: 999,
        kind: "test_append".to_owned(),
        count: 1,
        detail: "must remain invisible to the pin".to_owned(),
    }))
    .unwrap();
    appended.append(kind, &payload).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(directory.join("boundary.bamlmeta"))
        .unwrap()
        .write_all(&appended.into_inner())
        .unwrap();
    let pinned = open_run_meta_pinned(&directory, &pins).unwrap();
    assert_eq!(pinned.records.len(), 2);
    assert_eq!(pinned.summary.state, RunState::Complete);
    let grown = open_run_meta(&directory).unwrap();
    assert_eq!(grown.records.len(), 3);
    assert_eq!(grown.summary.state, RunState::PartialWithLoss);
    let engine = QueryEngine::new(FileSource::new());
    let native = engine.register_native_run(&pinned).unwrap();
    let generation = engine.source().generation(native.files[0]);
    engine.refresh_native_run(&native).unwrap();
    assert_eq!(
        engine.source().generation(native.files[0]),
        generation,
        "unchanged live polls must reuse the cached fold generation"
    );
    let cct = match engine.open_native_run(&pinned).unwrap() {
        QueryPoll::Ready(cct) => cct,
        QueryPoll::NeedData { .. } => panic!("native files must be resident"),
    };
    assert_eq!(cct.nodes.len(), 2);

    fs::remove_dir_all(root).unwrap();
}

fn direct_cct(windows: u32, calls: u64) -> FoldedCct {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        1,
        FoldedNode {
            node_id: 1,
            function_id: 10,
            logical_thread_id: 100,
            counters: Counters {
                enters: calls,
                ends_ok: calls,
                total_ns: u64::from(windows) * 1000,
                self_ns: u64::from(windows) * 800,
                ..Counters::default()
            },
            ..FoldedNode::default()
        },
    );
    nodes.insert(
        2,
        FoldedNode {
            node_id: 2,
            parent_node_id: 1,
            function_id: 20,
            logical_thread_id: 100,
            counters: Counters {
                enters: calls / 10,
                ends_ok: calls / 10,
                total_ns: u64::from(windows) * 100,
                self_ns: u64::from(windows) * 100,
                ..Counters::default()
            },
            ..FoldedNode::default()
        },
    );
    let windows = (0..windows)
        .map(|bucket| WindowDelta {
            first_ts_ns: u64::from(bucket) * 1000,
            last_ts_ns: u64::from(bucket + 1) * 1000,
            node_id: 1,
            counters: Counters {
                enters: calls / u64::from(windows.max(1)),
                ends_ok: calls / u64::from(windows.max(1)),
                total_ns: 1000,
                self_ns: 800,
                ..Counters::default()
            },
        })
        .collect();
    FoldedCct {
        nodes,
        windows,
        meta: bex_query::Completeness {
            complete: true,
            ..bex_query::Completeness::default()
        },
        ..FoldedCct::default()
    }
}
