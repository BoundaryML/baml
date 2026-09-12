//! Owned output: no references into transport buffers survive a builder call.
use btel_core::{
    ids::{BexCallId, BexThreadId, FunctionId},
    marker::{CallSiteSourceSpan, FunctionEndStatus, ThreadEndStatus},
};

/// Identity within one engine. A builder must never mix engines/processes.
/// OS source IDs are provenance, not identity: logical calls can migrate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallKey {
    pub thread_id: BexThreadId,
    pub call_id: BexCallId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanStart {
    pub source_id: u64,
    pub flags: u8,
    pub parent_call_id: BexCallId,
    pub function_id: FunctionId,
    pub call_site: Option<CallSiteSourceSpan>,
    pub ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanEnd {
    pub source_id: u64,
    pub status: FunctionEndStatus,
    pub ticks: u64,
    pub await_ns: u64,
    pub await_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedSpan {
    pub key: CallKey,
    pub start: SpanStart,
    pub end: SpanEnd,
}

impl ClosedSpan {
    /// Preserve raw ticks. Inverted clocks are unknown duration, never wrapped
    /// or silently clamped. Tick-to-time conversion belongs downstream.
    pub fn duration_ticks(&self) -> Option<u64> {
        self.end.ticks.checked_sub(self.start.ticks)
    }
}

/// An unmatched half, retained until its counterpart arrives (no eviction yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenSpan {
    Start(SpanStart),
    End(SpanEnd),
}

/// Current non-call markers are standalone facts, not closure barriers.
/// Late ID updates remain separate events; already-emitted spans aren't held
/// waiting for metadata. Logs can follow this same immediate-output path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelemetryEvent {
    ClosedSpan(ClosedSpan),
    /// Returned at input shutdown by the generic stage adapter; not a closed span.
    UnmatchedSpan {
        key: CallKey,
        half: OpenSpan,
    },
    ThreadStarted {
        source_id: u64,
        flags: u8,
        thread_id: BexThreadId,
        parent_thread_id: BexThreadId,
        parent_call_id: BexCallId,
        ticks: u64,
        /// None for the root encoding; Some(None) for a spawn with no site.
        spawn_site: Option<Option<CallSiteSourceSpan>>,
        name: Vec<u8>,
    },
    ThreadEnded {
        source_id: u64,
        thread_id: BexThreadId,
        ticks: u64,
        status: ThreadEndStatus,
    },
    BoundaryLocalId {
        source_id: u64,
        key: CallKey,
        ticks: u64,
        id: [u8; 16],
    },
}
