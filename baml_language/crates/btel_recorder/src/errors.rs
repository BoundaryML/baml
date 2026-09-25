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
    ErrorRaise, OriginVia, RaiseKind, RaiseOrigin, SpanRecord, UnresolvedOrigin, UnwindResult,
};
use btel_snapshot::Snapshot;
use btel_types::{FunctionId, TelemetryId};

use crate::{ConversionBuffer, proto, push_message};

struct ActiveUnwind {
    thread: TelemetryId,
    raise: TelemetryId,
    /// Waiting for the raise-frame marker to name a promoted raise frame.
    awaiting_raise_frame: bool,
    completions: u32,
    first_late: Option<TelemetryId>,
}

/// At most one unwind per telemetry thread; usually empty.
#[derive(Default)]
pub(crate) struct ActiveUnwinds(Vec<ActiveUnwind>);

impl ActiveUnwinds {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn clear_thread(&mut self, thread: TelemetryId) {
        self.0.retain(|unwind| unwind.thread != thread);
    }
    fn get_mut(&mut self, thread: TelemetryId) -> Option<&mut ActiveUnwind> {
        self.0.iter_mut().find(|unwind| unwind.thread == thread)
    }
}

impl ConversionBuffer {
    fn link(&mut self, raise: TelemetryId, call: TelemetryId, role: proto::ErrorLinkRole) {
        push_message(
            &mut self.pending.errors.call_links,
            &mut self.pending_encoded_bytes,
            2,
            proto::ErrorCallLink {
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

    pub(crate) fn error_raised(&mut self, thread: TelemetryId, raise: &ErrorRaise) {
        // A previous raise on this thread without an end keeps no claim.
        self.unwinds.clear_thread(thread);
        for function in raise
            .function
            .into_iter()
            .chain(raise.frames.iter().filter_map(|frame| frame.function))
        {
            self.define_function(function);
        }
        let message = raise_message(thread, raise);
        push_message(
            &mut self.pending.errors.raises,
            &mut self.pending_encoded_bytes,
            1,
            message,
        );
        if let Some(call) = raise.raise_call {
            self.link(raise.raise_id, call, proto::ErrorLinkRole::RaiseFrame);
        }
        self.unwinds.0.push(ActiveUnwind {
            thread,
            raise: raise.raise_id,
            awaiting_raise_frame: raise.raise_frame_may_promote && raise.raise_call.is_none(),
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
        push_message(
            &mut self.pending.errors.unwind_ends,
            &mut self.pending_encoded_bytes,
            3,
            proto::ErrorUnwindEnd {
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

fn raise_message(thread: TelemetryId, raise: &ErrorRaise) -> proto::ErrorRaise {
    let mut message = proto::ErrorRaise {
        raise_id: raise.raise_id.get(),
        thread_id: thread.get(),
        raised_at_ticks: raise.raised_at.get(),
        kind: match raise.kind {
            RaiseKind::Throw => proto::RaiseKind::Throw,
            RaiseKind::Rethrow => proto::RaiseKind::Rethrow,
            RaiseKind::PanicRethrow => proto::RaiseKind::PanicRethrow,
            RaiseKind::Await => proto::RaiseKind::Await,
            RaiseKind::AwaitCancelled => proto::RaiseKind::AwaitCancelled,
            RaiseKind::NativeBoundary => proto::RaiseKind::NativeBoundary,
            RaiseKind::HostBoundary => proto::RaiseKind::HostBoundary,
            RaiseKind::Runtime => proto::RaiseKind::Runtime,
        } as i32,
        function_id: raise.function.map(FunctionId::get),
        pc: raise.pc,
        frames: raise
            .frames
            .iter()
            .map(|frame| proto::ErrorFrame {
                function_id: frame.function.map(FunctionId::get),
                pc: frame.pc,
                native: frame.native,
            })
            .collect(),
        frame_count: raise.frame_count,
        inherited_frames: raise
            .inherited
            .iter()
            .map(|frame| proto::InheritedFrame {
                function_name: frame.function_name.clone(),
                file: frame.file.clone(),
                line: frame.line,
            })
            .collect(),
        inherited_frame_count: raise.inherited_count,
        ..proto::ErrorRaise::default()
    };
    match raise.origin {
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
    message
}

#[cfg(test)]
mod tests {
    use btel_records::{ErrorFrame, OriginVia, RaiseOrigin};
    use btel_types::{AwaitDuration, CallPathId, ClockInstant, allocate_telemetry_id};
    use prost::Message as _;

    use super::*;
    use crate::{RecordingBuilder, RecordingConfig, RecordingId};

    type Record = SpanRecord<Snapshot, Snapshot>;

    fn raise(id: TelemetryId, may_promote: bool, call: Option<TelemetryId>) -> Record {
        SpanRecord::ErrorRaised(Box::new(ErrorRaise {
            raise_id: id,
            raised_at: ClockInstant::from_ticks(5),
            kind: RaiseKind::Throw,
            function: None,
            pc: Some(3),
            raise_call: call,
            raise_frame_may_promote: may_promote,
            origin: RaiseOrigin::Fresh,
            frames: vec![ErrorFrame {
                function: None,
                pc: Some(3),
                native: false,
            }],
            frame_count: 1,
            inherited: Vec::new(),
            inherited_count: 0,
        }))
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
        let mut record = raise(allocate_telemetry_id(), false, None);
        if let SpanRecord::ErrorRaised(raise) = &mut record {
            raise.origin = RaiseOrigin::Proven {
                origin,
                previous: None,
                via: OriginVia::Await,
            };
        }
        builder.span_reference(thread, &record);
        let file = file(&mut builder);
        let raise = &file.errors.unwrap().raises[0];
        assert_eq!(raise.origin_state, proto::OriginState::Proven as i32);
        assert_eq!(raise.origin_raise_id, Some(origin.get()));
        assert_eq!(raise.previous_raise_id, None);
        assert_eq!(raise.origin_via, proto::OriginVia::Await as i32);
    }
}
