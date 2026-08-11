//! §6.5 boundary snapshot fold: at boundary completion, one partition's
//! rows fold into a sealed `cct.bamlcct` — same BCCT container, always
//! sealed, node ids re-densified 1..N with birth columns embedded. The
//! boundary dir becomes self-contained for share/export/delete; the
//! partition's RAM is then freed (§5.7).

use rustc_hash::FxHashMap;

use super::blocks;
use super::engine::CctEngine;
use super::segment::{self, BlockKind, SegmentHeader};

/// A folded partition, ready for encoding.
pub struct FoldedPartition {
    /// Old node id → dense new id (0 = the partition pseudo-root).
    pub remap: FxHashMap<u32, u32>,
    pub births: Vec<blocks::NodeBirthRow>,
    pub totals: Vec<blocks::CctDeltaRow>,
    pub hists: Vec<blocks::CctHistRow>,
    pub llm: Vec<blocks::LlmDeltaRow>,
    pub spawns: Vec<blocks::SpawnEdgeRow>,
    pub models: Vec<blocks::ModelBirthRow>,
    /// Count fields that exceeded the u32 wire width and were clamped.
    /// Nonzero folds carry a SATURATED marker in the encoded snapshot so
    /// "population-true" readers see explicit lower bounds, not silently
    /// wrong exact counts.
    pub clamped_fields: u64,
}

/// Fold one partition out of the engine's live state. Pure read.
#[must_use]
pub fn fold_partition(engine: &CctEngine, partition: u32) -> FoldedPartition {
    fold_where(engine, |p| p == partition)
}

/// §9.2 `LiveMirrorSource` tap: fold EVERY partition (whole-engine live
/// state). Same container, `partition_id` preserved per birth row.
#[must_use]
pub fn fold_all(engine: &CctEngine) -> FoldedPartition {
    fold_where(engine, |_| true)
}

