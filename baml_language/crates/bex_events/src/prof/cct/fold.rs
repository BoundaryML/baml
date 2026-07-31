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
        #[expect(
            clippy::cast_possible_truncation,
            reason = "fold counts saturate u32 deliberately"
        )]
        totals.push(blocks::CctDeltaRow {
            node_id: new_id,
            enters: nodes.enters[i].min(u64::from(u32::MAX)) as u32,
            ends_ok: nodes.ends_ok[i].min(u64::from(u32::MAX)) as u32,
            ends_err: nodes.ends_err[i].min(u64::from(u32::MAX)) as u32,
            ends_cancel: nodes.ends_cancel[i].min(u64::from(u32::MAX)) as u32,
            ends_exit: nodes.ends_exit[i].min(u64::from(u32::MAX)) as u32,
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
            #[expect(
                clippy::cast_possible_truncation,
                reason = "fold counts saturate u32 deliberately"
            )]
            llm.push(blocks::LlmDeltaRow {
                node_id: new_id,
                llm_calls_delta: counters.llm_calls.min(u64::from(u32::MAX)) as u32,
                tokens_in_delta: counters.tokens_in,
                tokens_out_delta: counters.tokens_out,
                provider_errs_delta: counters.provider_errs.min(u64::from(u32::MAX)) as u32,
                parse_errs_delta: counters.parse_errs.min(u64::from(u32::MAX)) as u32,
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
            #[expect(
                clippy::cast_possible_truncation,
                reason = "fold counts saturate u32 deliberately"
            )]
            spawns.push(blocks::SpawnEdgeRow {
                edge_id: u32::try_from(spawns.len()).unwrap_or(u32::MAX),
                parent_node: parent_new,
                entry_fn: edges.entry_fn[edge],
                child_root_node: remap
                    .get(&edges.child_root_node[edge])
                    .copied()
                    .unwrap_or(u32::MAX),
                spawn_delta: counters.spawned.min(u64::from(u32::MAX)) as u32,
                completed_delta: counters.completed.min(u64::from(u32::MAX)) as u32,
                errored_delta: counters.errored.min(u64::from(u32::MAX)) as u32,
                cancelled_delta: counters.cancelled.min(u64::from(u32::MAX)) as u32,
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
}
