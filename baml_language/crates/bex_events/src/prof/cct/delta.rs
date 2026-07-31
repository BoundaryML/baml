//! In-memory window output from the CCT aggregator.
//!
//! P2 deliberately stops at this target-neutral representation. P3 encodes
//! these rows into BCCT segment blocks without changing aggregation.

use super::{
    nodes::{Histogram, NodeCounters, NodeId},
    spawn::{SpawnCounters, SpawnEdgeId},
};

/// One dirty node's counter delta for a completed aggregation window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeDelta {
    pub node_id: NodeId,
    pub counters: NodeCounters,
}

/// One node's close-duration histogram for a completed window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeHistogramDelta {
    pub node_id: NodeId,
    pub buckets: Histogram,
}

/// Aggregated LLM usage for one `(calling-context, model)` pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LlmCounters {
    pub calls: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub provider_errs: u64,
    pub parse_errs: u64,
    pub retries: u64,
}

impl LlmCounters {
    pub(crate) fn is_zero(self) -> bool {
        self == Self::default()
    }

    pub(crate) fn add_meta(
        &mut self,
        tokens_in: u32,
        tokens_out: u32,
        provider_error: bool,
        parse_error: bool,
        retry: bool,
    ) {
        self.calls = self.calls.saturating_add(1);
        self.tokens_in = self.tokens_in.saturating_add(u64::from(tokens_in));
        self.tokens_out = self.tokens_out.saturating_add(u64::from(tokens_out));
        self.provider_errs = self.provider_errs.saturating_add(u64::from(provider_error));
        self.parse_errs = self.parse_errs.saturating_add(u64::from(parse_error));
        self.retries = self.retries.saturating_add(u64::from(retry));
    }
}

/// One dirty LLM side-table row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmDelta {
    pub node_id: NodeId,
    pub model_id: u32,
    pub counters: LlmCounters,
}

/// One dirty spawn edge's counter delta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnDelta {
    pub edge_id: SpawnEdgeId,
    pub counters: SpawnCounters,
}

/// All non-empty rows closed at one CCT window boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDelta {
    pub start_ns: u64,
    pub end_ns: u64,
    pub nodes: Vec<NodeDelta>,
    pub histograms: Vec<NodeHistogramDelta>,
    pub llm: Vec<LlmDelta>,
    pub spawn: Vec<SpawnDelta>,
}

impl WindowDelta {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.histograms.is_empty()
            && self.llm.is_empty()
            && self.spawn.is_empty()
    }
}
