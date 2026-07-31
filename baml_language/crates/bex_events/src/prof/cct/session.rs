//! The per-session CCT stream writer (§6.1–§6.3, native-only): owns the
//! session directory, the active `.bamlseg` segment, checkpoint cadence,
//! watermarks, and the D1 group commit with **off-thread fsync** (§6.6 —
//! fsync never runs on the drain path; a helper thread syncs and the
//! durable watermark advances on completion).
//!
//! Layout: `<baml_dir>/sessions/<started_secs>-<euid_hex32>-e<engine_id>/`
//! with `cct/seg-NNNNNN.bamlseg` and `session.bamlmeta`.

use std::{
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use super::blocks;
use super::engine::WindowFlush;
use super::meta::{MetaRecord, MetaWriter};
use super::nodes::Nodes;
use super::segment::{self, BlockKind, SegmentHeader};

/// §6.2: rotate the active segment at this size.
pub const SEGMENT_ROTATE_BYTES: u64 = 4 * 1024 * 1024;
/// §6.2: rotate the active segment at this age.
pub const SEGMENT_ROTATE_AGE: Duration = Duration::from_secs(15 * 60);
/// §6.6: D1 group commit cadence.
pub const GROUP_COMMIT_INTERVAL: Duration = Duration::from_secs(1);
/// §6.6: D1 group commit byte threshold.
pub const GROUP_COMMIT_BYTES: u64 = 1024 * 1024;
/// §6.3: idle heartbeat watermark cadence.
pub const IDLE_WATERMARK_INTERVAL: Duration = Duration::from_secs(10);
/// §6.1: rotate the whole session (fresh node table) at this many CCT
/// bytes...
pub const EPOCH_ROTATE_BYTES: u64 = 256 * 1024 * 1024;
/// ... or this age.
pub const EPOCH_ROTATE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Off-thread fsync service (§6.6). One helper thread per process; segment
/// files are handed over by clone, completions advance a shared watermark.
pub struct FsyncService {
    tx: mpsc::Sender<FsyncRequest>,
}

struct FsyncRequest {
    file: std::fs::File,
    /// Everything drained through this event-ts is durable once this sync
    /// completes.
    through_ts_ns: u64,
    /// Cumulative events drained at request time.
    events_drained: u64,
    done: Arc<DurableMark>,
}

/// Shared durable watermark, advanced by fsync completions.
#[derive(Default)]
pub struct DurableMark {
    pub through_ts_ns: AtomicU64,
    pub events_drained: AtomicU64,
}

impl FsyncService {
    #[must_use]
    pub fn start() -> FsyncService {
        let (tx, rx) = mpsc::channel::<FsyncRequest>();
        // The helper must never panic the process; sync errors surface via
        // the mark NOT advancing (the watermark row then attests less).
        let _ = std::thread::Builder::new()
            .name("bex-obs-fsync".into())
            .spawn(move || {
                while let Ok(req) = rx.recv() {
                    if req.file.sync_data().is_ok() {
                        req.done
                            .through_ts_ns
                            .fetch_max(req.through_ts_ns, Ordering::Release);
                        req.done
                            .events_drained
                            .fetch_max(req.events_drained, Ordering::Release);
                    }
                }
            });
        FsyncService { tx }
    }

    fn request(
        &self,
        file: std::fs::File,
        through_ts_ns: u64,
        events_drained: u64,
        done: Arc<DurableMark>,
    ) {
        let _ = self.tx.send(FsyncRequest {
            file,
            through_ts_ns,
            events_drained,
            done,
        });
    }
}

struct ActiveSegment {
    file: std::fs::File,
    path: PathBuf,
    block_seq: u32,
    bytes: u64,
    opened: Instant,
    total_rows: u64,
    /// footer_index entries (kind, offset, row_count, first/last ts).
    index: Vec<(u8, u64, u32, u64, u64)>,
}

/// First unused `seg-NNNNNN.bamlseg` sequence in `cct_dir` (0 for a fresh
/// session; existing sealed segments — epoch re-mints — are skipped past).
fn next_free_seg_seq(cct_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(cct_dir) else {
        return 0;
    };
    let mut next = 0u32;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(seq) = name
            .strip_prefix("seg-")
            .and_then(|s| s.strip_suffix(".bamlseg"))
            .and_then(|s| s.parse::<u32>().ok())
        {
            next = next.max(seq + 1);
        }
    }
    next
}