fn fold_where(engine: &CctEngine, keep: impl Fn(u32) -> bool) -> FoldedPartition {
    let nodes = engine.nodes();
    // Dense remap in node-id order (parents intern before children, so a
    // single ordered pass keeps parent < child in the new numbering).
    let mut remap: FxHashMap<u32, u32> = FxHashMap::default();
    let mut order: Vec<u32> = Vec::new();
    for node in 0..nodes.len() {
        if keep(nodes.partition[node]) {
            let old = u32::try_from(node).unwrap_or(u32::MAX);
            remap.insert(old, u32::try_from(order.len()).unwrap_or(u32::MAX));
            order.push(old);
        }
    }

    // Fold counts saturate to the u32 wire width deliberately — but every
    // engaged clamp is counted so the snapshot can carry an explicit
    // SATURATED marker instead of a silently short count.
    let mut clamped_fields: u64 = 0;
    let mut clamp = |v: u64| {
        if v > u64::from(u32::MAX) {
            clamped_fields += 1;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "explicitly clamped and counted just above"
        )]
        {
            v.min(u64::from(u32::MAX)) as u32
        }
    };

    let mut births = Vec::with_capacity(order.len());
    let mut totals = Vec::with_capacity(order.len());
    let mut hists = Vec::new();
    for &old in &order {
        let i = old as usize;
        let new_id = remap[&old];
        let parent = nodes.parent[i];
        births.push(blocks::NodeBirthRow {
            node_id: new_id,
            parent_node_id: remap.get(&parent).copied().unwrap_or(u32::MAX),
            function_id: nodes.function[i],
            logical_thread_id: 0,
            partition_id: nodes.partition[i],
        });
        totals.push(blocks::CctDeltaRow {
            node_id: new_id,
            enters: clamp(nodes.enters[i]),
            ends_ok: clamp(nodes.ends_ok[i]),
            ends_err: clamp(nodes.ends_err[i]),
            ends_cancel: clamp(nodes.ends_cancel[i]),
            ends_exit: clamp(nodes.ends_exit[i]),
            total_ns: nodes.total_ns[i],
            self_ns: nodes.self_ns[i],
            await_ns: nodes.await_ns[i],
        });
        if nodes.hist[i].iter().any(|&b| b != 0) {
            hists.push(blocks::CctHistRow {
                node_id: new_id,
                buckets: nodes.hist[i],
            });
        }
    }

    // LLM totals for the partition's nodes.
    let mut llm: Vec<blocks::LlmDeltaRow> = Vec::new();
    let mut model_ids: Vec<u32> = Vec::new();
    for (&(node, model_id), counters) in engine.llm_counters() {
        if let Some(&new_id) = remap.get(&node) {
            llm.push(blocks::LlmDeltaRow {
                node_id: new_id,
                llm_calls_delta: clamp(counters.llm_calls),
                tokens_in_delta: counters.tokens_in,
                tokens_out_delta: counters.tokens_out,
                provider_errs_delta: clamp(counters.provider_errs),
                parse_errs_delta: clamp(counters.parse_errs),
                model_id,
            });
            if !model_ids.contains(&model_id) {
                model_ids.push(model_id);
            }
        }
    }
    llm.sort_by_key(|row| (row.node_id, row.model_id));
    let mut models: Vec<blocks::ModelBirthRow> = model_ids
        .into_iter()
        .filter_map(|model_id| {
            engine
                .model_name(model_id)
                .map(|name| blocks::ModelBirthRow {
                    model_id,
                    name: name.to_string(),
                })
        })
        .collect();
    models.sort_by_key(|row| row.model_id);

    // Spawn-edge totals whose parent node lives in this partition.
    let edges = engine.spawn_edges();
    let mut spawns: Vec<blocks::SpawnEdgeRow> = Vec::new();
    for edge in 0..edges.len() {
        if let Some(&parent_new) = remap.get(&edges.parent_node[edge]) {
            let counters = edges.counters[edge];
            spawns.push(blocks::SpawnEdgeRow {
                edge_id: u32::try_from(spawns.len()).unwrap_or(u32::MAX),
                parent_node: parent_new,
                entry_fn: edges.entry_fn[edge],
                child_root_node: remap
                    .get(&edges.child_root_node[edge])
                    .copied()
                    .unwrap_or(u32::MAX),
                spawn_delta: clamp(counters.spawned),
                completed_delta: clamp(counters.completed),
                errored_delta: clamp(counters.errored),
                cancelled_delta: clamp(counters.cancelled),
                running_ns_delta: 0,
                awaiting_ns_delta: 0,
            });
        }
    }

    FoldedPartition {
        remap,
        births,
        totals,
        hists,
        llm,
        spawns,
        models,
        clamped_fields,
    }
}

/// Encode a folded partition as a sealed `cct.bamlcct` byte buffer (§6.5):
/// births, `node_total`s, hist totals, llm totals, spawn totals, one
/// `partition_bind` row, footer + trailer. Callers write via tmp+rename
/// (D2).
#[must_use]
pub fn encode_boundary_snapshot(
    folded: &FoldedPartition,
    header: &SegmentHeader,
    bind: blocks::PartitionBindRow,
) -> Vec<u8> {
    encode_snapshot_inner(folded, header, Some(bind))
}

