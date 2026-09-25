//! Exception evidence: raises, unwind ends, and the retained calls each raise
//! failed. Written to the file's error section, never to span sections, so a
//! bounded stack does not pass through the scalar span-event reservation.
//!
//! Linking relies on record order within one telemetry thread. The VM emits
//! a raise, then the completions of the frames it unwinds, then the end, all
//! in one synchronous step on one producer. Other threads' records can be
//! interleaved between them (other producers' chunks), so state is kept per
//! thread, never as one global "current error".
use btel_records::{
    OriginVia, RaiseKind, RaiseOrigin, RaiseStack, SpanRecord, UnresolvedOrigin, UnwindResult,
};
use btel_snapshot::Snapshot;
use btel_types::{CallPathId, FunctionId, TelemetryId};

use crate::{ConversionBuffer, proto};

struct ActiveUnwind {
    thread: TelemetryId,
    raise: TelemetryId,
    /// Waiting for the raise-frame marker to name a promoted raise frame.
    awaiting_raise_frame: bool,
    completions: u32,
    first_late: Option<TelemetryId>,
}

/// Evidence the VM emits just before a raise's fixed-size record, on the
/// same thread: a non-fresh origin, and rarely an explicit stack or an
/// inherited trace.
struct PendingRaise {
    thread: TelemetryId,
    raise: TelemetryId,
    origin: Option<RaiseOrigin>,
    frames: Vec<proto::ErrorFrame>,
    inherited: Vec<proto::InheritedFrame>,
    inherited_count: u32,
}

/// At most one unwind and one pending raise per telemetry thread; usually
/// empty.
#[derive(Default)]
pub(crate) struct ActiveUnwinds {
    active: Vec<ActiveUnwind>,
    pending: Vec<PendingRaise>,
}

impl ActiveUnwinds {
    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
    pub(crate) fn clear_thread(&mut self, thread: TelemetryId) {
        self.active.retain(|unwind| unwind.thread != thread);
        self.pending.retain(|pending| pending.thread != thread);
    }
    fn get_mut(&mut self, thread: TelemetryId) -> Option<&mut ActiveUnwind> {
        self.active
            .iter_mut()
            .find(|unwind| unwind.thread == thread)
    }
    /// The thread's pending evidence for `raise`, replacing any left over
    /// from a raise whose record never arrived.
    fn pending_mut(&mut self, thread: TelemetryId, raise: TelemetryId) -> &mut PendingRaise {
        let index = match self.pending.iter().position(|p| p.thread == thread) {
            Some(index) if self.pending[index].raise == raise => index,
            Some(index) => {
                self.pending[index] = PendingRaise::new(thread, raise);
                index
            }
            None => {
                self.pending.push(PendingRaise::new(thread, raise));
                self.pending.len() - 1
            }
        };
        &mut self.pending[index]
    }
    fn take_pending(&mut self, thread: TelemetryId, raise: TelemetryId) -> Option<PendingRaise> {
        let index = self.pending.iter().position(|p| p.thread == thread)?;
        let pending = self.pending.swap_remove(index);
        (pending.raise == raise).then_some(pending)
    }
}

impl PendingRaise {
    fn new(thread: TelemetryId, raise: TelemetryId) -> Self {
        Self {
            thread,
            raise,
            origin: None,
            frames: Vec::new(),
            inherited: Vec::new(),
            inherited_count: 0,
        }
    }
}

impl ConversionBuffer {
    /// Append one `ErrorBatch` field; repeated fields may interleave.
    fn push_error(&mut self, tag: u32, message: &impl prost::Message) {
        let before = self.pending.errors.len();
        prost::encoding::message::encode(tag, message, &mut self.pending.errors);
        self.pending_encoded_bytes = self
            .pending_encoded_bytes
            .saturating_add(self.pending.errors.len() - before);
    }

    fn link(&mut self, raise: TelemetryId, call: TelemetryId, role: proto::ErrorLinkRole) {
        self.push_error(
            2,
            &proto::ErrorCallLink {
                raise_id: raise.get(),
                call_id: call.get(),
                role: role as i32,
            },
        );
    }

    /// Every function completion on a thread with an active unwind was
    /// completed by that unwind.
    pub(crate) fn link_unwound(
        &mut self,
        thread: TelemetryId,
        record: &SpanRecord<Snapshot, Snapshot>,
    ) {
        let Some(done) = record.completion() else {
            return;
        };
        let Some(unwind) = self.unwinds.get_mut(thread) else {
            return;
        };
        unwind.completions = unwind.completions.saturating_add(1);
        if unwind.completions == 1 && done.late {
            unwind.first_late = Some(done.id);
        }
        let raise = unwind.raise;
        self.link(raise, done.id, proto::ErrorLinkRole::Unwound);
    }

