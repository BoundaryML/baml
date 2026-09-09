//! Node SDK bindings retain their issuing session and a weak engine reference.

use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{
    encoded_result::BamlEncodedResult,
    errors::bridge_error_to_napi,
    types::{HostSpanManager, collector::Collector},
};

use std::sync::{Arc, LazyLock, Mutex, Weak};

type OwnedCall = (
    Arc<dyn bex_project::Bex>,
    bridge_ctypes::TransferSession<'static>,
    bridge_cffi::PreparedCall,
);

/// Weak runtimes issued by this Node module, including ones retiring after a
/// replacement. Neither SDK bindings nor this exit registry own an idle heap.
#[derive(Default)]
struct ExitRuntimes {
    generation: u64,
    runtimes: Vec<Weak<dyn bex_project::Bex>>,
}
static EXIT_RUNTIMES: LazyLock<Mutex<ExitRuntimes>> = LazyLock::new(Default::default);

fn track_for_exit(runtime: &Arc<dyn bex_project::Bex>) {
    let mut state = EXIT_RUNTIMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.runtimes.retain(|runtime| runtime.strong_count() != 0);
    let weak = Arc::downgrade(runtime);
    if !state
        .runtimes
        .iter()
        .any(|existing| Weak::ptr_eq(existing, &weak))
    {
        state.generation = state.generation.wrapping_add(1);
        state.runtimes.push(weak);
    }
}

/// Called by the public SDK's beforeExit hook. N-API retains the event loop
/// for this Promise while background work drains. Admission stays open so
/// callbacks from that work can re-enter their issuing engine.
#[napi(js_name = "_waitForRuntimeIdle")]
pub async fn wait_for_runtime_idle() -> napi::Result<()> {
    loop {
        let (generation, runtimes) = {
            let mut state = EXIT_RUNTIMES
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.runtimes.retain(|runtime| runtime.strong_count() != 0);
            (
                state.generation,
                state
                    .runtimes
                    .iter()
                    .filter_map(Weak::upgrade)
                    .collect::<Vec<_>>(),
            )
        };
        for runtime in runtimes {
            runtime.wait_until_idle().await;
        }
        if EXIT_RUNTIMES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .generation
            == generation
        {
            return Ok(());
        }
    }
}

/// A binding to one engine. Retaining the SDK does not retain a closed heap.
#[napi]
pub struct BamlRuntime {
    runtime: Weak<dyn bex_project::Bex>,
    transfers: bridge_ctypes::TransferSession<'static>,
}

impl BamlRuntime {
    fn current() -> Result<Self> {
        let (runtime, transfers) =
            bridge_cffi::get_runtime_with_transfers().map_err(bridge_error_to_napi)?;
        track_for_exit(&runtime);
        Ok(Self {
            runtime: Arc::downgrade(&runtime),
            transfers,
        })
    }

    fn initialized(runtime: Arc<dyn bex_project::Bex>) -> Result<Self> {
        let (current, transfers) =
            bridge_cffi::get_runtime_with_transfers().map_err(bridge_error_to_napi)?;
        if !Arc::ptr_eq(&runtime, &current) {
            return Err(Error::from_reason(
                "BAML runtime was replaced during initialization",
            ));
        }
        track_for_exit(&runtime);
        Ok(Self {
            runtime: Arc::downgrade(&runtime),
            transfers,
        })
    }

    fn prepare(&self, bytes: &[u8]) -> std::result::Result<OwnedCall, bridge_cffi::BridgeError> {
        // Always consume transferred argument leases, even for an expired SDK.
        let call = bridge_cffi::prepare_call(bytes)?;
        let (runtime, transfers) = self.authority()?;
        Ok((runtime, transfers, call))
    }

    fn authority(
        &self,
    ) -> std::result::Result<
        (
            Arc<dyn bex_project::Bex>,
            bridge_ctypes::TransferSession<'static>,
        ),
        bridge_cffi::BridgeError,
    > {
        if self.transfers.is_closed() {
            return Err(bridge_cffi::BridgeError::InvalidInvocation(
                "SDK runtime was closed or replaced".into(),
            ));
        }
        let runtime = self.runtime.upgrade().ok_or_else(|| {
            bridge_cffi::BridgeError::InvalidInvocation("SDK runtime is no longer available".into())
        })?;
        Ok((runtime, self.transfers.clone()))
    }

