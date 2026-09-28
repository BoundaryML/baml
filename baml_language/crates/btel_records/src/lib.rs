//! Two disjoint in-process record vocabularies, constructed directly by producers.
//!
//! Timing records contain no invocation IDs or capture payloads. Span records
//! carry identities, definitions and explicit capture ownership handles. These
//! are Rust storage layouts, not a durable serialization format.
//!
//! Production captures use pointer-sized `btel_snapshot::Snapshot` owners.
//! They contain no VM pointers and move across threads without copying payloads.
//! Neither record enum is Clone: publication transfers ownership.

use std::sync::Arc;

use btel_clock::ClockEpoch;
use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, ClockInstant, FunctionId, InvocationOutcome,
    TelemetryId,
};

/// Frequent anonymous measurements with a 32-byte slot budget.
/// Records transfer ownership rather than implicitly duplicating publication.
///
/// ```compile_fail
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<btel_records::TimingRecord>();
/// ```
#[derive(Debug)]
pub enum TimingRecord {
    /// Ring-local thread context without adding an ID to every timing slot.
    ThreadSelected { thread_id: TelemetryId },
    /// Common anonymous completion; its layout determines the hot slot size.
    FunctionTimingCompletion {
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        outcome: InvocationOutcome,
        reentry: bool,
    },
}

