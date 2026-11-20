//! Synchronous borrowed-record to Protobuf encoding and aggregate merging.
//!
//! `RecordingBuilder` is shared by local and cloud publishers. `RecordingPublisher`
//! adapts it to completed-file callbacks. The `Publisher` interface is defined by
//! `btel_processor`; no storage or networking is implemented here.

use std::{collections::BTreeMap, sync::Arc};

use btel_core::recording::field;
use btel_processor::AggregateDelta;
use btel_records::SpanRecord;
use btel_snapshot::Snapshot;
use btel_types::{CallPathNodeId, TelemetryId};

mod clock;
mod encoding;
mod errors;
mod flags;
mod functions;
mod merge;
mod needs;
mod open;
mod recording;
pub use btel_core::recording::CompletionFlags;
/// The recording format's messages, as `btel-core` defines them: the wire, never the VM's
/// in-memory layout. Scalar span messages encode while borrowed; no input references escape the
/// callback.
pub use btel_core::recording::v2 as proto;
pub use recording::{
    Destination, ProcessRecording, RecordingBuilder, RecordingConfig, RecordingError, RecordingId,
    RecordingPublisher, SealedFile,
};

/// Cold sections retained until sealing; spans are already encoded.
#[derive(Default)]
struct PendingMessages {
    definitions: proto::Definitions,
    aggregates: proto::AggregateBatch,
    clock_states: proto::ClockStateBatch,
    /// `ErrorBatch` body, encoded as each message arrives: a throw-heavy
    /// program sends one raise and one end per throw, and prost would
    /// otherwise recompute every nested length several times per file.
    errors: Vec<u8>,
}

/// Worker-local conversion state. The recording publisher drains this after
/// chunk release; no input references escape conversion.
struct ConversionBuffer {
    pending: PendingMessages,
    spans: encoding::EncodedSpans,
    aggregates: merge::PendingAggregates,
    sysops: merge::PendingSysOps,
    functions: functions::FunctionDefinitions,
    // Referenced epochs not yet observed settled, released once final. Bounded
    // by live runs plus runs settled since the last sealed file.
    unsettled_clocks: BTreeMap<u64, Arc<btel_clock::ClockEpoch>>,
    // Cached repeated-message bytes. Span bytes are already materialized.
    pending_encoded_bytes: usize,
    // Unwinds in progress, at most one per telemetry thread. Empty unless an
    // exception is being unwound, so ordinary completions pay one length check.
    unwinds: errors::ActiveUnwinds,
    // Model usage awaiting its node's completion, which the VM writes right
    // after it, its own and its total; kept across files like `unwinds`.
    // Empty without model calls.
    usage: BTreeMap<u64, (Vec<proto::ModelUsage>, Vec<proto::ModelUsage>)>,
    /// What spans started and have not completed carry to their end records.
    open: open::OpenSpans,
    /// Where each definition was written, for the file's `definitions_from`.
    needs: needs::Needs,
    /// The sequence of the file being built: the home of what it defines.
    sequence: u64,
    /// The recording writes start records: a local one does, for a reader
    /// that shows running and crashed calls; one for an upload does not.
    starts: bool,
    /// The error each thread ended with, until the thread's completion, which carries it.
    thread_errors: BTreeMap<u64, proto::CasId>,
}
impl Default for ConversionBuffer {
    fn default() -> Self {
        Self {
            pending: PendingMessages::default(),
            spans: encoding::EncodedSpans::default(),
            aggregates: merge::PendingAggregates::default(),
            sysops: merge::PendingSysOps::default(),
            functions: functions::FunctionDefinitions::default(),
            unsettled_clocks: BTreeMap::new(),
            pending_encoded_bytes: 0,
            unwinds: errors::ActiveUnwinds::default(),
            usage: BTreeMap::new(),
            thread_errors: BTreeMap::new(),
            open: open::OpenSpans::default(),
            needs: needs::Needs::default(),
            sequence: 1,
            starts: true,
        }
    }
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
        clock: &Arc<btel_clock::ClockEpoch>,
        name: Option<&str>,
    ) {
        push_message(
            &mut self.pending.definitions.threads,
            &mut self.pending_encoded_bytes,
            field::definitions::THREADS,
            proto::ThreadDefinition {
                thread_id: id.get(),
                parent_id: parent.map(TelemetryId::get),
                spawn_call_path_id: path.get(),
                started_at_ticks: started.get(),
                clock_epoch_id: clock.metadata().epoch.get(),
                name: name.map(str::to_owned),
            },
        );
        push_message(
            &mut self.pending.definitions.clock_epochs,
            &mut self.pending_encoded_bytes,
            field::definitions::CLOCK_EPOCHS,
            clock::definition(clock.metadata()),
        );
        self.observe_clock(clock);
    }

    /// The current state of a thread's clock epoch, tracked until it settles.
    fn observe_clock(&mut self, clock: &Arc<btel_clock::ClockEpoch>) {
        let state = clock::state(clock);
        if state.r#final {
            self.unsettled_clocks.remove(&state.epoch_id);
        } else {
            self.unsettled_clocks
                .entry(state.epoch_id)
                .or_insert_with(|| Arc::clone(clock));
        }
        push_message(
            &mut self.pending.clock_states.states,
            &mut self.pending_encoded_bytes,
            field::clock_state_batch::STATES,
            state,
        );
    }

    /// Report and release epochs that settled since their last observation.
    /// `latest` also reports each still-unsettled epoch's current, non-final
    /// status. True when every observed epoch has been reported final.
    fn observe_unsettled_clocks(&mut self, latest: bool) -> bool {
        let states = &mut self.pending.clock_states.states;
        let bytes = &mut self.pending_encoded_bytes;
        self.unsettled_clocks.retain(|_, epoch| {
            let state = clock::state(epoch);
            let settled = state.r#final;
            if settled || latest {
                push_message(states, bytes, field::clock_state_batch::STATES, state);
            }
            !settled
        });
        self.unsettled_clocks.is_empty()
    }
}

