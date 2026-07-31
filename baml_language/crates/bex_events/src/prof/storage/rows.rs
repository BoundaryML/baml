use std::io;

use super::format::{BlockKind, get_u16, get_u32, get_u64, invalid_data};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CctDeltaRow {
    pub node_id: u32,
    pub enters: u32,
    pub ends_ok: u32,
    pub ends_err: u32,
    pub ends_cancel: u32,
    pub ends_exit: u32,
    pub total_ns: u64,
    pub self_ns: u64,
    pub await_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_field_names)]
pub struct NodeBirthRow {
    pub node_id: u32,
    pub parent_node_id: u32,
    pub function_id: u32,
    pub logical_thread_id: u64,
    pub partition_id: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnEdgeRow {
    pub edge_id: u32,
    pub parent_node: u32,
    pub entry_fn: u32,
    pub child_root_node: u32,
    pub spawn_delta: u32,
    pub completed_delta: u32,
    pub errored_delta: u32,
    pub cancelled_delta: u32,
    pub running_ns_delta: u64,
    pub awaiting_ns_delta: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WatermarkRow {
    pub wall_epoch_ns: u64,
    pub drained_through_ts_ns: u64,
    pub events_drained: u64,
    pub durable_kind: u8,
    pub reason: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartitionBindRow {
    pub partition_id: u32,
    pub boundary_local_id: u32,
    pub boundary_id: [u8; 16],
    pub created_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FooterIndexRow {
    pub kind: u8,
    pub offset: u64,
    pub row_count: u32,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    pub node_id_min: u32,
    pub node_id_max: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CctHistogramRow {
    pub node_id: u32,
    pub duration_buckets: [u32; 16],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LlmDeltaRow {
    pub node_id: u32,
    pub llm_calls_delta: u32,
    pub tokens_in_delta: u64,
    pub tokens_out_delta: u64,
    pub provider_errs_delta: u32,
    pub parse_errs_delta: u32,
    pub model_id: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelBirthRow {
    pub model_id: u32,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkerKind {
    Loss,
    #[default]
    Degraded,
    Shed,
    BudgetExhausted,
    EpochClose,
    Other(u8),
}

impl MarkerKind {
    fn to_raw(self) -> u8 {
        match self {
            Self::Loss => 1,
            Self::Degraded => 2,
            Self::Shed => 3,
            Self::BudgetExhausted => 4,
            Self::EpochClose => 5,
            Self::Other(raw) => raw,
        }
    }

    fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Loss,
            2 => Self::Degraded,
            3 => Self::Shed,
            4 => Self::BudgetExhausted,
            5 => Self::EpochClose,
            other => Self::Other(other),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkerRow {
    pub kind: MarkerKind,
    pub timestamp_ns: u64,
    /// `None` is encoded as `u32::MAX`.
    pub node_id: Option<u32>,
    pub count: u64,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstanceRow {
    pub thread_id: u64,
    pub edge_id: u32,
    pub status: u8,
    pub start_ns: u64,
    pub end_ns: u64,
    pub dump_seq: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockRows {
    CctDelta(Vec<CctDeltaRow>),
    NodeBirth(Vec<NodeBirthRow>),
    SpawnEdge(Vec<SpawnEdgeRow>),
    Watermark(Vec<WatermarkRow>),
    PartitionBind(Vec<PartitionBindRow>),
    FooterIndex(Vec<FooterIndexRow>),
    Reserved7(Vec<u8>),
    NodeTotal(Vec<CctDeltaRow>),
    CctHistogram(Vec<CctHistogramRow>),
    LlmDelta(Vec<LlmDeltaRow>),
    ModelBirth(Vec<ModelBirthRow>),
    Marker(Vec<MarkerRow>),
    Instance(Vec<InstanceRow>),
    Opaque {
        kind: u8,
        row_count: u32,
        bytes: Vec<u8>,
    },
}

impl BlockRows {
    #[must_use]
    pub fn kind_raw(&self) -> u8 {
        match self {
            Self::CctDelta(_) => BlockKind::CctDelta as u8,
            Self::NodeBirth(_) => BlockKind::NodeBirth as u8,
            Self::SpawnEdge(_) => BlockKind::SpawnEdge as u8,
            Self::Watermark(_) => BlockKind::Watermark as u8,
            Self::PartitionBind(_) => BlockKind::PartitionBind as u8,
            Self::FooterIndex(_) => BlockKind::FooterIndex as u8,
            Self::Reserved7(_) => BlockKind::Reserved7 as u8,
            Self::NodeTotal(_) => BlockKind::NodeTotal as u8,
            Self::CctHistogram(_) => BlockKind::CctHistogram as u8,
            Self::LlmDelta(_) => BlockKind::LlmDelta as u8,
            Self::ModelBirth(_) => BlockKind::ModelBirth as u8,
            Self::Marker(_) => BlockKind::Marker as u8,
            Self::Instance(_) => BlockKind::Instance as u8,
            Self::Opaque { kind, .. } => *kind,
        }
    }

    #[must_use]
    pub fn row_count(&self) -> u32 {
        let len = match self {
            Self::CctDelta(rows) | Self::NodeTotal(rows) => rows.len(),
            Self::NodeBirth(rows) => rows.len(),
            Self::SpawnEdge(rows) => rows.len(),
            Self::Watermark(rows) => rows.len(),
            Self::PartitionBind(rows) => rows.len(),
            Self::FooterIndex(rows) => rows.len(),
            Self::Reserved7(_) => 0,
            Self::CctHistogram(rows) => rows.len(),
            Self::LlmDelta(rows) => rows.len(),
            Self::ModelBirth(rows) => rows.len(),
            Self::Marker(rows) => rows.len(),
            Self::Instance(rows) => rows.len(),
            Self::Opaque { row_count, .. } => return *row_count,
        };
        u32::try_from(len).unwrap_or(u32::MAX)
    }

    pub(crate) fn encode_payload(&self) -> io::Result<Vec<u8>> {
        let mut out = ColumnEncoder::default();
        match self {
            Self::CctDelta(rows) | Self::NodeTotal(rows) => encode_cct_delta(&mut out, rows),
            Self::NodeBirth(rows) => {
                out.u32s(rows.iter().map(|row| row.node_id));
                out.u32s(rows.iter().map(|row| row.parent_node_id));
                out.u32s(rows.iter().map(|row| row.function_id));
                out.u64s(rows.iter().map(|row| row.logical_thread_id));
                out.u32s(rows.iter().map(|row| row.partition_id));
            }
            Self::SpawnEdge(rows) => encode_spawn_edge(&mut out, rows),
            Self::Watermark(rows) => {
                out.u64s(rows.iter().map(|row| row.wall_epoch_ns));
                out.u64s(rows.iter().map(|row| row.drained_through_ts_ns));
                out.u64s(rows.iter().map(|row| row.events_drained));
                out.u8s(rows.iter().map(|row| row.durable_kind));
                out.u8s(rows.iter().map(|row| row.reason));
            }
            Self::PartitionBind(rows) => {
                out.u32s(rows.iter().map(|row| row.partition_id));
                out.u32s(rows.iter().map(|row| row.boundary_local_id));
                out.fixed_16(rows.iter().map(|row| row.boundary_id));
                out.u64s(rows.iter().map(|row| row.created_ms));
            }
            Self::FooterIndex(rows) => {
                out.u8s(rows.iter().map(|row| row.kind));
                out.u64s(rows.iter().map(|row| row.offset));
                out.u32s(rows.iter().map(|row| row.row_count));
                out.u64s(rows.iter().map(|row| row.first_ts_ns));
                out.u64s(rows.iter().map(|row| row.last_ts_ns));
                out.u32s(rows.iter().map(|row| row.node_id_min));
                out.u32s(rows.iter().map(|row| row.node_id_max));
            }
            Self::Reserved7(bytes) | Self::Opaque { bytes, .. } => {
                out.bytes.extend_from_slice(bytes);
            }
            Self::CctHistogram(rows) => {
                out.u32s(rows.iter().map(|row| row.node_id));
                for bucket in 0..16 {
                    out.u32s(rows.iter().map(|row| row.duration_buckets[bucket]));
                }
            }
            Self::LlmDelta(rows) => {
                out.u32s(rows.iter().map(|row| row.node_id));
                out.u32s(rows.iter().map(|row| row.llm_calls_delta));
                out.u64s(rows.iter().map(|row| row.tokens_in_delta));
                out.u64s(rows.iter().map(|row| row.tokens_out_delta));
                out.u32s(rows.iter().map(|row| row.provider_errs_delta));
                out.u32s(rows.iter().map(|row| row.parse_errs_delta));
                out.u32s(rows.iter().map(|row| row.model_id));
            }
            Self::ModelBirth(rows) => {
                out.u32s(rows.iter().map(|row| row.model_id));
                out.strings(rows.iter().map(|row| row.name.as_str()))?;
            }
            Self::Marker(rows) => {
                out.u8s(rows.iter().map(|row| row.kind.to_raw()));
                out.u64s(rows.iter().map(|row| row.timestamp_ns));
                out.u32s(rows.iter().map(|row| row.node_id.unwrap_or(u32::MAX)));
                out.u64s(rows.iter().map(|row| row.count));
                out.strings(rows.iter().map(|row| row.message.as_str()))?;
            }
            Self::Instance(rows) => {
                out.u64s(rows.iter().map(|row| row.thread_id));
                out.u32s(rows.iter().map(|row| row.edge_id));
                out.u8s(rows.iter().map(|row| row.status));
                out.u16s(
                    rows.iter()
                        .map(|row| u16::try_from(row.name.len()))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| invalid_data("BCCT instance name exceeds u16"))?,
                );
                out.u64s(rows.iter().map(|row| row.start_ns));
                out.u64s(rows.iter().map(|row| row.end_ns));
                out.u32s(rows.iter().map(|row| row.dump_seq));
                out.raw_strings(rows.iter().map(|row| row.name.as_str()))?;
            }
        }
        out.align();
        Ok(out.bytes)
    }

    pub(crate) fn decode_payload(kind: u8, row_count: u32, bytes: &[u8]) -> io::Result<Self> {
        let count =
            usize::try_from(row_count).map_err(|_| invalid_data("BCCT row count overflow"))?;
        let Some(kind) = BlockKind::from_raw(kind) else {
            return Ok(Self::Opaque {
                kind,
                row_count,
                bytes: bytes.to_vec(),
            });
        };
        let mut input = ColumnDecoder::new(bytes);
        Ok(match kind {
            BlockKind::CctDelta | BlockKind::NodeTotal => {
                let rows = decode_cct_delta(&mut input, count)?;
                if kind == BlockKind::CctDelta {
                    Self::CctDelta(rows)
                } else {
                    Self::NodeTotal(rows)
                }
            }
            BlockKind::NodeBirth => {
                let node_id = input.u32s(count)?;
                let parent_node_id = input.u32s(count)?;
                let function_id = input.u32s(count)?;
                let logical_thread_id = input.u64s(count)?;
                let partition_id = input.u32s(count)?;
                Self::NodeBirth(
                    (0..count)
                        .map(|index| NodeBirthRow {
                            node_id: node_id[index],
                            parent_node_id: parent_node_id[index],
                            function_id: function_id[index],
                            logical_thread_id: logical_thread_id[index],
                            partition_id: partition_id[index],
                        })
                        .collect(),
                )
            }
            BlockKind::SpawnEdge => Self::SpawnEdge(decode_spawn_edge(&mut input, count)?),
            BlockKind::Watermark => {
                let wall_epoch_ns = input.u64s(count)?;
                let drained_through_ts_ns = input.u64s(count)?;
                let events_drained = input.u64s(count)?;
                let durable_kind = input.u8s(count)?;
                let reason = input.u8s(count)?;
                Self::Watermark(
                    (0..count)
                        .map(|index| WatermarkRow {
                            wall_epoch_ns: wall_epoch_ns[index],
                            drained_through_ts_ns: drained_through_ts_ns[index],
                            events_drained: events_drained[index],
                            durable_kind: durable_kind[index],
                            reason: reason[index],
                        })
                        .collect(),
                )
            }
            BlockKind::PartitionBind => {
                let partition_id = input.u32s(count)?;
                let boundary_local_id = input.u32s(count)?;
                let boundary_id = input.fixed_16(count)?;
                let created_ms = input.u64s(count)?;
                Self::PartitionBind(
                    (0..count)
                        .map(|index| PartitionBindRow {
                            partition_id: partition_id[index],
                            boundary_local_id: boundary_local_id[index],
                            boundary_id: boundary_id[index],
                            created_ms: created_ms[index],
                        })
                        .collect(),
                )
            }
            BlockKind::FooterIndex => {
                let kinds = input.u8s(count)?;
                let offset = input.u64s(count)?;
                let row_count = input.u32s(count)?;
                let first_ts_ns = input.u64s(count)?;
                let last_ts_ns = input.u64s(count)?;
                let node_id_min = input.u32s(count)?;
                let node_id_max = input.u32s(count)?;
                Self::FooterIndex(
                    (0..count)
                        .map(|index| FooterIndexRow {
                            kind: kinds[index],
                            offset: offset[index],
                            row_count: row_count[index],
                            first_ts_ns: first_ts_ns[index],
                            last_ts_ns: last_ts_ns[index],
                            node_id_min: node_id_min[index],
                            node_id_max: node_id_max[index],
                        })
                        .collect(),
                )
            }
            BlockKind::Reserved7 => Self::Reserved7(bytes.to_vec()),
            BlockKind::CctHistogram => {
                let node_id = input.u32s(count)?;
                let mut buckets = Vec::with_capacity(16);
                for _ in 0..16 {
                    buckets.push(input.u32s(count)?);
                }
                Self::CctHistogram(
                    (0..count)
                        .map(|index| CctHistogramRow {
                            node_id: node_id[index],
                            duration_buckets: std::array::from_fn(|bucket| buckets[bucket][index]),
                        })
                        .collect(),
                )
            }
            BlockKind::LlmDelta => {
                let node_id = input.u32s(count)?;
                let llm_calls_delta = input.u32s(count)?;
                let tokens_in_delta = input.u64s(count)?;
                let tokens_out_delta = input.u64s(count)?;
                let provider_errs_delta = input.u32s(count)?;
                let parse_errs_delta = input.u32s(count)?;
                let model_id = input.u32s(count)?;
                Self::LlmDelta(
                    (0..count)
                        .map(|index| LlmDeltaRow {
                            node_id: node_id[index],
                            llm_calls_delta: llm_calls_delta[index],
                            tokens_in_delta: tokens_in_delta[index],
                            tokens_out_delta: tokens_out_delta[index],
                            provider_errs_delta: provider_errs_delta[index],
                            parse_errs_delta: parse_errs_delta[index],
                            model_id: model_id[index],
                        })
                        .collect(),
                )
            }
            BlockKind::ModelBirth => {
                let model_id = input.u32s(count)?;
                let names = input.strings(count)?;
                Self::ModelBirth(
                    (0..count)
                        .map(|index| ModelBirthRow {
                            model_id: model_id[index],
                            name: names[index].clone(),
                        })
                        .collect(),
                )
            }
            BlockKind::Marker => {
                let kinds = input.u8s(count)?;
                let timestamp_ns = input.u64s(count)?;
                let node_id = input.u32s(count)?;
                let counts = input.u64s(count)?;
                let messages = input.strings(count)?;
                Self::Marker(
                    (0..count)
                        .map(|index| MarkerRow {
                            kind: MarkerKind::from_raw(kinds[index]),
                            timestamp_ns: timestamp_ns[index],
                            node_id: (node_id[index] != u32::MAX).then_some(node_id[index]),
                            count: counts[index],
                            message: messages[index].clone(),
                        })
                        .collect(),
                )
            }
            BlockKind::Instance => {
                let thread_id = input.u64s(count)?;
                let edge_id = input.u32s(count)?;
                let status = input.u8s(count)?;
                let name_lens = input.u16s(count)?;
                let start_ns = input.u64s(count)?;
                let end_ns = input.u64s(count)?;
                let dump_seq = input.u32s(count)?;
                let names = input.raw_strings(&name_lens)?;
                Self::Instance(
                    (0..count)
                        .map(|index| InstanceRow {
                            thread_id: thread_id[index],
                            edge_id: edge_id[index],
                            status: status[index],
                            start_ns: start_ns[index],
                            end_ns: end_ns[index],
                            dump_seq: dump_seq[index],
                            name: names[index].clone(),
                        })
                        .collect(),
                )
            }
        })
    }
}

fn encode_cct_delta(out: &mut ColumnEncoder, rows: &[CctDeltaRow]) {
    out.u32s(rows.iter().map(|row| row.node_id));
    out.u32s(rows.iter().map(|row| row.enters));
    out.u32s(rows.iter().map(|row| row.ends_ok));
    out.u32s(rows.iter().map(|row| row.ends_err));
    out.u32s(rows.iter().map(|row| row.ends_cancel));
    out.u32s(rows.iter().map(|row| row.ends_exit));
    out.u64s(rows.iter().map(|row| row.total_ns));
    out.u64s(rows.iter().map(|row| row.self_ns));
    out.u64s(rows.iter().map(|row| row.await_ns));
}

fn decode_cct_delta(input: &mut ColumnDecoder<'_>, count: usize) -> io::Result<Vec<CctDeltaRow>> {
    let node_id = input.u32s(count)?;
    let enters = input.u32s(count)?;
    let ends_ok = input.u32s(count)?;
    let ends_err = input.u32s(count)?;
    let ends_cancel = input.u32s(count)?;
    let ends_exit = input.u32s(count)?;
    let total_ns = input.u64s(count)?;
    let self_ns = input.u64s(count)?;
    let await_ns = input.u64s(count)?;
    Ok((0..count)
        .map(|index| CctDeltaRow {
            node_id: node_id[index],
            enters: enters[index],
            ends_ok: ends_ok[index],
            ends_err: ends_err[index],
            ends_cancel: ends_cancel[index],
            ends_exit: ends_exit[index],
            total_ns: total_ns[index],
            self_ns: self_ns[index],
            await_ns: await_ns[index],
        })
        .collect())
}

fn encode_spawn_edge(out: &mut ColumnEncoder, rows: &[SpawnEdgeRow]) {
    out.u32s(rows.iter().map(|row| row.edge_id));
    out.u32s(rows.iter().map(|row| row.parent_node));
    out.u32s(rows.iter().map(|row| row.entry_fn));
    out.u32s(rows.iter().map(|row| row.child_root_node));
    out.u32s(rows.iter().map(|row| row.spawn_delta));
    out.u32s(rows.iter().map(|row| row.completed_delta));
    out.u32s(rows.iter().map(|row| row.errored_delta));
    out.u32s(rows.iter().map(|row| row.cancelled_delta));
    out.u64s(rows.iter().map(|row| row.running_ns_delta));
    out.u64s(rows.iter().map(|row| row.awaiting_ns_delta));
}

fn decode_spawn_edge(input: &mut ColumnDecoder<'_>, count: usize) -> io::Result<Vec<SpawnEdgeRow>> {
    let edge_id = input.u32s(count)?;
    let parent_node = input.u32s(count)?;
    let entry_fn = input.u32s(count)?;
    let child_root_node = input.u32s(count)?;
    let spawn_delta = input.u32s(count)?;
    let completed_delta = input.u32s(count)?;
    let errored_delta = input.u32s(count)?;
    let cancelled_delta = input.u32s(count)?;
    let running_ns_delta = input.u64s(count)?;
    let awaiting_ns_delta = input.u64s(count)?;
    Ok((0..count)
        .map(|index| SpawnEdgeRow {
            edge_id: edge_id[index],
            parent_node: parent_node[index],
            entry_fn: entry_fn[index],
            child_root_node: child_root_node[index],
            spawn_delta: spawn_delta[index],
            completed_delta: completed_delta[index],
            errored_delta: errored_delta[index],
            cancelled_delta: cancelled_delta[index],
            running_ns_delta: running_ns_delta[index],
            awaiting_ns_delta: awaiting_ns_delta[index],
        })
        .collect())
}

#[derive(Default)]
struct ColumnEncoder {
    bytes: Vec<u8>,
}

impl ColumnEncoder {
    fn align(&mut self) {
        let padding = (8 - self.bytes.len() % 8) % 8;
        self.bytes.resize(self.bytes.len() + padding, 0);
    }

    fn u8s(&mut self, values: impl IntoIterator<Item = u8>) {
        self.align();
        self.bytes.extend(values);
    }

    fn u16s(&mut self, values: impl IntoIterator<Item = u16>) {
        self.align();
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn u32s(&mut self, values: impl IntoIterator<Item = u32>) {
        self.align();
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn u64s(&mut self, values: impl IntoIterator<Item = u64>) {
        self.align();
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn fixed_16(&mut self, values: impl IntoIterator<Item = [u8; 16]>) {
        self.align();
        for value in values {
            self.bytes.extend_from_slice(&value);
        }
    }

    fn strings<'a>(&mut self, values: impl IntoIterator<Item = &'a str>) -> io::Result<()> {
        let values = values.into_iter().collect::<Vec<_>>();
        let mut offset = 0_u32;
        let mut offsets = Vec::with_capacity(values.len() + 1);
        offsets.push(offset);
        for value in &values {
            offset = offset
                .checked_add(
                    u32::try_from(value.len())
                        .map_err(|_| invalid_data("BCCT string column exceeds u32"))?,
                )
                .ok_or_else(|| invalid_data("BCCT string column exceeds u32"))?;
            offsets.push(offset);
        }
        self.u32s(offsets);
        self.align();
        for value in values {
            self.bytes.extend_from_slice(value.as_bytes());
        }
        Ok(())
    }

    fn raw_strings<'a>(&mut self, values: impl IntoIterator<Item = &'a str>) -> io::Result<()> {
        self.align();
        for value in values {
            if value.len() > usize::from(u16::MAX) {
                return Err(invalid_data("BCCT instance name exceeds u16"));
            }
            self.bytes.extend_from_slice(value.as_bytes());
        }
        Ok(())
    }
}

struct ColumnDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ColumnDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn align(&mut self) -> io::Result<()> {
        self.offset = self
            .offset
            .checked_add((8 - self.offset % 8) % 8)
            .ok_or_else(|| invalid_data("BCCT payload offset overflow"))?;
        if self.offset > self.bytes.len() {
            return Err(invalid_data("truncated BCCT aligned column"));
        }
        Ok(())
    }

    fn take(&mut self, len: usize) -> io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| invalid_data("BCCT payload offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| invalid_data("truncated BCCT column"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u8s(&mut self, count: usize) -> io::Result<Vec<u8>> {
        self.align()?;
        Ok(self.take(count)?.to_vec())
    }

    fn u16s(&mut self, count: usize) -> io::Result<Vec<u16>> {
        self.align()?;
        let bytes = self.take(
            count
                .checked_mul(2)
                .ok_or_else(|| invalid_data("BCCT column length overflow"))?,
        )?;
        Ok((0..count).map(|index| get_u16(bytes, index * 2)).collect())
    }

    fn u32s(&mut self, count: usize) -> io::Result<Vec<u32>> {
        self.align()?;
        let bytes = self.take(
            count
                .checked_mul(4)
                .ok_or_else(|| invalid_data("BCCT column length overflow"))?,
        )?;
        Ok((0..count).map(|index| get_u32(bytes, index * 4)).collect())
    }

    fn u64s(&mut self, count: usize) -> io::Result<Vec<u64>> {
        self.align()?;
        let bytes = self.take(
            count
                .checked_mul(8)
                .ok_or_else(|| invalid_data("BCCT column length overflow"))?,
        )?;
        Ok((0..count).map(|index| get_u64(bytes, index * 8)).collect())
    }

    fn fixed_16(&mut self, count: usize) -> io::Result<Vec<[u8; 16]>> {
        self.align()?;
        let bytes = self.take(
            count
                .checked_mul(16)
                .ok_or_else(|| invalid_data("BCCT column length overflow"))?,
        )?;
        Ok((0..count)
            .map(|index| {
                bytes[index * 16..index * 16 + 16]
                    .try_into()
                    .expect("fixed-width column")
            })
            .collect())
    }

    fn strings(&mut self, count: usize) -> io::Result<Vec<String>> {
        let offsets = self.u32s(
            count
                .checked_add(1)
                .ok_or_else(|| invalid_data("BCCT string count overflow"))?,
        )?;
        self.align()?;
        if offsets.first().copied() != Some(0) {
            return Err(invalid_data("invalid BCCT string offsets"));
        }
        let total = usize::try_from(offsets[count])
            .map_err(|_| invalid_data("BCCT string length overflow"))?;
        let bytes = self.take(total)?;
        let mut strings = Vec::with_capacity(count);
        for pair in offsets.windows(2) {
            let start = usize::try_from(pair[0])
                .map_err(|_| invalid_data("BCCT string offset overflow"))?;
            let end = usize::try_from(pair[1])
                .map_err(|_| invalid_data("BCCT string offset overflow"))?;
            if start > end {
                return Err(invalid_data("invalid BCCT string offsets"));
            }
            strings.push(
                std::str::from_utf8(
                    bytes
                        .get(start..end)
                        .ok_or_else(|| invalid_data("invalid BCCT string offsets"))?,
                )
                .map_err(|_| invalid_data("invalid BCCT UTF-8"))?
                .to_owned(),
            );
        }
        Ok(strings)
    }

    fn raw_strings(&mut self, lengths: &[u16]) -> io::Result<Vec<String>> {
        self.align()?;
        let total = lengths.iter().try_fold(0_usize, |total, len| {
            total
                .checked_add(usize::from(*len))
                .ok_or_else(|| invalid_data("BCCT string length overflow"))
        })?;
        let bytes = self.take(total)?;
        let mut offset = 0;
        let mut strings = Vec::with_capacity(lengths.len());
        for length in lengths {
            let end = offset + usize::from(*length);
            strings.push(
                std::str::from_utf8(&bytes[offset..end])
                    .map_err(|_| invalid_data("invalid BCCT UTF-8"))?
                    .to_owned(),
            );
            offset = end;
        }
        Ok(strings)
    }
}
