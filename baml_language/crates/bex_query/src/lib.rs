//! Bounded, sans-I/O queries over BCCT observability data.

mod advanced_views;
mod bqf;
pub mod bql;
mod clickhouse;
mod engine;
mod error;
mod fold;
mod live;
mod model;
mod observe;
mod range_host;
mod runs;
mod scan;
mod source;
mod value_inspector;
mod views;

pub use advanced_views::{
    DiffPresence, DiffRequest, DiffResponse, DiffRow, FunctionDictionary, FunctionIdentity,
    SandwichDirection, SandwichRequest, SandwichResponse, SandwichRow, SearchRequest,
    SearchResponse, SearchRow, SignedCounters, diff_cct, sandwich, search_functions,
};
pub use bqf::{
    BQF_COLUMN_DIRECTORY_LEN, BQF_CRC_LEN, BQF_HEADER_LEN, BQF_VERSION, BqfBuilder, BqfFrame,
    Column, ColumnDirectory, ColumnType, FrameFlags, FrameHeader, FrameKind,
};
pub use clickhouse::{
    ClickHouseParam, ClickHouseParamType, CompiledClickHouseQuery, compile_clickhouse,
};
#[cfg(feature = "native")]
pub use engine::NativeRun;
pub use engine::{NATIVE_CACHE_BYTES, QueryEngine, WASM_CACHE_BYTES};
pub use error::{BqlDiagnostic, QueryError};
pub use fold::fold_bcct;
pub use live::{LiveFrameGate, LiveFrameOffer, MAX_LIVE_RATE_HZ};
pub use model::{
    CaptureLoss, Completeness, Counters, FoldedCct, FoldedNode, FoldedSpawnEdge, LlmCounters,
    SourceWatermark, Watermark, WindowDelta,
};
pub use observe::{ObserveEngine, ObservePoll};
pub use range_host::{HttpFile, HttpRangeRequest, HttpRangeResponse, HttpRangeSource};
pub use runs::{
    ListRunsRequest, ProcessLiveness, RunCursor, RunListing, RunMeta, RunMetaRecord, RunState,
    RunSummary, SessionMeta,
};
#[cfg(feature = "native")]
pub use runs::{list_runs, open_run_meta, open_run_meta_pinned};
pub use scan::{BcctScan, scan_bcct};
#[cfg(feature = "native")]
pub use source::FileSource;
pub use source::{
    ByteBudgetCache, ByteRange, ByteSource, ByteView, FileId, LiveMirrorSource, MemorySource,
    RangeCacheSource, SourceSnapshot,
};
pub use value_inspector::{
    HydratedValueNode, InspectorAvailability, StoredValueChunk, ValueCallKey, ValueChunkSource,
    ValueDagRowKind, ValueDiffNode, ValueDiffResponse, ValueHydration, ValueLossRow, ValueRefRow,
    ValueRefsRequest, ValueRefsResponse, diff_values, hydrate_value, inspect_value_file,
};
#[cfg(feature = "native")]
pub use value_inspector::{NativeValueStore, list_value_refs};
pub use views::{
    DEFAULT_MAX_BYTES, ExactTimelineCall, ExactTimelineTier, HARD_MAX_BYTES, LeftHeavyNode,
    LeftHeavyRequest, LeftHeavyResponse, MAX_LANES, MAX_PIXEL_WIDTH, TimelineBand, TimelineLane,
    TimelineOverlay, TimelineRect, TimelineResponse, TimelineTiers, Viewport, left_heavy, timeline,
    timeline_with_overlay,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryPoll<T> {
    Ready(T),
    NeedData { ranges: Vec<ByteRange> },
}

impl<T> QueryPoll<T> {
    #[must_use]
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> QueryPoll<U> {
        match self {
            Self::Ready(value) => QueryPoll::Ready(map(value)),
            Self::NeedData { ranges } => QueryPoll::NeedData { ranges },
        }
    }
}
