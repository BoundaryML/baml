//! Explicit operation ownership. Inheritance never keeps the owner alive.
use std::sync::{Arc, Mutex};

use bex_vm_types::{BexRustData, Meter, Value, trace::SpanId};
use btel_records::{ContextReference, ManualSpanAnnouncement, ManualSpanCompletion, SpanRecord};
use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, FunctionMetadata, InvocationMode, InvocationOutcome,
    RecordingId, TelemetryId, allocate_telemetry_id,
};

use super::{LAST_CALL_PATH_ID, TelemetryState, allocate_call_path_id, snapshot};

#[derive(Clone)]
#[cfg_attr(
    target_arch = "wasm32",
    expect(dead_code, reason = "wasm publishes no completions")
)]
pub(crate) struct OperationOptions {
    pub mode: InvocationMode,
    pub reserved_id: Option<TelemetryId>,
    pub inputs: bool,
    pub output: bool,
    pub error: bool,
    pub policy: super::TelemetryPolicy,
    pub(super) live_policy: Option<super::policy::OwnedPolicy>,
}

impl Default for OperationOptions {
    fn default() -> Self {
        Self {
            mode: InvocationMode::Span,
            reserved_id: None,
            inputs: true,
            output: true,
            error: true,
            policy: super::TelemetryPolicy::NONE,
            live_policy: None,
        }
    }
}

pub(crate) struct ManualSpan {
    pub(crate) scope: RecordingId,
    pub(crate) context: btel_types::context::Context,
    pub(crate) id: Option<SpanId>,
    // Separate from the recording owner so activation never retains ownership.
    template: Option<ManualContext>,
    #[allow(
        clippy::option_option,
        reason = "outer None is closed; Some(None) is open with recording disabled"
    )]
    inner: Mutex<Option<Option<Recording>>>,
}

/// Model usage charged to an owned span by the threads that ran under it,
/// written just before its completion.
type OwnedUsage = Arc<Mutex<Vec<btel_records::ModelUsage>>>;

#[derive(Clone)]
struct ManualContext {
    await_time: Arc<Mutex<AwaitDuration>>,
    // Present for a span; a Timing operation's usage belongs to its parent.
    usage: Option<OwnedUsage>,
    clock: Arc<btel_clock::ClockEpoch>,
    id: TelemetryId,
    call_path: CallPathId,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: Arc<btel_processor::TelemetryRuntime>,
}

/// Only ancestry and context change inside `run()`; execution and its clock stay put.
pub(crate) struct ManualScope {
    await_time: Arc<Mutex<AwaitDuration>>,
    // The span this scope ran under and its usage, which leaves the thread
    // when the scope does.
    usage: Option<(TelemetryId, OwnedUsage)>,
    clock: Arc<btel_clock::ClockEpoch>,
    parent_id: TelemetryId,
    call_path: CallPathId,
    context: btel_types::context::Context,
}

impl ManualScope {
    pub(crate) fn add_await(
        &self,
        elapsed: btel_types::ClockDuration,
        source: &btel_clock::ClockEpoch,
    ) {
        let mut total = self
            .await_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *total = total.saturating_add(self.clock.rescale_duration_from(elapsed, source));
    }

    pub(crate) fn restore(self, state: &mut TelemetryState) {
        if let Some((id, usage)) = &self.usage
            && let Some(entries) = state.take_model_usage(*id)
        {
            super::merge_usage(
                &mut usage
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                entries,
            );
        }
        state.thread.active_id = self.parent_id;
        state.thread.active_call_path = self.call_path;
        state.set_context(self.context);
    }
}

#[cfg_attr(
    target_arch = "wasm32",
    expect(dead_code, reason = "wasm publishes no completions")
)]
struct Recording {
    await_time: Arc<Mutex<AwaitDuration>>,
    usage: Option<OwnedUsage>,
    options: OperationOptions,
    thread: btel_types::TelemetryId,
    context: ContextReference,
    /// The snapshot `context` names, when the completion must deliver it.
    context_payload: Option<btel_snapshot::Snapshot>,
    clock: Arc<btel_clock::ClockEpoch>,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: Arc<btel_processor::TelemetryRuntime>,
    completion: ManualSpanCompletion<btel_snapshot::Snapshot>,
}

