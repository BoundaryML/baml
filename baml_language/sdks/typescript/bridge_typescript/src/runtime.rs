//! BamlRuntime napi class.
//!
//! A zero-sized handle: the single source of truth for the `Arc<dyn Bex>`
//! singleton is `bridge_cffi`, fetched via `bridge_cffi::get_runtime()`
//! at each call site (mirrors `bridge_python` after 31e-phase4), so this no
//! longer caches its own clone.

use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::errors::bridge_error_to_napi;

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
        // bridge_cffi's singleton owns the runtime, and initializes it on the
        // first call; we don't keep our own copy.
        match bridge_cffi::stage_runtime(&root_path, files) {
            Ok(()) => Ok(BamlRuntime {}),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Initialize the process-global runtime from precompiled BAML bytecode:
    /// a raw artifact, or the encoded string generated SDKs embed (decoded
    /// natively, never in JavaScript).
    #[napi(factory, js_name = "initializeRuntimeFromBlob")]
    pub fn initialize_runtime_from_blob(
        bytecode: Either<String, Buffer>,
        embedded_baml_toml: Option<String>,
    ) -> napi::Result<Self> {
        let bytecode: &[u8] = match &bytecode {
            Either::A(encoded) => encoded.as_bytes(),
            Either::B(bytes) => bytes.as_ref(),
        };
        match bridge_cffi::stage_runtime_from_blob(bytecode, embedded_baml_toml.as_deref()) {
            Ok(()) => Ok(BamlRuntime {}),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Call a BAML function synchronously (blocking).
    #[napi]
    pub fn call_function_sync(&self, args_proto: Buffer) -> napi::Result<Buffer> {
        let request = (|| -> std::result::Result<_, bridge_cffi::BridgeError> {
            let request = bridge_cffi::decode_invocation_request(args_proto.as_ref())?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((request, rt))
        })();

        let (request, rt) = match request {
            Ok(v) => v,
            Err(e) => return Ok(Buffer::from(bridge_cffi::error_to_outbound(e))),
        };

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary and error/panic routing) lives in
        // bridge_cffi; we just return the encoded envelope bytes for the TS
        // decoder to surface.
        let _sync_call = crate::host_value::SyncCallGuard::new(request.host_call_id());
        let bytes = rt.block_on(bridge_cffi::execute_invocation(request));

        Ok(Buffer::from(bytes))
    }

    /// Call a BAML function asynchronously.
    #[napi(ts_return_type = "Promise<Buffer>")]
    pub fn call_function<'e>(
        &self,
        env: &'e Env,
        args_proto: Buffer,
    ) -> napi::Result<PromiseRaw<'e, Buffer>> {
        // `decode_invocation_request` pins a handle target before the future is spawned,
        // so a JS-side release of that handle cannot race the call.
        let request = bridge_cffi::decode_invocation_request(args_proto.as_ref());

        // Same shared execute_invocation as the sync + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes for the TS decoder.
        env.spawn_future(async move {
            let bytes = match request {
                Ok(request) => bridge_cffi::execute_invocation(request).await,
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
