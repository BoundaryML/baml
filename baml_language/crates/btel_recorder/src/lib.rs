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
mod defs;
mod encoding;
mod errors;
mod flags;
mod functions;
mod merge;
mod open;
mod recording;
pub use btel_core::recording::CompletionFlags;
/// The recording format's messages, as `btel-core` defines them: the wire, never the VM's
/// in-memory layout. Scalar span messages encode while borrowed; no input references escape the
/// callback.
pub use btel_core::recording::v2 as proto;
pub use recording::{
    ProcessRecording, RecordingBuilder, RecordingConfig, RecordingError, RecordingId,
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
    /// The definitions the open file references, and what later files may need.
    defs: defs::FileDefinitions,
    /// The ancestors of the last function completion written, kept for the next.
    ancestors: Vec<u64>,
    /// Events a request keeps; the rest are counted.
    max_request_events: usize,
    /// Aggregate deltas whose call path the last sealed file did not know,
    /// for a later one.
    held: proto::AggregateBatch,
    /// How many seals each held delta's path has waited.
    holds: rustc_hash::FxHashMap<u32, u8>,
    /// Span records held back for definitions not known yet.
    deferred: Vec<Deferred>,
    /// Raises held back until their threads are defined, with the seals
    /// each waited.
    held_raises: Vec<HeldRaise>,
    /// Span records dropped in the open file, their definitions never known.
    dropped_spans: u64,
    /// Network events dropped in the open file: their request was written
    /// already, or never started here.
    dropped_network_events: u64,
    /// Tests of encoding alone write every record and delta, defined or not.
    #[cfg(test)]
    write_undefined: bool,
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
            defs: defs::FileDefinitions::default(),
            ancestors: Vec::new(),
            max_request_events: btel_settings::publisher::MAX_REQUEST_EVENTS,
            held: proto::AggregateBatch::default(),
            holds: rustc_hash::FxHashMap::default(),
            deferred: Vec::new(),
            held_raises: Vec::new(),
            dropped_spans: 0,
            dropped_network_events: 0,
            #[cfg(test)]
            write_undefined: false,
        }
    }
}

/// About what a thread's start record takes in a file beside its definition:
/// a section of its own (its keys, padded length and thread ID) and the
/// empty event.
const THREAD_START_BYTES: usize = 13;

/// Where the open file's definitions go: [`defs::Out`] of a [`ConversionBuffer`].
macro_rules! out {
    ($buffer:expr) => {
        &mut defs::Out {
            definitions: &mut $buffer.pending.definitions,
            clock_states: &mut $buffer.pending.clock_states.states,
            bytes: &mut $buffer.pending_encoded_bytes,
            functions: &mut $buffer.functions,
        }
    };
}

impl ConversionBuffer {
    /// An event of `thread` at call path `path` (0 for none), written into a
    /// section of the thread with the definitions it needs, or held until
    /// they are known.
    /// `context` is the context of the span's start, which its end record is
    /// written under; `None` for the thread's current one.
    #[allow(clippy::option_option, reason = "outer None: the current context")]
    fn emit(
        &mut self,
        thread: TelemetryId,
        path: u32,
        function: Option<btel_types::FunctionId>,
        context: Option<Option<proto::thread_section::Context>>,
        event: proto::span_event::Event,
    ) {
        if self.ready(thread.get(), path) {
            self.reference_thread(thread.get());
            self.reference_path(path);
            if let Some(function) = function {
                self.define_function(function);
            }
            match context {
                Some(context) => self.spans.event_in(thread, context, &event),
                None => self.spans.event(thread, &event),
            }
        } else {
            let context = context.unwrap_or_else(|| self.spans.context_of(thread));
            self.hold(thread, path, function, context, event);
        }
    }

    /// Every definition a record of `thread` at `path` references is known.
    #[inline]
    fn ready(&mut self, thread: u64, path: u32) -> bool {
        #[cfg(test)]
        if self.write_undefined {
            return true;
        }
        self.defs.ready(thread, path)
    }

