//! Runtime lifecycle and call execution over the engine's C ABI.
//!
//! Every operation goes through the [`crate::capi::Api`] symbol table of
//! the engine shared library loaded on first use, with the same
//! `BamlOutboundResult` envelope as every other language bridge on the
//! way out. Calls are fire-and-forget at the ABI (`call_function`), with
//! results delivered through the registered callback and correlated by
//! the completion registry in [`crate::completion`].

use std::{collections::HashMap, ffi::CString};

use prost::Message as _;

use crate::{BamlValue, Error, SdkError, capi, completion, decode, wire};

/// Initialize (or replace) the process-global runtime from the
/// borsh-encoded bytecode a generated SDK embeds.
///
/// Generated SDKs call this lazily on first use; it is public for hosts
/// that want eager, fallible startup.
pub fn initialize_from_bytecode(bytecode: &[u8]) -> Result<(), SdkError> {
    initialize_from_bytecode_with_metadata(bytecode, None)
}

pub fn initialize_from_bytecode_with_metadata(
    bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
) -> Result<(), SdkError> {
    let api = capi::api()?;
    // SAFETY: the bytecode slice is valid for the duration of the call;
    // the engine copies what it keeps, and returns an owned status buffer
    // that `take_status` reads and frees.
    let manifest = embedded_baml_toml
        .map(CString::new)
        .transpose()
        .map_err(|_| SdkError::new("embedded baml.toml contains an interior NUL byte"))?;
    #[expect(unsafe_code)]
    let status = unsafe {
        match manifest.as_ref() {
            Some(manifest) => (api.initialize_runtime_from_bytecode_with_metadata)(
                bytecode.as_ptr(),
                bytecode.len(),
                manifest.as_ptr(),
            ),
            None => (api.initialize_runtime_from_bytecode)(bytecode.as_ptr(), bytecode.len()),
        }
    };
    api.take_status(status).map_err(SdkError::new)
}

/// Initialize (or replace) the process-global runtime by compiling BAML
/// source files (`file name → content`, names relative to `root_path`).
pub fn initialize_from_files(
    root_path: &str,
    files: &HashMap<String, String>,
) -> Result<(), SdkError> {
    let api = capi::api()?;
    let root = CString::new(root_path)
        .map_err(|_| SdkError::new("root path contains an interior NUL byte"))?;
    let files_json = serde_json::to_string(files)
        .map_err(|e| SdkError::new(format!("failed to encode source files: {e}")))?;
    let files_json = CString::new(files_json)
        .map_err(|_| SdkError::new("source contents contain an interior NUL byte"))?;
    // SAFETY: both pointers are NUL-terminated C strings that outlive the
    // call; the engine copies what it keeps.
    #[expect(unsafe_code)]
    let ok = unsafe { (api.create_baml_runtime)(root.as_ptr(), files_json.as_ptr()) };
    if ok.is_null() {
        Err(SdkError::new(
            "failed to initialize the BAML runtime from source files \
             (details on the engine's diagnostic output)",
        ))
    } else {
        Ok(())
    }
}

/// Execute a BAML function call, blocking until it completes.
///
/// Refuses to run inside an async runtime (parking an executor thread on
/// the completion would stall it): generated sync functions return
/// [`Error::CalledSyncFromAsync`] and callers switch to the `_async`
/// sibling.
pub fn invoke_sync<R: BamlValue, E: BamlValue>(
    fqn: &str,
    kwargs: Vec<wire::InboundMapEntry>,
    type_args: Vec<wire::BamlTyArg>,
) -> Result<R, Error<E>> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::CalledSyncFromAsync);
    }
    let receiver = dispatch(fqn, kwargs, type_args).map_err(Error::Sdk)?;
    // Blocks until the engine delivers the result envelope via the callback.
    // There is no timeout: the engine is contracted to complete every call
    // (success, thrown error, or panic). A caller-facing timeout/cancellation
    // path lands with the cancellation feature (`cancel_function_call`).
    let bytes = receiver.wait_blocking();
    decode::decode_result(&bytes)
}

