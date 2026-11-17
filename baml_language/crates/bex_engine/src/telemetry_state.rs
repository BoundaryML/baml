//! Resources exist only when engine telemetry is enabled.
use std::sync::Arc;

use bex_vm::{
    BexVm,
    telemetry::{TelemetryPolicies, TelemetryState, ThreadSpawnContext},
};

pub(crate) struct EngineTelemetry {
    pub(super) clock: btel_clock::ClockRuntime,
    pub(super) policies: Arc<TelemetryPolicies>,
    /// Serialized heap-free native recording, with a VM-safe fallback queue.
    /// Raw header/body values are sanitized and permission-filtered on record.
    pub(super) network: Arc<sys_types::network::NetworkQueue>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) runtime: Arc<btel_processor::TelemetryRuntime>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) recording_id: Option<btel_recorder::RecordingId>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) delivery: Option<super::telemetry::RecordingDelivery>,
    /// Where the host records how the process ends; `None` without a recording.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) process_exit: Option<Arc<btel_types::ProcessExitSlot>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) gc_function: std::sync::OnceLock<Option<btel_types::FunctionMetadata>>,
    /// Where shutdown appends this engine's diagnostics row, when the host
    /// asked for one at construction.
    #[cfg(all(not(target_arch = "wasm32"), feature = "telemetry_diagnostics"))]
    pub(super) diagnostics_file: Option<std::path::PathBuf>,
}

/// No producer or snapshot is acquired while the heap is parked. Keeping the
/// clock attached lets us publish the original pause timestamps afterwards.
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct GcTelemetry(Option<TelemetryState>);

#[cfg(not(target_arch = "wasm32"))]
impl GcTelemetry {
    pub(super) fn start(telemetry: Option<&EngineTelemetry>) -> Self {
        Self(EngineTelemetry::new_root(telemetry))
    }