    fn prepare_host_operation(
        &self,
        bytes: &[u8],
    ) -> std::result::Result<OwnedHostOperation, bridge_cffi::BridgeError> {
        let prepared = bridge_cffi::host_registration::prepare_operation(bytes)?;
        let (runtime, transfers) = self.authority()?;
        Ok((runtime, transfers, prepared))
    }
}

#[napi]
impl BamlRuntime {
    /// Private SDK registration/projection channel; returns an owned receipt.
    #[napi(
        js_name = "_hostOperation",
        ts_return_type = "Promise<BamlEncodedResult>"
    )]
    pub fn host_operation<'e>(
        &self,
        env: &'e Env,
        args_proto: Buffer,
    ) -> napi::Result<PromiseRaw<'e, BamlEncodedResult>> {
        let _drain = DrainHostReleases;
        let prepared = self.prepare_host_operation(args_proto.as_ref());
        env.spawn_future(finish_host_operation(prepared))
    }

    #[napi(js_name = "_hostOperationSync")]
    pub fn host_operation_sync(&self, args_proto: Buffer) -> napi::Result<BamlEncodedResult> {
        let _drain = DrainHostReleases;
        let prepared = self.prepare_host_operation(args_proto.as_ref());
        match bridge_cffi::get_tokio_runtime() {
            Ok(executor) => executor.block_on(finish_host_operation(prepared)),
            Err(error) => host_operation_error(error),
        }
    }

    /// Initialize the process-global runtime from in-memory BAML source
    /// files. `bridge_cffi::initialize_runtime` is a single-slot singleton, so
    /// a second call replaces the prior runtime; the result is also reachable
    /// via the module-level `getRuntime()`. Renamed from `fromFiles` for
    /// parity with `bridge_python`'s sole `initialize_runtime` constructor and
    /// the `initializeRuntime(...)` import the spec docs use.
    #[napi(factory, js_name = "initializeRuntime")]
    pub fn initialize_runtime(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> napi::Result<Self> {
        match bridge_cffi::initialize_runtime(&root_path, files) {
            Ok(bex) => Self::initialized(bex),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Initialize the process-global runtime from precompiled BAML bytecode.
    #[napi(factory, js_name = "initializeRuntimeFromBytecode")]
    pub fn initialize_runtime_from_bytecode(
        bytecode: Buffer,
        embedded_baml_toml: Option<String>,
    ) -> napi::Result<Self> {
        match bridge_cffi::initialize_runtime_from_bytecode(
            bytecode.as_ref(),
            embedded_baml_toml.as_deref(),
        ) {
            Ok(bex) => Self::initialized(bex),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Call a BAML function synchronously (blocking).
    #[napi]
    pub fn call_function_sync(
        &self,
        args_proto: Buffer,
        ctx: Option<&HostSpanManager>,
        collectors: Option<Vec<&Collector>>,
    ) -> napi::Result<BamlEncodedResult> {
        let prepared = self.prepare(args_proto.as_ref());
        let _ = (&ctx, &collectors);
        invoke_sync(prepared)
    }

    /// Call a BAML function asynchronously.
    #[napi(ts_return_type = "Promise<BamlEncodedResult>")]
    pub fn call_function<'e>(
        &self,
        env: &'e Env,
        args_proto: Buffer,
        ctx: Option<&HostSpanManager>,
        collectors: Option<Vec<&Collector>>,
    ) -> napi::Result<PromiseRaw<'e, BamlEncodedResult>> {
        let prepared = self.prepare(args_proto.as_ref());
        let _ = (&ctx, &collectors);

        invoke_async(env, prepared)
    }
}

/// Return the process-global `BamlRuntime`, or a `BamlError`-shaped
/// `napi::Error` if `initializeRuntime` has not run yet. It binds the current
/// engine once; subsequent calls never switch its issuer.
#[napi(js_name = "getRuntime")]
pub fn get_runtime() -> napi::Result<BamlRuntime> {
    BamlRuntime::current()
}

pub(crate) fn prepare_owned_call(
    handle: &crate::handle::BamlHandle,
    bytes: &[u8],
) -> std::result::Result<OwnedCall, bridge_cffi::BridgeError> {
    let call = bridge_cffi::prepare_call(bytes)?;
    let (runtime, transfers) = handle.invocation_owner()?;
    Ok((runtime, transfers, call))
}

pub(crate) fn prepare_interface_call(
    handle: &crate::handle::BamlHandle,
    member: String,
    bytes: &[u8],
) -> std::result::Result<OwnedCall, bridge_cffi::BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::{
        BamlHandleType, CallFunctionArgs, InterfaceMethodTarget, call_function_args::CallTarget,
    };
    use prost::Message;
    let mut args = CallFunctionArgs::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    // A closed local lease supplies no receiver, but still decode/consume args.
    args.call_target = Some(CallTarget::InterfaceMethod(InterfaceMethodTarget {
        view: handle.raw_key().unwrap_or(0),
        member,
        type_args: std::mem::take(&mut args.type_args),
    }));
    let call = bridge_cffi::prepare_call(&args.encode_to_vec())?;
    if handle.handle_type() != BamlHandleType::AdtInterface as i32 {
        return Err(bridge_cffi::BridgeError::InvalidInvocation(
            "reference is not a BAML interface".into(),
        ));
    }
    let (runtime, transfers) = handle.invocation_owner()?;
    Ok((runtime, transfers, call))
}

struct DrainHostReleases;
impl Drop for DrainHostReleases {
    fn drop(&mut self) {
        bex_project::host_release_dispatch::drain();
    }
}

pub(crate) fn invoke_async<'e>(
    env: &'e Env,
    prepared: std::result::Result<OwnedCall, bridge_cffi::BridgeError>,
) -> napi::Result<PromiseRaw<'e, BamlEncodedResult>> {
    let _drain = DrainHostReleases;
    env.spawn_future(async move {
        let _drain = DrainHostReleases;
        match prepared {
            Ok((runtime, session, call)) => {
                let encoded = bridge_cffi::invoke_prepared_encoded(runtime.clone(), call).await;
                BamlEncodedResult::new(encoded, session, Some(runtime))
            }
            Err(error) => boundary_error(error),
        }
    })
}

