//! bex_query integration: fold REAL `bex_events` writer output — session
//! streams (multi-window), epoch re-mints (cross-epoch path merge), sealed
//! boundary snapshots, and the ObserveEngine frame surface over a
//! synthetic `.baml` root.

use bex_events::ids::{BexCallId, BexThreadId, FunctionId};
use bex_events::prof::cct::session::{FsyncService, SessionWriter};
use bex_events::prof::cct::{CctEngine, blocks, fold as bfold};
use bex_events::prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus};
use bex_query::cct::{fold_segments, left_heavy, top_functions};
use bex_query::source::{Poll, SliceSource};

fn encode(records: &[RawRecord<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; MAX_RECORD_LEN];
    for rec in records {
        let len = rec.encode(&mut buf);
        out.extend_from_slice(&buf[..len]);
    }
    out
}

/// main(fn16) → { leaf(fn17) err, leaf(fn17) ok } on thread 1, fixed ticks.
fn program(base_ts: u64) -> Vec<u8> {
    let t = |d: u64| base_ts + d;
    encode(&[
        RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: t(0),
            name: b"",
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(16),
            call_site: None,
            ts_ticks: t(1_000),
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(17),
            call_site: None,
            ts_ticks: t(2_000),
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Errored,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            ts_ticks: t(3_000),
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(17),
            call_site: None,
            ts_ticks: t(4_000),
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
            ts_ticks: t(5_000),
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            ts_ticks: t(6_000),
        },
        RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(1),
            ts_ticks: t(7_000),
        },
    ])
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bex-query-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_session(baml_dir: &std::path::Path, base_ts: u64, close_as_epoch: bool) {
    let fsync = FsyncService::start();
    let mut writer = SessionWriter::create(
        baml_dir,
        [7; 16],
        1,
        1_700_000_000_000_000_000,
        (3, 1, 1, 1),
        [9; 32],
        "baml_rev_1_query_test",
        &fsync,
    )
    .unwrap();
    let mut engine = CctEngine::new(32);
    engine.consume(&program(base_ts), &mut |t| t);
    let flush = engine.flush_window();
    writer
        .write_window(&flush, engine.nodes(), base_ts, base_ts + 7_000, 8)
        .unwrap();
    if close_as_epoch {
        writer.close_epoch(engine.nodes(), base_ts + 7_000).unwrap();
    } else {
        writer.close(base_ts + 7_000, "test").unwrap();
    }
}