/// One engine's session stream writer.
pub struct SessionWriter {
    session_dir: PathBuf,
    header_template: SegmentHeader,
    seg_seq: u32,
    segment: Option<ActiveSegment>,
    meta: MetaWriter,
    bytes_since_checkpoint: u64,
    bytes_since_commit: u64,
    last_commit: Instant,
    last_watermark: Instant,
    /// Durable watermark advanced by the fsync helper; the next flush
    /// after an advance writes the kind-4 row attesting it.
    durable: Arc<DurableMark>,
    last_attested_ts: u64,
    events_drained: u64,
    blocks_since_commit: u32,
    last_meta_heartbeat: Instant,
    /// Cumulative CCT bytes this session (across segments) — §6.1 epoch
    /// rotation input.
    session_cct_bytes: u64,
    session_started: Instant,
}

impl SessionWriter {
    /// Create the session dir + meta + first segment.
    /// `baml_dir` is the project `.baml/` directory.
    pub fn create(
        baml_dir: &Path,
        process_euid: [u8; 16],
        engine_id: u64,
        started_epoch_ns: u64,
        clock: (u8, u8, u64, u64),
        revision_id: [u8; 32],
        revision_id_string: &str,
        fsync: &FsyncService,
    ) -> io::Result<SessionWriter> {
        let _ = fsync;
        let started_secs = started_epoch_ns / 1_000_000_000;
        let euid_hex: String = process_euid.iter().map(|b| format!("{b:02x}")).collect();
        let session_dir = baml_dir
            .join("sessions")
            .join(format!("{started_secs}-{euid_hex}-e{engine_id}"));
        std::fs::create_dir_all(session_dir.join("cct"))?;
        // Epoch rotation re-mints the SAME deterministic dir (§6.1): resume
        // segment numbering after any existing sealed segments instead of
        // colliding on `create_new(seg-000000)` — a collision here would
        // silently drop every post-rotation window.
        let seg_seq = next_free_seg_seq(&session_dir.join("cct"));
        let mut meta = MetaWriter::create(&session_dir.join("session.bamlmeta"))?;
        meta.append(&MetaRecord::SessionBegin {
            process_euid,
            engine_id,
            pid: std::process::id(),
            started_epoch_ns,
            revision_id: revision_id_string.to_string(),
        })?;
        // Session begin is a D2 milestone (§6.6).
        meta.sync_data()?;
        let header_template = SegmentHeader {
            process_euid,
            engine_id,
            session_seg_seq: 0,
            started_epoch_ns: started_epoch_ns as u64,
            clock_kind: clock.0,
            clock_quality: clock.1,
            tick_ns_numer: clock.2,
            tick_ns_denom: clock.3,
            revision_id,
        };
        let mut writer = SessionWriter {
            session_dir,
            header_template,
            seg_seq,
            segment: None,
            meta,
            bytes_since_checkpoint: 0,
            bytes_since_commit: 0,
            last_commit: Instant::now(),
            last_watermark: Instant::now(),
            durable: Arc::new(DurableMark::default()),
            last_attested_ts: 0,
            events_drained: 0,
            blocks_since_commit: 0,
            last_meta_heartbeat: Instant::now(),
            session_cct_bytes: 0,
            session_started: Instant::now(),
        };
        writer.open_segment()?;
        Ok(writer)
    }

