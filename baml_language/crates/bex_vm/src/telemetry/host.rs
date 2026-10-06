//! Host bodies use the ordinary host executor. Only entry and exit bind a
//! recording producer; no worker-local scope is retained across host execution.
use super::{
    AwaitDuration, CallPathEdge, FrameTelemetry, InvocationMode, InvocationOutcome,
    LAST_CALL_PATH_ID, REQUIRES_ANNOUNCEMENT, SPAN, SpanRecord, TelemetryState, TimingRecord,
    allocate_call_path_id, allocate_telemetry_id,
};

impl TelemetryState {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture_host(
        &self,
        value: &btel_snapshot::host::HostValue,
    ) -> Option<btel_snapshot::Snapshot> {
        self.capture_host_with(value, |_| None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture_host_with<'a>(
        &self,
        value: &btel_snapshot::host::HostValue,
        resolve: impl Fn(&str) -> Option<&'a btel_snapshot::host::HostDeclaration>,
    ) -> Option<btel_snapshot::Snapshot> {
        self.runtime
            .acquire_snapshot()
            .map(|builder| btel_snapshot::host::capture_with(builder, value, resolve))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn host_recording_scope(&self) -> btel_processor::ExecutionScope {
        self.execution_scope()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn abandon_host(&mut self) {
        self.abandon();
    }

    /// Enter an already registered host definition. Source locations are host
    /// adapter coordinates, never VM pointers or function-object addresses.
    pub fn enter_host(
        &mut self,
        metadata: &btel_types::FunctionMetadata,
        caller_pc: u32,
        mode: InvocationMode,
        captured_inputs: Option<btel_snapshot::Snapshot>,
        reserved_id: Option<btel_types::TelemetryId>,
    ) -> Option<FrameTelemetry> {
        if mode == InvocationMode::Hidden {
            return None;
        }
        let saved_call_path = self.thread.active_call_path;
        let saved_parent_id = self.thread.active_id;
        let call_path = allocate_call_path_id(&LAST_CALL_PATH_ID);
        self.write_span(SpanRecord::HostFunctionDefined(Box::new(metadata.clone())));
        self.write_span(SpanRecord::CallPathDefined {
            call_path,
            parent_call_path: saved_call_path,
            visible_caller: None,
            caller_pc,
            callee: metadata.function_id,
            edge: CallPathEdge::Synchronous,
        });
        let entered_at = self.clock.read();
        let span_id = (mode == InvocationMode::Span)
            .then(|| reserved_id.unwrap_or_else(allocate_telemetry_id));
        let mut flags = 0;
        if let Some(id) = span_id {
            flags |= SPAN;
            if captured_inputs.is_some() {
                flags |= REQUIRES_ANNOUNCEMENT;
            }
            self.thread.active_id = id;
            self.write_span(SpanRecord::FunctionSpanAnnouncement {
                id,
                parent_id: saved_parent_id,
                call_path,
                entered_at,
                captured_inputs,
                captured_type_args: None,
            });
        }
        self.thread.active_call_path = call_path;
        Some(FrameTelemetry {
            entered_at,
            saved_parent_id,
            span_id,
            await_duration: AwaitDuration::ZERO,
            saved_call_path,
            flags,
            output_request: false,
            error_request: false,
        })
    }

    /// Actual host exit owns completion; cancellation of a waiter is not exit.
    pub fn complete_host(
        &mut self,
        frame: FrameTelemetry,
        outcome: InvocationOutcome,
        captured_value: Option<btel_snapshot::Snapshot>,
    ) {
        let exited_at = self.clock.read();
        let call_path = self.thread.active_call_path;
        if let Some(id) = frame.span_id {
            let parent_id = frame.saved_parent_id;
            let entered_at = frame.entered_at;
            let await_time = frame.await_duration;
            let announcement = frame.flags & REQUIRES_ANNOUNCEMENT != 0;
            let record = match (outcome, announcement) {
                (InvocationOutcome::Ok, false) => SpanRecord::FunctionSpanCompletionOk {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                },
                (InvocationOutcome::Ok, true) => {
                    SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement {
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at,
                        await_time,
                        captured_value,
                    }
                }
                (InvocationOutcome::Errored, false) => SpanRecord::FunctionSpanCompletionErrored {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                },
                (InvocationOutcome::Errored, true) => {
                    SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement {
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at,
                        await_time,
                        captured_value,
                    }
                }
                (InvocationOutcome::Cancelled, false) => {
                    SpanRecord::FunctionSpanCompletionCancelled {
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at,
                        await_time,
                        captured_value,
                    }
                }
                (InvocationOutcome::Cancelled, true) => {
                    SpanRecord::FunctionSpanCompletionCancelledNeedsAnnouncement {
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at,
                        await_time,
                        captured_value,
                    }
                }
                (InvocationOutcome::Panicked, false) => {
                    SpanRecord::FunctionSpanCompletionPanicked {
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at,
                        await_time,
                        captured_value,
                    }
                }
                (InvocationOutcome::Panicked, true) => {
                    SpanRecord::FunctionSpanCompletionPanickedNeedsAnnouncement {
                        id,
                        parent_id,
                        call_path,
                        entered_at,
                        exited_at,
                        await_time,
                        captured_value,
                    }
                }
            };
            self.write_span(record);
        } else {
            self.write_timing(TimingRecord::FunctionTimingCompletion {
                call_path,
                entered_at: frame.entered_at,
                exited_at,
                await_time: frame.await_duration,
                outcome,
                reentry: false,
            });
        }
        self.thread.active_id = frame.saved_parent_id;
        self.thread.active_call_path = frame.saved_call_path;
    }
}
