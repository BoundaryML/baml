//! Runtime lifecycle and call execution.
//!
//! Thin composition over `bridge_cffi`'s in-process Rust API: the same
//! process-global engine singleton and tokio runtime the other language
//! bridges use, with the identical `BamlOutboundResult` envelope on the
//! way out. The inbound leg skips byte serialization entirely — prost
//! structs feed `bridge_ctypes::kwargs_to_bex_values` directly.

use std::collections::HashMap;

use bridge_ctypes::HANDLE_TABLE;

use crate::{BamlValue, Error, SdkError, decode, wire};

/// Initialize (or replace) the process-global runtime from the
/// borsh-encoded bytecode a generated SDK embeds.
///
/// Generated SDKs call this lazily on first use; it is public for hosts
/// that want eager, fallible startup.
pub fn initialize_from_bytecode(bytecode: &[u8]) -> Result<(), SdkError> {
    bridge_cffi::initialize_runtime_from_bytecode(bytecode)
        .map(drop)
        .map_err(SdkError::new)
}

/// Initialize (or replace) the process-global runtime by compiling BAML
/// source files (`file name → content`, names relative to `root_path`).
pub fn initialize_from_files(
    root_path: &str,
    files: HashMap<String, String>,
) -> Result<(), SdkError> {
    bridge_cffi::initialize_runtime(root_path, files)
        .map(drop)
        .map_err(SdkError::new)
}

/// Execute a BAML function call, blocking until it completes.
///
/// Refuses to run inside an async runtime (`block_on` there would stall
/// the executor): generated sync functions return
/// [`Error::CalledSyncFromAsync`] and callers switch to the `_async`
/// sibling.
pub fn invoke_sync<R: BamlValue, E: BamlValue>(
    fqn: &str,
    kwargs: Vec<wire::InboundMapEntry>,
) -> Result<R, Error<E>> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::CalledSyncFromAsync);
    }
    let call = prepare(fqn, kwargs).map_err(Error::Sdk)?;
    let bytes = tokio_runtime().map_err(Error::Sdk)?.block_on(call);
    decode::decode_result(&bytes)
}

/// Execute a BAML function call asynchronously.
///
/// The call runs on the bridge's own tokio runtime (the engine needs its
/// reactor); awaiting the `JoinHandle` keeps the returned future
/// executor-agnostic.
pub async fn invoke<R: BamlValue, E: BamlValue>(
    fqn: &str,
    kwargs: Vec<wire::InboundMapEntry>,
) -> Result<R, Error<E>> {
    let call = prepare(fqn, kwargs).map_err(Error::Sdk)?;
    let bytes = tokio_runtime()
        .map_err(Error::Sdk)?
        .spawn(call)
        .await
        .map_err(|e| Error::Sdk(SdkError::new(format!("BAML call task failed: {e}"))))?;
    decode::decode_result(&bytes)
}

/// Resolve the runtime, decode the kwargs, and build the engine call
/// future. Shared front half of [`invoke_sync`] / [`invoke`];
/// `call_and_encode` supplies the catch-unwind and envelope encoding.
fn prepare(
    fqn: &str,
    kwargs: Vec<wire::InboundMapEntry>,
) -> Result<impl Future<Output = Vec<u8>> + Send + use<>, SdkError> {
    let runtime = bridge_cffi::get_runtime().map_err(SdkError::new)?;
    let args = bridge_ctypes::kwargs_to_bex_values(kwargs, &HANDLE_TABLE)
        .map_err(|e| SdkError::new(format!("failed to encode arguments: {e}")))?;
    let call_id = sys_types::CallId(bridge_cffi::new_function_call_id());
    let ctx = bridge_cffi::function_call_context_builder(call_id).build();
    Ok(bridge_cffi::call_and_encode(
        runtime,
        fqn.to_string(),
        args.into(),
        ctx,
    ))
}

fn tokio_runtime() -> Result<std::sync::Arc<tokio::runtime::Runtime>, SdkError> {
    bridge_cffi::get_tokio_runtime().map_err(SdkError::new)
}
