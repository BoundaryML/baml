#![allow(
    clippy::cast_precision_loss,
    clippy::print_stdout,
    clippy::too_many_lines
)]

use std::{
    fs::{self, File},
    hint::black_box,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use bex_events::{
    ids::FunctionId,
    prof::{
        cct::{CctEvent, EngineCct},
        exact_index::{ExactEventPoint, IndexBudget, build_exact_index},
        record::{FunctionEndStatus, ThreadEndStatus},
        storage::{
            BcctHeader, BcctWriter, BlockRows, CctDeltaRow, ClockDescriptor, WatermarkRow,
            scan_bcct_bytes,
        },
    },
};

const WINDOWS: u64 = 100_000;

fn main() {
    let header = BcctHeader {
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
    };
    let rows = BlockRows::CctDelta(vec![
        CctDeltaRow {
            node_id: 1,
            enters: 1,
            ends_ok: 1,
            total_ns: 100,
            self_ns: 80,
            await_ns: 20,
            ..CctDeltaRow::default()
        },
        CctDeltaRow {
            node_id: 2,
            enters: 1,
            ends_ok: 1,
            total_ns: 80,
            self_ns: 80,
            ..CctDeltaRow::default()
        },
        CctDeltaRow {
            node_id: 3,
            enters: 1,
            ends_ok: 1,
            total_ns: 20,
            self_ns: 20,
            ..CctDeltaRow::default()
        },
    ]);

    let start = Instant::now();
    let mut encoded_bytes = 0_u64;
    for window in 0..WINDOWS {
        let mut writer = BcctWriter::create(Vec::with_capacity(512), &header).unwrap();
        writer
            .append(
                black_box(&rows),
                window * 250_000_000,
                (window + 1) * 250_000_000,
            )
            .unwrap();
        let bytes = writer.into_inner();
        encoded_bytes += bytes.len() as u64;
        black_box(bytes);
    }
    let encode_elapsed = start.elapsed();

    let mut writer = BcctWriter::create(Vec::new(), &header).unwrap();
    for window in 0..10_000 {
        writer
            .append(&rows, window * 250_000_000, (window + 1) * 250_000_000)
            .unwrap();
    }
    let bytes = writer.into_inner();
    let start = Instant::now();
    let mut scanned_blocks = 0_usize;
    for _ in 0..100 {
        scanned_blocks += black_box(scan_bcct_bytes(black_box(&bytes)).unwrap().blocks.len());
    }
    let scan_elapsed = start.elapsed();

    println!(
        "{{\"schema_version\":1,\"bench_id\":\"bcct_encode_3_rows\",\
         \"evidence\":\"measured\",\
         \"iterations\":{WINDOWS},\"elapsed_ns\":{},\"encoded_bytes\":{encoded_bytes},\
         \"bytes_per_window\":{:.3},\"bytes_per_second_at_4hz\":{:.3},\
         \"ns_per_block\":{:.3}}}",
        encode_elapsed.as_nanos(),
        encoded_bytes as f64 / WINDOWS as f64,
        encoded_bytes as f64 / WINDOWS as f64 * 4.0,
        encode_elapsed.as_secs_f64() * 1e9 / WINDOWS as f64
    );
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"bcct_recovery_scan_10k\",\
         \"evidence\":\"measured\",\
         \"iterations\":100,\"elapsed_ns\":{},\"scanned_blocks\":{scanned_blocks},\
         \"ns_per_block\":{:.3}}}",
        scan_elapsed.as_nanos(),
        scan_elapsed.as_secs_f64() * 1e9 / scanned_blocks as f64
    );
    bench_exact_index();
    bench_partition_lifecycle();
    bench_async_durability(&header, &rows);
}