fn encode_snapshot_inner(
    folded: &FoldedPartition,
    header: &SegmentHeader,
    bind: Option<blocks::PartitionBindRow>,
) -> Vec<u8> {
    let mut bytes = header.encode().to_vec();
    let mut block_seq: u32 = 0;
    let mut total_rows: u64 = 0;
    let mut index: Vec<(u8, u64, u32)> = Vec::new();
    let push = |bytes: &mut Vec<u8>,
                kind: BlockKind,
                row_count: u32,
                payload: &[u8],
                block_seq: &mut u32,
                total_rows: &mut u64,
                index: &mut Vec<(u8, u64, u32)>| {
        let block =
            segment::encode_block(bytes.len(), kind, 0, row_count, 0, 0, payload, *block_seq);
        let pad =
            block.len() - (segment::BLOCK_HEADER_LEN + payload.len() + segment::BLOCK_TRAILER_LEN);
        index.push((kind as u8, (bytes.len() + pad) as u64, row_count));
        bytes.extend_from_slice(&block);
        *block_seq += 1;
        *total_rows += u64::from(row_count);
    };

    if !folded.births.is_empty() {
        let payload = blocks::encode_node_birth(&folded.births);
        let n = u32::try_from(folded.births.len()).unwrap_or(u32::MAX);
        push(
            &mut bytes,
            BlockKind::NodeBirth,
            n,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if !folded.totals.is_empty() {
        let payload = blocks::encode_cct_delta(&folded.totals);
        let n = u32::try_from(folded.totals.len()).unwrap_or(u32::MAX);
        push(
            &mut bytes,
            BlockKind::NodeTotal,
            n,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if !folded.hists.is_empty() {
        let payload = blocks::encode_cct_hist(&folded.hists);
        let n = u32::try_from(folded.hists.len()).unwrap_or(u32::MAX);
        push(
            &mut bytes,
            BlockKind::CctHist,
            n,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if !folded.llm.is_empty() {
        let payload = blocks::encode_llm_delta(&folded.llm);
        let n = u32::try_from(folded.llm.len()).unwrap_or(u32::MAX);
        push(
            &mut bytes,
            BlockKind::LlmDelta,
            n,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if !folded.models.is_empty() {
        let payload = blocks::encode_model_birth(&folded.models);
        let n = u32::try_from(folded.models.len()).unwrap_or(u32::MAX);
        push(
            &mut bytes,
            BlockKind::ModelBirth,
            n,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if !folded.spawns.is_empty() {
        let payload = blocks::encode_spawn_edge(&folded.spawns);
        let n = u32::try_from(folded.spawns.len()).unwrap_or(u32::MAX);
        push(
            &mut bytes,
            BlockKind::SpawnEdge,
            n,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if let Some(bind) = bind {
        let payload = blocks::encode_partition_bind(&[bind]);
        push(
            &mut bytes,
            BlockKind::PartitionBind,
            1,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }
    if folded.clamped_fields > 0 {
        // Explicit saturation evidence: every clamped count field makes
        // the snapshot's totals lower bounds, and the snapshot says so.
        let payload = blocks::encode_marker(&[blocks::MarkerRow {
            marker_kind: blocks::marker_kind::SATURATED,
            detail: format!(
                "{} counter field(s) clamped at u32::MAX; totals are lower bounds",
                folded.clamped_fields
            ),
        }]);
        push(
            &mut bytes,
            BlockKind::Marker,
            1,
            &payload,
            &mut block_seq,
            &mut total_rows,
            &mut index,
        );
    }

    // Footer index + seal trailer (always sealed).
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

/// §9.2 `LiveMirrorSource` wire: encode the whole-engine live fold as an
/// always-sealed segment (no partition bind — this is session-level RAM
/// truth, identical block format to disk so query code cannot tell).
#[must_use]
pub fn encode_live_snapshot(folded: &FoldedPartition, header: &SegmentHeader) -> Vec<u8> {
    encode_snapshot_inner(folded, header, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{BexCallId, BexThreadId, FunctionId};
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
    fn fold_re_densifies_and_snapshot_seals() {
        let mut engine = CctEngine::new(16);
        let records = [
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
        ];
        engine.consume(&encode_records(&records), &mut |t| t);

        let folded = fold_partition(&engine, 0);
        assert_eq!(folded.births.len(), 2, "pseudo-root + fn100 context");
        assert_eq!(folded.totals.len(), 2);
        // Dense ids from 0; parent precedes child.
        assert_eq!(folded.births[0].node_id, 0);
        assert_eq!(folded.births[1].node_id, 1);
        assert_eq!(folded.births[1].parent_node_id, 0);
        let fn_row = &folded.totals[1];
        assert_eq!((fn_row.enters, fn_row.ends_ok, fn_row.total_ns), (1, 1, 30));
        assert_eq!(folded.hists.len(), 1, "closed node has a hist row");

        let header = SegmentHeader {
            process_euid: [1; 16],
            engine_id: 7,
            session_seg_seq: 0,
            started_epoch_ns: 0,
            clock_kind: 3,
            clock_quality: 1,
            tick_ns_numer: 1,
            tick_ns_denom: 1,
            revision_id: [2; 32],
        };
        let bind = blocks::PartitionBindRow {
            partition_id: 0,
            boundary_local_id: 0,
            boundary_id: [9; 16],
            created_ms: 1,
        };
        let snapshot = encode_boundary_snapshot(&folded, &header, bind);
        let contents = segment::scan_segment(&snapshot).expect("snapshot parses");
        assert_eq!(contents.end, segment::ScanEnd::Sealed, "always sealed");
        // births + totals + hist + bind + footer = 5 blocks.
        assert_eq!(contents.blocks.len(), 5);
        let totals_block = contents
            .blocks
            .iter()
            .find(|b| b.kind == BlockKind::NodeTotal as u8)
            .expect("node_total block");
        let rows = blocks::decode_cct_delta(totals_block.payload, totals_block.row_count as usize)
            .expect("totals decode");
        assert_eq!(rows[1].total_ns, 30);
    }

    /// A fold that engaged a u32 clamp writes an explicit SATURATED marker
    /// into the sealed snapshot: totals become declared lower bounds, not
    /// silently wrong exact counts.
    #[test]
    fn clamped_fold_carries_saturated_marker() {
        let header = SegmentHeader {
            process_euid: [1; 16],
            engine_id: 7,
            session_seg_seq: 0,
            started_epoch_ns: 0,
            clock_kind: 3,
            clock_quality: 1,
            tick_ns_numer: 1,
            tick_ns_denom: 1,
            revision_id: [2; 32],
        };
        let folded = FoldedPartition {
            remap: FxHashMap::default(),
            births: vec![blocks::NodeBirthRow {
                node_id: 0,
                parent_node_id: u32::MAX,
                function_id: 0,
                logical_thread_id: 0,
                partition_id: 0,
            }],
            totals: vec![blocks::CctDeltaRow {
                node_id: 0,
                enters: u32::MAX,
                ends_ok: u32::MAX,
                ends_err: 0,
                ends_cancel: 0,
                ends_exit: 0,
                total_ns: 1,
                self_ns: 1,
                await_ns: 0,
            }],
            hists: Vec::new(),
            llm: Vec::new(),
            spawns: Vec::new(),
            models: Vec::new(),
            clamped_fields: 2,
        };
        let snapshot = encode_live_snapshot(&folded, &header);
        let contents = segment::scan_segment(&snapshot).expect("snapshot parses");
        let marker_block = contents
            .blocks
            .iter()
            .find(|b| b.kind == BlockKind::Marker as u8)
            .expect("clamped fold must carry a marker block");
        let markers = blocks::decode_marker(marker_block.payload, marker_block.row_count as usize)
            .expect("marker decodes");
        assert_eq!(markers[0].marker_kind, blocks::marker_kind::SATURATED);
        assert!(
            markers[0].detail.contains("2 counter field(s)"),
            "{}",
            markers[0].detail
        );
        assert!(markers[0].detail.contains("lower bounds"));

        // An unclamped fold carries no marker.
        let clean = FoldedPartition {
            clamped_fields: 0,
            ..folded
        };
        let snapshot = encode_live_snapshot(&clean, &header);
        let contents = segment::scan_segment(&snapshot).expect("snapshot parses");
        assert!(
            !contents
                .blocks
                .iter()
                .any(|b| b.kind == BlockKind::Marker as u8),
            "no marker without clamping"
        );
    }
}
