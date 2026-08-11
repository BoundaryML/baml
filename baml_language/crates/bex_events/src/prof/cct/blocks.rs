//! §6.3 typed row codecs for the BCCT block kinds. Fixed-width kinds are
//! column-major (each column a contiguous little-endian array, padded to
//! 8-byte alignment) so a mmap view exposes zero-copy Arrow-compatible
//! columns; variable-width kinds (model/marker/instance) are row-major.

/// kind 1 `cct_delta` / kind 8 `node_total` — 48 B per row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

/// kind 2 `node_birth` — 24 B per row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeBirthRow {
    pub node_id: u32,
    pub parent_node_id: u32,
    pub function_id: u32,
    pub logical_thread_id: u64,
    pub partition_id: u32,
}

/// kind 3 `spawn_edge` deltas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

/// kind 4 `watermark`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WatermarkRow {
    pub wall_epoch_ns: u64,
    pub drained_through_ts_ns: u64,
    pub events_drained: u64,
    pub durable_kind: u8,
    pub reason: u8,
}

/// kind 5 `partition_bind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionBindRow {
    pub partition_id: u32,
    pub boundary_local_id: u32,
    pub boundary_id: [u8; 16],
    pub created_ms: u64,
}

/// kind 9 `cct_hist` — 68 B per row (16 × u32 buckets, ×4 stride).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CctHistRow {
    pub node_id: u32,
    pub buckets: [u32; super::nodes::HIST_BUCKETS],
}

/// kind 10 `llm_delta`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LlmDeltaRow {
    pub node_id: u32,
    pub llm_calls_delta: u32,
    pub tokens_in_delta: u64,
    pub tokens_out_delta: u64,
    pub provider_errs_delta: u32,
    pub parse_errs_delta: u32,
    pub model_id: u32,
}

/// kind 11 `model_birth` (row-major, variable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelBirthRow {
    pub model_id: u32,
    pub name: String,
}

/// kind 12 `marker` (row-major, variable): loss / degraded / shed /
/// budget-exhausted / epoch-close diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerRow {
    pub marker_kind: u8,
    pub detail: String,
}

/// Marker kinds.
pub mod marker_kind {
    pub const LOSS: u8 = 1;
    pub const DEGRADED: u8 = 2;
    pub const SHED: u8 = 3;
    pub const BUDGET_EXHAUSTED: u8 = 4;
    pub const EPOCH_CLOSE: u8 = 5;
    /// A counter clamped at its wire width (u32::MAX): the written totals
    /// are exact lower bounds, not exact counts. Additive kind — readers
    /// that predate it pass it through as an opaque marker row.
    pub const SATURATED: u8 = 6;
}

/// kind 13 `instance` (row-major, variable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRow {
    pub thread_id: u64,
    pub edge_id: u32,
    pub status: u8,
    pub start_ns: u64,
    pub end_ns: u64,
    pub dump_seq: u32,
    pub name: String,
}

fn pad8(buf: &mut Vec<u8>) {
    while buf.len() % 8 != 0 {
        buf.push(0);
    }
}

/// Column-major encoding macro: for each field, one contiguous array, each
/// column padded to 8-byte alignment.
macro_rules! columnar_codec {
    ($encode:ident, $decode:ident, $row:ty, [$(($field:ident, $ty:ty)),+ $(,)?]) => {
        #[must_use]
        pub fn $encode(rows: &[$row]) -> Vec<u8> {
            let mut out = Vec::new();
            $(
                for row in rows {
                    out.extend_from_slice(&row.$field.to_le_bytes());
                }
                pad8(&mut out);
            )+
            out
        }

        /// Decode `row_count` rows; `None` on size mismatch (a reader must
        /// never fabricate rows from a short payload).
        #[must_use]
        pub fn $decode(payload: &[u8], row_count: usize) -> Option<Vec<$row>> {
            let mut rows = vec![<$row>::default(); row_count];
            let mut offset = 0usize;
            $(
                {
                    let width = std::mem::size_of::<$ty>();
                    let col_len = width * row_count;
                    let col = payload.get(offset..offset + col_len)?;
                    for (i, chunk) in col.chunks_exact(width).enumerate() {
                        rows[i].$field = <$ty>::from_le_bytes(chunk.try_into().ok()?);
                    }
                    offset += col_len;
                    offset = offset.next_multiple_of(8);
                }
            )+
            (offset <= payload.len()).then_some(rows)
        }
    };
}

