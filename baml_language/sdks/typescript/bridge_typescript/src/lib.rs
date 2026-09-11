//! bridge_typescript - napi-rs Node.js bindings for BAML using bex_engine.
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
pub mod unhandled_spawn;
mod version;

use napi_derive::napi;

#[napi_derive::module_init]
fn init() {
    if let Err(error) = bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::NodeJs,
        bridge_runtime_name: version::BRIDGE_RUNTIME_NAME.to_string(),
        bridge_runtime_version: version::BRIDGE_RUNTIME_VERSION.to_string(),
        toolchain_version: version::TOOLCHAIN_VERSION.to_string(),
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
    get_toolchain_version()
}

#[napi(js_name = "getToolchainVersion")]
pub fn get_toolchain_version() -> &'static str {
    version::TOOLCHAIN_VERSION
}

#[napi(js_name = "getBridgeRuntimeVersion")]
pub fn get_bridge_runtime_version() -> &'static str {
    version::BRIDGE_RUNTIME_VERSION
}

/// No-op: tracing has been removed. Kept as a live symbol for ABI stability.
#[napi]
pub fn flush_events() {}

#[napi(js_name = "shutdownRuntime")]
pub async fn shutdown_runtime() -> napi::Result<()> {
    bridge_cffi::shutdown_runtime()
        .await
        .map_err(errors::bridge_error_to_napi)
}

/// Resolve once the process-global runtime has no active call and no pending
/// spawned future, without closing it. The SDK's `beforeExit` hook awaits
/// this before `shutdownRuntime`: registered host callables no longer pin the
/// event loop (their tsfns are weak), so this pending promise is what keeps
/// Node alive while real BAML work — including a host callback that re-enters
/// the engine — is still in flight. A never-initialized runtime is idle.
#[napi(js_name = "_waitForRuntimeIdle")]
pub async fn wait_for_runtime_idle() -> napi::Result<()> {
    match bridge_cffi::get_runtime() {
        Ok(runtime) => {
            runtime.wait_until_idle().await;
            Ok(())
        }
        Err(bridge_cffi::BridgeError::NotInitialized) => Ok(()),
        Err(e) => Err(errors::bridge_error_to_napi(e)),
    }
}

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