    /// `thread` selects `context`: a section now when the thread is defined,
    /// else only for the events that follow, which wait for it.
    fn select_context(
        &mut self,
        thread: TelemetryId,
        context: Option<proto::thread_section::Context>,
    ) {
        if self.ready(thread.get(), 0) {
            self.reference_thread(thread.get());
            self.spans.select_context(thread, context);
        } else {
            self.spans.remember_context(thread, context);
        }
    }

    /// Holds back `event`, which references a definition not known yet:
    /// records of one thread can reach the recorder out of order.
    #[cold]
    #[inline(never)]
    fn hold(
        &mut self,
        thread: TelemetryId,
        path: u32,
        function: Option<btel_types::FunctionId>,
        context: Option<proto::thread_section::Context>,
        event: proto::span_event::Event,
    ) {
        self.deferred.push(Deferred {
            thread,
            path,
            function,
            context,
            event,
            holds: 0,
        });
    }

    /// The held events whose definitions are now known, written; the others
    /// wait, up to [`MAX_HOLDS`] seals and never past the `last` file, then are
    /// dropped and counted.
    fn write_deferred(&mut self, last: bool) {
        if self.deferred.is_empty() {
            return;
        }
        for mut held in std::mem::take(&mut self.deferred) {
            if self.ready(held.thread.get(), held.path) {
                self.reference_thread(held.thread.get());
                self.reference_path(held.path);
                if let Some(function) = held.function {
                    self.define_function(function);
                }
                self.spans.event_in(held.thread, held.context, &held.event);
            } else if !last && held.holds < MAX_HOLDS {
                held.holds += 1;
                self.deferred.push(held);
            } else {
                self.dropped_spans += 1;
            }
        }
    }

    /// Thread `id` is defined, with its clock epoch, for every file that
    /// references it while it runs. Returns the encoded size of its
    /// definition.
    fn thread_definition(
        &mut self,
        id: TelemetryId,
        parent: Option<TelemetryId>,
        path: btel_types::CallPathId,
        started: btel_types::ClockInstant,
        clock: &Arc<btel_clock::ClockEpoch>,
        name: Option<&str>,
    ) -> usize {
        let definition = proto::ThreadDefinition {
            thread_id: id.get(),
            parent_id: parent.map(TelemetryId::get),
            spawn_call_path_id: path.get(),
            started_at_ticks: started.get(),
            clock_epoch_id: clock.metadata().epoch.get(),
            name: name.map(str::to_owned),
        };
        let bytes = prost::encoding::message::encoded_len(field::definitions::THREADS, &definition);
        self.defs
            .define_thread(definition, || clock::definition(clock.metadata()));
        self.observe_clock(clock);
        bytes
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
        self.defs.observe(state, out!(self));
    }

    /// Report and release epochs that settled since their last observation.
    /// `latest` also reports each still-unsettled epoch's current, non-final
    /// status. True when every observed epoch has been reported final.
    fn observe_unsettled_clocks(&mut self, latest: bool) -> bool {
        let mut reported = Vec::new();
        self.unsettled_clocks.retain(|_, epoch| {
            let state = clock::state(epoch);
            let settled = state.r#final;
            if settled || latest {
                reported.push(state);
            }
            !settled
        });
        for state in reported {
            self.defs.observe(state, out!(self));
        }
        self.unsettled_clocks.is_empty()
    }
}

impl ConversionBuffer {
    /// The open file references `function`: it carries its definition.
    fn define_function(&mut self, function: btel_types::FunctionId) {
        self.defs.function(function, out!(self));
    }

    /// The open file references call path `path` (0 for none).
    #[inline]
    fn reference_path(&mut self, path: u32) {
        self.defs.path(path, out!(self));
    }

    /// The open file references thread `thread`.
    #[inline]
    fn reference_thread(&mut self, thread: u64) {
        self.defs.thread(thread, out!(self));
    }

    fn aggregate(&mut self, delta: AggregateDelta) {
        self.reference_path(delta.node.call_path().get());
        // One recording window before serialization and delivery to its sink.
        // Span callbacks do not enter this map: the processor already counted them.
        self.pending_encoded_bytes = self
            .pending_encoded_bytes
            .saturating_add(self.aggregates.observe(delta, &mut self.pending.aggregates));
    }

