//! `obs-bench corpus` (design §10.2/C7): seeded synthetic sealed-segment
//! corpus generation and the scan gate over it.
//!
//! `synth` writes a `.baml/sessions/...` tree of sealed `.bamlseg` files
//! whose delta blocks cover a configurable node population (seeded LCG —
//! same seed, same bytes). `scan` folds every session via `bex_query`,
//! timing wall latency and reporting peak RSS (`VmHWM`) against the C7
//! byte-budget claim (caches are byte-budgeted, not entry-capped).

use std::path::Path;
use std::time::Instant;

use anyhow::Context as _;
use bex_events::prof::cct::blocks::{self, CctDeltaRow, NodeBirthRow};
use bex_events::prof::cct::segment::{self, BlockKind, SegmentHeader};

use crate::rows::{Basis, BenchRow};

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

/// Generate ~`total_bytes` of sealed segments under `root/sessions/`,
/// split into sessions of ~`session_bytes` with `nodes_per_session`
/// distinct contexts. Returns bytes written.
pub fn synth(
    root: &Path,
    total_bytes: u64,
    session_bytes: u64,
    nodes_per_session: u32,
    seed: u64,
) -> anyhow::Result<u64> {
    let mut rng = seed | 1;
    let mut written = 0u64;
    let mut session_idx = 0u32;
    while written < total_bytes {
        let dir = root.join("sessions").join(format!(
            "1700000{session_idx:03}-{:032x}-e{session_idx}",
            0xC0FFEE_u128 + u128::from(session_idx)
        ));
        std::fs::create_dir_all(dir.join("cct"))
            .with_context(|| format!("creating {}", dir.display()))?;
        // Minimal session meta so runs listing sees the session.
        let mut meta = bex_events::prof::cct::meta::encode_header().to_vec();
        meta.extend_from_slice(&bex_events::prof::cct::meta::encode_record(
            &bex_events::prof::cct::meta::MetaRecord::SessionBegin {
                process_euid: [0xC7; 16],
                engine_id: u64::from(session_idx),
                pid: 1,
                started_epoch_ns: 1_700_000_000_000_000_000,
                revision_id: "baml_rev_1_corpus".to_string(),
            },
        ));
        meta.extend_from_slice(&bex_events::prof::cct::meta::encode_record(
            &bex_events::prof::cct::meta::MetaRecord::SessionEnd {
                reason: "corpus".to_string(),
            },
        ));
        std::fs::write(dir.join("session.bamlmeta"), meta)?;

        let bytes = synth_segment(nodes_per_session, session_bytes, session_idx, &mut rng);
        written += bytes.len() as u64;
        std::fs::write(dir.join("cct").join("seg-000000.bamlseg"), bytes)?;
        session_idx += 1;
    }
    Ok(written)
}