impl ManualSpan {
    /// # Safety
    /// Values and definition metadata are rooted under the calling heap permit.
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn start_with_options(
        state: Option<&mut TelemetryState>,
        scope: RecordingId,
        context: btel_types::context::Context,
        metadata: &FunctionMetadata,
        inputs: Option<Value>,
        parent: Option<&Self>,
        options: OperationOptions,
        captured_type_args: Option<btel_snapshot::Snapshot>,
    ) -> Self {
        let mut template = None;
        let recording = state
            .filter(|_| options.mode != InvocationMode::Hidden)
            .map(|state| {
                let entered_at = state.clock.read();
                let id = options.reserved_id.unwrap_or_else(allocate_telemetry_id);
                let parent = parent.and_then(|parent| parent.template.as_ref());
                let parent_id = parent.map_or(state.thread.active_id, |parent| parent.id);
                let parent_call_path =
                    parent.map_or(state.thread.active_call_path, |parent| parent.call_path);
                let call_path = allocate_call_path_id(&LAST_CALL_PATH_ID);
                #[cfg(not(target_arch = "wasm32"))]
                let _scope = state.execution_scope();
                state.write_span(SpanRecord::HostFunctionDefined(Box::new(metadata.clone())));
                state.write_span(SpanRecord::CallPathDefined {
                    call_path,
                    parent_call_path,
                    visible_caller: None,
                    caller_pc: 0,
                    callee: metadata.function_id,
                    edge: CallPathEdge::Synchronous,
                });
                let captured_inputs = {
                    (options.mode == InvocationMode::Span && options.inputs)
                        .then(|| {
                            inputs.and_then(|value| state.capture(snapshot::Input::Value(value)))
                        })
                        .flatten()
                };
                let requires_announcement =
                    captured_inputs.is_some() || captured_type_args.is_some();
                let old_context = state.context.clone();
                state.set_context(context.clone());
                if options.mode == InvocationMode::Span {
                    state.write_span(SpanRecord::ManualSpanAnnouncement(Box::new(
                        ManualSpanAnnouncement {
                            id,
                            parent_id,
                            call_path,
                            entered_at,
                            captured_inputs,
                            captured_type_args,
                        },
                    )));
                }
                // The operation's own context, not the caller's: the
                // announcement may not have delivered it (Timing writes none).
                #[cfg(all(not(test), not(target_arch = "wasm32")))]
                let (recorded_context, context_payload) = state.context_reference();
                #[cfg(any(test, target_arch = "wasm32"))]
                let (recorded_context, context_payload) = (ContextReference::Unavailable, None);
                state.set_context(old_context);
                let await_time = Arc::new(Mutex::new(AwaitDuration::ZERO));
                let usage = (options.mode == InvocationMode::Span)
                    .then(|| Arc::new(Mutex::new(Vec::new())));
                template = Some(ManualContext {
                    await_time: await_time.clone(),
                    usage: usage.clone(),
                    clock: state.clock.clone(),
                    id: if options.mode == InvocationMode::Span {
                        id
                    } else {
                        parent_id
                    },
                    call_path,
                    #[cfg(not(target_arch = "wasm32"))]
                    runtime: state.runtime.clone(),
                });
                state.clock.attach_thread();
                Recording {
                    await_time,
                    usage,
                    options,
                    thread: state.thread.id,
                    context: recorded_context,
                    context_payload,
                    clock: state.clock.clone(),
                    #[cfg(not(target_arch = "wasm32"))]
                    runtime: state.runtime.clone(),
                    completion: ManualSpanCompletion {
                        await_time: AwaitDuration::ZERO,
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at: entered_at,
                        outcome: None,
                        requires_announcement,
                        captured_value: None,
                    },
                }
            });
        Self {
            scope,
            context,
            id: recording
                .as_ref()
                .filter(|r| r.options.mode == InvocationMode::Span)
                .map(|r| SpanId::new(scope, r.completion.id)),
            template,
            inner: Mutex::new(Some(recording)),
        }
    }

    pub(crate) fn into_owner(self) -> Arc<Self> {
        let owner = Arc::new(self);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(template) = &owner.template {
            let erased: Arc<dyn btel_processor::SpanOwner> = owner.clone();
            template
                .runtime
                .register_span_owner(Arc::downgrade(&erased));
        }
        owner
    }

    #[cfg_attr(
        target_arch = "wasm32",
        expect(dead_code, reason = "wasm publishes no completions")
    )]
    fn close_deferred(&self) {
        if let Some(mut recording) = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .flatten()
        {
            recording.completion.exited_at = recording.clock.read();
            #[cfg(not(target_arch = "wasm32"))]
            recording.publish(true);
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
    }

    pub(crate) fn enter(&self, state: &mut TelemetryState) -> Option<ManualScope> {
        let template = self.template.as_ref()?;
        let saved = ManualScope {
            await_time: template.await_time.clone(),
            usage: template.usage.clone().map(|usage| (template.id, usage)),
            clock: template.clock.clone(),
            parent_id: state.thread.active_id,
            call_path: state.thread.active_call_path,
            context: state.context.clone(),
        };
        state.thread.active_id = template.id;
        state.thread.active_call_path = template.call_path;
        state.set_context(self.context.clone());
        Some(saved)
    }

    /// # Safety
    /// Caller holds the heap permit. Only the winning completion traverses value.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(unused_variables, reason = "wasm publishes no completions")
    )]
    pub(crate) unsafe fn finish(&self, outcome: Option<InvocationOutcome>, value: Option<Value>) {
        let recording = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .flatten();
        if let Some(mut recording) = recording {
            recording.completion.exited_at = recording.clock.read();
            recording.completion.outcome = outcome;
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _scope = recording.runtime.enter();
                let policy = recording.current_policy();
                if let Some(value) = value.filter(|_| match outcome {
                    Some(InvocationOutcome::Ok) => {
                        recording.options.output || policy.capture_output
                    }
                    Some(_) => recording.options.error || policy.capture_error,
                    None => false,
                }) {
                    recording.completion.captured_value =
                        recording.runtime.acquire_snapshot().map(|builder| {
                            // SAFETY: the native entry holds the heap permit and roots value.
                            unsafe {
                                snapshot::Scratch::default()
                                    .capture(builder, snapshot::Input::Value(value))
                            }
                        });
                }
                recording.publish(false);
            }
        }
    }
}