fn bench_exact_index() {
    let points = (0..100_000_u64)
        .map(|event| ExactEventPoint {
            lane: event % 86,
            timestamp_ns: event * 100,
            byte_offset: event * 48,
            byte_end: event * 48 + 47,
        })
        .collect::<Vec<_>>();
    let segment_bytes = points.last().map_or(0, |point| point.byte_end + 1) as usize;
    let started = Instant::now();
    let index = build_exact_index(&points, IndexBudget::for_segment_bytes(segment_bytes)).unwrap();
    let elapsed = started.elapsed();
    let index_ratio = index.encoded.len() as f64 / segment_bytes as f64;
    assert!(index.encoded.len() * 4 <= segment_bytes);
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"c11_exact_index_100k\",\
         \"evidence\":\"measured\",\"events\":{},\"segment_bytes\":{segment_bytes},\
         \"index_bytes\":{},\"index_ratio\":{index_ratio:.6},\
         \"levels_shed\":{},\"elapsed_ns\":{}}}",
        points.len(),
        index.encoded.len(),
        index.levels_shed,
        elapsed.as_nanos(),
    );
}

fn bench_partition_lifecycle() {
    const BOUNDARIES: u64 = 10_000;
    let rss_before = resident_bytes().unwrap_or(0);
    let started = Instant::now();
    let mut cct = EngineCct::default();
    for boundary in 0..BOUNDARIES {
        let timestamp = boundary * 10;
        cct.ingest(CctEvent::StartThread {
            flags: 0,
            thread_id: 1,
            parent_thread_id: 0,
            parent_call_id: 0,
            timestamp_ns: timestamp,
            name: None,
        });
        let partition = cct.partition_for_thread(1).unwrap();
        cct.ingest(CctEvent::CallFunction {
            flags: 0,
            thread_id: 1,
            call_id: 1,
            parent_call_id: 0,
            function_id: FunctionId(16),
            timestamp_ns: timestamp + 1,
        });
        cct.ingest(CctEvent::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: 1,
            call_id: 1,
            timestamp_ns: timestamp + 2,
        });
        cct.ingest(CctEvent::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: 1,
            timestamp_ns: timestamp + 3,
        });
        cct.finish_sweep();
        assert!(cct.release_partition(partition));
    }
    assert!(cct.can_rotate_epoch());
    let elapsed = started.elapsed();
    let rss_after = resident_bytes().unwrap_or(rss_before);
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"c11_partition_lifecycle_10k\",\
         \"evidence\":\"measured\",\"boundaries\":{BOUNDARIES},\
         \"elapsed_ns\":{},\"rss_before_bytes\":{rss_before},\
         \"rss_after_bytes\":{rss_after},\"rss_delta_bytes\":{}}}",
        elapsed.as_nanos(),
        rss_after.saturating_sub(rss_before),
    );
}

fn bench_async_durability(header: &BcctHeader, rows: &BlockRows) {
    const SAMPLES: u64 = 128;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "baml-bcct-durability-{}-{nonce}.bamlseg",
        std::process::id()
    ));
    let file = File::create(&path).unwrap();
    let mut writer = BcctWriter::create(file, header).unwrap();
    writer.append(rows, 0, 1).unwrap();
    let worker = writer.async_sync_worker().unwrap();
    let mut enqueue_ns = Vec::with_capacity(SAMPLES as usize);
    for ticket in 1..=SAMPLES {
        let started = Instant::now();
        writer
            .append_watermark_and_request_sync(
                &worker,
                WatermarkRow {
                    wall_epoch_ns: ticket,
                    drained_through_ts_ns: ticket,
                    events_drained: ticket,
                    durable_kind: 1,
                    reason: 0,
                },
            )
            .unwrap();
        enqueue_ns.push(started.elapsed().as_nanos());
    }
    for _ in 0..SAMPLES {
        worker.wait_complete().unwrap().result.unwrap();
    }
    worker.finish().unwrap();
    enqueue_ns.sort_unstable();
    let p99 = enqueue_ns[enqueue_ns.len() * 99 / 100];
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"c12_async_fsync_stall\",\
         \"evidence\":\"measured\",\"durability\":\"process_crash\",\
         \"samples\":{SAMPLES},\"producer_stall_p99_ns\":{p99},\
         \"fsyncs_completed\":{SAMPLES}}}"
    );
    fs::remove_file(path).ok();
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
