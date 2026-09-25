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
}

const _: () =
    assert!(std::mem::size_of::<TimingRecord>() <= btel_settings::layout::TIMING_RECORD_MAX_BYTES);
const _: () = assert!(
    std::mem::size_of::<SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot>>()
        <= btel_settings::layout::SPAN_RECORD_MAX_BYTES
);

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
            | Self::CallPathDefined { .. } => None,
        }
    }
}
