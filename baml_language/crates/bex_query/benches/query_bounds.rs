#![allow(clippy::print_stdout)]

use std::{
    collections::BTreeMap,
    fs,
    hint::black_box,
    path::PathBuf,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use bex_events::prof::storage::{
    BcctHeader, BcctWriter, BlockRows, CctDeltaRow, ClockDescriptor, NodeBirthRow,
};
use bex_query::{
    BqfBuilder, Completeness, Counters, FileId, FileSource, FoldedCct, FoldedNode, FrameKind,
    LiveFrameGate, LiveFrameOffer, QueryEngine, QueryPoll, Viewport, WindowDelta, timeline,
};

fn main() {
    let viewport = Viewport {
        start_ns: 0,
        end_ns: 4_096_000,
        pixel_width: 1024,
        lanes: 1,
        max_bytes: 200 * 1024,
    };
    for calls in [1_000_000_u64, 36_000_000] {
        let cct = fixture(calls);
        let started = Instant::now();
        let mut frame_bytes = 0;
        for request_id in 0..100 {
            let response = timeline(black_box(&cct), viewport).unwrap();
            let frame = response.to_bqf(request_id, viewport.max_bytes).unwrap();
            frame_bytes = frame.as_bytes().len();
            black_box(frame);
        }
        let elapsed = started.elapsed();
        let ns_per_iteration = elapsed.as_nanos() / 100;
        println!(
            "{{\"schema_version\":1,\"bench_id\":\"query_timeline_{calls}\",\
             \"evidence\":\"measured\",\
             \"iterations\":100,\"elapsed_ns\":{},\
             \"ns_per_iteration\":{ns_per_iteration},\"frame_bytes\":{frame_bytes},\
             \"source_calls\":{calls}}}",
            elapsed.as_nanos()
        );
    }
    c6_open_fold_live_frame(4_096);
    c6_open_fold_live_frame(100_000);
    c13_live_wire_bound();
}

fn c6_open_fold_live_frame(events: u32) {
    let bytes = bcct_fixture(events);
    let wire_source_bytes = bytes.len();
    let path = temp_path(&format!("c6-{events}.bamlcct"));
    fs::write(&path, &bytes).expect("write native query fixture");
    let source = FileSource::new();
    let file = FileId(1);
    source.open(file, &path).expect("open native query fixture");
    let engine = QueryEngine::new(source);
    let rss_before = resident_bytes().unwrap_or(0);
    let started = Instant::now();
    let cct = match engine.open_run(&[file], Some(1)).unwrap() {
        QueryPoll::Ready(cct) => cct,
        QueryPoll::NeedData { .. } => panic!("resident source requested data"),
    };
    let response = timeline(
        &cct,
        Viewport {
            start_ns: 0,
            end_ns: u64::from(events).max(1),
            pixel_width: 1024,
            lanes: 1,
            max_bytes: 200 * 1024,
        },
    )
    .unwrap();
    let frame = response.to_bqf(1, 200 * 1024).unwrap();
    let elapsed = started.elapsed();
    let rss_after = resident_bytes().unwrap_or(rss_before);
    let rss_delta_bytes = rss_after.saturating_sub(rss_before);
    assert!(frame.as_bytes().len() <= 200 * 1024);
    assert!(wire_source_bytes <= 10 * 1024 * 1024 || events > 4_096);
    assert!(engine.cache_retained_bytes() <= engine.cache_max_bytes());
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"c6_open_fold_live_{events}\",\
         \"evidence\":\"measured\",\"source\":\"native_file\",\
         \"events\":{events},\"elapsed_ns\":{},\"source_bytes\":{wire_source_bytes},\
         \"frame_bytes\":{},\"cache_bytes\":{},\"cache_max_bytes\":{},\
         \"rss_delta_bytes\":{rss_delta_bytes}}}",
        elapsed.as_nanos(),
        frame.as_bytes().len(),
        engine.cache_retained_bytes(),
        engine.cache_max_bytes(),
    );
    fs::remove_file(path).ok();
}