    #[must_use]
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    fn open_segment(&mut self) -> io::Result<()> {
        let mut header = self.header_template.clone();
        header.session_seg_seq = self.seg_seq;
        let path = self
            .session_dir
            .join("cct")
            .join(format!("seg-{:06}.bamlseg", self.seg_seq));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)?;
        file.write_all(&header.encode())?;
        // Header is fsynced at create (§6.2) — cheap, once per segment.
        file.sync_data()?;
        self.segment = Some(ActiveSegment {
            file,
            path,
            block_seq: 0,
            bytes: segment::HEADER_LEN as u64,
            opened: Instant::now(),
            total_rows: 0,
            index: Vec::new(),
        });
        Ok(())
    }

    fn append_block(
        &mut self,
        kind: BlockKind,
        row_count: u32,
        first_ts: u64,
        last_ts: u64,
        payload: &[u8],
    ) -> io::Result<()> {
        let seg = self.segment.as_mut().expect("segment open");
        let encoded = segment::encode_block(
            seg.bytes as usize,
            kind,
            0,
            row_count,
            first_ts,
            last_ts,
            payload,
            seg.block_seq,
        );
        seg.file.write_all(&encoded)?;
        let offset = seg.bytes
            + (encoded.len()
                - (segment::BLOCK_HEADER_LEN + payload.len() + segment::BLOCK_TRAILER_LEN))
                as u64;
        seg.index
            .push((kind as u8, offset, row_count, first_ts, last_ts));
        seg.block_seq += 1;
        seg.bytes += encoded.len() as u64;
        seg.total_rows += u64::from(row_count);
        self.bytes_since_checkpoint += encoded.len() as u64;
        self.bytes_since_commit += encoded.len() as u64;
        self.session_cct_bytes += encoded.len() as u64;
        self.blocks_since_commit += 1;
        Ok(())
    }

    /// Write one window's §6.3 blocks (births first), then run checkpoint /
    /// commit / rotation cadences.
    pub fn write_window(
        &mut self,
        flush: &WindowFlush,
        nodes: &Nodes,
        window_first_ts: u64,
        window_last_ts: u64,
        events_drained: u64,
    ) -> io::Result<()> {
        self.events_drained = events_drained;
        if !flush.birth_rows.is_empty() {
            let payload = blocks::encode_node_birth(&flush.birth_rows);
            self.append_block(
                BlockKind::NodeBirth,
                u32::try_from(flush.birth_rows.len()).unwrap_or(u32::MAX),
                window_first_ts,
                window_last_ts,
                &payload,
            )?;
        }
        if !flush.delta_rows.is_empty() {
            let payload = blocks::encode_cct_delta(&flush.delta_rows);
            self.append_block(
                BlockKind::CctDelta,
                u32::try_from(flush.delta_rows.len()).unwrap_or(u32::MAX),
                window_first_ts,
                window_last_ts,
                &payload,
            )?;
        }
        if !flush.hist_rows.is_empty() {
            let payload = blocks::encode_cct_hist(&flush.hist_rows);
            self.append_block(
                BlockKind::CctHist,
                u32::try_from(flush.hist_rows.len()).unwrap_or(u32::MAX),
                window_first_ts,
                window_last_ts,
                &payload,
            )?;
        }
        if !flush.llm_rows.is_empty() {
            let payload = blocks::encode_llm_delta(&flush.llm_rows);
            self.append_block(
                BlockKind::LlmDelta,
                u32::try_from(flush.llm_rows.len()).unwrap_or(u32::MAX),
                window_first_ts,
                window_last_ts,
                &payload,
            )?;
        }
        if !flush.spawn_rows.is_empty() {
            let payload = blocks::encode_spawn_edge(&flush.spawn_rows);
            self.append_block(
                BlockKind::SpawnEdge,
                u32::try_from(flush.spawn_rows.len()).unwrap_or(u32::MAX),
                window_first_ts,
                window_last_ts,
                &payload,
            )?;
        }
        if !flush.model_rows.is_empty() {
            let payload = blocks::encode_model_birth(&flush.model_rows);
            self.append_block(
                BlockKind::ModelBirth,
                u32::try_from(flush.model_rows.len()).unwrap_or(u32::MAX),
                window_first_ts,
                window_last_ts,
                &payload,
            )?;
        }

        // Checkpoint-by-bytes (§6.3 review fix): a full kind-8 table when
        // delta bytes since the last checkpoint reach the checkpoint's own
        // size (amortized ≤2× write volume) — never on a fixed cadence.
        let checkpoint_size = (nodes.len() * 48) as u64;
        if checkpoint_size > 0 && self.bytes_since_checkpoint >= checkpoint_size.max(4096) {
            self.write_checkpoint(nodes, window_last_ts)?;
        }
        Ok(())
    }

    /// kind-8 `node_total`: ABSOLUTE values for every node.
    fn write_checkpoint(&mut self, nodes: &Nodes, ts: u64) -> io::Result<()> {
        let mut rows = Vec::with_capacity(nodes.len());
        for node in 0..nodes.len() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "checkpoint enters/ends saturate u32 deliberately"
            )]
            rows.push(blocks::CctDeltaRow {
                node_id: u32::try_from(node).unwrap_or(u32::MAX),
                enters: nodes.enters[node].min(u64::from(u32::MAX)) as u32,
                ends_ok: nodes.ends_ok[node].min(u64::from(u32::MAX)) as u32,
                ends_err: nodes.ends_err[node].min(u64::from(u32::MAX)) as u32,
                ends_cancel: nodes.ends_cancel[node].min(u64::from(u32::MAX)) as u32,
                ends_exit: nodes.ends_exit[node].min(u64::from(u32::MAX)) as u32,
                total_ns: nodes.total_ns[node],
                self_ns: nodes.self_ns[node],
                await_ns: nodes.await_ns[node],
            });
        }
        let payload = blocks::encode_cct_delta(&rows);
        self.append_block(
            BlockKind::NodeTotal,
            u32::try_from(rows.len()).unwrap_or(u32::MAX),
            ts,
            ts,
            &payload,
        )?;
        self.bytes_since_checkpoint = 0;
        Ok(())
    }

    /// Cadence tick from the consumer loop: D1 group commit (1 s / 1 MiB,
    /// fsync off-thread), durable-watermark attestation rows, idle
    /// heartbeat watermarks, and segment rotation.
    pub fn tick(
        &mut self,
        fsync: &FsyncService,
        wall_epoch_ns: u64,
        max_seen_ts: u64,
    ) -> io::Result<()> {
        // Attest fsync completions with a kind-4 watermark row (durable).
        let durable_ts = self.durable.through_ts_ns.load(Ordering::Acquire);
        if durable_ts > self.last_attested_ts {
            let events = self.durable.events_drained.load(Ordering::Acquire);
            let payload = blocks::encode_watermark(&[blocks::WatermarkRow {
                wall_epoch_ns,
                drained_through_ts_ns: durable_ts,
                events_drained: events,
                durable_kind: 1, // D1
                reason: 0,
            }]);
            self.append_block(BlockKind::Watermark, 1, durable_ts, durable_ts, &payload)?;
            self.last_attested_ts = durable_ts;
            self.last_watermark = Instant::now();
        } else if self.last_watermark.elapsed() >= IDLE_WATERMARK_INTERVAL {
            // Idle heartbeat watermark (§6.3): ~7 B/s amortized.
            let payload = blocks::encode_watermark(&[blocks::WatermarkRow {
                wall_epoch_ns,
                drained_through_ts_ns: max_seen_ts,
                events_drained: self.events_drained,
                durable_kind: 0, // D0
                reason: 1,       // heartbeat
            }]);
            self.append_block(BlockKind::Watermark, 1, max_seen_ts, max_seen_ts, &payload)?;
            self.last_watermark = Instant::now();
        }

        // D1 group commit: hand the file to the fsync helper.
        if self.blocks_since_commit > 0
            && (self.bytes_since_commit >= GROUP_COMMIT_BYTES
                || self.last_commit.elapsed() >= GROUP_COMMIT_INTERVAL)
        {
            let seg = self.segment.as_mut().expect("segment open");
            seg.file.flush()?;
            if let Ok(clone) = seg.file.try_clone() {
                fsync.request(
                    clone,
                    max_seen_ts,
                    self.events_drained,
                    self.durable.clone(),
                );
            }
            self.bytes_since_commit = 0;
            self.blocks_since_commit = 0;
            self.last_commit = Instant::now();
        }

        // Rotation (§6.2): 4 MiB or 15 min.
        let rotate = {
            let seg = self.segment.as_ref().expect("segment open");
            seg.bytes >= SEGMENT_ROTATE_BYTES || seg.opened.elapsed() >= SEGMENT_ROTATE_AGE
        };
        if rotate {
            self.seal_active(max_seen_ts)?;
            self.seg_seq += 1;
            self.open_segment()?;
        }
        Ok(())
    }

    /// §6.5 `partition_bind` row: written when the host binds a boundary.
    pub fn write_partition_bind(
        &mut self,
        row: blocks::PartitionBindRow,
        ts: u64,
    ) -> io::Result<()> {
        let payload = blocks::encode_partition_bind(&[row]);
        self.append_block(BlockKind::PartitionBind, 1, ts, ts, &payload)
    }

    /// kind-11 `model_birth` rows (once per newly interned model).
    pub fn write_model_births(
        &mut self,
        rows: &[blocks::ModelBirthRow],
        ts: u64,
    ) -> io::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let payload = blocks::encode_model_birth(rows);
        self.append_block(
            BlockKind::ModelBirth,
            u32::try_from(rows.len()).unwrap_or(u32::MAX),
            ts,
            ts,
            &payload,
        )
    }

    /// kind-12 diagnostics marker.
    pub fn write_marker(&mut self, marker_kind: u8, detail: &str, ts: u64) -> io::Result<()> {
        let payload = blocks::encode_marker(&[blocks::MarkerRow {
            marker_kind,
            detail: detail.to_string(),
        }]);
        self.append_block(BlockKind::Marker, 1, ts, ts, &payload)
    }

    /// Seal the active segment by append (§6.2): footer_index block, then
    /// the 48 B trailer, then D2.
    fn seal_active(&mut self, ts: u64) -> io::Result<()> {
        // Encode the footer index as marker-style rows: (kind u8, offset
        // u64, row_count u32, first_ts u64, last_ts u64) packed row-major.
        let seg = self.segment.as_mut().expect("segment open");
        let mut payload = Vec::with_capacity(seg.index.len() * 29);
        for (kind, offset, rows, first, last) in &seg.index {
            payload.push(*kind);
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&rows.to_le_bytes());
            payload.extend_from_slice(&first.to_le_bytes());
            payload.extend_from_slice(&last.to_le_bytes());
        }
        let index_offset = seg.bytes;
        let row_count = u32::try_from(seg.index.len()).unwrap_or(u32::MAX);
        let total_rows = seg.total_rows;
        self.append_block(BlockKind::FooterIndex, row_count, ts, ts, &payload)?;
        let seg = self.segment.as_mut().expect("segment open");
        let index_len = seg.bytes - index_offset;
        let trailer = segment::encode_seal_trailer(index_offset, index_len, total_rows);
        seg.file.write_all(&trailer)?;
        seg.bytes += trailer.len() as u64;
        // Seal is D2 (§6.6): file sync + parent-dir sync.
        seg.file.sync_data()?;
        if let Some(parent) = seg.path.parent()
            && let Ok(dir) = std::fs::File::open(parent)
        {
            let _ = dir.sync_data();
        }
        Ok(())
    }

    /// §6.1: does this session need an epoch rotation (bounding the node
    /// table and birth growth of months-long processes)?
    #[must_use]
    pub fn should_rotate_epoch(&self) -> bool {
        self.session_cct_bytes >= EPOCH_ROTATE_BYTES
            || self.session_started.elapsed() >= EPOCH_ROTATE_AGE
    }

    /// §6.1 epoch close: carry-over checkpoint (absolute totals), an
    /// epoch-close marker, seal, and the meta record. The caller then
    /// rotates the CCT engine's node table and lets the next window mint a
    /// fresh session dir.
    pub fn close_epoch(mut self, nodes: &Nodes, ts: u64) -> io::Result<()> {
        self.write_checkpoint(nodes, ts)?;
        self.write_marker(
            blocks::marker_kind::EPOCH_CLOSE,
            "session epoch rotation",
            ts,
        )?;
        let cct_bytes = self.session_cct_bytes;
        self.seal_active(ts)?;
        self.meta.append(&MetaRecord::SessionEpochClose {
            reason: "epoch rotation (bytes/age bound)".to_string(),
            cct_bytes,
        })?;
        self.meta.sync_data()?;
        Ok(())
    }

    /// Engine close: seal, meta end, final syncs.
    pub fn close(mut self, ts: u64, reason: &str) -> io::Result<()> {
        self.seal_active(ts)?;
        self.meta.append(&MetaRecord::SessionEnd {
            reason: reason.to_string(),
        })?;
        self.meta.sync_data()?;
        Ok(())
    }

    /// Session heartbeat record (D0, §6.4 crash detection: heartbeat + pid
    /// liveness). Rate-limited internally to the 10 s cadence.
    pub fn heartbeat(&mut self, wall_epoch_ns: u64) -> io::Result<()> {
        if self.last_meta_heartbeat.elapsed() < IDLE_WATERMARK_INTERVAL {
            return Ok(());
        }
        self.last_meta_heartbeat = Instant::now();
        self.meta
            .append(&MetaRecord::SessionHeartbeat { wall_epoch_ns })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{BexCallId, BexThreadId, FunctionId};
    use crate::prof::cct::CctEngine;
    use crate::prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus};

    fn encode_records(records: &[RawRecord<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; MAX_RECORD_LEN];
        for rec in records {
            let len = rec.encode(&mut buf);
            out.extend_from_slice(&buf[..len]);
        }
        out
    }

    #[test]
    fn session_writer_end_to_end_scan() {
        let dir = std::env::temp_dir().join(format!("baml-session-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let fsync = FsyncService::start();
        let mut writer = SessionWriter::create(
            &dir,
            [7; 16],
            42,
            1_700_000_000_000_000_000,
            (3, 1, 1, 1),
            [9; 32],
            "baml_rev_1_test",
            &fsync,
        )
        .unwrap();

        // Feed a real engine and flush one window through the writer.
        let mut engine = CctEngine::new(16);
        engine.consume(
            &encode_records(&[
                RawRecord::StartThread {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    parent_thread_id: BexThreadId(0),
                    parent_call_id: BexCallId(0),
                    ts_ticks: 0,
                    name: b"",
                },
                RawRecord::CallFunction {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: BexCallId(0),
                    function_id: FunctionId(100),
                    call_site: None,
                    ts_ticks: 10,
                },
                RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    ts_ticks: 40,
                },
                RawRecord::EndThread {
                    status: ThreadEndStatus::Completed,
                    thread_id: BexThreadId(1),
                    ts_ticks: 50,
                },
            ]),
            &mut |t| t,
        );
        let flush = engine.flush_window();
        writer
            .write_window(&flush, engine.nodes(), 0, 50, 4)
            .unwrap();
        let session_dir = writer.session_dir().to_path_buf();
        writer.close(50, "test").unwrap();

        // The segment scans as sealed, with births + deltas + hist blocks.
        let seg_path = session_dir.join("cct").join("seg-000000.bamlseg");
        let bytes = std::fs::read(&seg_path).unwrap();
        let contents = super::segment::scan_segment(&bytes).unwrap();
        assert_eq!(contents.end, super::segment::ScanEnd::Sealed);
        let kinds: Vec<u8> = contents.blocks.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&(super::BlockKind::NodeBirth as u8)));
        assert!(kinds.contains(&(super::BlockKind::CctDelta as u8)));
        assert!(kinds.contains(&(super::BlockKind::CctHist as u8)));
        assert!(kinds.contains(&(super::BlockKind::FooterIndex as u8)));

        // The meta stream has begin + end.
        let meta_bytes = std::fs::read(session_dir.join("session.bamlmeta")).unwrap();
        let meta = super::super::meta::read_meta(&meta_bytes).unwrap();
        assert!(!meta.truncated);
        assert!(matches!(
            meta.records.first(),
            Some(super::super::meta::MetaRecord::SessionBegin { engine_id: 42, .. })
        ));
        assert!(matches!(
            meta.records.last(),
            Some(super::super::meta::MetaRecord::SessionEnd { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// §6.1 epoch re-mint: a second writer over the SAME session dir must
    /// resume segment numbering (not collide on create_new) and append —
    /// not truncate — the meta stream.
    #[test]
    fn remint_resumes_segment_numbering_and_appends_meta() {
        let dir = std::env::temp_dir().join(format!("baml-session-remint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fsync = FsyncService::start();
        let make = |fsync: &FsyncService| {
            SessionWriter::create(
                &dir,
                [7; 16],
                42,
                1_700_000_000_000_000_000,
                (3, 1, 1, 1),
                [9; 32],
                "baml_rev_1_test",
                fsync,
            )
            .unwrap()
        };
        let writer = make(&fsync);
        let session_dir = writer.session_dir().to_path_buf();
        writer.close(10, "epoch_test").unwrap();

        let second = make(&fsync);
        assert_eq!(second.session_dir(), session_dir.as_path());
        second.close(20, "epoch_test").unwrap();

        let mut segs: Vec<String> = std::fs::read_dir(session_dir.join("cct"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        segs.sort();
        assert_eq!(segs, vec!["seg-000000.bamlseg", "seg-000001.bamlseg"]);

        let meta_bytes = std::fs::read(session_dir.join("session.bamlmeta")).unwrap();
        let contents = super::super::meta::read_meta(&meta_bytes).unwrap();
        let kinds: Vec<u8> = contents.records.iter().map(MetaRecord::kind).collect();
        assert_eq!(
            kinds.iter().filter(|&&k| k == 1).count(),
            2,
            "two SessionBegin records: {kinds:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
