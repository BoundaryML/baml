//! BamlRuntime napi class.
//!
//! A zero-sized handle: the single source of truth for the `Arc<dyn Bex>`
//! singleton is `bridge_cffi`, fetched via `bridge_cffi::get_runtime()`
//! at each call site (mirrors `bridge_python` after 31e-phase4), so this no
//! longer caches its own clone.

use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{
    errors::bridge_error_to_napi,
    types::{HostSpanManager, collector::Collector},
};

/// The main BAML runtime. A zero-sized handle (see module docs).
#[napi]
pub struct BamlRuntime {}

#[napi]
impl BamlRuntime {
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
        // `initialize_runtime` stores the `Arc<dyn Bex>` in bridge_cffi's
        // singleton; we don't keep our own copy.
        match bridge_cffi::initialize_runtime(&root_path, files) {
            Ok(_bex) => Ok(BamlRuntime {}),
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
            Ok(_bex) => Ok(BamlRuntime {}),
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
    ) -> napi::Result<Buffer> {
        let prepared = (|| -> std::result::Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let prepared = bridge_cffi::prepare_call(args_proto.as_ref())?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, prepared, rt))
        })();
        let _ = (&ctx, &collectors);

        let (runtime, prepared, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return Ok(Buffer::from(bridge_cffi::error_to_outbound(e))),
        };

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary and error/panic routing) lives in
        // bridge_cffi; we just return the encoded envelope bytes for the TS
        // decoder to surface.
        let bytes = rt.block_on(bridge_cffi::invoke_prepared(runtime, prepared));

        Ok(Buffer::from(bytes))
    }

    /// Call a BAML function asynchronously.
    #[napi(ts_return_type = "Promise<Buffer>")]
    pub fn call_function<'e>(
        &self,
        env: &'e Env,
        args_proto: Buffer,
        ctx: Option<&HostSpanManager>,
        collectors: Option<Vec<&Collector>>,
    ) -> napi::Result<PromiseRaw<'e, Buffer>> {
        // `prepare_call` pins a handle target before the future is spawned,
        // so a JS-side release of that handle cannot race the call.
        let prepared = (|| -> std::result::Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let prepared = bridge_cffi::prepare_call(args_proto.as_ref())?;
            Ok((runtime, prepared))
        })();
        let _ = (&ctx, &collectors);

        // Same shared invoke_prepared as the sync + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes for the TS decoder.
        env.spawn_future(async move {
            let bytes = match prepared {
                Ok((runtime, prepared)) => bridge_cffi::invoke_prepared(runtime, prepared).await,
                Err(e) => bridge_cffi::error_to_outbound(e),
            };
            Ok(Buffer::from(bytes))
        })
    }
}

/// Return the process-global `BamlRuntime`, or a `BamlError`-shaped
/// `napi::Error` if `initializeRuntime` has not run yet. The handle is
/// zero-sized; the `Arc<dyn Bex>` lives in `bridge_cffi`. Mirrors
/// `bridge_python`'s module-level `get_runtime()`.
#[napi(js_name = "getRuntime")]
pub fn get_runtime() -> napi::Result<BamlRuntime> {
    bridge_cffi::get_runtime().map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized => napi::Error::new(
            napi::Status::GenericFailure,
            "BamlError: BAML runtime has not been initialized — call BamlRuntime.initializeRuntime first.",
        ),
        other => bridge_error_to_napi(other),
    })?;
    Ok(BamlRuntime {})
}
