//! BEX profiling transport and segmented local backend.
//!
//! Producers write compact fixed-layout records to bounded per-thread rings;
//! the consumer decodes them directly into the CCT/evidence backend.
//!
//! When a segment fills, the producer links a fresh or recycled segment
//! without blocking. Memory is bounded by
//! the derived transport reserve; a record that cannot obtain
//! growth capacity is rejected and reported through boundary health without
//! aborting BAML execution.
//!
//! Capacity model (design D6): the documented 100M events/s figure is a
//! *burst* write budget. The sustainable rate is bounded by the consumer's
//! per-core transcode rate (measured in the consumer milestone), and burst
//! tolerance is `max_overflow_bytes / (produce_rate − drain_rate)` seconds of
//! backlog growth.
//!
//! Naming note: `sys_types::CallId` identifies one *engine root invocation*
//! and is unrelated to the per-function-call ids that flow through these
//! records as plain `u64`s. The id newtypes ([`crate::ids::BexCallId`],
//! [`crate::ids::BexThreadId`], [`crate::ids::FunctionId`]) landed with the
//! M0 `ids.rs` milestone; adopting them in [`record::RawRecord`]'s fields
//! is the remaining follow-up. Nothing here should reuse
//! `sys_types::CallId`.

pub use bex_prof_store::prof::{clock, config, record};

#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub(crate) mod consumer;
pub(crate) mod registry;
pub(crate) mod ring;
pub(crate) mod sync;
pub(crate) mod wake;

/// The segmented store backend, re-exported from the `bex_prof_store` leaf
/// crate so existing callers are unchanged.
///
/// `register_engine_session` is shimmed here: the leaf crate cannot name the
/// ring transport (registry/consumer stay in this crate), so the transport
/// hooks are installed at the one choke point every profiled engine passes
/// through before any backend work can need a wake.
pub mod backend {
    pub use bex_prof_store::prof::backend::*;

    #[cfg(not(target_arch = "wasm32"))]
    pub fn register_engine_session(
        engine_id: crate::ids::EngineId,
        session: &std::sync::Arc<ProfilerSession>,
    ) {
        #[cfg(not(baml_loom))]
        crate::prof::install_transport_hooks_once();
        bex_prof_store::prof::backend::register_engine_session(engine_id, session);
    }
}

/// Install the leaf crate's transport hooks (idempotent; first install wins).
#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub(crate) fn install_transport_hooks_once() {
    use bex_prof_store::prof::backend::hooks::{TransportHooks, install_transport_hooks};
    install_transport_hooks(TransportHooks {
        wake_consumer: || registry::global_ctx().wake().force_wake(),
        wake_for_backend_terminal: consumer::wake_for_backend_terminal,
        configure_transport: registry::configure_global_transport,
    });
}

#[cfg(test)]
mod concurrency_tests;

pub use config::ProfConfig;
#[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
pub use consumer::{consumer_thread_started, engine_closed, flush_and_join};
#[cfg(not(baml_loom))]
pub use registry::ring_for_engine;
pub use ring::{Ring, RingHandle};

// wasm32 has no native background consumer. Generic embedders keep profiling
// off through config, while adapters such as bridge_wasm may opt into a
// cooperative drain; this function remains a no-op because there is no
// background thread to flush.
#[cfg(target_arch = "wasm32")]
pub fn flush_and_join(_timeout: std::time::Duration) -> bool {
    true
}

/// WASM has no native consumer, so engine close has nothing to release.
#[cfg(target_arch = "wasm32")]
pub fn engine_closed(_engine_id: u64) {}
