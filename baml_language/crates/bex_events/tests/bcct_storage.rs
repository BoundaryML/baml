use std::{
    fs,
    io::Cursor,
    sync::atomic::{AtomicU32, Ordering},
};

use bex_events::prof::storage::{
    AsyncFileSync, BCCT_HEADER_LEN, BCCT_TRAILER_LEN, BLOCK_HEADER_LEN, BLOCK_TRAILER_LEN,
    BcctHeader, BcctWriter, BlockKind, BlockRows, BoundaryMetaKind, BoundarySnapshot, CctDeltaRow,
    CctHistogramRow, CheckpointCadence, ClockDescriptor, InstanceRow, LlmDeltaRow, MarkerKind,
    MarkerRow, MetaWriter, ModelBirthRow, NodeBirthRow, PartitionBindRow, RawBlock, SegmentState,
    SessionLayout, SpawnEdgeRow, WatermarkRow, append_meta_d2, crc32c, probe_sealed_index,
    scan_bcct_bytes, scan_bcct_reader, scan_meta_bytes, scan_meta_reader,
};

fn header() -> BcctHeader {
    BcctHeader {
        process_euid: [0x11; 16],
        engine_id: 0x2233_4455_6677_8899,
        session_seg_seq: 7,
        started_epoch_ns: 123_456_789,
        clock: ClockDescriptor {
            kind: 2,
            quality: 1,
            tick_ns_numer: 125,
            tick_ns_denom: 3,
        },
        revision_id: [0xAB; 32],
    }
}

