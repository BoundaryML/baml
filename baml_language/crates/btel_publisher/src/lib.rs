//! Synchronous borrowed-record to Protobuf encoding and aggregate merging.
//!
//! One shared accumulation/sealing strategy precedes all destinations. The
//! engine drives this publisher on its chunk processor worker. Delivery is a
//! completed-file callback; no storage or networking is implemented here.

use btel_processor::AggregateDelta;
use btel_records::{CaptureDeferred, SpanRecord};
use btel_types::{CallPathNodeId, TelemetryId};

mod clock;
mod encoding;
mod flags;
mod merge;
mod recording;
pub use flags::CompletionFlags;
pub use recording::{RecordingConfig, RecordingError, RecordingId, RecordingPublisher, SealedFile};

/// Generated wire messages, never the VM's in-memory layout. Scalar span
/// messages encode while borrowed; no input references escape the callback.
#[allow(clippy::all, clippy::pedantic, clippy::empty_structs_with_brackets)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/baml.btel.recording.v1.rs"));
}

/// Cold sections retained until sealing; spans are already encoded.
#[derive(Default)]
struct PendingMessages {
    definitions: proto::Definitions,
    aggregates: proto::AggregateBatch,
    clock_states: proto::ClockStateBatch,
}

/// Worker-local conversion state. The recording publisher drains this after
/// chunk release; no input references escape conversion.
#[derive(Default)]
struct ConversionBuffer {
    pending: PendingMessages,
    spans: encoding::EncodedSpans,
    aggregates: merge::PendingAggregates,
    estimate: usize,
}
impl ConversionBuffer {
    fn event(&mut self, thread: TelemetryId, event: proto::span_event::Event) {
        self.estimate = self.estimate.saturating_add(128);
        self.spans.event(thread, event);
    }

    fn thread_definition(
        &mut self,
        id: TelemetryId,
        parent: Option<TelemetryId>,
        path: btel_types::CallPathId,
        started: btel_types::ClockInstant,
        clock: &btel_clock::ClockEpoch,
    ) {
        // These fixed-schema definitions have a bounded encoded size; no scan
        // of the growing pending file is performed.
        self.estimate = self.estimate.saturating_add(512);
        self.pending
            .definitions
            .threads
            .push(proto::ThreadDefinition {
                thread_id: id.get(),
                parent_id: parent.map(TelemetryId::get),
                spawn_call_path_id: path.get(),
                started_at_ticks: started.get(),
                clock_epoch_id: clock.metadata().epoch.get(),
            });
        self.pending
            .definitions
            .clock_epochs
            .push(clock::definition(clock.metadata()));
        self.pending.clock_states.states.push(clock::state(clock));
    }
}

impl ConversionBuffer {
    fn aggregate(&mut self, delta: AggregateDelta) {
        // One shared recording window before serialization and sink fan-out.
        // Span callbacks do not enter this map: the processor already counted them.
        self.estimate = self
            .estimate
            .saturating_add(self.aggregates.observe(delta, &mut self.pending.aggregates) * 37);
    }

    fn span(&mut self, thread: TelemetryId, record: &SpanRecord<CaptureDeferred, CaptureDeferred>) {
        use proto::span_event::Event;
        match record {
            SpanRecord::ThreadSelected { .. } => panic!("processor must consume selectors"),
            SpanRecord::ThreadSpanAnnouncement {
                id,
                parent_id,
                spawn_call_path,
                started_at,
                clock,
            } => {
                assert_eq!(thread, *id, "thread definition disagrees with selector");
                self.thread_definition(*id, *parent_id, *spawn_call_path, *started_at, clock);
                self.event(
                    thread,
                    Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
                );
            }
            SpanRecord::ThreadSpanCompletion {
                id,
                parent_id,
                spawn_call_path,
                started_at,
                completed_at,
                outcome,
                clock,
            } => {
                assert_eq!(thread, *id, "thread definition disagrees with selector");
                self.thread_definition(*id, *parent_id, *spawn_call_path, *started_at, clock);
                self.event(
                    thread,
                    Event::ThreadCompletion(proto::ThreadCompletion {
                        completed_at_ticks: completed_at.get(),
                        outcome: flags::outcome(*outcome) as i32,
                    }),
                );
            }
            SpanRecord::FunctionSpanAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                captured_inputs,
            } => {
                self.event(
                    thread,
                    Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        call_path_id: call_path.get(),
                        entered_at_ticks: entered_at.get(),
                        inputs: if captured_inputs.is_some() {
                            proto::CaptureState::Deferred
                        } else {
                            proto::CaptureState::NotRecorded
                        } as i32,
                    }),
                );
            }
            SpanRecord::CallPathDefined {
                call_path,
                parent_call_path,
                visible_caller,
                caller_pc,
                callee,
                edge,
            } => {
                self.estimate = self.estimate.saturating_add(192);
                self.pending
                    .definitions
                    .call_paths
                    .push(proto::CallPathDefinition {
                        call_path_id: call_path.get(),
                        thread_id: thread.get(),
                        parent_call_path_id: parent_call_path.get(),
                        visible_caller_function_id: visible_caller.map(btel_types::FunctionId::get),
                        caller_pc: *caller_pc,
                        callee_function_id: callee.get(),
                        edge: match edge {
                            btel_types::CallPathEdge::Synchronous => {
                                proto::CallPathEdge::Synchronous
                            }
                            btel_types::CallPathEdge::Spawn => proto::CallPathEdge::Spawn,
                        } as i32,
                    });
                // No heap access here. A later metadata resolver may enrich these
                // explicit placeholders; GC-collected functions may stay unnamed.
                for function in [Some(*callee), *visible_caller].into_iter().flatten() {
                    self.pending
                        .definitions
                        .functions
                        .push(proto::FunctionDefinition {
                            function_id: function.get(),
                            resolution: Some(proto::function_definition::Resolution::Unavailable(
                                proto::MetadataUnavailable {},
                            )),
                        });
                }
            }
            SpanRecord::FunctionSpanCompletionOk {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            1,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            9,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionOkReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            1,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionOkReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            9,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionErrored {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            2,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            10,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionErroredReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            2,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionErroredReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            10,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionCancelled {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            3,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionCancelledNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            11,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionCancelledReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            3,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            11,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::LateFunctionSpanCompletionOk {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::LateFunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            1,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::LateFunctionSpanCompletionOkReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::LateFunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            1,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::LateFunctionSpanCompletionErrored {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::LateFunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            2,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::LateFunctionSpanCompletionErroredReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::LateFunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            2,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::LateFunctionSpanCompletionCancelled {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::LateFunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, false).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            3,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
            SpanRecord::LateFunctionSpanCompletionCancelledReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
            } => {
                self.event(
                    thread,
                    Event::LateFunctionCompletion(proto::FunctionCompletion {
                        id: id.get(),
                        parent_id: parent_id.get(),
                        node: CallPathNodeId::new(*call_path, true).get(),
                        entered_at_ticks: entered_at.get(),
                        exited_at_ticks: exited_at.get(),
                        self_await_ticks: await_time.get().get(),
                        completion_flags: CompletionFlags::from_variant(
                            3,
                            captured_value.is_some(),
                        )
                        .bits(),
                    }),
                );
            }
        }
    }

    fn take(&mut self) -> PendingMessages {
        self.aggregates.drain_into(&mut self.pending.aggregates);
        // Reuse a bounded warm map; its allocation remains charged while empty.
        self.aggregates.trim();
        self.estimate = 0;
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests;
