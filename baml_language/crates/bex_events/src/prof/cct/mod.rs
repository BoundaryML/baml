//! Target-neutral, raw-record calling-context-tree aggregation.
//!
//! The native consumer and cooperative drain both feed this engine after
//! ring drain. It owns no filesystem handles and emits only in-memory dirty
//! window rows; durable BCCT segments are a later storage layer.

mod delta;
mod engine;
mod nodes;
mod spawn;
mod stacks;

pub use delta::{LlmCounters, LlmDelta, NodeDelta, NodeHistogramDelta, SpawnDelta, WindowDelta};
pub use engine::{
    CctEvent, CctHealth, CctSnapshot, DEFAULT_WINDOW_NS, DEFER_MAX_SWEEPS, EngineCct, LlmMetaFlags,
    LlmSnapshot,
};
pub use nodes::{
    Histogram, NODE_FLAG_PARTITION_ROOT, NODE_FLAG_RECURSION_FOLD, NODE_FLAG_UNATTRIBUTABLE,
    NodeCounters, NodeId, NodeIdentity, NodeSnapshot,
};
pub use spawn::{
    EXCEPTIONAL_INSTANCE_LIMIT, FIRST_INSTANCE_LIMIT, SpawnCounters, SpawnEdgeId,
    SpawnEdgeIdentity, SpawnEdgeSnapshot, SpawnInstance,
};
pub use stacks::{RECENT_CALL_CAPACITY, RecentCall};