    fn sysop_time(&mut self, path: btel_types::CallPathId, elapsed: btel_types::ClockDuration) {
        self.reference_path(path.get());
        self.pending_encoded_bytes =
            self.pending_encoded_bytes
                .saturating_add(
                    self.sysops
                        .observe(path, elapsed, &mut self.pending.aggregates),
                );
    }

    fn is_empty(&self) -> bool {
        self.pending_encoded_bytes == 0
            && self.spans.is_empty()
            && !self.open.has_parked()
            && self.open.start_bytes() == 0
            && self.held.entries.is_empty()
            && self.held.sysop_times.is_empty()
            && self.deferred.is_empty()
            && self.held_raises.is_empty()
    }

    fn encoded_size_hint(&self) -> usize {
        if self.is_empty() {
            return 0;
        }
        self.spans
            .len()
            .saturating_add(self.pending_encoded_bytes)
            .saturating_add(self.open.start_bytes())
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
                let level = match level {
                    btel_records::LogLevel::Info => proto::LogLevel::Info,
                    btel_records::LogLevel::Debug => proto::LogLevel::Debug,
                    btel_records::LogLevel::Warn => proto::LogLevel::Warn,
                    btel_records::LogLevel::Error => proto::LogLevel::Error,
                };
                self.emit(
                    thread,
                    0,
                    Some(*function),
                    None,
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
                self.select_context(
                    thread,
                    captured_context.as_ref().map(|snapshot| {
                        proto::thread_section::Context::ContextCasId(cas_id(snapshot))
                    }),
                );
            }
            SpanRecord::ContextCleared => {
                self.select_context(
                    thread,
                    Some(proto::thread_section::Context::EmptyContext(true)),
                );
            }
            SpanRecord::ContextReferenced { id } => {
                self.select_context(
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
                let definition = self.thread_definition(
                    *id,
                    *parent_id,
                    *spawn_call_path,
                    *started_at,
                    clock,
                    name.as_ref().map(|name| &***name),
                );
                let context = self.spans.context_of(thread);
                // Its start record in a section of its own, with its definition.
                self.open.thread_started(
                    id.get(),
                    parent_id.map(TelemetryId::get),
                    context,
                    THREAD_START_BYTES + definition,
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
                name,
            } => {
                assert_eq!(thread, *id, "thread definition disagrees with selector");
                if self.defs.has_thread(id.get()) {
                    self.observe_clock(clock);
                } else {
                    // A thread that never announced itself is defined by its
                    // completion.
                    self.thread_definition(
                        *id,
                        *parent_id,
                        *spawn_call_path,
                        *started_at,
                        clock,
                        name.as_ref().map(|name| &***name),
                    );
                }
                self.defs.thread_finished(id.get());
                let end = self.open.thread_completed(id.get());
                let (usage, total_usage) = self.usage.remove(&id.get()).unwrap_or_default();
                let error_cas_id = self.thread_errors.remove(&id.get());
                self.emit(
                    thread,
                    0,
                    None,
                    end.context,
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
                        running_at_ticks: end.running_at,
                        total_usage,
                        lineage: end.place.map(|place| place.wire()),
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
                    self.max_request_events,
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
                            dropped_events: request.dropped_events,
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
                self.defs.define_path(
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
                    *callee,
                    *visible_caller,
                );
                // Written into each file that references the path, with its
                // functions. No heap access here: metadata comes from the owned
                // table supplied at startup; functions outside it (runtime
                // definitions, GC-collected ones) keep the explicit placeholder.
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
                let (usage, total_usage) = self.usage.remove(&done.id.get()).unwrap_or_default();
                self.emit(
                    thread,
                    done.call_path.get(),
                    None,
                    start.context,
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
        let path = completion.call_path_id;
        let event = proto::span_event::Event::NetworkCompletion(completion);
        if self.ready(thread.get(), path) {
            self.reference_thread(thread.get());
            self.reference_path(path);
            self.spans.event_in(thread, parked.context, &event);
        } else {
            self.hold(thread, path, None, parked.context, event);
        }
    }

    /// Write the requests that completed in the open file, before it seals.
    fn write_parked(&mut self) {
        if self.open.has_parked() {
            for parked in self.open.take_parked() {
                self.write_request(parked);
            }
        }
    }

    /// A call starts: what its completion will carry, and the fields of the
    /// slim start record a file carries if the call is still open when it
    /// seals, so a reader can show it running.
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
        let start = open::Start {
            thread: thread.get(),
            context: self.spans.context_of(thread),
            record: open::StartRecord::Call {
                parent: parent_id.get(),
                call_path: call_path.get(),
                entered_at: entered_at.get(),
            },
            bytes: open::CALL_START_BYTES,
        };
        self.open
            .call_started(id.get(), parent_id.get(), inputs, type_args, start);
    }

    /// The slim start records of the spans still open as the file seals,
    /// which no earlier file carries.
    fn write_open_starts(&mut self) {
        use proto::span_event::Event;
        for (id, start) in self.open.take_open_starts() {
            let Some(thread) = TelemetryId::from_raw(start.thread) else {
                continue;
            };
            let path = match &start.record {
                open::StartRecord::Thread => 0,
                open::StartRecord::Call { call_path, .. }
                | open::StartRecord::Request { call_path, .. } => *call_path,
            };
            // A start whose definitions are not known yet: the next file.
            if !self.ready(start.thread, path) {
                self.open.rearm(id, start);
                continue;
            }
            let event = match start.record {
                open::StartRecord::Thread => {
                    Event::ThreadAnnouncement(proto::ThreadAnnouncement {})
                }
                open::StartRecord::Call {
                    parent,
                    call_path,
                    entered_at,
                } => {
                    self.reference_path(call_path);
                    Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                        id,
                        parent_id: parent,
                        call_path_id: call_path,
                        entered_at_ticks: entered_at,
                        ..proto::FunctionAnnouncement::default()
                    })
                }
                open::StartRecord::Request {
                    parent,
                    call_path,
                    started_at,
                    method,
                    url,
                } => {
                    self.reference_path(call_path);
                    Event::NetworkAnnouncement(proto::NetworkAnnouncement {
                        id,
                        parent_id: parent,
                        call_path_id: call_path,
                        started_at_ticks: started_at,
                        method,
                        url,
                        request_cas_id: None,
                    })
                }
            };
            self.reference_thread(start.thread);
            self.spans.event_in(thread, start.context, &event);
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
            lineage: start
                .place
                .map(|place| place.wire_into(std::mem::take(&mut self.ancestors))),
        };
        let event = if done.late {
            Event::LateFunctionCompletion(completion)
        } else {
            Event::FunctionCompletion(completion)
        };
        let path = done.call_path.get();
        if !self.ready(thread.get(), path) {
            let context = start
                .context
                .unwrap_or_else(|| self.spans.context_of(thread));
            self.hold(thread, path, None, context, event);
            return;
        }
        self.reference_thread(thread.get());
        self.reference_path(path);
        // Under the context the call started in, as its start record was.
        match start.context {
            Some(context) => self.spans.event_in(thread, context, &event),
            None => self.spans.event(thread, &event),
        }
        if let Event::FunctionCompletion(completion) | Event::LateFunctionCompletion(completion) =
            event
            && let Some(lineage) = completion.lineage
        {
            self.ancestors = lineage.truncated_ancestor_span_ids;
        }
    }

    /// What the open file holds besides its spans, with the definitions of
    /// everything it references. Its spans are complete after this.
    ///
    /// An aggregate delta whose call path is not defined yet waits for the
    /// next file, unless this is the `last`.
    fn take(&mut self, last: bool) -> PendingMessages {
        self.write_parked();
        self.write_open_starts();
        self.write_deferred(last);
        self.write_held_raises(last);
        self.aggregates.drain_into(&mut self.pending.aggregates);
        self.sysops.drain_into(&mut self.pending.aggregates);
        // Reuse warm map capacity, trimming oversized allocations after bursts.
        self.aggregates.trim();
        // Deltas held back by earlier seals, whose paths they did not know.
        let held = std::mem::take(&mut self.held);
        let holds = std::mem::take(&mut self.holds);
        for path in holds.keys() {
            self.reference_path(*path);
        }
        self.pending.aggregates.entries.extend(held.entries);
        self.pending.aggregates.sysop_times.extend(held.sysop_times);
        // What was referenced before its definition arrived, now defined. A
        // delta whose path, or a path above it, is still unknown waits for a
        // later file, up to MAX_HOLDS times: timing and span records travel
        // apart, and a thread's records written from different OS threads
        // can reach the recorder out of order.
        let missing = self.defs.resolve_missing(out!(self));
        #[cfg(test)]
        let check = !self.write_undefined;
        #[cfg(not(test))]
        let check = true;
        if check && (!missing.is_empty() || !holds.is_empty()) {
            let defs = &mut self.defs;
            // Still incomplete: waits for a later file, or past MAX_HOLDS
            // seals or in the last file, is dropped and counted. A file never
            // references what it does not define.
            let mut fate = |node: u64| match path_of(node) {
                Some(path) if !defs.complete(path) => {
                    if !last && holds.get(&path).copied().unwrap_or(0) < MAX_HOLDS {
                        Fate::Wait
                    } else {
                        Fate::Drop
                    }
                }
                _ => Fate::Write,
            };
            let batch = &mut self.pending.aggregates;
            for entry in std::mem::take(&mut batch.entries) {
                match fate(entry.node) {
                    Fate::Write => batch.entries.push(entry),
                    Fate::Wait => self.held.entries.push(entry),
                    Fate::Drop => {
                        batch.dropped_invocations =
                            batch.dropped_invocations.saturating_add(entry.count);
                    }
                }
            }
            for time in std::mem::take(&mut batch.sysop_times) {
                match fate(time.node) {
                    Fate::Write => batch.sysop_times.push(time),
                    Fate::Wait => self.held.sysop_times.push(time),
                    Fate::Drop => {
                        batch.dropped_sysops = batch.dropped_sysops.saturating_add(time.sysops);
                    }
                }
            }
            let held = (self.held.entries.iter().map(|entry| entry.node))
                .chain(self.held.sysop_times.iter().map(|time| time.node));
            for path in held.filter_map(path_of) {
                self.holds
                    .insert(path, holds.get(&path).copied().unwrap_or(0) + 1);
            }
        }
        self.pending_encoded_bytes = 0;
        // A thread is in use while a request it made is open or a record of
        // it is held: those still need its definitions.
        let held: rustc_hash::FxHashSet<u64> = (self.deferred.iter().map(|held| held.thread.get()))
            .chain(self.held_raises.iter().map(|held| held.message.thread_id))
            .collect();
        let open = &self.open;
        self.defs.seal(web_time::Instant::now(), |thread| {
            held.contains(&thread) || open.has_request_on(thread)
        });
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests;

/// How many seals an aggregate delta waits, at most, for its call path's
/// definition.
const MAX_HOLDS: u8 = defs::HOLD_SEALS;

/// A span record held back until the definitions it references are known.
struct Deferred {
    thread: TelemetryId,
    path: u32,
    /// The function a log names, which the file that writes it defines.
    function: Option<btel_types::FunctionId>,
    context: Option<proto::thread_section::Context>,
    event: proto::span_event::Event,
    /// Seals it has waited.
    holds: u8,
}

/// A raise held back until its thread is defined, with the functions it and
/// its frames name.
pub(crate) struct HeldRaise {
    pub(crate) message: proto::ErrorRaise,
    pub(crate) functions: Vec<btel_types::FunctionId>,
    /// The links and the end that name it, written after it, or dropped
    /// with it: none may name a raise no file has.
    pub(crate) after: Vec<errors::RaiseEvidence>,
    /// Seals it has waited.
    pub(crate) holds: u8,
}

/// What becomes of an aggregate delta as its file seals.
enum Fate {
    Write,
    /// Its call path is not fully defined yet: a later file.
    Wait,
    /// Its call path never was: counted, not written.
    Drop,
}

/// The call path of an aggregate's node.
fn path_of(node: u64) -> Option<u32> {
    btel_core::recording::split_node(node).map(|(path, _)| path)
}

pub(crate) fn cas_id(snapshot: &Snapshot) -> proto::CasId {
    cas_reference(snapshot.root_id())
}

fn cas_reference(bytes: btel_snapshot::CasId) -> proto::CasId {
    proto::CasId {
        low: u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap()),
        high: u64::from_le_bytes(bytes.as_bytes()[8..].try_into().unwrap()),
    }
}
