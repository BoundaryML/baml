use std::collections::BTreeMap;

use bex_events::prof::storage::{InstanceRow, MarkerKind};

use crate::{BcctScan, FileId, SourceSnapshot};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Completeness {
    pub complete: bool,
    pub watermarks: Vec<Watermark>,
    pub capture_loss: Vec<CaptureLoss>,
    pub sources_consulted: Vec<FileId>,
    pub truncated: bool,
    pub lod_degraded: bool,
    pub partial_tail: bool,
    pub more_lanes: bool,
    pub warnings: Vec<String>,
    pub snapshot: Vec<SourceWatermark>,
}

impl Completeness {
    pub(crate) fn from_scans(scans: &[BcctScan]) -> Self {
        let sources_consulted = scans.iter().map(|scan| scan.file).collect();
        let snapshot = scans
            .iter()
            .map(|scan| SourceWatermark {
                file: scan.file,
                source: scan.source,
                parsed_through: scan.committed_len,
            })
            .collect();
        let partial_tail = scans.iter().any(|scan| {
            !matches!(
                scan.state,
                bex_events::prof::storage::SegmentState::Sealed(_)
            )
        });
        Self {
            complete: !partial_tail,
            sources_consulted,
            partial_tail,
            snapshot,
            ..Self::default()
        }
    }

    pub(crate) fn finalize(&mut self) {
        self.complete =
            self.complete && !self.partial_tail && self.capture_loss.is_empty() && !self.truncated;
        self.sources_consulted.sort();
        self.sources_consulted.dedup();
        self.snapshot.sort_by_key(|source| source.file);
        self.snapshot.dedup_by_key(|source| source.file);
        self.warnings.sort();
        self.warnings.dedup();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWatermark {
    pub file: FileId,
    pub source: SourceSnapshot,
    pub parsed_through: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watermark {
    pub wall_epoch_ns: u64,
    pub drained_through_ts_ns: u64,
    pub events_drained: u64,
    pub durable_kind: u8,
    pub reason: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureLoss {
    pub kind: MarkerKind,
    pub timestamp_ns: u64,
    pub node_id: Option<u32>,
    pub count: u64,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub enters: u64,
    pub ends_ok: u64,
    pub ends_err: u64,
    pub ends_cancel: u64,
    pub ends_exit: u64,
    pub total_ns: u64,
    pub self_ns: u64,
    pub await_ns: u64,
}

impl Counters {
    #[must_use]
    pub fn errors(self) -> u64 {
        self.ends_err
            .saturating_add(self.ends_cancel)
            .saturating_add(self.ends_exit)
    }

    pub(crate) fn add_delta(&mut self, row: bex_events::prof::storage::CctDeltaRow) {
        self.enters = self.enters.saturating_add(u64::from(row.enters));
        self.ends_ok = self.ends_ok.saturating_add(u64::from(row.ends_ok));
        self.ends_err = self.ends_err.saturating_add(u64::from(row.ends_err));
        self.ends_cancel = self.ends_cancel.saturating_add(u64::from(row.ends_cancel));
        self.ends_exit = self.ends_exit.saturating_add(u64::from(row.ends_exit));
        self.total_ns = self.total_ns.saturating_add(row.total_ns);
        self.self_ns = self.self_ns.saturating_add(row.self_ns);
        self.await_ns = self.await_ns.saturating_add(row.await_ns);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LlmCounters {
    pub calls: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub provider_errors: u64,
    pub parse_errors: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldedNode {
    pub node_id: u32,
    pub parent_node_id: u32,
    pub function_id: u32,
    pub logical_thread_id: u64,
    pub partition_id: u32,
    pub counters: Counters,
    pub duration_buckets: [u64; 16],
    pub llm: LlmCounters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldedSpawnEdge {
    pub edge_id: u32,
    pub parent_node: u32,
    pub entry_fn: u32,
    pub child_root_node: u32,
    pub spawns: u64,
    pub completed: u64,
    pub errored: u64,
    pub cancelled: u64,
    pub running_ns: u64,
    pub awaiting_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowDelta {
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    pub node_id: u32,
    pub counters: Counters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldedCct {
    pub nodes: BTreeMap<u32, FoldedNode>,
    pub spawn_edges: BTreeMap<u32, FoldedSpawnEdge>,
    pub instances: Vec<InstanceRow>,
    pub windows: Vec<WindowDelta>,
    pub models: BTreeMap<u32, String>,
    pub partition_id: Option<u32>,
    pub first_ts_ns: Option<u64>,
    pub last_ts_ns: Option<u64>,
    pub meta: Completeness,
}

impl FoldedCct {
    #[must_use]
    pub fn estimated_bytes(&self) -> usize {
        self.nodes
            .len()
            .saturating_mul(std::mem::size_of::<FoldedNode>() + 32)
            .saturating_add(
                self.spawn_edges
                    .len()
                    .saturating_mul(std::mem::size_of::<FoldedSpawnEdge>() + 32),
            )
            .saturating_add(
                self.windows
                    .len()
                    .saturating_mul(std::mem::size_of::<WindowDelta>()),
            )
            .saturating_add(
                self.instances
                    .iter()
                    .map(|row| std::mem::size_of::<InstanceRow>() + row.name.len())
                    .sum::<usize>(),
            )
            .saturating_add(self.models.values().map(String::len).sum::<usize>())
    }
}