    pub(crate) fn error_raise_origin(
        &mut self,
        thread: TelemetryId,
        raise: TelemetryId,
        origin: RaiseOrigin,
    ) {
        self.unwinds.pending_mut(thread, raise).origin = Some(origin);
    }

    pub(crate) fn error_raise_stack(&mut self, thread: TelemetryId, stack: &RaiseStack) {
        // A call path's functions were defined with it; an explicit stack
        // names functions of its own.
        for function in stack.frames.iter().filter_map(|frame| frame.function) {
            self.define_function(function);
        }
        let pending = self.unwinds.pending_mut(thread, stack.raise_id);
        pending.frames = stack
            .frames
            .iter()
            .map(|frame| proto::ErrorFrame {
                function_id: frame.function.map(FunctionId::get),
                pc: frame.pc,
                native: frame.native,
            })
            .collect();
        pending.inherited = stack
            .inherited
            .iter()
            .map(|frame| proto::InheritedFrame {
                function_name: frame.function_name.clone(),
                file: frame.file.clone(),
                line: frame.line,
            })
            .collect();
        pending.inherited_count = stack.inherited_count;
    }

    pub(crate) fn error_raised(
        &mut self,
        thread: TelemetryId,
        record: &SpanRecord<Snapshot, Snapshot>,
    ) {
        let &SpanRecord::ErrorRaised {
            raise_id,
            raised_at,
            kind,
            function,
            pc,
            raise_call,
            raise_frame_may_promote,
            call_path,
            frame_count,
        } = record
        else {
            unreachable!("dispatched on ErrorRaised");
        };
        let pending = self.unwinds.take_pending(thread, raise_id);
        // A previous raise on this thread without an end keeps no claim.
        self.unwinds.clear_thread(thread);
        // A call path published its callee, the raise function, with it.
        if call_path == CallPathId::ROOT
            && let Some(function) = function
        {
            self.define_function(function);
        }
        let mut message = proto::ErrorRaise {
            raise_id: raise_id.get(),
            thread_id: thread.get(),
            raised_at_ticks: raised_at.get(),
            kind: raise_kind(kind) as i32,
            function_id: function.map(FunctionId::get),
            pc,
            call_path_id: (call_path != CallPathId::ROOT).then(|| call_path.get()),
            frame_count,
            ..proto::ErrorRaise::default()
        };
        let origin = pending
            .as_ref()
            .and_then(|pending| pending.origin)
            .unwrap_or(RaiseOrigin::Fresh);
        set_origin(&mut message, origin);
        if let Some(pending) = pending {
            message.frames = pending.frames;
            message.inherited_frames = pending.inherited;
            message.inherited_frame_count = pending.inherited_count;
        }
        self.push_error(1, &message);
        if let Some(call) = raise_call {
            self.link(raise_id, call, proto::ErrorLinkRole::RaiseFrame);
        }
        self.unwinds.active.push(ActiveUnwind {
            thread,
            raise: raise_id,
            awaiting_raise_frame: raise_frame_may_promote && raise_call.is_none(),
            completions: 0,
            first_late: None,
        });
    }

    pub(crate) fn error_raise_frame_completed(&mut self, thread: TelemetryId, raise: TelemetryId) {
        let Some(unwind) = self.unwinds.get_mut(thread) else {
            return;
        };
        if unwind.raise != raise || !unwind.awaiting_raise_frame {
            return;
        }
        unwind.awaiting_raise_frame = false;
        // Only the raise frame completed since the raise: a late completion
        // here is that frame, promoted when it completed.
        if unwind.completions == 1
            && let Some(call) = unwind.first_late
        {
            self.link(raise, call, proto::ErrorLinkRole::RaiseFrame);
        }
    }

    pub(crate) fn error_unwind_ended(
        &mut self,
        thread: TelemetryId,
        raise: TelemetryId,
        result: UnwindResult,
        handler_function: Option<FunctionId>,
        handler_pc: Option<u32>,
        unwound_frames: u32,
    ) {
        self.unwinds.clear_thread(thread);
        if let Some(function) = handler_function {
            self.define_function(function);
        }
        self.push_error(
            3,
            &proto::ErrorUnwindEnd {
                raise_id: raise.get(),
                result: match result {
                    UnwindResult::Caught => proto::UnwindResult::Caught,
                    UnwindResult::Unhandled => proto::UnwindResult::Unhandled,
                    UnwindResult::EscapedToNative => proto::UnwindResult::EscapedToNative,
                    UnwindResult::Aborted => proto::UnwindResult::Aborted,
                } as i32,
                handler_function_id: handler_function.map(FunctionId::get),
                handler_pc,
                unwound_frames,
            },
        );
    }
}