columnar_codec!(
    encode_cct_delta,
    decode_cct_delta,
    CctDeltaRow,
    [
        (node_id, u32),
        (enters, u32),
        (ends_ok, u32),
        (ends_err, u32),
        (ends_cancel, u32),
        (ends_exit, u32),
        (total_ns, u64),
        (self_ns, u64),
        (await_ns, u64),
    ]
);

columnar_codec!(
    encode_node_birth,
    decode_node_birth,
    NodeBirthRow,
    [
        (node_id, u32),
        (parent_node_id, u32),
        (function_id, u32),
        (logical_thread_id, u64),
        (partition_id, u32),
    ]
);

columnar_codec!(
    encode_spawn_edge,
    decode_spawn_edge,
    SpawnEdgeRow,
    [
        (edge_id, u32),
        (parent_node, u32),
        (entry_fn, u32),
        (child_root_node, u32),
        (spawn_delta, u32),
        (completed_delta, u32),
        (errored_delta, u32),
        (cancelled_delta, u32),
        (running_ns_delta, u64),
        (awaiting_ns_delta, u64),
    ]
);

columnar_codec!(
    encode_watermark,
    decode_watermark,
    WatermarkRow,
    [
        (wall_epoch_ns, u64),
        (drained_through_ts_ns, u64),
        (events_drained, u64),
        (durable_kind, u8),
        (reason, u8),
    ]
);

columnar_codec!(
    encode_llm_delta,
    decode_llm_delta,
    LlmDeltaRow,
    [
        (node_id, u32),
        (llm_calls_delta, u32),
        (tokens_in_delta, u64),
        (tokens_out_delta, u64),
        (provider_errs_delta, u32),
        (parse_errs_delta, u32),
        (model_id, u32),
    ]
);

/// `partition_bind` has an array field — hand-rolled columnar codec.
#[must_use]
pub fn encode_partition_bind(rows: &[PartitionBindRow]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        out.extend_from_slice(&row.partition_id.to_le_bytes());
    }
    pad8(&mut out);
    for row in rows {
        out.extend_from_slice(&row.boundary_local_id.to_le_bytes());
    }
    pad8(&mut out);
    for row in rows {
        out.extend_from_slice(&row.boundary_id);
    }
    pad8(&mut out);
    for row in rows {
        out.extend_from_slice(&row.created_ms.to_le_bytes());
    }
    out
}

#[must_use]
pub fn decode_partition_bind(payload: &[u8], row_count: usize) -> Option<Vec<PartitionBindRow>> {
    let mut rows = vec![
        PartitionBindRow {
            partition_id: 0,
            boundary_local_id: 0,
            boundary_id: [0; 16],
            created_ms: 0,
        };
        row_count
    ];
    let mut offset = 0usize;
    for row in rows.iter_mut() {
        row.partition_id = u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;
    }
    offset = offset.next_multiple_of(8);
    for row in rows.iter_mut() {
        row.boundary_local_id =
            u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;
    }
    offset = offset.next_multiple_of(8);
    for row in rows.iter_mut() {
        row.boundary_id = payload.get(offset..offset + 16)?.try_into().ok()?;
        offset += 16;
    }
    offset = offset.next_multiple_of(8);
    for row in rows.iter_mut() {
        row.created_ms = u64::from_le_bytes(payload.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;
    }
    Some(rows)
}

/// `cct_hist` — node column then 16 bucket columns.
#[must_use]
pub fn encode_cct_hist(rows: &[CctHistRow]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        out.extend_from_slice(&row.node_id.to_le_bytes());
    }
    pad8(&mut out);
    for bucket in 0..super::nodes::HIST_BUCKETS {
        for row in rows {
            out.extend_from_slice(&row.buckets[bucket].to_le_bytes());
        }
        pad8(&mut out);
    }
    out
}

