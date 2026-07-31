//! Consumer-facing durable session stream.
//!
//! `EngineCct` deliberately stays sans-IO. This adapter snapshots newly-born
//! identities before their first delta, converts each closed window to the
//! v1 column schemas, checkpoints by bytes, and owns segment durability.

use std::{collections::HashSet, fs::File, io, path::Path};

use crate::prof::cct::{CctHealth, CctSnapshot, NodeCounters, WindowDelta};

use super::{
    AsyncFileSync, BcctHeader, BcctWriter, BlockRows, CctDeltaRow, CctHistogramRow,
    CheckpointCadence, InstanceRow, LlmDeltaRow, MarkerKind, MarkerRow, ModelBirthRow,
    NodeBirthRow, PartitionBindRow, SessionBeginMeta, SessionEndMeta, SessionHeartbeatMeta,
    SessionLayout, SpawnEdgeRow, TypedSessionMeta, WatermarkRow, append_meta_d0, append_meta_d2,
    encode_typed_session_meta,
};

pub const SEGMENT_ROTATE_BYTES: u64 = 4 * 1024 * 1024;
pub const SEGMENT_ROTATE_NS: u64 = 15 * 60 * 1_000_000_000;
pub(crate) const SESSION_EPOCH_ROTATE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const SESSION_EPOCH_ROTATE_NS: u64 = 24 * 60 * 60 * 1_000_000_000;
pub const IDLE_WATERMARK_NS: u64 = 10 * 1_000_000_000;

/// One engine epoch's append-only CCT stream.
pub struct SessionStreamWriter {
    layout: SessionLayout,
    header: BcctHeader,
    writer: Option<BcctWriter<File>>,
    sync_worker: Option<AsyncFileSync>,
    cadence: CheckpointCadence,
    segment_started_ns: u64,
    emitted_nodes: usize,
    emitted_instances: HashSet<u64>,
    emitted_models: HashSet<u32>,
    last_health: CctHealth,
    blocks_since_watermark: bool,
    last_watermark_ns: u64,
    last_durable_block_seq: u32,
    rotated_bytes: u64,
    epoch_started_timestamp_ns: u64,
}

impl SessionStreamWriter {
    pub fn create(project_root: &Path, header: BcctHeader) -> io::Result<Self> {
        Self::create_at_process_timestamp(project_root, header, 0)
    }

    fn create_at_process_timestamp(
        project_root: &Path,
        header: BcctHeader,
        epoch_started_timestamp_ns: u64,
    ) -> io::Result<Self> {
        let layout = SessionLayout::new(
            project_root,
            header.started_epoch_ns / 1_000_000_000,
            header.process_euid,
            header.engine_id,
        );
        layout.create_dirs()?;
        let begin = TypedSessionMeta::Begin(SessionBeginMeta {
            process_euid: header.process_euid,
            engine_id: header.engine_id,
            pid: std::process::id(),
            started_epoch_ns: header.started_epoch_ns,
            revision_id: header.revision_id,
        });
        append_typed_d2(&layout.meta_path(), &begin)?;

        let writer = layout.create_segment(&header)?;
        let sync_worker = writer.async_sync_worker()?;
        Ok(Self {
            layout,
            header,
            writer: Some(writer),
            sync_worker: Some(sync_worker),
            cadence: CheckpointCadence::default(),
            segment_started_ns: 0,
            emitted_nodes: 0,
            emitted_instances: HashSet::new(),
            emitted_models: HashSet::new(),
            last_health: CctHealth::default(),
            blocks_since_watermark: false,
            last_watermark_ns: 0,
            last_durable_block_seq: 0,
            rotated_bytes: 0,
            epoch_started_timestamp_ns,
        })
    }

    #[must_use]
    pub fn layout(&self) -> &SessionLayout {
        &self.layout
    }

    #[must_use]
    pub fn segment_sequence(&self) -> u32 {
        self.header.session_seg_seq
    }

