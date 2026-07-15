//! bridge_nodejs - napi-rs Node.js bindings for BAML using bex_engine.
//!
//! This crate provides the same API surface as `bridge_python`
//! but powered by napi-rs instead of PyO3.

mod baml_call_context;
mod errors;
pub mod handle;
pub mod host_value;
pub mod media;
pub mod runtime;
mod types;

use napi_derive::napi;

#[napi_derive::module_init]
fn init() {
    if let Err(error) = bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::NodeJs,
        sdk_version: baml_version::CANONICAL_VERSION.to_string(),
    }) {
        eprintln!("failed to register BAML Node.js bridge: {error}");
    }

    // Wire the bridge_cffi C entry points to this bridge's per-process
    // Node host-value registry. First-call-wins inside bridge_cffi, so
    // repeated module loads (rare under napi-rs, which loads each addon
    // once per Node process) are harmless.
    bridge_cffi::register_host_dispatch_callback(host_value::host_dispatch_callback);
    bridge_cffi::register_host_release_callback(host_value::host_release_callback);
}

#[napi]
pub fn get_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}

/// No-op: tracing has been removed. Kept as a live symbol for ABI stability.
#[napi]
pub fn flush_events() {}

#[napi(js_name = "newFunctionCall")]
pub fn new_function_call() -> String {
    bridge_cffi::new_function_call_id().to_string()
}

#[napi(js_name = "cancelFunctionCall")]
pub fn cancel_function_call(call_id: String) -> napi::Result<bool> {
    let id = call_id.parse::<u64>().map_err(|_| {
        napi::Error::new(
            napi::Status::InvalidArg,
            "callId must be a decimal uint64 string",
        )
    })?;
    Ok(bridge_cffi::cancel_function_call_by_id(id))
}