#[must_use]
pub fn decode_cct_hist(payload: &[u8], row_count: usize) -> Option<Vec<CctHistRow>> {
    let mut rows = vec![
        CctHistRow {
            node_id: 0,
            buckets: [0; super::nodes::HIST_BUCKETS],
        };
        row_count
    ];
    let mut offset = 0usize;
    for row in rows.iter_mut() {
        row.node_id = u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;
    }
    offset = offset.next_multiple_of(8);
    for bucket in 0..super::nodes::HIST_BUCKETS {
        for row in rows.iter_mut() {
            row.buckets[bucket] =
                u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
            offset += 4;
        }
        offset = offset.next_multiple_of(8);
    }
    Some(rows)
}

/// Variable-width row-major kinds: `model_birth`, `marker`, `instance`.
#[must_use]
pub fn encode_model_birth(rows: &[ModelBirthRow]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        out.extend_from_slice(&row.model_id.to_le_bytes());
        let name = row.name.as_bytes();
        out.extend_from_slice(
            &u16::try_from(name.len().min(u16::MAX as usize))
                .unwrap()
                .to_le_bytes(),
        );
        out.extend_from_slice(&name[..name.len().min(u16::MAX as usize)]);
    }
    out
}

#[must_use]
pub fn decode_model_birth(payload: &[u8], row_count: usize) -> Option<Vec<ModelBirthRow>> {
    let mut rows = Vec::with_capacity(row_count);
    let mut offset = 0usize;
    for _ in 0..row_count {
        let model_id = u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;
        let len = u16::from_le_bytes(payload.get(offset..offset + 2)?.try_into().ok()?) as usize;
        offset += 2;
        let name = std::str::from_utf8(payload.get(offset..offset + len)?).ok()?;
        offset += len;
        rows.push(ModelBirthRow {
            model_id,
            name: name.to_string(),
        });
    }
    Some(rows)
}

#[must_use]
pub fn encode_marker(rows: &[MarkerRow]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row.marker_kind);
        let detail = row.detail.as_bytes();
        out.extend_from_slice(
            &u16::try_from(detail.len().min(u16::MAX as usize))
                .unwrap()
                .to_le_bytes(),
        );
        out.extend_from_slice(&detail[..detail.len().min(u16::MAX as usize)]);
    }
    out
}

#[must_use]
pub fn decode_marker(payload: &[u8], row_count: usize) -> Option<Vec<MarkerRow>> {
    let mut rows = Vec::with_capacity(row_count);
    let mut offset = 0usize;
    for _ in 0..row_count {
        let marker_kind = *payload.get(offset)?;
        offset += 1;
        let len = u16::from_le_bytes(payload.get(offset..offset + 2)?.try_into().ok()?) as usize;
        offset += 2;
        let detail = std::str::from_utf8(payload.get(offset..offset + len)?).ok()?;
        offset += len;
        rows.push(MarkerRow {
            marker_kind,
            detail: detail.to_string(),
        });
    }
    Some(rows)
}

#[must_use]
pub fn encode_instance(rows: &[InstanceRow]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        out.extend_from_slice(&row.thread_id.to_le_bytes());
        out.extend_from_slice(&row.edge_id.to_le_bytes());
        out.push(row.status);
        let name = row.name.as_bytes();
        let len = name.len().min(u16::MAX as usize);
        out.extend_from_slice(&u16::try_from(len).unwrap().to_le_bytes());
        out.extend_from_slice(&row.start_ns.to_le_bytes());
        out.extend_from_slice(&row.end_ns.to_le_bytes());
        out.extend_from_slice(&row.dump_seq.to_le_bytes());
        out.extend_from_slice(&name[..len]);
    }
    out
}