fn c13_live_wire_bound() {
    const MAX_BYTES: usize = 50 * 1024;
    const RATE_HZ: u8 = 30;
    const SECONDS: u64 = 10;
    let mut gate = LiveFrameGate::new(MAX_BYTES, RATE_HZ).unwrap();
    let interval_ns = 1_000_000_000_u64.div_ceil(u64::from(RATE_HZ));
    for frame_index in 0..u64::from(RATE_HZ) * SECONDS {
        let frame = BqfBuilder::new(
            FrameKind::Completeness,
            frame_index,
            frame_index.saturating_mul(1_000_000),
            0,
        )
        .finish(MAX_BYTES)
        .unwrap();
        assert!(matches!(
            gate.offer(frame_index * interval_ns, frame).unwrap(),
            LiveFrameOffer::Send(_)
        ));
        gate.acknowledge();
    }
    assert!(gate.bytes_sent() <= MAX_BYTES as u64 * u64::from(RATE_HZ) * SECONDS);
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"c13_live_wire_hotloop\",\
         \"evidence\":\"measured\",\
         \"seconds\":{SECONDS},\"max_bytes\":{MAX_BYTES},\"rate_hz\":{RATE_HZ},\
         \"frames\":{},\"bytes\":{}}}",
        gate.frames_sent(),
        gate.bytes_sent()
    );
}

fn temp_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "baml-query-bench-{}-{nonce}-{label}",
        std::process::id()
    ))
}

fn resident_bytes() -> Option<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kib| kib.saturating_mul(1024))
}

fn bcct_fixture(events: u32) -> Vec<u8> {
    let mut writer = BcctWriter::create(
        Vec::new(),
        &BcctHeader {
            process_euid: [1; 16],
            engine_id: 1,
            session_seg_seq: 1,
            started_epoch_ns: 1,
            clock: ClockDescriptor {
                kind: 1,
                quality: 1,
                tick_ns_numer: 1,
                tick_ns_denom: 1,
            },
            revision_id: [2; 32],
        },
    )
    .unwrap();
    writer
        .append(
            &BlockRows::NodeBirth(vec![NodeBirthRow {
                node_id: 1,
                parent_node_id: 0,
                function_id: 10,
                logical_thread_id: 1,
                partition_id: 1,
            }]),
            0,
            0,
        )
        .unwrap();
    let rows = (0..events)
        .map(|_| CctDeltaRow {
            node_id: 1,
            enters: 1,
            ends_ok: 1,
            total_ns: 1,
            self_ns: 1,
            ..CctDeltaRow::default()
        })
        .collect();
    writer
        .append(&BlockRows::CctDelta(rows), 0, u64::from(events))
        .unwrap();
    writer.seal().unwrap();
    writer.into_inner()
}

fn fixture(calls: u64) -> FoldedCct {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        1,
        FoldedNode {
            node_id: 1,
            function_id: 10,
            logical_thread_id: 1,
            counters: Counters {
                enters: calls,
                ends_ok: calls,
                total_ns: 4_096_000,
                self_ns: 3_000_000,
                ..Counters::default()
            },
            ..FoldedNode::default()
        },
    );
    let calls_per_window = calls / 4096;
    let windows = (0..4096_u64)
        .map(|window| WindowDelta {
            first_ts_ns: window * 1000,
            last_ts_ns: (window + 1) * 1000,
            node_id: 1,
            counters: Counters {
                enters: calls_per_window,
                ends_ok: calls_per_window,
                total_ns: 1000,
                self_ns: 800,
                ..Counters::default()
            },
        })
        .collect();
    FoldedCct {
        nodes,
        windows,
        meta: Completeness {
            complete: true,
            ..Completeness::default()
        },
        ..FoldedCct::default()
    }
}