/// Execute a BAML function call asynchronously.
///
/// The engine drives the call on its own runtime inside the bridge
/// library; the returned future only awaits the completion, so it is
/// executor-agnostic.
pub async fn invoke<R: BamlValue, E: BamlValue>(
    fqn: &str,
    kwargs: Vec<wire::InboundMapEntry>,
    type_args: Vec<wire::BamlTyArg>,
) -> Result<R, Error<E>> {
    let receiver = dispatch(fqn, kwargs, type_args).map_err(Error::Sdk)?;
    let bytes = receiver.wait().await;
    decode::decode_result(&bytes)
}

pub fn invoke_handle_sync<R: BamlValue, E: BamlValue>(
    handle_key: u64,
    kwargs: Vec<wire::InboundMapEntry>,
) -> Result<R, Error<E>> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::CalledSyncFromAsync);
    }
    let receiver = dispatch_handle(handle_key, kwargs).map_err(Error::Sdk)?;
    decode::decode_result(&receiver.wait_blocking())
}

pub async fn invoke_handle<R: BamlValue, E: BamlValue>(
    handle_key: u64,
    kwargs: Vec<wire::InboundMapEntry>,
) -> Result<R, Error<E>> {
    let receiver = dispatch_handle(handle_key, kwargs).map_err(Error::Sdk)?;
    decode::decode_result(&receiver.wait().await)
}

/// Encode the call and fire it through the C ABI. The registered
/// completion is returned to be waited on; pre-call failures inside the
/// engine also arrive through it as error envelopes (one result channel).
fn dispatch(
    fqn: &str,
    kwargs: Vec<wire::InboundMapEntry>,
    type_args: Vec<wire::BamlTyArg>,
) -> Result<completion::Receiver, SdkError> {
    let api = capi::api()?;
    let receiver = completion::register(api);
    // Host-callable dispatch must be installed before the engine can hold
    // a callable handle; every handle rides a call that passes through
    // here first.
    crate::host_value::ensure_callbacks_registered(api);
    // SAFETY: takes no arguments; allocates an id inside the engine.
    #[expect(unsafe_code)]
    let call_id = unsafe { (api.new_function_call)() };
    let args = wire::CallFunctionArgs {
        kwargs,
        call_id,
        type_args,
        call_target: Some(wire::call_function_args::CallTarget::FunctionName(
            fqn.to_string(),
        )),
    }
    .encode_to_vec();
    // SAFETY: `args` outlives the call; the engine copies it before returning.
    #[expect(unsafe_code)]
    unsafe {
        (api.call_function)(args.as_ptr(), args.len(), receiver.dispatch_id());
    }
    Ok(receiver)
}

fn dispatch_handle(
    handle_key: u64,
    kwargs: Vec<wire::InboundMapEntry>,
) -> Result<completion::Receiver, SdkError> {
    if handle_key == 0 {
        return Err(SdkError::new("cannot invoke a zero BAML function handle"));
    }
    let api = capi::api()?;
    let receiver = completion::register(api);
    crate::host_value::ensure_callbacks_registered(api);
    // SAFETY: this ABI function takes no arguments and returns a fresh engine
    // call id; the loaded API table was layout-checked during initialization.
    #[expect(unsafe_code)]
    let call_id = unsafe { (api.new_function_call)() };
    let args = wire::CallFunctionArgs {
        kwargs,
        call_id,
        type_args: Vec::new(),
        call_target: Some(wire::call_function_args::CallTarget::FunctionHandle(
            handle_key,
        )),
    }
    .encode_to_vec();
    // SAFETY: `args` remains alive for the synchronous ABI call, which copies
    // the protobuf bytes before returning; `receiver` owns the dispatch id.
    #[expect(unsafe_code)]
    unsafe {
        (api.call_function)(args.as_ptr(), args.len(), receiver.dispatch_id());
    }
    Ok(receiver)
}

#[cfg(test)]
mod tests {
    #[test]
    fn zero_function_handle_fails_before_loading_the_c_api() {
        let Err(error) = super::dispatch_handle(0, Vec::new()) else {
            panic!("a zero function handle must fail before dispatch")
        };
        assert_eq!(
            error.to_string(),
            "cannot invoke a zero BAML function handle"
        );
    }
}