impl ConversionBuffer {
    fn define_function(&mut self, function: btel_types::FunctionId) {
        if let Some(resolution) = self.functions.resolve(function) {
            self.needs.function(function.get(), self.sequence);
            push_message(
                &mut self.pending.definitions.functions,
                &mut self.pending_encoded_bytes,
                field::definitions::FUNCTIONS,
                proto::FunctionDefinition {
                    function_id: function.get(),
                    resolution: Some(resolution),
                },
            );
        }
    }

    fn aggregate(&mut self, delta: AggregateDelta) {
        // One recording window before serialization and delivery to its sink.
        // Span callbacks do not enter this map: the processor already counted them.
        self.pending_encoded_bytes = self
            .pending_encoded_bytes
            .saturating_add(self.aggregates.observe(delta, &mut self.pending.aggregates));
    }

    fn sysop_time(&mut self, path: btel_types::CallPathId, elapsed: btel_types::ClockDuration) {
        self.pending_encoded_bytes =
            self.pending_encoded_bytes
                .saturating_add(
                    self.sysops
                        .observe(path, elapsed, &mut self.pending.aggregates),
                );
    }

    fn is_empty(&self) -> bool {
        self.pending_encoded_bytes == 0 && self.spans.is_empty() && !self.open.has_parked()
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
        if !self.unwinds.is_empty() {
            self.link_unwound(thread, record);
        }
        if let Some(done) = record.completion() {
            self.function_completion(thread, &done);
            return;
        }
        match record {
            SpanRecord::HostFunctionDefined(metadata) => {
                self.functions.register_host(metadata);
            }
            SpanRecord::ThreadSelected { .. } => panic!("processor must consume selectors"),
            SpanRecord::Log {
                parent_id,
                function,
                pc,
                at,
                level,
                event_name,
                captured_data,
            } => {
                self.define_function(*function);
                self.needs.log(thread.get(), function.get());
                let level = match level {
                    btel_records::LogLevel::Info => proto::LogLevel::Info,
                    btel_records::LogLevel::Debug => proto::LogLevel::Debug,
                    btel_records::LogLevel::Warn => proto::LogLevel::Warn,
                    btel_records::LogLevel::Error => proto::LogLevel::Error,
                };
                self.event(
                    thread,
                    Event::Log(proto::LogEvent {
                        parent_id: parent_id.get(),
                        function_id: function.get(),
                        pc: *pc,
                        at_ticks: at.get(),
                        level: level as i32,
                        event_name: event_name.as_deref().map(str::to_owned),
                        data_cas_id: captured_data.as_ref().map(cas_id),
                        line: self.functions.line(*function, *pc),
                    }),
                );
            }
            SpanRecord::ContextSelected { captured_context } => {
                self.spans.select_context(
                    thread,
                    captured_context.as_ref().map(|snapshot| {
                        proto::thread_section::Context::ContextCasId(cas_id(snapshot))
                    }),
                );
            }
            SpanRecord::ContextCleared => {
                self.spans.select_context(
                    thread,
                    Some(proto::thread_section::Context::EmptyContext(true)),
                );
            }
            SpanRecord::ContextReferenced { id } => {
                self.spans.select_context(
                    thread,
                    Some(proto::thread_section::Context::ContextCasId(cas_reference(
                        *id,
                    ))),
                );
            }
            SpanRecord::ThreadSpanError { captured_error } => {
                if let Some(error) = captured_error.as_ref().map(cas_id) {
                    self.thread_errors.insert(thread.get(), error);
                }
            }
            SpanRecord::SpanUsage(usage) => {
                let wire = |entries: &[btel_records::ModelUsage]| {
                    entries
                        .iter()
                        .map(|entry| proto::ModelUsage {
                            provider: entry.provider.to_string(),
                            model: entry.model.as_deref().map(str::to_owned),
                            calls: entry.calls,
                            input_tokens: entry.input_tokens,
                            output_tokens: entry.output_tokens,
                            cache_read_tokens: entry.cache_read_tokens,
                            cache_write_tokens: entry.cache_write_tokens,
                            reasoning_tokens: entry.reasoning_tokens,
                        })
                        .collect::<Vec<_>>()
                };
                self.usage
                    .insert(usage.node.get(), (wire(&usage.entries), wire(&usage.total)));
            }
            SpanRecord::ThreadSpawned { child, parent } => {
                self.open.spawned(child.get(), parent.get());
            }
            SpanRecord::ThreadSpanAnnouncement {
                id,
                parent_id,
                spawn_call_path,
                started_at,
                clock,
                name,
            } => {
                assert_eq!(thread, *id, "thread definition disagrees with selector");
                self.thread_definition(
                    *id,
                    *parent_id,
                    *spawn_call_path,
                    *started_at,
                    clock,
                    name.as_ref().map(|name| &***name),
                );
                self.needs
                    .thread(id.get(), spawn_call_path.get(), self.sequence);
                self.open
                    .thread_started(id.get(), parent_id.map(TelemetryId::get));
                if self.starts {
                    self.event(
                        thread,
                        Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
                    );
                }
            }
            SpanRecord::ThreadSpanCompletion {
                id,
                parent_id,
                spawn_call_path,
                started_at,
                completed_at,
                outcome,
                clock,
                name,
            } => {
                assert_eq!(thread, *id, "thread definition disagrees with selector");
                if self.open.thread_known(id.get()) {
                    self.observe_clock(clock);
                } else {
                    // A thread that never announced itself is defined by its
                    // completion, once.
                    self.thread_definition(
                        *id,
                        *parent_id,
                        *spawn_call_path,
                        *started_at,
                        clock,
                        name.as_ref().map(|name| &***name),
                    );
                    self.needs
                        .thread(id.get(), spawn_call_path.get(), self.sequence);
                }
                self.needs.span(id.get(), 0);
                let (place, running_at) = self.open.thread_completed(id.get());
                let (usage, total_usage) = self.usage.remove(&id.get()).unwrap_or_default();
                let error_cas_id = self.thread_errors.remove(&id.get());
                self.event(
                    thread,
                    Event::ThreadCompletion(proto::ThreadCompletion {
                        completed_at_ticks: completed_at.get(),
                        outcome: flags::outcome(*outcome) as i32,
                        panicked: *outcome == btel_types::InvocationOutcome::Panicked,
                        usage,
                        error_cas_id,
                        parent_id: parent_id.map(TelemetryId::get),
                        spawn_call_path_id: spawn_call_path.get(),
                        started_at_ticks: started_at.get(),
                        clock_epoch_id: clock.metadata().epoch.get(),
                        name: name.as_ref().map(ToString::to_string),
                        running_at_ticks: running_at,
                        total_usage,
                        lineage: place.map(|place| place.wire()),
                    }),
                );
                // A raise that never ended cannot claim anything after its thread.
                self.unwinds.clear_thread(thread);
            }
            SpanRecord::ThreadSpanRunning { at } => {
                self.open.thread_running(thread.get(), at.get());
            }
            SpanRecord::NetworkSpanAnnouncement(span) => {
                let context = self.spans.context_of(thread);
                self.open.request_started(
                    span.id.get(),
                    thread.get(),
                    context,
                    span.parent_id.get(),
                    span.call_path.get(),
                    span.started_at.get(),
                    span.method.to_string(),
                    span.url.to_string(),
                    span.request.as_ref().map(cas_id),
                );
                if self.starts {
                    self.event(
                        thread,
                        Event::NetworkAnnouncement(proto::NetworkAnnouncement {
                            id: span.id.get(),
                            parent_id: span.parent_id.get(),
                            call_path_id: span.call_path.get(),
                            started_at_ticks: span.started_at.get(),
                            method: span.method.to_string(),
                            url: span.url.to_string(),
                            request_cas_id: None,
                        }),
                    );
                }
            }
            SpanRecord::NetworkEvent(event) => {
                if let Some(parked) = self.open.request_event(
                    event.span.get(),
                    proto::NetworkEvent {
                        span_id: 0,
                        name: event.name.as_str().to_owned(),
                        at_ticks: event.at.get(),
                        payload_cas_id: event.payload.as_ref().map(cas_id),
                    },
                ) {
                    self.write_request(parked);
                }
            }
            SpanRecord::NetworkSpanCompletion(done) => {
                // A request whose start this recording never saw cannot be
                // written whole.
                if let Some(request) = self.open.request_completed(done.span.get()) {
                    let done_with = request.events.iter().any(open::finishes);
                    let parked = open::Parked {
                        context: request.context,
                        completion: proto::NetworkCompletion {
                            span_id: done.span.get(),
                            completed_at_ticks: done.completed_at.get(),
                            outcome: flags::outcome(done.outcome) as i32,
                            panicked: done.outcome == btel_types::InvocationOutcome::Panicked,
                            error_cas_id: done.error.as_ref().map(cas_id),
                            thread_id: request.thread,
                            parent_id: request.parent,
                            call_path_id: request.call_path,
                            started_at_ticks: request.started_at,
                            method: request.method,
                            url: request.url,
                            request_cas_id: request.request,
                            events: request.events,
                            lineage: request.place.map(|place| place.wire()),
                        },
                    };
                    if done_with {
                        self.write_request(parked);
                    } else {
                        self.open.park(parked);
                    }
                }
            }
            SpanRecord::ErrorRaiseOrigin { raise_id, origin } => {
                self.error_raise_origin(thread, *raise_id, *origin);
            }
            SpanRecord::ErrorRaiseStack(stack) => self.error_raise_stack(thread, stack),
            SpanRecord::ErrorRaised { .. } => self.error_raised(thread, record),
            SpanRecord::ErrorRaisedAndEnded {
                raise_id,
                result,
                handler_function,
                handler_pc,
                unwound_frames,
                ..
            } => {
                self.error_raised(thread, record);
                self.error_unwind_ended(
                    thread,
                    *raise_id,
                    *result,
                    *handler_function,
                    (*handler_pc != btel_records::NO_PC).then_some(*handler_pc),
                    *unwound_frames,
                );
            }
            SpanRecord::ErrorRaiseFrameCompleted { raise_id } => {
                self.error_raise_frame_completed(thread, *raise_id);
            }
            SpanRecord::ErrorUnwindEnded {
                raise_id,
                result,
                handler_function,
                handler_pc,
                unwound_frames,
            } => self.error_unwind_ended(
                thread,
                *raise_id,
                *result,
                *handler_function,
                *handler_pc,
                *unwound_frames,
            ),
            SpanRecord::FunctionSpanAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                captured_inputs,
                captured_type_args,
            } => self.call_started(
                thread,
                *id,
                *parent_id,
                *call_path,
                *entered_at,
                captured_inputs.as_ref().map(cas_id),
                captured_type_args.as_ref().map(cas_id),
            ),
            SpanRecord::ManualSpanAnnouncement(entry) => {
                self.spans.require(btel_core::recording::minor::OWNED_SPAN);
                self.call_started(
                    thread,
                    entry.id,
                    entry.parent_id,
                    entry.call_path,
                    entry.entered_at,
                    entry.captured_inputs.as_ref().map(cas_id),
                    entry.captured_type_args.as_ref().map(cas_id),
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
                    field::definitions::CALL_PATHS,
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
                // No heap access here. Metadata comes from the owned table supplied
                // at startup, once per recording; functions outside it (runtime
                // definitions, GC-collected ones) keep the explicit placeholder.
                for function in [Some(*callee), *visible_caller].into_iter().flatten() {
                    self.define_function(function);
                }
                self.needs.path(
                    call_path.get(),
                    parent_call_path.get(),
                    callee.get(),
                    self.sequence,
                );
            }
            SpanRecord::ManualSpanCompletion(done) => {
                self.spans.require(btel_core::recording::minor::OWNED_SPAN);
                // A span closed without an outcome is closed, not completed.
                let completion_flags = match done.outcome {
                    Some(outcome) => flags::completed(flags::outcome(outcome), false),
                    None => CompletionFlags::closed(false, false).bits(),
                };
                let start = self
                    .open
                    .call_completed(done.id.get(), done.parent_id.get());
                self.needs.span(thread.get(), done.call_path.get());
                let (usage, total_usage) = self.usage.remove(&done.id.get()).unwrap_or_default();
                self.event(
                    thread,
                    Event::FunctionCompletion(proto::FunctionCompletion {
                        id: done.id.get(),
                        parent_id: done.parent_id.get(),
                        node: CallPathNodeId::new(done.call_path, false).get(),
                        entered_at_ticks: done.entered_at.get(),
                        exited_at_ticks: done.exited_at.get(),
                        self_await_ticks: done.await_time.get().get(),
                        completion_flags,
                        value_cas_id: done.captured_value.as_ref().map(cas_id),
                        panicked: done.outcome == Some(btel_types::InvocationOutcome::Panicked),
                        usage,
                        inputs_cas_id: start.inputs,
                        type_args_cas_id: start.type_args,
                        manual: false,
                        total_usage,
                        lineage: start.place.map(|place| place.wire()),
                    }),
                );
            }
            // Every other completion is a function completion, handled above.
            _ => unreachable!("a function completion is handled before the match"),
        }
    }