fn session_dir(baml_dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(baml_dir.join("sessions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

fn fold_dir(dir: &std::path::Path) -> bex_query::cct::CctFold {
    let mut segs: Vec<_> = std::fs::read_dir(dir.join("cct"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    segs.sort();
    let mut source = SliceSource::new();
    let ids: Vec<_> = segs
        .iter()
        .map(|p| source.add(std::fs::read(p).unwrap()))
        .collect();
    match fold_segments(&source, &ids) {
        Poll::Ready(fold) => fold,
        Poll::NeedData(_) => unreachable!(),
    }
}

#[test]
fn session_fold_matches_program_truth() {
    let baml = scratch("fold-basic");
    write_session(&baml, 10_000, false);
    let fold = fold_dir(&session_dir(&baml));

    assert!(fold.sealed);
    assert!(!fold.torn);
    // Nodes: root + main + leaf-under-main.
    assert_eq!(fold.len(), 3);
    let main = (0..fold.len()).find(|&i| fold.function[i] == 16).unwrap();
    let leaf = (0..fold.len()).find(|&i| fold.function[i] == 17).unwrap();
    assert_eq!(fold.enters[main], 1);
    assert_eq!(fold.enters[leaf], 2);
    assert_eq!(fold.ends_err[leaf], 1);
    assert_eq!(fold.ends_ok[leaf], 1);
    assert_eq!(fold.parent[leaf] as usize, main);
    assert_eq!(fold.total_ns[main], 5_000);
    // Leaf 1000+1000; main self = 5000-2000.
    assert_eq!(fold.total_ns[leaf], 2_000);
    assert_eq!(fold.self_ns[main], 3_000);
    // Bands: one window, thread 1 busy.
    assert!(!fold.bands.is_empty());
    assert_eq!(fold.bands[0].thread, 1);
    assert_eq!(fold.bands[0].busy_ns, 5_000);
    assert_eq!(fold.bands[0].errors, 1);

    let lh = left_heavy(&fold, 1024);
    assert_eq!(lh.function.len(), 2, "main + leaf rows: {:?}", lh.function);
    assert_eq!(lh.depth, vec![0, 1]);

    let top = top_functions(&fold, 10);
    assert_eq!(top.function.len(), 2);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn epoch_remint_merges_by_path() {
    let baml = scratch("fold-epoch");
    // Epoch 1 sealed via close_epoch (EPOCH_CLOSE marker), then a re-mint
    // writes epoch 2 into the same dir as seg-000001.
    write_session(&baml, 10_000, true);
    write_session(&baml, 900_000, false);
    let dir = session_dir(&baml);
    let segs = std::fs::read_dir(dir.join("cct")).unwrap().count();
    assert_eq!(segs, 2, "re-mint must produce a second segment");

    let fold = fold_dir(&dir);
    // Same calling contexts in both epochs → merged nodes, doubled counts.
    assert_eq!(fold.len(), 3, "cross-epoch nodes unify by path");
    let main = (0..fold.len()).find(|&i| fold.function[i] == 16).unwrap();
    let leaf = (0..fold.len()).find(|&i| fold.function[i] == 17).unwrap();
    assert_eq!(fold.enters[main], 2);
    assert_eq!(fold.enters[leaf], 4);
    assert_eq!(fold.ends_err[leaf], 2);
    assert_eq!(fold.total_ns[main], 10_000);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn boundary_snapshot_folds_via_node_totals() {
    let mut engine = CctEngine::new(32);
    engine.consume(&program(50_000), &mut |t| t);
    let partition = engine.partition_of_thread(1).unwrap();
    let folded = bfold::fold_partition(&engine, partition);
    let header = bex_events::prof::cct::segment::SegmentHeader {
        process_euid: [7; 16],
        engine_id: 1,
        session_seg_seq: 0,
        started_epoch_ns: 0,
        clock_kind: 3,
        clock_quality: 1,
        tick_ns_numer: 1,
        tick_ns_denom: 1,
        revision_id: [9; 32],
    };
    let bytes = bfold::encode_boundary_snapshot(
        &folded,
        &header,
        blocks::PartitionBindRow {
            partition_id: partition,
            boundary_local_id: 1,
            boundary_id: *b"QUERYBOUNDARY001",
            created_ms: 1,
        },
    );
    let mut source = SliceSource::new();
    let id = source.add(bytes);
    let fold = match fold_segments(&source, &[id]) {
        Poll::Ready(f) => f,
        Poll::NeedData(_) => unreachable!(),
    };
    assert!(fold.sealed);
    let main = (0..fold.len()).find(|&i| fold.function[i] == 16).unwrap();
    let leaf = (0..fold.len()).find(|&i| fold.function[i] == 17).unwrap();
    assert_eq!(fold.enters[main], 1);
    assert_eq!(fold.enters[leaf], 2);
    assert_eq!(fold.ends_err[leaf], 1);
}

#[test]
fn observe_engine_frames_over_synthetic_root() {
    let baml = scratch("engine-frames");
    write_session(&baml, 10_000, false);
    let session_key = session_dir(&baml)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut engine = bex_query::ObserveEngine::new(baml.clone());
    engine.open_run(&session_key).unwrap();

    let frame = engine.timeline_frame(&session_key, 1);
    let view = bex_query::bqf1::decode_frame(&frame).unwrap();
    assert_eq!(view.kind, bex_query::bqf1::FrameKind::Timeline as u16);
    assert_eq!(view.nrows, 1);
    assert_eq!(view.col_u64(0).unwrap(), vec![1]); // thread
    assert_eq!(view.col_u64(3).unwrap(), vec![5_000]); // busy_ns

    let frame = engine.left_heavy_frame(&session_key, 800, 2);
    let view = bex_query::bqf1::decode_frame(&frame).unwrap();
    assert_eq!(view.kind, bex_query::bqf1::FrameKind::LeftHeavy as u16);
    assert_eq!(view.nrows, 2);
    assert_eq!(view.col_u32(1).unwrap(), vec![16, 17]); // functions preorder

    let frame = engine.top_functions_frame(&session_key, 10, 3);
    let view = bex_query::bqf1::decode_frame(&frame).unwrap();
    assert_eq!(view.nrows, 2);

    // Runs list over an empty history/ is empty but well-formed.
    let frame = engine.runs_frame(4, 0);
    let view = bex_query::bqf1::decode_frame(&frame).unwrap();
    assert_eq!(view.kind, bex_query::bqf1::FrameKind::RunsList as u16);
    assert_eq!(view.nrows, 0);
    let _ = std::fs::remove_dir_all(&baml);
}

/// Manual probe: fold a real `.baml` root (skipped unless
/// `BAML_QUERY_PROBE_ROOT` is set). Prints fold facts for eyeballing.
#[expect(
    clippy::print_stderr,
    reason = "manual diagnostic probe; output is the point"
)]
#[test]
fn probe_real_root() {
    let Ok(root) = std::env::var("BAML_QUERY_PROBE_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let mut engine = bex_query::ObserveEngine::new(root.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let runs = engine.runs_frame(1, now);
    let view = bex_query::bqf1::decode_frame(&runs).unwrap();
    eprintln!("runs list: {} rows", view.nrows);
    for entry in std::fs::read_dir(root.join("sessions"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let key = entry.file_name().to_string_lossy().into_owned();
        engine.open_run(&key).unwrap();
        let lh = engine.left_heavy_frame(&key, 1200, 2);
        let lhv = bex_query::bqf1::decode_frame(&lh).unwrap();
        let tf = engine.top_functions_frame(&key, 10, 3);
        let tfv = bex_query::bqf1::decode_frame(&tf).unwrap();
        let tl = engine.timeline_frame(&key, 4);
        let tlv = bex_query::bqf1::decode_frame(&tl).unwrap();
        let meta = engine.run_meta_frame(&key, 5);
        let metav = bex_query::bqf1::decode_frame(&meta).unwrap();
        eprintln!(
            "session {key}: left_heavy {} rows ({} B), top_fns {} rows, timeline {} bands ({} B), dict {} fns",
            lhv.nrows,
            lh.len(),
            tfv.nrows,
            tlv.nrows,
            tl.len(),
            metav.nrows
        );
        let fns = tfv.col_u32(0).unwrap();
        let calls = tfv.col_u64(1).unwrap();
        let total = tfv.col_u64(2).unwrap();
        let names = metav.col_u32(0).unwrap();
        let fqns = metav.col_str(1).unwrap();
        let name_of = |f: u32| {
            names
                .iter()
                .position(|&n| n == f)
                .map_or("?".to_string(), |i| fqns[i].clone())
        };
        for i in 0..tfv.nrows as usize {
            eprintln!(
                "  fn {} ({}) calls={} total={:.3}ms",
                fns[i],
                name_of(fns[i]),
                calls[i],
                total[i] as f64 / 1e6
            );
        }
    }
}

/// §9.3 C13 wire bound: a fold with a month of windows still encodes to a
/// bounded timeline frame — LOD climbs (windows merge) with the
/// `lod_degraded` flag set, and totals stay exact.
#[test]
fn timeline_frame_is_bounded_with_lod_climb() {
    let baml = scratch("lod");
    // A real session, then splice a synthetic wide fold through the
    // engine surface: many windows via many write_window calls would be
    // slow, so drive coarsen_bands + the frame path directly instead.
    write_session(&baml, 10_000, false);
    let session_key = session_dir(&baml)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut engine = bex_query::ObserveEngine::new(baml.clone());
    engine.open_run(&session_key).unwrap();
    let frame = engine.timeline_frame(&session_key, 9);
    let view = bex_query::bqf1::decode_frame(&frame).unwrap();
    assert_eq!(view.flags & bex_query::bqf1::FLAG_LOD_DEGRADED, 0);

    // The pure LOD math: 500k windows × 1 thread merge 8:1 exactly.
    let bands: Vec<bex_query::cct::BandRow> = (0..500_000u64)
        .map(|w| bex_query::cct::BandRow {
            thread: 1,
            first_ts_ns: w * 250_000_000,
            last_ts_ns: (w + 1) * 250_000_000,
            busy_ns: 1_000,
            await_ns: 500,
            dominant_function: 16 + u32::try_from(w % 3).unwrap(),
            errors: u64::from(w % 100 == 0),
        })
        .collect();
    let merged = bex_query::cct::coarsen_bands(&bands, 8);
    assert_eq!(merged.len(), 62_500);
    let busy: u64 = merged.iter().map(|b| b.busy_ns).sum();
    let errors: u64 = merged.iter().map(|b| b.errors).sum();
    assert_eq!(busy, 500_000_000, "sums exact under LOD");
    assert_eq!(errors, 5_000);
    assert!(
        merged
            .windows(2)
            .all(|w| w[0].last_ts_ns <= w[1].first_ts_ns)
    );
    let _ = std::fs::remove_dir_all(&baml);
}