    /// Header template for a self-contained boundary snapshot. Boundary
    /// snapshots are independent sealed containers, so their local segment
    /// sequence is always one.
    #[must_use]
    pub fn boundary_snapshot_header(&self) -> BcctHeader {
        let mut header = self.header.clone();
        header.session_seg_seq = 1;
        header
    }

    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.rotated_bytes
            .saturating_add(self.writer.as_ref().map_or(0, BcctWriter::bytes_written))
    }

    /// Whether this session epoch has reached its independent identity-table
    /// lifetime bound. The consumer performs the actual rotation only when
    /// [`EngineCct`](crate::prof::cct::EngineCct) reports a quiescent gap.
    #[must_use]
    pub fn epoch_rotation_due(&self, timestamp_ns: u64) -> bool {
        self.epoch_rotation_due_with_limits(
            timestamp_ns,
            SESSION_EPOCH_ROTATE_BYTES,
            SESSION_EPOCH_ROTATE_NS,
        )
    }

    fn epoch_rotation_due_with_limits(
        &self,
        timestamp_ns: u64,
        max_bytes: u64,
        max_age_ns: u64,
    ) -> bool {
        self.bytes_written() >= max_bytes
            || timestamp_ns.saturating_sub(self.epoch_started_timestamp_ns) >= max_age_ns
    }

    /// Seals this epoch and starts a new session directory with segment and
    /// identity ids restarting from one/zero respectively.
    pub fn rotate_epoch(
        self,
        started_epoch_ns: u64,
        started_timestamp_ns: u64,
    ) -> io::Result<Self> {
        let project_root = self.layout.project_root.clone();
        let mut next_header = self.header.clone();
        next_header.session_seg_seq = 1;
        next_header.started_epoch_ns = started_epoch_ns;
        self.finish(started_epoch_ns, "session_epoch_rotate")?;
        Self::create_at_process_timestamp(&project_root, next_header, started_timestamp_ns)
    }

    /// Writes all rows for one closed CCT window. Identity births always
    /// precede rows that can reference them.
    pub fn write_window(&mut self, window: &WindowDelta, snapshot: &CctSnapshot) -> io::Result<()> {
        self.rotate_if_due(window.start_ns)?;
        if self.segment_started_ns == 0 {
            self.segment_started_ns = window.start_ns;
        }

        let new_nodes = snapshot
            .nodes
            .iter()
            .skip(self.emitted_nodes)
            .map(|node| NodeBirthRow {
                node_id: node.node_id,
                parent_node_id: node.identity.parent,
                function_id: node.identity.function_id.0,
                logical_thread_id: node.identity.first_thread_id,
                partition_id: node.identity.partition,
            })
            .collect::<Vec<_>>();
        if !new_nodes.is_empty() {
            self.append(
                &BlockRows::NodeBirth(new_nodes),
                window.start_ns,
                window.start_ns,
            )?;
            self.emitted_nodes = snapshot.nodes.len();
        }

        let node_rows = window
            .nodes
            .iter()
            .flat_map(|delta| counter_rows(delta.node_id, delta.counters))
            .collect::<Vec<_>>();
        if !node_rows.is_empty() {
            let writer = self
                .writer
                .as_mut()
                .ok_or_else(|| io::Error::other("BCCT writer is closed"))?;
            let outcome = writer.append_cct_delta(
                &mut self.cadence,
                node_rows,
                window.start_ns,
                window.end_ns,
            )?;
            self.blocks_since_watermark |= outcome.encoded_len != 0;
        }

        if !window.histograms.is_empty() {
            self.append(
                &BlockRows::CctHistogram(
                    window
                        .histograms
                        .iter()
                        .map(|row| CctHistogramRow {
                            node_id: row.node_id,
                            duration_buckets: row.buckets,
                        })
                        .collect(),
                ),
                window.start_ns,
                window.end_ns,
            )?;
        }

        let llm_rows = window
            .llm
            .iter()
            .flat_map(|row| {
                llm_rows(
                    row.node_id,
                    row.model_id,
                    row.counters.calls,
                    row.counters.tokens_in,
                    row.counters.tokens_out,
                    row.counters.provider_errs,
                    row.counters.parse_errs,
                )
            })
            .collect::<Vec<_>>();
        if !llm_rows.is_empty() {
            self.append(
                &BlockRows::LlmDelta(llm_rows),
                window.start_ns,
                window.end_ns,
            )?;
        }

        if !window.spawn.is_empty() {
            let mut rows = Vec::new();
            for delta in &window.spawn {
                let Some(edge) = snapshot
                    .spawn_edges
                    .iter()
                    .find(|edge| edge.edge_id == delta.edge_id)
                else {
                    continue;
                };
                rows.extend(spawn_rows(
                    delta.edge_id,
                    edge.identity.spawn_context,
                    edge.identity.child_entry_function,
                    edge.identity.child_root_node,
                    delta.counters.spawned,
                    delta.counters.completed,
                    delta.counters.errored,
                    delta.counters.cancelled,
                    delta.counters.running_ns,
                    delta.counters.awaiting_ns,
                ));
            }
            if !rows.is_empty() {
                self.append(&BlockRows::SpawnEdge(rows), window.start_ns, window.end_ns)?;
            }
        }

        let instances = snapshot
            .spawn_instances
            .iter()
            .filter(|instance| {
                instance.end_ns.is_some() && !self.emitted_instances.contains(&instance.thread_id)
            })
            .map(|instance| InstanceRow {
                thread_id: instance.thread_id,
                edge_id: instance.edge_id,
                status: instance.status.map_or(0, |status| status as u8),
                start_ns: instance.start_ns,
                end_ns: instance.end_ns.unwrap_or(0),
                dump_seq: 0,
                name: instance.name.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        if !instances.is_empty() {
            for instance in &instances {
                self.emitted_instances.insert(instance.thread_id);
            }
            self.append(
                &BlockRows::Instance(instances),
                window.start_ns,
                window.end_ns,
            )?;
        }

        self.append_health_markers(snapshot.health, window.end_ns)?;

        let totals = snapshot
            .nodes
            .iter()
            .flat_map(|node| counter_rows(node.node_id, node.counters))
            .collect::<Vec<_>>();
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("BCCT writer is closed"))?;
        let checkpoint = writer.append_checkpoint_if_due(
            &mut self.cadence,
            &totals,
            window.start_ns,
            window.end_ns,
        )?;
        self.blocks_since_watermark |= checkpoint.is_some();
        Ok(())
    }

    /// Persists a model dictionary row once per engine epoch.
    pub fn register_model(
        &mut self,
        model_id: u32,
        name: &str,
        timestamp_ns: u64,
    ) -> io::Result<()> {
        if !self.emitted_models.insert(model_id) {
            return Ok(());
        }
        self.append(
            &BlockRows::ModelBirth(vec![ModelBirthRow {
                model_id,
                name: name.to_owned(),
            }]),
            timestamp_ns,
            timestamp_ns,
        )
    }

    /// Persists the structural session-partition to boundary mapping. This is
    /// deliberately consumer-owned: only this writer knows the segment
    /// sequence in which the row became durable.
    pub fn bind_partition(&mut self, row: PartitionBindRow, timestamp_ns: u64) -> io::Result<()> {
        self.append(
            &BlockRows::PartitionBind(vec![row]),
            timestamp_ns,
            timestamp_ns,
        )
    }

    /// Emits a watermark after new data, or at the 10-second idle cadence,
    /// and sends D1 to the dedicated sync worker.
    pub fn watermark(
        &mut self,
        wall_epoch_ns: u64,
        drained_through_ts_ns: u64,
        events_drained: u64,
        force: bool,
    ) -> io::Result<()> {
        self.poll_sync()?;
        let idle_due =
            drained_through_ts_ns.saturating_sub(self.last_watermark_ns) >= IDLE_WATERMARK_NS;
        if !force && !self.blocks_since_watermark && !idle_due {
            return Ok(());
        }
        let worker = self
            .sync_worker
            .as_ref()
            .ok_or_else(|| io::Error::other("BCCT sync worker is unavailable"))?;
        let outcome = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("BCCT writer is closed"))?
            .append_watermark_and_request_sync(
                worker,
                WatermarkRow {
                    wall_epoch_ns,
                    drained_through_ts_ns,
                    events_drained,
                    durable_kind: 1,
                    reason: u8::from(force),
                },
            )?;
        self.last_durable_block_seq = outcome.block_seq;
        self.last_watermark_ns = drained_through_ts_ns;
        self.blocks_since_watermark = false;
        let heartbeat = TypedSessionMeta::Heartbeat(SessionHeartbeatMeta {
            pid: std::process::id(),
            wall_epoch_ns,
            durable_block_seq: outcome.block_seq,
        });
        append_typed_d0(&self.layout.meta_path(), &heartbeat)?;
        Ok(())
    }

    pub fn sync(&mut self, wall_epoch_ns: u64, timestamp_ns: u64, events: u64) -> io::Result<()> {
        self.watermark(wall_epoch_ns, timestamp_ns, events, true)?;
        let requested = u64::from(self.last_durable_block_seq);
        loop {
            let completion = self
                .sync_worker
                .as_ref()
                .ok_or_else(|| io::Error::other("BCCT sync worker is unavailable"))?
                .wait_complete()?;
            completion.result?;
            self.last_durable_block_seq =
                u32::try_from(completion.ticket).unwrap_or(self.last_durable_block_seq);
            if completion.ticket >= requested {
                break;
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.poll_sync()?;
        self.writer_mut()?.flush()
    }

    /// Seals the current segment and writes the D2 session end milestone.
    pub fn finish(mut self, ended_epoch_ns: u64, reason: &str) -> io::Result<()> {
        self.finish_sync_worker()?;
        if let Some(writer) = self.writer.as_mut() {
            writer.seal_synced()?;
            self.rotated_bytes = self.rotated_bytes.saturating_add(writer.bytes_written());
        }
        let end = TypedSessionMeta::End(SessionEndMeta {
            ended_epoch_ns,
            last_seg_seq: self.header.session_seg_seq,
            reason: reason.to_owned(),
        });
        append_typed_d2(&self.layout.meta_path(), &end)?;
        Ok(())
    }

    fn append(&mut self, rows: &BlockRows, first_ts_ns: u64, last_ts_ns: u64) -> io::Result<()> {
        if rows.row_count() == 0 && !matches!(rows, BlockRows::Reserved7(_)) {
            return Ok(());
        }
        self.writer_mut()?.append(rows, first_ts_ns, last_ts_ns)?;
        self.blocks_since_watermark = true;
        Ok(())
    }

    fn writer_mut(&mut self) -> io::Result<&mut BcctWriter<File>> {
        self.writer
            .as_mut()
            .ok_or_else(|| io::Error::other("BCCT writer is closed"))
    }

    fn poll_sync(&self) -> io::Result<()> {
        let Some(worker) = self.sync_worker.as_ref() else {
            return Ok(());
        };
        while let Some(completion) = worker.try_complete() {
            completion.result?;
        }
        Ok(())
    }

    fn rotate_if_due(&mut self, timestamp_ns: u64) -> io::Result<()> {
        let rotate_bytes = self
            .writer
            .as_ref()
            .is_some_and(|writer| writer.bytes_written() >= SEGMENT_ROTATE_BYTES);
        let rotate_time = self.segment_started_ns != 0
            && timestamp_ns.saturating_sub(self.segment_started_ns) >= SEGMENT_ROTATE_NS;
        if !rotate_bytes && !rotate_time {
            return Ok(());
        }
        self.finish_sync_worker()?;
        if let Some(writer) = self.writer.as_mut() {
            writer.seal_synced()?;
            self.rotated_bytes = self.rotated_bytes.saturating_add(writer.bytes_written());
        }
        self.header.session_seg_seq = self
            .header
            .session_seg_seq
            .checked_add(1)
            .ok_or_else(|| io::Error::other("BCCT segment sequence exhausted"))?;
        let writer = self.layout.create_segment(&self.header)?;
        let worker = writer.async_sync_worker()?;
        self.writer = Some(writer);
        self.sync_worker = Some(worker);
        self.cadence = CheckpointCadence::default();
        self.segment_started_ns = timestamp_ns;
        // Every segment is independently readable and therefore begins with
        // the complete identity table before any later delta references it.
        self.emitted_nodes = 0;
        self.emitted_models.clear();
        Ok(())
    }

    fn finish_sync_worker(&mut self) -> io::Result<()> {
        if let Some(worker) = self.sync_worker.take() {
            worker.finish()?;
        }
        Ok(())
    }

    fn append_health_markers(&mut self, health: CctHealth, timestamp_ns: u64) -> io::Result<()> {
        let mut rows = Vec::new();
        push_health_delta(
            &mut rows,
            "resync_records",
            self.last_health.resync_records,
            health.resync_records,
            MarkerKind::Degraded,
            timestamp_ns,
        );
        push_health_delta(
            &mut rows,
            "corrupt_ranges",
            self.last_health.corrupt_ranges,
            health.corrupt_ranges,
            MarkerKind::Loss,
            timestamp_ns,
        );
        push_health_delta(
            &mut rows,
            "reorder_clamped",
            self.last_health.reorder_clamped,
            health.reorder_clamped,
            MarkerKind::Degraded,
            timestamp_ns,
        );
        push_health_delta(
            &mut rows,
            "instances_dropped",
            self.last_health.instances_dropped,
            health.instances_dropped,
            MarkerKind::Loss,
            timestamp_ns,
        );
        push_health_delta(
            &mut rows,
            "shed_ranges",
            self.last_health.shed_ranges,
            health.shed_ranges,
            MarkerKind::Shed,
            timestamp_ns,
        );
        push_health_delta(
            &mut rows,
            "shed_events",
            self.last_health.shed_events,
            health.shed_events,
            MarkerKind::Loss,
            timestamp_ns,
        );
        self.last_health = health;
        if rows.is_empty() {
            Ok(())
        } else {
            self.append(&BlockRows::Marker(rows), timestamp_ns, timestamp_ns)
        }
    }
}

fn append_typed_d0(path: &Path, value: &TypedSessionMeta) -> io::Result<u64> {
    let (kind, payload) = encode_typed_session_meta(value)?;
    append_meta_d0(path, kind, &payload)
}

fn append_typed_d2(path: &Path, value: &TypedSessionMeta) -> io::Result<u64> {
    let (kind, payload) = encode_typed_session_meta(value)?;
    append_meta_d2(path, kind, &payload)
}

fn counter_rows(node_id: u32, mut counters: NodeCounters) -> Vec<CctDeltaRow> {
    let mut rows = Vec::new();
    loop {
        let has_counts = counters.enters != 0
            || counters.ends_ok != 0
            || counters.ends_err != 0
            || counters.ends_cancel != 0
            || counters.ends_exit != 0;
        let has_time = counters.total_ns != 0 || counters.self_ns != 0 || counters.await_ns != 0;
        if !has_counts && !has_time {
            break;
        }
        rows.push(CctDeltaRow {
            node_id,
            enters: take_u32(&mut counters.enters),
            ends_ok: take_u32(&mut counters.ends_ok),
            ends_err: take_u32(&mut counters.ends_err),
            ends_cancel: take_u32(&mut counters.ends_cancel),
            ends_exit: take_u32(&mut counters.ends_exit),
            total_ns: std::mem::take(&mut counters.total_ns),
            self_ns: std::mem::take(&mut counters.self_ns),
            await_ns: std::mem::take(&mut counters.await_ns),
        });
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn llm_rows(
    node_id: u32,
    model_id: u32,
    mut calls: u64,
    mut tokens_in: u64,
    mut tokens_out: u64,
    mut provider_errs: u64,
    mut parse_errs: u64,
) -> Vec<LlmDeltaRow> {
    let mut rows = Vec::new();
    while calls != 0 || tokens_in != 0 || tokens_out != 0 || provider_errs != 0 || parse_errs != 0 {
        rows.push(LlmDeltaRow {
            node_id,
            llm_calls_delta: take_u32(&mut calls),
            tokens_in_delta: std::mem::take(&mut tokens_in),
            tokens_out_delta: std::mem::take(&mut tokens_out),
            provider_errs_delta: take_u32(&mut provider_errs),
            parse_errs_delta: take_u32(&mut parse_errs),
            model_id,
        });
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn spawn_rows(
    edge_id: u32,
    parent_node: u32,
    entry_fn: u32,
    child_root_node: u32,
    mut spawned: u64,
    mut completed: u64,
    mut errored: u64,
    mut cancelled: u64,
    mut running_ns: u64,
    mut awaiting_ns: u64,
) -> Vec<SpawnEdgeRow> {
    let mut rows = Vec::new();
    while spawned != 0
        || completed != 0
        || errored != 0
        || cancelled != 0
        || running_ns != 0
        || awaiting_ns != 0
    {
        rows.push(SpawnEdgeRow {
            edge_id,
            parent_node,
            entry_fn,
            child_root_node,
            spawn_delta: take_u32(&mut spawned),
            completed_delta: take_u32(&mut completed),
            errored_delta: take_u32(&mut errored),
            cancelled_delta: take_u32(&mut cancelled),
            running_ns_delta: std::mem::take(&mut running_ns),
            awaiting_ns_delta: std::mem::take(&mut awaiting_ns),
        });
    }
    rows
}

fn take_u32(value: &mut u64) -> u32 {
    let taken = (*value).min(u64::from(u32::MAX));
    *value -= taken;
    taken as u32
}

fn push_health_delta(
    rows: &mut Vec<MarkerRow>,
    message: &str,
    before: u64,
    after: u64,
    kind: MarkerKind,
    timestamp_ns: u64,
) {
    let count = after.saturating_sub(before);
    if count != 0 {
        rows.push(MarkerRow {
            kind,
            timestamp_ns,
            node_id: None,
            count,
            message: message.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::FunctionId,
        prof::{
            cct::{CctEvent, EngineCct},
            record::FunctionEndStatus,
            storage::{
                BlockKind, ClockDescriptor, SegmentState, TypedSessionMeta,
                decode_typed_session_meta, scan_bcct_bytes, scan_meta_bytes,
            },
        },
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn large_u32_counters_are_split_without_loss() {
        let rows = counter_rows(
            7,
            NodeCounters {
                enters: u64::from(u32::MAX) + 9,
                ends_ok: u64::from(u32::MAX) * 2 + 3,
                total_ns: 42,
                ..NodeCounters::default()
            },
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|row| u64::from(row.enters)).sum::<u64>(),
            u64::from(u32::MAX) + 9
        );
        assert_eq!(
            rows.iter().map(|row| u64::from(row.ends_ok)).sum::<u64>(),
            u64::from(u32::MAX) * 2 + 3
        );
        assert_eq!(rows.iter().map(|row| row.total_ns).sum::<u64>(), 42);
    }

    #[test]
    fn session_stream_orders_births_seals_and_has_typed_lifecycle() {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "baml-session-stream-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut cct = EngineCct::new(250);
        cct.ingest(CctEvent::StartThread {
            flags: 0,
            thread_id: 1,
            parent_thread_id: 0,
            parent_call_id: 0,
            timestamp_ns: 10,
            name: None,
        });
        cct.ingest(CctEvent::CallFunction {
            flags: 0,
            thread_id: 1,
            call_id: 1,
            parent_call_id: 0,
            function_id: FunctionId(16),
            timestamp_ns: 20,
        });
        cct.ingest(CctEvent::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: 1,
            call_id: 1,
            timestamp_ns: 80,
        });
        cct.close_final_window_through(100);
        let windows = cct.take_windows();
        let snapshot = cct.snapshot();
        assert_eq!(windows.len(), 1);

        let header = BcctHeader {
            process_euid: [9; 16],
            engine_id: 7,
            session_seg_seq: 1,
            started_epoch_ns: 7_000_000_000,
            clock: ClockDescriptor {
                kind: 3,
                quality: 1,
                tick_ns_numer: 1,
                tick_ns_denom: 1,
            },
            revision_id: [3; 32],
        };
        let mut writer = SessionStreamWriter::create(&root, header).unwrap();
        for window in &windows {
            writer.write_window(window, &snapshot).unwrap();
        }
        writer.watermark(7_000_000_100, 100, 3, true).unwrap();
        let layout = writer.layout().clone();
        writer.finish(7_000_000_200, "test").unwrap();

        let scan = scan_bcct_bytes(&std::fs::read(layout.cct_segment_path(1)).unwrap()).unwrap();
        assert!(matches!(scan.state, SegmentState::Sealed(_)));
        let kinds = scan
            .blocks
            .iter()
            .filter_map(super::super::writer::ScannedBlock::known_kind)
            .collect::<Vec<_>>();
        let birth = kinds
            .iter()
            .position(|kind| *kind == BlockKind::NodeBirth)
            .unwrap();
        let delta = kinds
            .iter()
            .position(|kind| *kind == BlockKind::CctDelta)
            .unwrap();
        assert!(birth < delta);

        let meta = scan_meta_bytes(&std::fs::read(layout.meta_path()).unwrap());
        assert!(!meta.torn_tail);
        assert!(matches!(
            decode_typed_session_meta(&meta.records[0]).unwrap(),
            TypedSessionMeta::Begin(_)
        ));
        assert!(meta.records.iter().any(|record| matches!(
            decode_typed_session_meta(record).unwrap(),
            TypedSessionMeta::Heartbeat(_)
        )));
        assert!(matches!(
            decode_typed_session_meta(meta.records.last().unwrap()).unwrap(),
            TypedSessionMeta::End(_)
        ));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn session_epoch_deadlines_are_independent_of_segment_rotation() {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "baml-session-epoch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let header = BcctHeader {
            process_euid: [4; 16],
            engine_id: 8,
            session_seg_seq: 1,
            started_epoch_ns: 9_000_000_000,
            clock: ClockDescriptor {
                kind: 3,
                quality: 1,
                tick_ns_numer: 1,
                tick_ns_denom: 1,
            },
            revision_id: [5; 32],
        };
        let writer =
            SessionStreamWriter::create_at_process_timestamp(&root, header, 1_000).unwrap();
        assert!(!writer.epoch_rotation_due_with_limits(1_999, u64::MAX, 1_000));
        assert!(writer.epoch_rotation_due_with_limits(2_000, u64::MAX, 1_000));
        assert!(writer.epoch_rotation_due_with_limits(1_000, 1, u64::MAX));

        let old_layout = writer.layout().clone();
        let next = writer
            .rotate_epoch(11_000_000_000, 2_000)
            .expect("quiescent epoch rotation");
        assert_ne!(old_layout.session_dir, next.layout().session_dir);
        assert_eq!(next.segment_sequence(), 1);
        let old_meta = scan_meta_bytes(&std::fs::read(old_layout.meta_path()).unwrap());
        assert!(matches!(
            decode_typed_session_meta(old_meta.records.last().unwrap()).unwrap(),
            TypedSessionMeta::End(SessionEndMeta { ref reason, .. })
                if reason == "session_epoch_rotate"
        ));
        next.finish(11_000_000_100, "test").unwrap();
        std::fs::remove_dir_all(root).ok();
    }
}