fn via(via: OriginVia) -> proto::OriginVia {
    match via {
        OriginVia::Rethrow => proto::OriginVia::Rethrow,
        OriginVia::Await => proto::OriginVia::Await,
        OriginVia::Normalization => proto::OriginVia::Normalization,
    }
}

fn raise_kind(kind: RaiseKind) -> proto::RaiseKind {
    match kind {
        RaiseKind::Throw => proto::RaiseKind::Throw,
        RaiseKind::Rethrow => proto::RaiseKind::Rethrow,
        RaiseKind::PanicRethrow => proto::RaiseKind::PanicRethrow,
        RaiseKind::Await => proto::RaiseKind::Await,
        RaiseKind::AwaitCancelled => proto::RaiseKind::AwaitCancelled,
        RaiseKind::NativeBoundary => proto::RaiseKind::NativeBoundary,
        RaiseKind::HostBoundary => proto::RaiseKind::HostBoundary,
        RaiseKind::Runtime => proto::RaiseKind::Runtime,
    }
}

fn set_origin(message: &mut proto::ErrorRaise, origin: RaiseOrigin) {
    match origin {
        RaiseOrigin::Fresh => message.origin_state = proto::OriginState::Fresh as i32,
        RaiseOrigin::Proven {
            origin,
            previous,
            via: how,
        } => {
            message.origin_state = proto::OriginState::Proven as i32;
            message.origin_raise_id = Some(origin.get());
            message.previous_raise_id = previous.map(TelemetryId::get);
            message.origin_via = via(how) as i32;
        }
        RaiseOrigin::Ambiguous {
            candidates,
            via: how,
        } => {
            message.origin_state = proto::OriginState::Ambiguous as i32;
            message.origin_candidates = candidates;
            message.origin_via = via(how) as i32;
        }
        RaiseOrigin::Unresolved { reason, via: how } => {
            message.origin_state = proto::OriginState::Unresolved as i32;
            message.origin_via = via(how) as i32;
            message.unresolved_reason = match reason {
                UnresolvedOrigin::NoLanding => proto::UnresolvedReason::NoLanding,
                UnresolvedOrigin::LandingsEvicted => proto::UnresolvedReason::LandingsEvicted,
                UnresolvedOrigin::FutureLinkMissing => proto::UnresolvedReason::FutureLinkMissing,
                UnresolvedOrigin::FutureLinkEvicted => proto::UnresolvedReason::FutureLinkEvicted,
                UnresolvedOrigin::NoFuture => proto::UnresolvedReason::NoFuture,
                UnresolvedOrigin::SourceNotRecorded => proto::UnresolvedReason::SourceNotRecorded,
            } as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use btel_records::{ErrorFrame, InheritedFrame, OriginVia, RaiseOrigin};
    use btel_types::{AwaitDuration, ClockInstant, allocate_telemetry_id};
    use prost::Message as _;

    use super::*;
    use crate::{RecordingBuilder, RecordingConfig, RecordingId};

    type Record = SpanRecord<Snapshot, Snapshot>;

    fn raise(id: TelemetryId, may_promote: bool, call: Option<TelemetryId>) -> Record {
        SpanRecord::ErrorRaised {
            raise_id: id,
            raised_at: ClockInstant::from_ticks(5),
            kind: RaiseKind::Throw,
            function: None,
            pc: Some(3),
            raise_call: call,
            raise_frame_may_promote: may_promote,
            call_path: CallPathId::new_non_root(4).unwrap(),
            frame_count: 1,
        }
    }

    fn failed(id: TelemetryId, parent: TelemetryId, late: bool) -> Record {
        let path = CallPathId::new_non_root(1).unwrap();
        let (entered_at, exited_at) = (ClockInstant::from_ticks(1), ClockInstant::from_ticks(2));
        if late {
            SpanRecord::LateFunctionSpanCompletionErrored {
                id,
                parent_id: parent,
                call_path: path,
                entered_at,
                exited_at,
                await_time: AwaitDuration::ZERO,
                captured_value: None,
            }
        } else {
            SpanRecord::FunctionSpanCompletionErrored {
                id,
                parent_id: parent,
                call_path: path,
                entered_at,
                exited_at,
                await_time: AwaitDuration::ZERO,
                captured_value: None,
            }
        }
    }

    fn end(id: TelemetryId) -> Record {
        SpanRecord::ErrorUnwindEnded {
            raise_id: id,
            result: UnwindResult::Unhandled,
            handler_function: None,
            handler_pc: None,
            unwound_frames: 2,
        }
    }

    fn file(builder: &mut RecordingBuilder) -> proto::RecordingFile {
        let sealed = builder.flush_recording().unwrap().unwrap();
        proto::RecordingFile::decode(sealed.bytes()).unwrap()
    }

    fn links(file: &proto::RecordingFile) -> Vec<(u64, u64, i32)> {
        file.errors
            .as_ref()
            .map(|errors| {
                errors
                    .call_links
                    .iter()
                    .map(|l| (l.raise_id, l.call_id, l.role))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn completions_link_only_to_their_own_threads_active_raise() {
        let mut builder =
            RecordingBuilder::new(RecordingId::generate(), RecordingConfig::default()).unwrap();
        let [a, b] = [allocate_telemetry_id(), allocate_telemetry_id()];
        let [ra, rb] = [allocate_telemetry_id(), allocate_telemetry_id()];
        let calls: Vec<TelemetryId> = (0..6).map(|_| allocate_telemetry_id()).collect();
        let unwound = proto::ErrorLinkRole::Unwound as i32;
        let raise_frame = proto::ErrorLinkRole::RaiseFrame as i32;
        let records = [
            (a, raise(ra, false, Some(calls[0]))),
            (b, failed(calls[1], b, false)), // another thread, no raise yet
            (a, failed(calls[0], a, false)),
            (b, raise(rb, false, None)),
            (a, failed(calls[2], a, false)), // interleaved: still thread a's
            (b, failed(calls[3], b, false)),
            (a, end(ra)),
            (a, failed(calls[4], a, false)), // after its end: unlinked
            (b, end(rb)),
        ];
        for (thread, record) in &records {
            builder.span_reference(*thread, record);
        }
        let file = file(&mut builder);
        assert_eq!(
            links(&file),
            [
                (ra.get(), calls[0].get(), raise_frame),
                (ra.get(), calls[0].get(), unwound),
                (ra.get(), calls[2].get(), unwound),
                (rb.get(), calls[3].get(), unwound),
            ]
        );
        let errors = file.errors.unwrap();
        assert_eq!(errors.raises.len(), 2);
        assert_eq!(errors.raises[0].thread_id, a.get());
        assert_eq!(errors.unwind_ends.len(), 2);
    }

    #[test]
    fn a_raise_without_an_end_claims_nothing_after_its_thread_or_a_new_raise() {
        let mut builder =
            RecordingBuilder::new(RecordingId::generate(), RecordingConfig::default()).unwrap();
        let thread = allocate_telemetry_id();
        let [first, second] = [allocate_telemetry_id(), allocate_telemetry_id()];
        let calls: Vec<TelemetryId> = (0..3).map(|_| allocate_telemetry_id()).collect();
        builder.span_reference(thread, &raise(first, false, None));
        builder.span_reference(thread, &failed(calls[0], thread, false));
        // The second raise begins without the first ever ending.
        builder.span_reference(thread, &raise(second, false, None));
        builder.span_reference(thread, &failed(calls[1], thread, false));
        let clock = btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
        clock.attach_thread();
        builder.span_reference(
            thread,
            &SpanRecord::ThreadSpanCompletion {
                id: thread,
                parent_id: None,
                spawn_call_path: CallPathId::ROOT,
                started_at: ClockInstant::from_ticks(1),
                completed_at: ClockInstant::from_ticks(9),
                outcome: btel_types::InvocationOutcome::Errored,
                clock,
            },
        );
        builder.span_reference(thread, &failed(calls[2], thread, false));
        let unwound = proto::ErrorLinkRole::Unwound as i32;
        assert_eq!(
            links(&file(&mut builder)),
            [
                (first.get(), calls[0].get(), unwound),
                (second.get(), calls[1].get(), unwound),
            ]
        );
    }

    #[test]
    fn a_promoted_raise_frame_is_named_by_its_marker() {
        let mut builder =
            RecordingBuilder::new(RecordingId::generate(), RecordingConfig::default()).unwrap();
        let thread = allocate_telemetry_id();
        let [promoted, plain] = [allocate_telemetry_id(), allocate_telemetry_id()];
        let [late, outer, other] = [
            allocate_telemetry_id(),
            allocate_telemetry_id(),
            allocate_telemetry_id(),
        ];
        // The raise frame is promoted when it completes: the marker names it.
        builder.span_reference(thread, &raise(promoted, true, None));
        builder.span_reference(thread, &failed(late, thread, true));
        builder.span_reference(
            thread,
            &SpanRecord::ErrorRaiseFrameCompleted { raise_id: promoted },
        );
        builder.span_reference(thread, &failed(outer, thread, false));
        builder.span_reference(thread, &end(promoted));
        // Not promoted: the next completion is an ancestor, never the frame.
        builder.span_reference(thread, &raise(plain, true, None));
        builder.span_reference(
            thread,
            &SpanRecord::ErrorRaiseFrameCompleted { raise_id: plain },
        );
        builder.span_reference(thread, &failed(other, thread, true));
        builder.span_reference(thread, &end(plain));
        let unwound = proto::ErrorLinkRole::Unwound as i32;
        let raise_frame = proto::ErrorLinkRole::RaiseFrame as i32;
        assert_eq!(
            links(&file(&mut builder)),
            [
                (promoted.get(), late.get(), unwound),
                (promoted.get(), late.get(), raise_frame),
                (promoted.get(), outer.get(), unwound),
                (plain.get(), other.get(), unwound),
            ]
        );
    }

    #[test]
    fn an_error_free_file_has_no_error_section_and_origins_encode() {
        let mut builder =
            RecordingBuilder::new(RecordingId::generate(), RecordingConfig::default()).unwrap();
        let thread = allocate_telemetry_id();
        builder.span_reference(thread, &failed(allocate_telemetry_id(), thread, false));
        assert!(file(&mut builder).errors.is_none());

        let origin = allocate_telemetry_id();
        let raise_id = allocate_telemetry_id();
        builder.span_reference(
            thread,
            &SpanRecord::ErrorRaiseOrigin {
                raise_id,
                origin: RaiseOrigin::Proven {
                    origin,
                    previous: None,
                    via: OriginVia::Await,
                },
            },
        );
        builder.span_reference(thread, &raise(raise_id, false, None));
        let file = file(&mut builder);
        let raise = &file.errors.unwrap().raises[0];
        assert_eq!(raise.origin_state, proto::OriginState::Proven as i32);
        assert_eq!(raise.origin_raise_id, Some(origin.get()));
        assert_eq!(raise.previous_raise_id, None);
        assert_eq!(raise.origin_via, proto::OriginVia::Await as i32);
        assert_eq!(raise.call_path_id, Some(4));
        assert!(raise.frames.is_empty());
    }

    #[test]
    fn preceding_evidence_joins_only_its_own_raise() {
        let mut builder =
            RecordingBuilder::new(RecordingId::generate(), RecordingConfig::default()).unwrap();
        let [a, b] = [allocate_telemetry_id(), allocate_telemetry_id()];
        let [stale, listed, other] = [
            allocate_telemetry_id(),
            allocate_telemetry_id(),
            allocate_telemetry_id(),
        ];
        let stack = |raise_id| {
            SpanRecord::ErrorRaiseStack(Box::new(RaiseStack {
                raise_id,
                frames: vec![ErrorFrame {
                    function: None,
                    pc: None,
                    native: true,
                }],
                inherited: vec![InheritedFrame {
                    function_name: "f".into(),
                    file: "a.baml".into(),
                    line: 2,
                }],
                inherited_count: 3,
            }))
        };
        // Evidence of a raise whose record never arrived is replaced; another
        // thread's evidence never attaches here.
        builder.span_reference(
            a,
            &SpanRecord::ErrorRaiseOrigin {
                raise_id: stale,
                origin: RaiseOrigin::Ambiguous {
                    candidates: 2,
                    via: OriginVia::Rethrow,
                },
            },
        );
        builder.span_reference(b, &stack(other));
        builder.span_reference(a, &stack(listed));
        let mut record = raise(listed, false, None);
        if let SpanRecord::ErrorRaised { call_path, .. } = &mut record {
            *call_path = CallPathId::ROOT;
        }
        builder.span_reference(a, &record);
        builder.span_reference(b, &raise(allocate_telemetry_id(), false, None));
        let errors = file(&mut builder).errors.unwrap();
        let [first, second] = errors.raises.as_slice() else {
            panic!("two raises");
        };
        assert_eq!(first.raise_id, listed.get());
        assert_eq!(first.origin_state, proto::OriginState::Fresh as i32);
        assert_eq!(first.call_path_id, None);
        assert_eq!(first.frames.len(), 1);
        assert_eq!(first.inherited_frame_count, 3);
        assert_eq!(second.call_path_id, Some(4));
        assert!(second.frames.is_empty() && second.inherited_frames.is_empty());
    }
}