/// Less frequent identified spans, capture handles, and supporting definitions.
///
/// Production capture handles own pooled snapshot storage directly, with no
/// additional box around the handle. The generic parameters support ownership
/// probes in tests; asynchronous records must never contain VM-backed values.
/// Even cloneable capture handles do not make records cloneable:
///
/// ```compile_fail
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<btel_records::SpanRecord<u8, u8>>();
/// ```
///
/// These larger variants share a 56-byte slot; ordinary timing completions must
/// stay in `TimingRecord` so they do not pay this stride or compete for its cache.
#[derive(Debug)]
pub enum SpanRecord<InputCapture, ValueCapture> {
    /// This ring needs its own thread context; the Timing ring's is independent.
    ThreadSelected { thread_id: TelemetryId },
    /// Optional early thread identity; its retained clock exceeds a Timing slot.
    ThreadSpanAnnouncement {
        id: TelemetryId,
        parent_id: Option<TelemetryId>,
        spawn_call_path: CallPathId,
        started_at: ClockInstant,
        clock: Arc<ClockEpoch>,
    },
    /// Completes an identified thread node; clock ownership keeps it here.
    ThreadSpanCompletion {
        id: TelemetryId,
        parent_id: Option<TelemetryId>,
        spawn_call_path: CallPathId,
        started_at: ClockInstant,
        completed_at: ClockInstant,
        outcome: InvocationOutcome,
        clock: Arc<ClockEpoch>,
    },
    /// Entry identity and owned inputs exceed the Timing slot budget.
    FunctionSpanAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        captured_inputs: Option<InputCapture>,
    },
    /// Completion variants specialize outcome, reentry and announcement dependency.
    /// This is an in-process dispatch choice; Rust discriminants are not wire IDs.
    /// Payload ownership and the 56-byte slot are unchanged.
    FunctionSpanCompletionOk {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionOkNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionOkReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionOkReentryNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionErrored {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionErroredNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionErroredReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionErroredReentryNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionCancelled {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionCancelledNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionCancelledReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    /// Exit-promoted identity and capture need the larger Span slot.
    LateFunctionSpanCompletionOk {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    LateFunctionSpanCompletionOkReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    LateFunctionSpanCompletionErrored {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    LateFunctionSpanCompletionErroredReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    LateFunctionSpanCompletionCancelled {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    LateFunctionSpanCompletionCancelledReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<ValueCapture>,
    },
    /// Rare definition stays off the frequent Timing ring, even if it fits.
    CallPathDefined {
        call_path: CallPathId,
        parent_call_path: CallPathId,
        visible_caller: Option<FunctionId>,
        caller_pc: u32,
        callee: FunctionId,
        edge: CallPathEdge,
    },
    /// Exception path only: how the raise that the next `ErrorRaised` on this
    /// thread starts relates to earlier ones. Absent for a fresh raise.
    ErrorRaiseOrigin {
        raise_id: TelemetryId,
        origin: RaiseOrigin,
    },
    /// Exception path only, rare: evidence of the next `ErrorRaised` on this
    /// thread that does not fit its slot. Boxed so it never widens the slot.
    ErrorRaiseStack(Box<RaiseStack>),
    /// Exception path only: unwinding started. Completions until the matching
    /// end belong to it. Fixed size: every raise has one, so it allocates
    /// nothing. Its stack is `call_path` and that path's callers.
    ErrorRaised {
        raise_id: TelemetryId,
        raised_at: ClockInstant,
        kind: RaiseKind,
        /// Innermost bytecode frame and its live PC (compact byte offset).
        function: Option<FunctionId>,
        pc: Option<u32>,
        /// That frame's retained call, when it was a span at raise time.
        raise_call: Option<TelemetryId>,
        /// That frame was timing-only and its function has a policy, so its
        /// completion may promote it: an `ErrorRaiseFrameCompleted` marker
        /// then follows the completion.
        raise_frame_may_promote: bool,
        /// The raise frame's call path when every bytecode frame on the
        /// thread was observed: the path and its callers are the stack.
        /// `ROOT` otherwise; a preceding `ErrorRaiseStack` then lists it.
        call_path: CallPathId,
        /// Frames on the stack, native ones included.
        frame_count: u32,
    },
    /// Exception path only: a raise whose unwind completed no retained call,
    /// so nothing had to sit between its start and its end. Stands for
    /// `ErrorRaised` then `ErrorUnwindEnded` in one slot; the raise frame
    /// was not a span and could not be promoted. PCs are `NO_PC` when absent.
    ErrorRaisedAndEnded {
        raise_id: TelemetryId,
        raised_at: ClockInstant,
        kind: RaiseKind,
        function: Option<FunctionId>,
        pc: u32,
        call_path: CallPathId,
        frame_count: u32,
        result: UnwindResult,
        handler_function: Option<FunctionId>,
        handler_pc: u32,
        unwound_frames: u32,
    },
    /// Exception path only: the raise frame, which was timing-only and could
    /// be promoted, has completed. A late completion just before this marker
    /// is that frame's call.
    ErrorRaiseFrameCompleted { raise_id: TelemetryId },
    /// Exception path only: unwinding stopped.
    ErrorUnwindEnded {
        raise_id: TelemetryId,
        result: UnwindResult,
        handler_function: Option<FunctionId>,
        handler_pc: Option<u32>,
        unwound_frames: u32,
    },
}

/// An absent PC in `SpanRecord::ErrorRaisedAndEnded`. Compact code never
/// reaches it; larger PCs already saturate to it.
pub const NO_PC: u32 = u32::MAX;

/// A raise's evidence that does not fit its fixed-size record. Owned and
/// bounded; contains no VM pointers.
#[derive(Debug)]
pub struct RaiseStack {
    pub raise_id: TelemetryId,
    /// Live stack, innermost first, at most `MAX_ERROR_FRAMES`, when the
    /// raise has no call path: some bytecode frame was not observed, or no
    /// bytecode frame raised. Empty when the call path describes it.
    pub frames: Vec<ErrorFrame>,
    /// Diagnostic trace carried from an earlier throw, only when the origin
    /// is not proven. At most `MAX_INHERITED_FRAMES`, strings clipped.
    pub inherited: Vec<InheritedFrame>,
    pub inherited_count: u32,
}

/// Bounds for one raise's owned evidence.
pub const MAX_ERROR_FRAMES: usize = 64;
pub const MAX_INHERITED_FRAMES: usize = 16;
pub const MAX_INHERITED_TEXT_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaiseKind {
    Throw,
    Rethrow,
    PanicRethrow,
    Await,
    AwaitCancelled,
    NativeBoundary,
    HostBoundary,
    Runtime,
}

/// How a raise relates to earlier ones. Never derived from value equality
/// alone; see the VM's landing notes and future links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaiseOrigin {
    Fresh,
    Proven {
        origin: TelemetryId,
        previous: Option<TelemetryId>,
        via: OriginVia,
    },
    Ambiguous {
        candidates: u32,
        via: OriginVia,
    },
    Unresolved {
        reason: UnresolvedOrigin,
        via: OriginVia,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginVia {
    Rethrow,
    Await,
    Normalization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnresolvedOrigin {
    NoLanding,
    LandingsEvicted,
    FutureLinkMissing,
    FutureLinkEvicted,
    NoFuture,
    /// `UnknownError` conversion: the VM does not record where the converted
    /// value came from, so an equal value in a catch slot proves nothing.
    SourceNotRecorded,
}

/// An occurrence's identity as carried between raises: its first raise, or
/// why it is not known. Plain IDs, so it can cross threads and GC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginEvidence {
    Known(TelemetryId),
    Ambiguous(u32),
    Unresolved(UnresolvedOrigin),
}

/// A failed future's escaping raise, stored before its awaiters wake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FutureErrorLink {
    pub raise: TelemetryId,
    pub origin: OriginEvidence,
}

/// Looking up the raise that failed a future.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FutureErrorLookup {
    Found(FutureErrorLink),
    Missing,
    /// No entry, and an entry with this key or an older one was evicted.
    PossiblyEvicted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorFrame {
    pub function: Option<FunctionId>,
    /// Innermost: live PC; outer: call-site PC. None for native frames.
    pub pc: Option<u32>,
    pub native: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InheritedFrame {
    pub function_name: String,
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnwindResult {
    Caught,
    /// Escaped the thread's outermost frame.
    Unhandled,
    /// Returned to a native caller without a bytecode handler.
    EscapedToNative,
    /// The unwinder stopped with an internal error.
    Aborted,
}

const _: () =
    assert!(std::mem::size_of::<TimingRecord>() <= btel_settings::layout::TIMING_RECORD_MAX_BYTES);
const _: () = assert!(
    std::mem::size_of::<SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot>>()
        <= btel_settings::layout::SPAN_RECORD_MAX_BYTES
);

#[cfg(test)]
mod layout_tests {
    use std::mem::{align_of, size_of};

    use super::*;

    /// A raise fits the span slot inline and rare evidence is boxed: the
    /// slot keeps the exact size and alignment it had before error records
    /// existed.
    #[test]
    fn error_variants_keep_the_span_slot() {
        type Span = SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot>;
        assert_eq!(size_of::<Span>(), 56);
        assert_eq!(align_of::<Span>(), 8);
        assert_eq!(size_of::<TimingRecord>(), 32);
        assert_eq!(size_of::<Box<RaiseStack>>(), 8);
    }
}

// Independently owned captures and retained clock epochs support cross-thread
// consumption. This does not make a VM-backed payload safe to transfer.
const _: fn() = || {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<TimingRecord>();
    send_sync::<SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot>>();
};

/// Borrowed semantic view for inspection; the hot processor dispatches directly.
#[derive(Debug)]
pub struct FunctionCompletionRef<'a, V> {
    pub id: TelemetryId,
    pub parent_id: TelemetryId,
    pub call_path: CallPathId,
    pub entered_at: ClockInstant,
    pub exited_at: ClockInstant,
    pub await_time: AwaitDuration,
    pub outcome: InvocationOutcome,
    pub reentry: bool,
    pub requires_announcement: bool,
    pub late: bool,
    pub captured_value: Option<&'a V>,
}
impl<I, V> SpanRecord<I, V> {
    pub fn completion(&self) -> Option<FunctionCompletionRef<'_, V>> {
        match self {
            Self::FunctionSpanCompletionOk {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Ok,
                reentry: false,
                requires_announcement: false,
                late: false,
            }),
            Self::FunctionSpanCompletionOkNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Ok,
                reentry: false,
                requires_announcement: true,
                late: false,
            }),
            Self::FunctionSpanCompletionOkReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Ok,
                reentry: true,
                requires_announcement: false,
                late: false,
            }),
            Self::FunctionSpanCompletionOkReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Ok,
                reentry: true,
                requires_announcement: true,
                late: false,
            }),
            Self::FunctionSpanCompletionErrored {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Errored,
                reentry: false,
                requires_announcement: false,
                late: false,
            }),
            Self::FunctionSpanCompletionErroredNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Errored,
                reentry: false,
                requires_announcement: true,
                late: false,
            }),
            Self::FunctionSpanCompletionErroredReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Errored,
                reentry: true,
                requires_announcement: false,
                late: false,
            }),
            Self::FunctionSpanCompletionErroredReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Errored,
                reentry: true,
                requires_announcement: true,
                late: false,
            }),
            Self::FunctionSpanCompletionCancelled {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: false,
                requires_announcement: false,
                late: false,
            }),
            Self::FunctionSpanCompletionCancelledNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: false,
                requires_announcement: true,
                late: false,
            }),
            Self::FunctionSpanCompletionCancelledReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: true,
                requires_announcement: false,
                late: false,
            }),
            Self::FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: true,
                requires_announcement: true,
                late: false,
            }),
            Self::LateFunctionSpanCompletionOk {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Ok,
                reentry: false,
                requires_announcement: false,
                late: true,
            }),
            Self::LateFunctionSpanCompletionOkReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Ok,
                reentry: true,
                requires_announcement: false,
                late: true,
            }),
            Self::LateFunctionSpanCompletionErrored {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Errored,
                reentry: false,
                requires_announcement: false,
                late: true,
            }),
            Self::LateFunctionSpanCompletionErroredReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Errored,
                reentry: true,
                requires_announcement: false,
                late: true,
            }),
            Self::LateFunctionSpanCompletionCancelled {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: false,
                requires_announcement: false,
                late: true,
            }),
            Self::LateFunctionSpanCompletionCancelledReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => Some(FunctionCompletionRef {
                id: *id,
                parent_id: *parent_id,
                call_path: *call_path,
                entered_at: *entered_at,
                exited_at: *exited_at,
                await_time: *await_time,
                captured_value: captured_value.as_ref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: true,
                requires_announcement: false,
                late: true,
            }),
            _ => None,
        }
    }
}

