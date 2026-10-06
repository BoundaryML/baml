//! Resources exist only when engine telemetry is enabled.
use std::sync::Arc;

use bex_vm::{
    BexVm,
    telemetry::{TelemetryPolicies, TelemetryState, ThreadSpawnContext},
};

pub(crate) struct EngineTelemetry {
    pub(super) clock: Arc<btel_clock::ClockRuntime>,
    pub(super) policies: Arc<TelemetryPolicies>,
    /// Events of traced HTTP requests, queued by the IO side and written by
    /// whichever VM thread runs the next sys-op. Holds raw header values:
    /// never handed to anything but the IO side's `NetworkContext`.
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
                        namespace: vec![],
                        argument_layout: None,
                        source_map: None,
                    })
            })
            .as_ref()
        else {
            return;
        };

        let level = match stats.level {
            bex_heap::CollectionLevel::Minor => "minor",
            bex_heap::CollectionLevel::Major => "major",
        };
        let mut patch = ContextPatch::default();
        for (key, value) in [
            ("collection_level", ContextValue::String(level.into())),
            ("trigger", ContextValue::String(reason.into())),
        ] {
            patch.metadata.insert(key.into(), Some(value));
        }
        // These are slots, including unused TLAB reservations, not object deaths.
        for (key, count) in [
            ("live_slots", Some(stats.live_count)),
            ("reclaimed_slots", Some(stats.collected_count)),
            ("promoted_to_gen1", Some(stats.promoted_to_gen1)),
            ("promoted_to_gen2", Some(stats.promoted_to_gen2)),
            ("live_payload_bytes", stats.live_payload_bytes),
        ] {
            if let Some(value) = count.and_then(|count| i64::try_from(count).ok()) {
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