fn delta(node_id: u32) -> CctDeltaRow {
    CctDeltaRow {
        node_id,
        enters: 2,
        ends_ok: 1,
        ends_err: 1,
        ends_cancel: 0,
        ends_exit: 0,
        total_ns: 900,
        self_ns: 700,
        await_ns: 200,
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "baml-bcct-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn header_is_exactly_112_bytes_and_crc_protects_every_field() {
    assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    let encoded = header().encode();
    assert_eq!(encoded.len(), BCCT_HEADER_LEN);
    assert_eq!(&encoded[..4], b"BCCT");
    assert_eq!(u16::from_le_bytes(encoded[6..8].try_into().unwrap()), 112);
    assert_eq!(u32::from_le_bytes(encoded[36..40].try_into().unwrap()), 64);
    assert_eq!(&encoded[60..92], &[0xAB; 32]);
    assert_eq!(
        u32::from_le_bytes(encoded[108..112].try_into().unwrap()),
        crc32c(&encoded[..108])
    );
    assert_eq!(BcctHeader::decode(&encoded).unwrap(), header());

    let mut corrupt = encoded;
    corrupt[24] ^= 1;
    assert!(BcctHeader::decode(&corrupt).is_err());
}

#[test]
fn all_v1_block_schemas_round_trip_and_columns_are_aligned() {
    let variants = vec![
        BlockRows::CctDelta(vec![delta(1), delta(2)]),
        BlockRows::NodeBirth(vec![NodeBirthRow {
            node_id: 1,
            parent_node_id: 0,
            function_id: 17,
            logical_thread_id: 8,
            partition_id: 4,
        }]),
        BlockRows::SpawnEdge(vec![SpawnEdgeRow {
            edge_id: 1,
            parent_node: 2,
            entry_fn: 18,
            child_root_node: 3,
            spawn_delta: 4,
            completed_delta: 3,
            errored_delta: 1,
            cancelled_delta: 0,
            running_ns_delta: 55,
            awaiting_ns_delta: 21,
        }]),
        BlockRows::Watermark(vec![WatermarkRow {
            wall_epoch_ns: 100,
            drained_through_ts_ns: 90,
            events_drained: 500,
            durable_kind: 1,
            reason: 2,
        }]),
        BlockRows::PartitionBind(vec![PartitionBindRow {
            partition_id: 4,
            boundary_local_id: 2,
            boundary_id: [9; 16],
            created_ms: 88,
        }]),
        BlockRows::Reserved7(vec![1, 2, 3, 0, 0, 0, 0, 0]),
        BlockRows::NodeTotal(vec![delta(1)]),
        BlockRows::CctHistogram(vec![CctHistogramRow {
            node_id: 1,
            duration_buckets: std::array::from_fn(|index| u32::try_from(index).unwrap()),
        }]),
        BlockRows::LlmDelta(vec![LlmDeltaRow {
            node_id: 1,
            llm_calls_delta: 2,
            tokens_in_delta: 300,
            tokens_out_delta: 200,
            provider_errs_delta: 1,
            parse_errs_delta: 0,
            model_id: 7,
        }]),
        BlockRows::ModelBirth(vec![ModelBirthRow {
            model_id: 7,
            name: "claude-sonnet".to_owned(),
        }]),
        BlockRows::Marker(vec![MarkerRow {
            kind: MarkerKind::BudgetExhausted,
            timestamp_ns: 99,
            node_id: Some(3),
            count: 12,
            message: "staging bytes".to_owned(),
        }]),
        BlockRows::Instance(vec![InstanceRow {
            thread_id: 4,
            edge_id: 3,
            status: 2,
            start_ns: 10,
            end_ns: 20,
            dump_seq: 1,
            name: "worker".to_owned(),
        }]),
    ];

    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    for rows in &variants {
        writer.append(rows, 10, 20).unwrap();
    }
    let bytes = writer.into_inner();
    let scan = scan_bcct_reader(Cursor::new(&bytes)).unwrap();
    assert_eq!(scan.state, SegmentState::Active);
    assert_eq!(scan.blocks.len(), variants.len());
    for (block, expected) in scan.blocks.iter().zip(&variants) {
        assert_eq!(block.header.payload_len % 8, 0);
        assert_eq!(
            block.encoded_len,
            u64::try_from(BLOCK_HEADER_LEN).unwrap()
                + u64::from(block.header.payload_len)
                + u64::try_from(BLOCK_TRAILER_LEN).unwrap()
        );
        assert_eq!(&block.decode_rows().unwrap(), expected);
    }
}

#[test]
fn recovery_skips_unknown_committed_kinds_and_keeps_scanning() {
    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    writer
        .append_raw(RawBlock {
            kind: 200,
            flags: 0,
            row_count: 3,
            first_ts_ns: 10,
            last_ts_ns: 11,
            payload: vec![1, 2, 3],
            node_bounds: None,
        })
        .unwrap();
    writer
        .append(&BlockRows::CctDelta(vec![delta(9)]), 12, 13)
        .unwrap();
    let scan = scan_bcct_bytes(&writer.into_inner()).unwrap();
    assert_eq!(scan.blocks.len(), 2);
    assert_eq!(scan.blocks[0].known_kind(), None);
    assert!(matches!(
        scan.blocks[0].decode_rows().unwrap(),
        BlockRows::Opaque {
            kind: 200,
            row_count: 3,
            ..
        }
    ));
    assert_eq!(scan.blocks[1].known_kind(), Some(BlockKind::CctDelta));
}

#[test]
fn torn_or_corrupt_tail_stops_at_last_committed_block_without_mutating() {
    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    let first = writer
        .append(&BlockRows::CctDelta(vec![delta(1)]), 1, 2)
        .unwrap();
    let second = writer
        .append(&BlockRows::CctDelta(vec![delta(2)]), 3, 4)
        .unwrap();
    let bytes = writer.into_inner();
    let first_end = usize::try_from(first.offset + first.encoded_len).unwrap();

    for cut in usize::try_from(second.offset).unwrap() + 1..bytes.len() {
        let truncated = &bytes[..cut];
        let scan = scan_bcct_bytes(truncated).unwrap();
        assert_eq!(scan.state, SegmentState::Torn);
        assert_eq!(scan.blocks.len(), 1);
        assert_eq!(scan.committed_len, first_end as u64);
        assert_eq!(truncated.len(), cut);
    }

    let mut corrupt_payload = bytes.clone();
    corrupt_payload[usize::try_from(second.offset).unwrap() + BLOCK_HEADER_LEN] ^= 0x80;
    let scan = scan_bcct_bytes(&corrupt_payload).unwrap();
    assert_eq!(scan.state, SegmentState::Torn);
    assert_eq!(scan.blocks.len(), 1);
    assert_eq!(scan.committed_len, first_end as u64);

    let mut corrupt_marker = bytes.clone();
    *corrupt_marker.last_mut().unwrap() ^= 1;
    assert_eq!(scan_bcct_bytes(&corrupt_marker).unwrap().blocks.len(), 1);

    let mut corrupt_sequence = bytes;
    let sequence_byte = corrupt_sequence.len() - BLOCK_TRAILER_LEN + 4;
    corrupt_sequence[sequence_byte] ^= 1;
    assert_eq!(scan_bcct_bytes(&corrupt_sequence).unwrap().blocks.len(), 1);
}

#[test]
fn seal_appends_index_and_fixed_48_byte_trailer() {
    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    writer
        .append(&BlockRows::CctDelta(vec![delta(3)]), 10, 20)
        .unwrap();
    writer
        .append(
            &BlockRows::CctHistogram(vec![CctHistogramRow {
                node_id: 3,
                duration_buckets: [1; 16],
            }]),
            10,
            20,
        )
        .unwrap();
    let trailer = writer.seal().unwrap();
    let bytes = writer.into_inner();
    assert_eq!(&bytes[bytes.len() - 4..], b"TSEG");
    assert_eq!(
        trailer.index_offset + trailer.index_len + BCCT_TRAILER_LEN as u64,
        bytes.len() as u64
    );
    let scan = scan_bcct_bytes(&bytes).unwrap();
    assert_eq!(scan.state, SegmentState::Sealed(trailer));
    assert_eq!(
        scan.blocks.last().unwrap().known_kind(),
        Some(BlockKind::FooterIndex)
    );
    assert!(matches!(
        scan.blocks.last().unwrap().decode_rows().unwrap(),
        BlockRows::FooterIndex(rows) if rows.len() == 2
    ));
    let sealed_index = probe_sealed_index(&bytes).unwrap().unwrap();
    assert_eq!(sealed_index.trailer, trailer);
    assert_eq!(sealed_index.rows.len(), 2);

    let mut corrupt = bytes;
    let trailer_crc_byte = corrupt.len() - 8;
    corrupt[trailer_crc_byte] ^= 1;
    assert_eq!(scan_bcct_bytes(&corrupt).unwrap().state, SegmentState::Torn);
}

#[test]
fn checkpoint_cadence_is_bytes_not_block_count() {
    let mut writer = BcctWriter::create(Vec::new(), &header()).unwrap();
    let mut cadence = CheckpointCadence::default();
    assert!(
        writer
            .append_checkpoint_if_due(&mut cadence, &[delta(1)], 1, 2)
            .unwrap()
            .is_none()
    );
    writer
        .append_cct_delta(&mut cadence, vec![delta(1)], 1, 2)
        .unwrap();
    assert!(cadence.delta_bytes_since_checkpoint() > 0);
    assert!(
        writer
            .append_checkpoint_if_due(&mut cadence, &[delta(1)], 1, 2)
            .unwrap()
            .is_some()
    );
    assert_eq!(cadence.delta_bytes_since_checkpoint(), 0);
}

#[test]
fn bmet_scan_keeps_whole_records_and_reports_torn_tail() {
    let mut writer = MetaWriter::new(Vec::new());
    writer.append(1, b"session begin").unwrap();
    let first_len = writer.bytes_written();
    writer.append(99, b"future record").unwrap();
    let bytes = writer.into_inner();

    let whole = scan_meta_reader(Cursor::new(&bytes)).unwrap();
    assert!(!whole.torn_tail);
    assert_eq!(whole.records.len(), 2);
    assert_eq!(whole.records[1].kind, 99);

    let torn = scan_meta_bytes(&bytes[..bytes.len() - 2]);
    assert!(torn.torn_tail);
    assert_eq!(torn.records.len(), 1);
    assert_eq!(torn.committed_len, first_len);

    let mut corrupt = bytes;
    corrupt[usize::try_from(first_len).unwrap() + 9] ^= 1;
    let scan = scan_meta_bytes(&corrupt);
    assert!(scan.torn_tail);
    assert_eq!(scan.records.len(), 1);
    assert_eq!(scan.committed_len, first_len);
}

#[test]
fn session_layout_snapshot_commit_and_async_sync_are_usable_primitives() {
    let root = temp_dir("layout");
    let layout = SessionLayout::new(&root, 123, [0xCD; 16], 9);
    layout.create_dirs().unwrap();
    assert!(layout.cct_dir().is_dir());
    assert!(layout.flight_dir().is_dir());
    assert!(
        layout
            .session_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-e9")
    );

    let mut segment_header = header();
    segment_header.session_seg_seq = 1;
    let mut segment = layout.create_segment(&segment_header).unwrap();
    let worker: AsyncFileSync = segment.async_sync_worker().unwrap();
    let watermark = segment
        .append_watermark_and_request_sync(&worker, WatermarkRow::default())
        .unwrap();
    let completion = worker.wait_complete().unwrap();
    assert_eq!(completion.ticket, u64::from(watermark.block_seq));
    completion.result.unwrap();
    worker.finish().unwrap();

    let boundary_dir = root.join(".baml/history/example-boundary");
    append_meta_d2(
        &boundary_dir.join("boundary.bamlmeta"),
        BoundaryMetaKind::Begin as u8,
        b"typed payload supplied by host",
    )
    .unwrap();
    let mut snapshot = BoundarySnapshot::create(&boundary_dir, &segment_header).unwrap();
    snapshot
        .writer_mut()
        .append(&BlockRows::NodeTotal(vec![delta(1)]), 1, 2)
        .unwrap();
    let final_path = snapshot.final_path().to_path_buf();
    snapshot.seal_and_commit().unwrap();
    assert!(final_path.is_file());
    assert!(matches!(
        scan_bcct_bytes(&fs::read(final_path).unwrap())
            .unwrap()
            .state,
        SegmentState::Sealed(_)
    ));

    fs::remove_dir_all(root).unwrap();
}