impl<C> SpanRecord<C, C> {
    /// Transfer the exclusive capture owner; record destruction handles everything else.
    pub fn take_capture(&mut self) -> Option<C> {
        match self {
            Self::FunctionSpanAnnouncement {
                captured_inputs, ..
            } => captured_inputs.take(),
            Self::FunctionSpanCompletionOk { captured_value, .. }
            | Self::FunctionSpanCompletionOkNeedsAnnouncement { captured_value, .. }
            | Self::FunctionSpanCompletionOkReentry { captured_value, .. }
            | Self::FunctionSpanCompletionOkReentryNeedsAnnouncement { captured_value, .. }
            | Self::FunctionSpanCompletionErrored { captured_value, .. }
            | Self::FunctionSpanCompletionErroredNeedsAnnouncement { captured_value, .. }
            | Self::FunctionSpanCompletionErroredReentry { captured_value, .. }
            | Self::FunctionSpanCompletionErroredReentryNeedsAnnouncement {
                captured_value, ..
            }
            | Self::FunctionSpanCompletionCancelled { captured_value, .. }
            | Self::FunctionSpanCompletionCancelledNeedsAnnouncement { captured_value, .. }
            | Self::FunctionSpanCompletionCancelledReentry { captured_value, .. }
            | Self::FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
                captured_value,
                ..
            }
            | Self::LateFunctionSpanCompletionOk { captured_value, .. }
            | Self::LateFunctionSpanCompletionOkReentry { captured_value, .. }
            | Self::LateFunctionSpanCompletionErrored { captured_value, .. }
            | Self::LateFunctionSpanCompletionErroredReentry { captured_value, .. }
            | Self::LateFunctionSpanCompletionCancelled { captured_value, .. }
            | Self::LateFunctionSpanCompletionCancelledReentry { captured_value, .. } => {
                captured_value.take()
            }
            Self::ThreadSelected { .. }
            | Self::ThreadSpanAnnouncement { .. }
            | Self::ThreadSpanCompletion { .. }
            | Self::CallPathDefined { .. }
            | Self::ErrorRaiseOrigin { .. }
            | Self::ErrorRaiseStack(_)
            | Self::ErrorRaised { .. }
            | Self::ErrorRaisedAndEnded { .. }
            | Self::ErrorRaiseFrameCompleted { .. }
            | Self::ErrorUnwindEnded { .. } => None,
        }
    }
}
