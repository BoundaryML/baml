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
    pub(super) recording_id: btel_publisher::RecordingId,
}
impl EngineTelemetry {
    pub(super) fn new_root(&self) -> TelemetryState {
        self.new_state(self.clock.start_run())
    }
    fn new_state(&self, clock: Arc<btel_clock::ClockEpoch>) -> TelemetryState {
        TelemetryState::new_root(
            Arc::clone(&self.policies),
            clock,
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.runtime),
        )
    }
    pub(super) fn new_child(&self, context: &ThreadSpawnContext) -> TelemetryState {
        let mut state = self.new_state(Arc::clone(&context.clock));
        state.configure_spawn(context);
        state
    }
    pub(super) fn validate(&self, vm: &BexVm, force: bool) {
        if let Some(clock) = vm.telemetry_clock() {
            self.clock.validate(clock, force);
        }
    }
}
