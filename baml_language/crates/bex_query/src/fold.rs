use std::collections::{BTreeMap, BTreeSet};

use bex_events::prof::storage::{BlockRows, CctDeltaRow, MarkerKind, NodeBirthRow, SegmentState};

use crate::{
    BcctScan, CaptureLoss, Completeness, Counters, FoldedCct, FoldedNode, FoldedSpawnEdge,
    LlmCounters, QueryError, Watermark, WindowDelta,
};

pub fn fold_bcct(scans: &[BcctScan], partition_id: Option<u32>) -> Result<FoldedCct, QueryError> {
    if scans.is_empty() {
        return Err(QueryError::invalid_request(
            "at least one BCCT segment is required",
        ));
    }
    let mut ordered = scans.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|scan| {
        (
            scan.header.started_epoch_ns,
            scan.header.engine_id,
            scan.header.session_seg_seq,
        )
    });
    let mut result = FoldedCct {
        partition_id,
        meta: Completeness::from_scans(scans),
        ..FoldedCct::default()
    };

    let mut births = BTreeMap::<u32, NodeBirthRow>::new();
    for scan in &ordered {
        if matches!(scan.state, SegmentState::Torn) {
            result.meta.warnings.push(format!(
                "file {} has a torn tail; parsed through byte {}",
                scan.file.0, scan.committed_len
            ));
        }
        for block in &scan.blocks {
            if let BlockRows::NodeBirth(rows) = block.decode_rows()? {
                for row in rows {
                    match births.get(&row.node_id) {
                        Some(existing) if *existing != row => {
                            return Err(QueryError::invalid_data(format!(
                                "node {} has conflicting birth rows",
                                row.node_id
                            )));
                        }
                        _ => {
                            births.insert(row.node_id, row);
                        }
                    }
                }
            }
        }
    }
    let selected = births
        .values()
        .filter(|birth| partition_id.is_none_or(|partition| birth.partition_id == partition))
        .map(|birth| birth.node_id)
        .collect::<BTreeSet<_>>();
    for birth in births
        .values()
        .filter(|birth| selected.contains(&birth.node_id))
    {
        result.nodes.insert(
            birth.node_id,
            FoldedNode {
                node_id: birth.node_id,
                parent_node_id: birth.parent_node_id,
                function_id: birth.function_id,
                logical_thread_id: birth.logical_thread_id,
                partition_id: birth.partition_id,
                ..FoldedNode::default()
            },
        );
    }

    for scan in ordered {
        for block in &scan.blocks {
            result.first_ts_ns = nonzero_min(result.first_ts_ns, block.header.first_ts_ns);
            result.last_ts_ns = nonzero_max(result.last_ts_ns, block.header.last_ts_ns);
            match block.decode_rows()? {
                BlockRows::CctDelta(rows) => {
                    for row in rows {
                        if let Some(node) = result.nodes.get_mut(&row.node_id) {
                            node.counters.add_delta(row);
                            result.windows.push(WindowDelta {
                                first_ts_ns: block.header.first_ts_ns,
                                last_ts_ns: block.header.last_ts_ns,
                                node_id: row.node_id,
                                counters: counters_from_delta(row),
                            });
                        }
                    }
                }
                BlockRows::NodeTotal(rows) => {
                    let mut absolute = BTreeMap::<u32, Counters>::new();
                    for row in rows {
                        if result.nodes.contains_key(&row.node_id) {
                            absolute.entry(row.node_id).or_default().add_delta(row);
                        }
                    }
                    for (node_id, counters) in absolute {
                        if let Some(node) = result.nodes.get_mut(&node_id) {
                            node.counters = counters;
                        }
                    }
                }
                BlockRows::CctHistogram(rows) => {
                    for row in rows {
                        if let Some(node) = result.nodes.get_mut(&row.node_id) {
                            for (total, delta) in
                                node.duration_buckets.iter_mut().zip(row.duration_buckets)
                            {
                                *total = total.saturating_add(u64::from(delta));
                            }
                        }
                    }
                }
                BlockRows::LlmDelta(rows) => {
                    for row in rows {
                        if let Some(node) = result.nodes.get_mut(&row.node_id) {
                            add_llm(
                                &mut node.llm,
                                LlmCounters {
                                    calls: u64::from(row.llm_calls_delta),
                                    tokens_in: row.tokens_in_delta,
                                    tokens_out: row.tokens_out_delta,
                                    provider_errors: u64::from(row.provider_errs_delta),
                                    parse_errors: u64::from(row.parse_errs_delta),
                                },
                            );
                        }
                    }
                }
                BlockRows::SpawnEdge(rows) => {
                    for row in rows {
                        if !selected.contains(&row.parent_node)
                            && !selected.contains(&row.child_root_node)
                        {
                            continue;
                        }
                        let edge = result.spawn_edges.entry(row.edge_id).or_insert_with(|| {
                            FoldedSpawnEdge {
                                edge_id: row.edge_id,
                                parent_node: row.parent_node,
                                entry_fn: row.entry_fn,
                                child_root_node: row.child_root_node,
                                ..FoldedSpawnEdge::default()
                            }
                        });
                        if edge.parent_node != row.parent_node
                            || edge.entry_fn != row.entry_fn
                            || edge.child_root_node != row.child_root_node
                        {
                            return Err(QueryError::invalid_data(format!(
                                "spawn edge {} changes identity",
                                row.edge_id
                            )));
                        }
                        edge.spawns = edge.spawns.saturating_add(u64::from(row.spawn_delta));
                        edge.completed = edge
                            .completed
                            .saturating_add(u64::from(row.completed_delta));
                        edge.errored = edge.errored.saturating_add(u64::from(row.errored_delta));
                        edge.cancelled = edge
                            .cancelled
                            .saturating_add(u64::from(row.cancelled_delta));
                        edge.running_ns = edge.running_ns.saturating_add(row.running_ns_delta);
                        edge.awaiting_ns = edge.awaiting_ns.saturating_add(row.awaiting_ns_delta);
                    }
                }
                BlockRows::Watermark(rows) => {
                    result
                        .meta
                        .watermarks
                        .extend(rows.into_iter().map(|row| Watermark {
                            wall_epoch_ns: row.wall_epoch_ns,
                            drained_through_ts_ns: row.drained_through_ts_ns,
                            events_drained: row.events_drained,
                            durable_kind: row.durable_kind,
                            reason: row.reason,
                        }));
                }
                BlockRows::Marker(rows) => {
                    for row in rows {
                        if matches!(
                            row.kind,
                            MarkerKind::Loss
                                | MarkerKind::Degraded
                                | MarkerKind::Shed
                                | MarkerKind::BudgetExhausted
                        ) {
                            result.meta.capture_loss.push(CaptureLoss {
                                kind: row.kind,
                                timestamp_ns: row.timestamp_ns,
                                node_id: row.node_id,
                                count: row.count,
                                message: row.message,
                            });
                        }
                    }
                }
                BlockRows::ModelBirth(rows) => {
                    for row in rows {
                        if let Some(existing) = result.models.insert(row.model_id, row.name.clone())
                            && existing != row.name
                        {
                            return Err(QueryError::invalid_data(format!(
                                "model {} changes name",
                                row.model_id
                            )));
                        }
                    }
                }
                BlockRows::Instance(rows) => {
                    result.instances.extend(rows.into_iter().filter(|row| {
                        result.spawn_edges.get(&row.edge_id).is_some_and(|edge| {
                            selected.contains(&edge.parent_node)
                                || selected.contains(&edge.child_root_node)
                        })
                    }));
                }
                BlockRows::Opaque { kind, .. } => result
                    .meta
                    .warnings
                    .push(format!("skipped unknown committed block kind {kind}")),
                BlockRows::NodeBirth(_)
                | BlockRows::PartitionBind(_)
                | BlockRows::FooterIndex(_)
                | BlockRows::Reserved7(_) => {}
            }
        }
    }
    for node in result.nodes.values() {
        if node.parent_node_id != 0 && !result.nodes.contains_key(&node.parent_node_id) {
            result.meta.warnings.push(format!(
                "node {} references unavailable parent {}",
                node.node_id, node.parent_node_id
            ));
        }
    }
    result.meta.watermarks.sort_by_key(|row| {
        (
            row.drained_through_ts_ns,
            row.wall_epoch_ns,
            row.events_drained,
        )
    });
    result.meta.finalize();
    Ok(result)
}

fn counters_from_delta(row: CctDeltaRow) -> Counters {
    let mut counters = Counters::default();
    counters.add_delta(row);
    counters
}

fn add_llm(total: &mut LlmCounters, delta: LlmCounters) {
    total.calls = total.calls.saturating_add(delta.calls);
    total.tokens_in = total.tokens_in.saturating_add(delta.tokens_in);
    total.tokens_out = total.tokens_out.saturating_add(delta.tokens_out);
    total.provider_errors = total.provider_errors.saturating_add(delta.provider_errors);
    total.parse_errors = total.parse_errors.saturating_add(delta.parse_errors);
}

fn nonzero_min(current: Option<u64>, value: u64) -> Option<u64> {
    if value == 0 {
        current
    } else {
        Some(current.map_or(value, |existing| existing.min(value)))
    }
}

fn nonzero_max(current: Option<u64>, value: u64) -> Option<u64> {
    if value == 0 {
        current
    } else {
        Some(current.map_or(value, |existing| existing.max(value)))
    }
}
