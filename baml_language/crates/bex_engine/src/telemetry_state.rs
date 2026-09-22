//! Resources exist only when engine telemetry is enabled.
use std::sync::Arc;

use bex_vm::{
    BexVm,
    telemetry::{TelemetryPolicies, TelemetryState, ThreadSpawnContext},
};

pub(crate) struct EngineTelemetry {
    pub(super) clock: btel_clock::ClockRuntime,
    pub(super) policies: Arc<TelemetryPolicies>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) runtime: Arc<btel_processor::TelemetryRuntime>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) recording_id: Option<btel_publisher::RecordingId>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) delivery: Option<super::telemetry::RecordingDelivery>,
}
impl EngineTelemetry {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        let processing = self.runtime.result();
        let Some(sink) = &self.delivery else {
            return processing;
        };
        let delivery = sink.result();
        match (processing, delivery) {
            (_, Some(Err(error))) | (Some(Err(error)), _) => Some(Err(error)),
            (Some(Ok(())), Some(Ok(()))) => Some(Ok(())),
            _ => None,
        }
    }

    pub(super) fn new_root(&self) -> Option<TelemetryState> {
        #[cfg(not(target_arch = "wasm32"))]
        if self.runtime.is_disabled() {
            return None;
        }
        Some(self.new_state(self.clock.start_run()))
    }
    fn new_state(&self, clock: Arc<btel_clock::ClockEpoch>) -> TelemetryState {
        TelemetryState::new_root(
            Arc::clone(&self.policies),
            clock,
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.runtime),
        )
    }
    pub(super) fn new_child(&self, context: Option<&ThreadSpawnContext>) -> Option<TelemetryState> {
        #[cfg(not(target_arch = "wasm32"))]
        if self.runtime.is_disabled() {
            return None;
        }
        let context = context.expect("enabled parent supplies telemetry spawn context");
        let mut state = self.new_state(Arc::clone(&context.clock));
        state.configure_spawn(context);
        Some(state)
    }
    pub(super) fn validate(&self, vm: &BexVm, force: bool) {
        if let Some(clock) = vm.telemetry_clock() {
            self.clock.validate(clock, force);
        }
    }
}