/// One sealed segment: births for `node_count` contexts (binary-tree
/// parents), then delta windows until `target_bytes`.
fn synth_segment(node_count: u32, target_bytes: u64, engine: u32, rng: &mut u64) -> Vec<u8> {
    let header = SegmentHeader {
        process_euid: [0xC7; 16],
        engine_id: u64::from(engine),
        session_seg_seq: 0,
        started_epoch_ns: 1_700_000_000_000_000_000,
        clock_kind: 3,
        clock_quality: 1,
        tick_ns_numer: 1,
        tick_ns_denom: 1,
        revision_id: [0xC7; 32],
    };
    let mut bytes = header.encode().to_vec();
    let mut block_seq: u32 = 0;
    let mut total_rows: u64 = 0;
    let mut index: Vec<(u8, u64, u32)> = Vec::new();

    let mut push = |bytes: &mut Vec<u8>,
                    kind: BlockKind,
                    row_count: u32,
                    first_ts: u64,
                    last_ts: u64,
                    payload: &[u8],
                    index: &mut Vec<(u8, u64, u32)>| {
        let block = segment::encode_block(
            bytes.len(),
            kind,
            0,
            row_count,
            first_ts,
            last_ts,
            payload,
            block_seq,
        );
        let pad =
            block.len() - (segment::BLOCK_HEADER_LEN + payload.len() + segment::BLOCK_TRAILER_LEN);
        index.push((kind as u8, (bytes.len() + pad) as u64, row_count));
        bytes.extend_from_slice(&block);
        block_seq += 1;
        total_rows += u64::from(row_count);
    };

    // Births: node 1..=node_count, binary-tree parents, fns cycled 16..16+64.
    let births: Vec<NodeBirthRow> = (1..=node_count)
        .map(|id| NodeBirthRow {
            node_id: id,
            parent_node_id: id / 2,
            function_id: 16 + (id % 64),
            logical_thread_id: u64::from(1 + id % 8),
            partition_id: 0,
        })
        .collect();
    let payload = blocks::encode_node_birth(&births);
    push(
        &mut bytes,
        BlockKind::NodeBirth,
        node_count,
        0,
        0,
        &payload,
        &mut index,
    );

    // Delta windows (250 ms apart) until the byte target.
    let mut window: u64 = 0;
    while (bytes.len() as u64) < target_bytes {
        let rows_in_window = (node_count / 4).clamp(1, 4096);
        let deltas: Vec<CctDeltaRow> = (0..rows_in_window)
            .map(|_| {
                let node = 1 + u32::try_from(lcg(rng) % u64::from(node_count)).unwrap_or(0);
                let enters = 1 + u32::try_from(lcg(rng) % 50).unwrap_or(0);
                CctDeltaRow {
                    node_id: node,
                    enters,
                    ends_ok: enters,
                    ends_err: u32::from(lcg(rng) % 100 == 0),
                    ends_cancel: 0,
                    ends_exit: 0,
                    total_ns: u64::from(enters) * (lcg(rng) % 10_000),
                    self_ns: u64::from(enters) * (lcg(rng) % 5_000),
                    await_ns: 0,
                }
            })
            .collect();
        let payload = blocks::encode_cct_delta(&deltas);
        let first = window * 250_000_000;
        push(
            &mut bytes,
            BlockKind::CctDelta,
            rows_in_window,
            first,
            first + 250_000_000,
            &payload,
            &mut index,
        );
        window += 1;
    }

    // Footer index + seal.
    let mut footer_payload = Vec::with_capacity(index.len() * 13);
    for (kind, offset, rows) in &index {
        footer_payload.push(*kind);
        footer_payload.extend_from_slice(&offset.to_le_bytes());
        footer_payload.extend_from_slice(&rows.to_le_bytes());
    }
    let index_offset = bytes.len() as u64;
    let n = u32::try_from(index.len()).unwrap_or(u32::MAX);
    let block = segment::encode_block(
        bytes.len(),
        BlockKind::FooterIndex,
        0,
        n,
        0,
        0,
        &footer_payload,
        block_seq,
    );
    bytes.extend_from_slice(&block);
    let index_len = bytes.len() as u64 - index_offset;
    bytes.extend_from_slice(&segment::encode_seal_trailer(
        index_offset,
        index_len,
        total_rows,
    ));
    bytes
}

/// Peak RSS of this process in bytes (`VmHWM`), 0 if unreadable.
fn peak_rss_bytes() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: u64 = rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .unwrap_or(0);
            return kb * 1024;
        }
    }
    0
}

pub struct ScanReport {
    pub rows: Vec<BenchRow>,
    pub summary: String,
}

/// C7 scan gate: fold every session in the corpus, one at a time (the
/// byte-budget contract — RSS must track the cache budget, not corpus
/// size).
pub fn scan(root: &Path) -> anyhow::Result<ScanReport> {
    use std::fmt::Write as _;
    let mut engine = bex_query::ObserveEngine::new(root.to_path_buf());
    let sessions: Vec<String> = std::fs::read_dir(root.join("sessions"))
        .with_context(|| format!("no sessions under {}", root.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    anyhow::ensure!(!sessions.is_empty(), "empty corpus");

    let corpus_bytes: u64 = walk_bytes(&root.join("sessions"));
    let start = Instant::now();
    let mut folded = 0usize;
    for key in &sessions {
        engine
            .open_run(key)
            .map_err(|e| anyhow::anyhow!("open {key}: {e}"))?;
        let frame = engine.left_heavy_frame(key, 1024, 1);
        anyhow::ensure!(frame.len() > 48, "frame for {key}");
        folded += 1;
    }
    let wall_s = start.elapsed().as_secs_f64();
    let rss = peak_rss_bytes();

    let mut rows = Vec::new();
    let mut summary = String::new();
    let _ = writeln!(
        summary,
        "corpus scan: {folded} sessions, {corpus_bytes} B in {wall_s:.2} s ({:.1} MB/s), peak RSS {:.1} MiB",
        corpus_bytes as f64 / wall_s / 1e6,
        rss as f64 / (1 << 20) as f64,
    );
    rows.push(BenchRow::new(
        "c7.corpus.scan_wall_s",
        "corpus",
        "bexq",
        "scan_wall_s",
        wall_s,
        "s",
        Basis::Measured,
    ));
    rows.push(BenchRow::new(
        "c7.corpus.scan_mb_per_s",
        "corpus",
        "bexq",
        "scan_mb_per_s",
        corpus_bytes as f64 / wall_s / 1e6,
        "MB/s",
        Basis::Measured,
    ));
    rows.push(BenchRow::new(
        "c7.corpus.peak_rss_mib",
        "corpus",
        "bexq",
        "peak_rss_mib",
        rss as f64 / f64::from(1u32 << 20),
        "MiB",
        Basis::Measured,
    ));
    Ok(ScanReport { rows, summary })
}

fn walk_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                total += walk_bytes(&p);
            } else {
                total += e.metadata().map_or(0, |m| m.len());
            }
        }
    }
    total
}
