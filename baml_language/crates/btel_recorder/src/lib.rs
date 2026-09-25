//! Synchronous borrowed-record to Protobuf encoding and aggregate merging.
//!
//! `RecordingBuilder` is shared by local and cloud publishers. `RecordingPublisher`
//! adapts it to completed-file callbacks. The `Publisher` interface is defined by
//! `btel_processor`; no storage or networking is implemented here.

use btel_processor::AggregateDelta;
use btel_records::SpanRecord;
use btel_snapshot::Snapshot;
use btel_types::{CallPathNodeId, TelemetryId};

mod clock;
mod encoding;
mod flags;
mod merge;
mod recording;
pub use flags::CompletionFlags;
pub use recording::{
    RecordingBuilder, RecordingConfig, RecordingError, RecordingId, RecordingPublisher, SealedFile,
};

/// Generated wire messages, never the VM's in-memory layout. Scalar span
/// messages encode while borrowed; no input references escape the callback.
#[allow(clippy::all, clippy::pedantic, clippy::empty_structs_with_brackets)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/baml.btel.recording.v2.rs"));
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
    // Cached repeated-message bytes. Span bytes are already materialized.
    pending_encoded_bytes: usize,
}
// Count each newly constructed metadata message once; never rescan the file.
fn push_message<M: prost::Message>(output: &mut Vec<M>, bytes: &mut usize, tag: u32, value: M) {
    *bytes = bytes.saturating_add(prost::encoding::message::encoded_len(tag, &value));
    output.push(value);
}
impl ConversionBuffer {
    fn event(&mut self, thread: TelemetryId, event: proto::span_event::Event) {
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
        push_message(
            &mut self.pending.definitions.threads,
            &mut self.pending_encoded_bytes,
            3,
            proto::ThreadDefinition {
                thread_id: id.get(),
                parent_id: parent.map(TelemetryId::get),
                spawn_call_path_id: path.get(),
                started_at_ticks: started.get(),
                clock_epoch_id: clock.metadata().epoch.get(),
            },
        );
        push_message(
            &mut self.pending.definitions.clock_epochs,
            &mut self.pending_encoded_bytes,
            4,
            clock::definition(clock.metadata()),
        );
        push_message(
            &mut self.pending.clock_states.states,
            &mut self.pending_encoded_bytes,
            1,
            clock::state(clock),
        );
    }
}

impl ConversionBuffer {
    fn aggregate(&mut self, delta: AggregateDelta) {
        // One recording window before serialization and delivery to its sink.
        // Span callbacks do not enter this map: the processor already counted them.
        self.pending_encoded_bytes = self
            .pending_encoded_bytes
            .saturating_add(self.aggregates.observe(delta, &mut self.pending.aggregates));
    }

    fn is_empty(&self) -> bool {
        self.pending_encoded_bytes == 0 && self.spans.is_empty()
    }

    fn encoded_size_hint(&self) -> usize {
        if self.is_empty() {
            return 0;
        }
        self.spans
            .len()
            .saturating_add(self.pending_encoded_bytes)
            .saturating_add(btel_settings::publisher::FILE_ENVELOPE_BYTES)
    }

    fn span(&mut self, thread: TelemetryId, record: &SpanRecord<Snapshot, Snapshot>) {
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
                        inputs_cas_id: captured_inputs.as_ref().map(snapshot_id),
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
                push_message(
                    &mut self.pending.definitions.call_paths,
                    &mut self.pending_encoded_bytes,
                    2,
                    proto::CallPathDefinition {
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
                    },
                );
                // No heap access here. A later metadata resolver may enrich these
                // explicit placeholders; GC-collected functions may stay unnamed.
                for function in [Some(*callee), *visible_caller].into_iter().flatten() {
                    push_message(
                        &mut self.pending.definitions.functions,
                        &mut self.pending_encoded_bytes,
                        1,
                        proto::FunctionDefinition {
                            function_id: function.get(),
                            resolution: Some(proto::function_definition::Resolution::Unavailable(
                                proto::MetadataUnavailable {},
                            )),
                        },
                    );
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
                        completion_flags: CompletionFlags::from_variant(1).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(9).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(1).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(9).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(2).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(10).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(2).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(10).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(3).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(11).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(3).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(11).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(1).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(1).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(2).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(2).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(3).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
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
                        completion_flags: CompletionFlags::from_variant(3).bits(),
                        value_cas_id: captured_value.as_ref().map(snapshot_id),
                    }),
                );
            }
        }
    }

    fn take(&mut self) -> PendingMessages {
        self.aggregates.drain_into(&mut self.pending.aggregates);
        // Reuse warm map capacity, trimming oversized allocations after bursts.
        self.aggregates.trim();
        self.pending_encoded_bytes = 0;
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests;

fn snapshot_id(snapshot: &Snapshot) -> proto::SnapshotId {
    let bytes = snapshot.id();
    proto::SnapshotId {
        low: u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap()),
        high: u64::from_le_bytes(bytes.as_bytes()[8..].try_into().unwrap()),
    }
}
