//! Function-pointer hooks into the ring transport substrate.
//!
//! The transport (rings, registry, consumer thread) lives in `bex_events`;
//! this leaf crate must not depend on it. The backend's three outbound calls
//! — waking the consumer, waking it for a terminal transition, and
//! configuring the process-global transport allocator — go through hooks
//! that `bex_events` installs before it registers the first engine session
//! (`bex_events::prof::backend::register_engine_session` shim).
//!
//! A hook that has not been installed is a no-op: wakes are advisory (the
//! consumer's timed park bounds the latency), and transport configuration
//! before any engine session exists falls back to the registry's defaults.

use std::sync::OnceLock;

use super::ProfilerMemoryGovernor;

pub struct TransportHooks {
    /// Force-wake the consumer thread.
    pub wake_consumer: fn(),
    /// Wake the consumer for a terminal backend transition (ensures the
    /// consumer thread exists first).
    pub wake_for_backend_terminal: fn(),
    /// Configure the process-global transport allocator
    /// (`memory, transport_segment_bytes, transport_freelist_segments`).
    pub configure_transport: fn(ProfilerMemoryGovernor, u64, u32),
}

static HOOKS: OnceLock<TransportHooks> = OnceLock::new();

/// Install the transport hooks. First install wins; later calls are ignored.
pub fn install_transport_hooks(hooks: TransportHooks) {
    let _ = HOOKS.set(hooks);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn wake_consumer() {
    if let Some(hooks) = HOOKS.get() {
        (hooks.wake_consumer)();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn wake_for_backend_terminal() {
    if let Some(hooks) = HOOKS.get() {
        (hooks.wake_for_backend_terminal)();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn configure_transport(
    memory: ProfilerMemoryGovernor,
    segment_bytes: u64,
    freelist_segments: u32,
) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.configure_transport)(memory, segment_bytes, freelist_segments);
    }
}