    /// A request's end record, whole, in a section of the thread that made
    /// it, under the context it started in.
    fn write_request(&mut self, parked: open::Parked) {
        let completion = parked.completion;
        let thread = TelemetryId::from_raw(completion.thread_id)
            .expect("a request's thread is a telemetry node");
        self.needs
            .span(completion.thread_id, completion.call_path_id);
        self.spans.event_in(
            thread,
            parked.context,
            proto::span_event::Event::NetworkCompletion(completion),
        );
    }

    /// Write the requests that completed in the open file, before it seals.
    fn write_parked(&mut self) {
        if self.open.has_parked() {
            for parked in self.open.take_parked() {
                self.write_request(parked);
            }
        }
    }

    /// A call starts: what its completion will carry, and in a local
    /// recording a slim start record, so a reader can show it running.
    #[allow(clippy::too_many_arguments, reason = "the start fields of one call")]
    fn call_started(
        &mut self,
        thread: TelemetryId,
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: btel_types::CallPathId,
        entered_at: btel_types::ClockInstant,
        inputs: Option<proto::CasId>,
        type_args: Option<proto::CasId>,
    ) {
        self.open
            .call_started(id.get(), parent_id.get(), inputs, type_args);
        if self.starts {
            self.event(
                thread,
                proto::span_event::Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                    id: id.get(),
                    parent_id: parent_id.get(),
                    call_path_id: call_path.get(),
                    entered_at_ticks: entered_at.get(),
                    inputs_cas_id: None,
                    type_args_cas_id: None,
                    manual: false,
                }),
            );
        }
    }

    /// A function completion, whole: its start, its usage and its lineage.
    fn function_completion(
        &mut self,
        thread: TelemetryId,
        done: &btel_records::FunctionCompletionRef<'_, Snapshot>,
    ) {
        use proto::span_event::Event;
        let start = self
            .open
            .call_completed(done.id.get(), done.parent_id.get());
        self.needs.span(thread.get(), done.call_path.get());
        let (usage, total_usage) = if self.usage.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            self.usage.remove(&done.id.get()).unwrap_or_default()
        };
        let completion = proto::FunctionCompletion {
            id: done.id.get(),
            parent_id: done.parent_id.get(),
            node: CallPathNodeId::new(done.call_path, done.reentry).get(),
            entered_at_ticks: done.entered_at.get(),
            exited_at_ticks: done.exited_at.get(),
            self_await_ticks: done.await_time.get().get(),
            completion_flags: flags::completed(flags::outcome(done.outcome), false),
            value_cas_id: done.captured_value.map(cas_id),
            panicked: done.outcome == btel_types::InvocationOutcome::Panicked,
            usage,
            inputs_cas_id: start.inputs,
            type_args_cas_id: start.type_args,
            manual: false,
            total_usage,
            lineage: start.place.map(|place| place.wire()),
        };
        self.event(
            thread,
            if done.late {
                Event::LateFunctionCompletion(completion)
            } else {
                Event::FunctionCompletion(completion)
            },
        );
    }

    fn take(&mut self) -> PendingMessages {
        self.write_parked();
        self.aggregates.drain_into(&mut self.pending.aggregates);
        self.sysops.drain_into(&mut self.pending.aggregates);
        // Reuse warm map capacity, trimming oversized allocations after bursts.
        self.aggregates.trim();
        self.pending_encoded_bytes = 0;
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn cas_id(snapshot: &Snapshot) -> proto::CasId {
    cas_reference(snapshot.root_id())
}

fn cas_reference(bytes: btel_snapshot::CasId) -> proto::CasId {
    proto::CasId {
        low: u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap()),
        high: u64::from_le_bytes(bytes.as_bytes()[8..].try_into().unwrap()),
    }
}