pub(crate) fn invoke_sync(
    prepared: std::result::Result<OwnedCall, bridge_cffi::BridgeError>,
) -> napi::Result<BamlEncodedResult> {
    let _drain = DrainHostReleases;
    let prepared = prepared.and_then(|call| bridge_cffi::get_tokio_runtime().map(|rt| (call, rt)));
    match prepared {
        Ok(((runtime, session, call), rt)) => {
            let encoded = rt.block_on(bridge_cffi::invoke_prepared_encoded(runtime.clone(), call));
            BamlEncodedResult::new(encoded, session, Some(runtime))
        }
        Err(error) => boundary_error(error),
    }
}

fn boundary_error(error: bridge_cffi::BridgeError) -> napi::Result<BamlEncodedResult> {
    BamlEncodedResult::new(
        bridge_cffi::baml_to_host::error_to_outbound_encoded(error),
        bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE),
        None,
    )
}

type OwnedHostOperation = (
    Arc<dyn bex_project::Bex>,
    bridge_ctypes::TransferSession<'static>,
    bridge_cffi::host_registration::PreparedHostOperation,
);

async fn finish_host_operation(
    prepared: std::result::Result<OwnedHostOperation, bridge_cffi::BridgeError>,
) -> napi::Result<BamlEncodedResult> {
    let _drain = DrainHostReleases;
    match prepared {
        Ok((runtime, session, operation)) => {
            let encoded =
                bridge_cffi::host_registration::execute_operation(runtime.clone(), operation).await;
            BamlEncodedResult::new(encoded, session, Some(runtime))
        }
        Err(error) => host_operation_error(error),
    }
}

fn host_operation_error(error: bridge_cffi::BridgeError) -> napi::Result<BamlEncodedResult> {
    BamlEncodedResult::new(
        bridge_cffi::host_registration::operation_error(error),
        bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE),
        None,
    )
}

#[napi(js_name = "_pendingTransferCount")]
pub fn pending_transfer_count() -> napi::Result<u32> {
    let (_, session) = bridge_cffi::get_runtime_with_transfers().map_err(bridge_error_to_napi)?;
    Ok(session.pending_count() as u32)
}