#[must_use]
pub fn decode_instance(payload: &[u8], row_count: usize) -> Option<Vec<InstanceRow>> {
    let mut rows = Vec::with_capacity(row_count);
    let mut offset = 0usize;
    for _ in 0..row_count {
        let thread_id = u64::from_le_bytes(payload.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;
        let edge_id = u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;
        let status = *payload.get(offset)?;
        offset += 1;
        let len = u16::from_le_bytes(payload.get(offset..offset + 2)?.try_into().ok()?) as usize;
        offset += 2;
        let start_ns = u64::from_le_bytes(payload.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;
        let end_ns = u64::from_le_bytes(payload.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;
        let dump_seq = u32::from_le_bytes(payload.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;
        let name = std::str::from_utf8(payload.get(offset..offset + len)?).ok()?;
        offset += len;
        rows.push(InstanceRow {
            thread_id,
            edge_id,
            status,
            start_ns,
            end_ns,
            dump_seq,
            name: name.to_string(),
        });
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_width_kinds_roundtrip_column_major() {
        let deltas = vec![
            CctDeltaRow {
                node_id: 1,
                enters: 2,
                ends_ok: 3,
                ends_err: 4,
                ends_cancel: 5,
                ends_exit: 6,
                total_ns: 7,
                self_ns: 8,
                await_ns: 9,
            },
            CctDeltaRow {
                node_id: 10,
                ..Default::default()
            },
        ];
        let payload = encode_cct_delta(&deltas);
        assert_eq!(decode_cct_delta(&payload, 2).unwrap(), deltas);
        // Column-major: the first 8 bytes are BOTH node ids.
        assert_eq!(&payload[0..4], &1u32.to_le_bytes());
        assert_eq!(&payload[4..8], &10u32.to_le_bytes());
        // Short payload never fabricates rows.
        assert!(decode_cct_delta(&payload[..10], 2).is_none());

        let births = vec![NodeBirthRow {
            node_id: 1,
            parent_node_id: 2,
            function_id: 16,
            logical_thread_id: 9,
            partition_id: 0,
        }];
        assert_eq!(
            decode_node_birth(&encode_node_birth(&births), 1).unwrap(),
            births
        );

        let edges = vec![SpawnEdgeRow {
            edge_id: 1,
            running_ns_delta: 55,
            ..Default::default()
        }];
        assert_eq!(
            decode_spawn_edge(&encode_spawn_edge(&edges), 1).unwrap(),
            edges
        );

        let marks = vec![WatermarkRow {
            wall_epoch_ns: 1,
            drained_through_ts_ns: 2,
            events_drained: 3,
            durable_kind: 1,
            reason: 0,
        }];
        assert_eq!(
            decode_watermark(&encode_watermark(&marks), 1).unwrap(),
            marks
        );

        let llm = vec![LlmDeltaRow {
            node_id: 4,
            llm_calls_delta: 1,
            tokens_in_delta: 100,
            tokens_out_delta: 50,
            provider_errs_delta: 0,
            parse_errs_delta: 1,
            model_id: 2,
        }];
        assert_eq!(decode_llm_delta(&encode_llm_delta(&llm), 1).unwrap(), llm);
    }

    #[test]
    fn hist_and_bind_roundtrip() {
        let mut buckets = [0u32; crate::prof::cct::HIST_BUCKETS];
        buckets[3] = 77;
        let rows = vec![CctHistRow {
            node_id: 5,
            buckets,
        }];
        assert_eq!(decode_cct_hist(&encode_cct_hist(&rows), 1).unwrap(), rows);

        let binds = vec![PartitionBindRow {
            partition_id: 1,
            boundary_local_id: 0,
            boundary_id: [3; 16],
            created_ms: 1_700_000,
        }];
        assert_eq!(
            decode_partition_bind(&encode_partition_bind(&binds), 1).unwrap(),
            binds
        );
    }

    #[test]
    fn variable_kinds_roundtrip() {
        let models = vec![
            ModelBirthRow {
                model_id: 1,
                name: "claude-fable-5".to_string(),
            },
            ModelBirthRow {
                model_id: 2,
                name: "gpt-4o".to_string(),
            },
        ];
        assert_eq!(
            decode_model_birth(&encode_model_birth(&models), 2).unwrap(),
            models
        );

        let marks = vec![MarkerRow {
            marker_kind: marker_kind::SHED,
            detail: "shed_ranges=3".to_string(),
        }];
        assert_eq!(decode_marker(&encode_marker(&marks), 1).unwrap(), marks);

        let instances = vec![InstanceRow {
            thread_id: 9,
            edge_id: 1,
            status: 2,
            start_ns: 10,
            end_ns: 20,
            dump_seq: 0,
            name: "worker-1".to_string(),
        }];
        assert_eq!(
            decode_instance(&encode_instance(&instances), 1).unwrap(),
            instances
        );
    }
}