    pub(super) fn finish(
        &mut self,
        telemetry: Option<&EngineTelemetry>,
        heap: &bex_heap::BexHeap,
        stats: &bex_heap::GcStats,
        reason: &'static str,
    ) {
        use btel_types::{
            FunctionMetadata, RuntimeFunctionKind, RuntimeFunctionOrigin,
            context::{Context, ContextPatch, ContextValue},
        };

        let (Some(state), Some(telemetry)) = (&mut self.0, telemetry) else {
            return;
        };
        // Capture the end before metadata allocation, snapshots or publication.
        let completed_at = state.clock().read();
        telemetry.clock.validate(state.clock(), true);
        let Some(metadata) = telemetry
            .gc_function
            .get_or_init(|| {
                heap.allocate_host_function_id()
                    .ok()
                    .map(|function_id| FunctionMetadata {
                        function_id,
                        fqn: "baml.gc".into(),
                        display_name: "baml.gc".into(),
                        source_file: None,
                        source_span: None,
                        kind: RuntimeFunctionKind::Native,
                        origin: RuntimeFunctionOrigin::Builtin,
                        owner_type: None,
                        parent_function: None,
                        lambda_path: None,
                        definition_key: None,
                        package_name: Some("baml".into()),
                        compilation_unit_hash: None,
                        namespace: vec![],
                        argument_layout: None,
                        source_map: None,
                        model_invoke: false,
                    })
            })
            .as_ref()
        else {
            return;
        };

        let mut patch = ContextPatch::default();
        // Every collection is a full one, which promotes each survivor
        // straight to the survivor space. The level and promotion keys stay,
        // with what a full collection always reports, so recorded telemetry
        // keeps one schema.
        for (key, value) in [
            ("collection_level", ContextValue::String("major".into())),
            ("trigger", ContextValue::String(reason.into())),
        ] {
            patch.metadata.insert(key.into(), Some(value));
        }
        // These are slots, including unused TLAB reservations, not object deaths.
        for (key, count) in [
            ("live_slots", stats.live_count),
            ("reclaimed_slots", stats.collected_count),
            ("promoted_to_gen1", 0),
            ("promoted_to_gen2", stats.live_count),
            ("live_payload_bytes", stats.live_payload_bytes),
        ] {
            if let Ok(value) = i64::try_from(count) {
                patch
                    .metadata
                    .insert(key.into(), Some(ContextValue::Int(value)));
            }
        }
        state.set_context(Context::default().with_patch(&patch));
        let _scope = state.host_recording_scope();
        state.complete_runtime_root(metadata, completed_at);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for GcTelemetry {
    fn drop(&mut self) {
        if let Some(state) = &mut self.0 {
            state.abandon_host();
        }
    }
}
impl EngineTelemetry {
    #[cfg(not(target_arch = "wasm32"))]
    /// A heap-free recording producer makes native EOF/error visible even
    /// while every BAML VM is idle. The queue serializes calls to this writer.
    pub(super) fn start_network_recording(&self) {
        use bex_vm::telemetry::{ClockInstant, InvocationOutcome, TelemetryId};
        use std::sync::Mutex;
        use sys_types::network::NetworkEventKind;
        struct Writer {
            state: Mutex<TelemetryState>,
            used: std::sync::atomic::AtomicBool,
        }
        impl Writer {
            fn record(&self, event: &sys_types::network::NetworkEvent) {
                self.used.store(true, std::sync::atomic::Ordering::Relaxed);
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let _scope = state.host_recording_scope();
                let Some(span) = TelemetryId::from_raw(event.span) else {
                    return;
                };
                let at = ClockInstant::from_ticks(event.at_ticks);
                match &event.kind {
                    NetworkEventKind::Complete { error } => {
                        state.complete_network_io(span, at, error.as_ref());
                    }
                    kind => {
                        state.network_event(span, at, kind);
                        if *kind == NetworkEventKind::Drop {
                            // SAFETY: owned network metadata contains no VM values.
                            unsafe {
                                state.close_network_span(
                                    span,
                                    at,
                                    InvocationOutcome::Cancelled,
                                    None,
                                    &[],
                                );
                            }
                        }
                    }
                }
            }
        }
        impl Drop for Writer {
            fn drop(&mut self) {
                let state = self
                    .state
                    .get_mut()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let _scope = state.host_recording_scope();
                if self.used.load(std::sync::atomic::Ordering::Relaxed) {
                    state.complete_thread(InvocationOutcome::Ok);
                } else {
                    state.abandon_host();
                }
            }
        }
        let Some(mut state) = Self::new_root(Some(self)) else {
            return;
        };
        state.name_root_thread("baml.http.response_readers");
        let writer = Writer {
            state: Mutex::new(state),
            used: std::sync::atomic::AtomicBool::new(false),
        };
        self.network
            .set_recorder(Arc::new(move |event| writer.record(&event)));
    }

    pub(super) fn result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        let processing = self.runtime.result();
        let Some(delivery) = &self.delivery else {
            return processing;
        };
        let delivery = delivery.result();
        match (processing, delivery) {
            (_, Some(Err(error))) | (Some(Err(error)), _) => Some(Err(error)),
            (Some(Ok(())), Some(Ok(()))) => Some(Ok(())),
            _ => None,
        }
    }

    pub(super) fn new_root(telemetry: Option<&Self>) -> Option<TelemetryState> {
        let telemetry = telemetry?;
        #[cfg(not(target_arch = "wasm32"))]
        if telemetry.runtime.is_disabled() {
            return None;
        }
        Some(telemetry.new_state(telemetry.clock.start_run()))
    }
    fn new_state(&self, clock: Arc<btel_clock::ClockEpoch>) -> TelemetryState {
        TelemetryState::new_root(
            Arc::clone(&self.policies),
            clock,
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.runtime),
        )
    }
    pub(super) fn new_child(
        telemetry: Option<&Self>,
        context: Option<&ThreadSpawnContext>,
    ) -> Option<TelemetryState> {
        let telemetry = telemetry?;
        #[cfg(not(target_arch = "wasm32"))]
        if telemetry.runtime.is_disabled() {
            return None;
        }
        let context = context.expect("enabled parent supplies telemetry spawn context");
        let mut state = telemetry.new_state(Arc::clone(&context.clock));
        state.configure_spawn(context);
        Some(state)
    }
    pub(super) fn validate(&self, vm: &BexVm, force: bool) {
        if let Some(clock) = vm.telemetry_clock() {
            self.clock.validate(clock, force);
        }
    }
}