impl Recording {
    #[cfg_attr(
        target_arch = "wasm32",
        expect(dead_code, reason = "wasm publishes no completions")
    )]
    fn current_policy(&self) -> super::TelemetryPolicy {
        self.options
            .live_policy
            .as_ref()
            .map_or(self.options.policy, super::policy::OwnedPolicy::get)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn publish(&mut self, deferred: bool) {
        self.completion.await_time = *self
            .await_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.options.mode == InvocationMode::Timing {
            let Some(outcome) = self.completion.outcome else {
                return;
            };
            if !super::policy::promotes(
                self.current_policy(),
                self.completion
                    .entered_at
                    .elapsed_until(self.completion.exited_at),
                outcome,
                self.clock.domain(),
            ) {
                self.runtime.write_timing(
                    self.thread,
                    btel_records::TimingRecord::FunctionTimingCompletion {
                        call_path: self.completion.call_path,
                        entered_at: self.completion.entered_at,
                        exited_at: self.completion.exited_at,
                        await_time: self.completion.await_time,
                        outcome,
                        reentry: false,
                    },
                );
                return;
            }
        }
        let placeholder = ManualSpanCompletion {
            await_time: AwaitDuration::ZERO,
            id: self.completion.id,
            parent_id: self.completion.parent_id,
            call_path: self.completion.call_path,
            entered_at: self.completion.entered_at,
            exited_at: self.completion.exited_at,
            outcome: None,
            requires_announcement: false,
            captured_value: None,
        };
        let completion = std::mem::replace(&mut self.completion, placeholder);
        let record = if self.options.mode == InvocationMode::Timing {
            let ManualSpanCompletion {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value,
                outcome,
                ..
            } = completion;
            match outcome.expect("only terminal Timing operations promote") {
                InvocationOutcome::Ok => SpanRecord::LateFunctionSpanCompletionOk {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                },
                InvocationOutcome::Errored => SpanRecord::LateFunctionSpanCompletionErrored {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                },
                InvocationOutcome::Panicked => SpanRecord::LateFunctionSpanCompletionPanicked {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                },
                InvocationOutcome::Cancelled => SpanRecord::LateFunctionSpanCompletionCancelled {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                },
            }
        } else {
            SpanRecord::ManualSpanCompletion(Box::new(completion))
        };
        // The usage rides the completion written next.
        let entries = self.usage.as_ref().map_or_else(Vec::new, |usage| {
            std::mem::take(
                &mut *usage
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )
        });
        if !entries.is_empty() {
            let usage = SpanRecord::SpanUsage(Box::new(btel_records::SpanUsage {
                node: self.completion.id,
                entries,
            }));
            if deferred {
                self.runtime.defer_span(self.thread, self.context, usage);
            } else {
                self.runtime
                    .write_span_with_context(self.thread, self.context, None, usage);
            }
        }
        if deferred {
            // A deferred record cannot carry the snapshot it would name.
            let context = if self.context_payload.is_some() {
                ContextReference::Unavailable
            } else {
                self.context
            };
            self.runtime.defer_span(self.thread, context, record);
        } else {
            self.runtime.write_span_with_context(
                self.thread,
                self.context,
                self.context_payload.take(),
                record,
            );
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.clock.finish_thread();
    }
}

impl Drop for ManualSpan {
    fn drop(&mut self) {
        if let Some(mut recording) = self
            .inner
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .flatten()
        {
            recording.completion.exited_at = recording.clock.read();
            #[cfg(not(target_arch = "wasm32"))]
            recording.publish(true);
        }
    }
}

impl BexRustData for ManualSpan {
    fn measure(&self, meter: &mut Meter) {
        meter.bytes(self.context.distinct_id().map_or(0, str::len));
        for (key, value) in self.context.metadata() {
            meter.bytes(key.capacity() + size_of_val(value));
            if let btel_types::context::ContextValue::String(value) = value {
                meter.bytes(value.capacity());
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl btel_processor::SpanOwner for ManualSpan {
    fn close_for_shutdown(&self) {
        self.close_deferred();
    }
}
