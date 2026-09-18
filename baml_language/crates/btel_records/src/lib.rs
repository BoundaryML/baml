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
    /// Identified completion with optional owned output/error capture.
    FunctionSpanCompletion {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        outcome: InvocationOutcome,
        reentry: bool,
        captured_value: Option<Box<ValueCapture>>,
    },
    /// Exit-promoted identity and capture need the larger Span slot.
    LateFunctionSpanCompletion {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        outcome: InvocationOutcome,
        reentry: bool,
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
