//! Two disjoint in-process record vocabularies, constructed directly by producers.
//!
//! Timing records contain no invocation IDs or capture payloads. Span records
//! carry identities, definitions and explicit capture ownership handles. These
//! are Rust storage layouts, not a durable serialization format.
//!
//! Capture ownership is supplied by the producer: VM-local captures must remain
//! local and GC-visible; asynchronous consumers require independently owned data.
//! VM-backed handles can migrate with their owning VM, so `Send` alone does not
//! prove independent ownership. The processor must use owned snapshot handles;
//! that implementation can reuse these enums without a second vocabulary.
//! Neither record enum is Clone: publication transfers ownership.

use std::sync::Arc;

use btel_clock::ClockEpoch;
use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, ClockInstant, FunctionId, InvocationOutcome,
    TelemetryId,
};

/// Capture was requested, but an independent value snapshot is not implemented
/// yet. This explicit placeholder contains no VM reference and must never be
/// presented as successfully captured data. `None` still means no capture.
#[derive(Debug)]
pub struct CaptureDeferred;

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
/// Captures are boxed so their contents cannot inflate the slot. Inputs may be
/// a slice; completion payloads are sized, keeping their ownership handle thin.
/// Boxing VM references does not make their referenced objects independently owned.
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
pub enum SpanRecord<InputCapture: ?Sized, ValueCapture> {
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
        captured_inputs: Option<Box<InputCapture>>,
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
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionOkNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionOkReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionOkReentryNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionErrored {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionErroredNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionErroredReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionErroredReentryNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionCancelled {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionCancelledNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionCancelledReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    /// Exit-promoted identity and capture need the larger Span slot.
    LateFunctionSpanCompletionOk {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    LateFunctionSpanCompletionOkReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    LateFunctionSpanCompletionErrored {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    LateFunctionSpanCompletionErroredReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    LateFunctionSpanCompletionCancelled {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
    },
    LateFunctionSpanCompletionCancelledReentry {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        captured_value: Option<Box<ValueCapture>>,
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
}

const _: () = assert!(std::mem::size_of::<TimingRecord>() <= 32);
const _: () = assert!(std::mem::size_of::<SpanRecord<[()], ()>>() <= 56);

// Independently owned captures and retained clock epochs support cross-thread
// consumption. This does not make a VM-backed payload safe to transfer.
const _: fn() = || {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<TimingRecord>();
    send_sync::<SpanRecord<[u8], Vec<u8>>>();
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
impl<I: ?Sized, V> SpanRecord<I, V> {
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
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
                captured_value: captured_value.as_deref(),
                outcome: InvocationOutcome::Cancelled,
                reentry: true,
                requires_announcement: false,
                late: true,
            }),
            _ => None,
        }
    }
}
