//! bridge_nodejs - napi-rs Node.js bindings for BAML using bex_engine.
//!
//! This crate provides the same API surface as `bridge_python`
//! but powered by napi-rs instead of PyO3.

mod abort_controller;
mod errors;
pub mod handle;
mod runtime;
mod types;

use napi_derive::napi;

#[napi_derive::module_init]
fn init() {
    // Initialize logging if BAML_TRACE_FILE is set.
    // Unlike language_client_typescript, we don't use baml_log —
    // we follow bridge_python's pattern and rely on bridge_cffi's event sink.
}

#[napi]
pub fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Flush all buffered trace events to the JSONL file (if BAML_TRACE_FILE is set).
#[napi]
pub fn flush_events() {
    bridge_cffi::flush_event_sink();
}
