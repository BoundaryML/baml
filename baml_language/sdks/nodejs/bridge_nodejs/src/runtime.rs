//! BamlRuntime napi class.
//!
//! A zero-sized handle: the single source of truth for the `Arc<dyn Bex>`
//! singleton is `bridge_cffi`, fetched via `bridge_cffi::get_runtime()`
//! at each call site (mirrors `bridge_python` after 31e-phase4), so this no
//! longer caches its own clone.

use std::sync::Arc;

use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use prost::Message;

use crate::{
    errors::bridge_error_to_napi,
    types::{HostSpanManager, collector::Collector},
};

struct DecodedCallArgs {
    kwargs: bex_project::BexArgs,
    call_id: bex_project::CallId,
}

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
    pub fn initialize_runtime_from_bytecode(bytecode: Buffer) -> napi::Result<Self> {
        match bridge_cffi::initialize_runtime_from_bytecode(bytecode.as_ref()) {
            Ok(_bex) => Ok(BamlRuntime {}),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Call a BAML function synchronously (blocking).
    #[napi]
    pub fn call_function_sync(
        &self,
        function_name: String,
        args_proto: Buffer,
        ctx: Option<&HostSpanManager>,
        collectors: Option<Vec<&Collector>>,
    ) -> napi::Result<Buffer> {
        let prepared = (|| -> std::result::Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let decoded = decode_args(args_proto.as_ref(), &function_name)?;
            let host_ctx = ctx.and_then(|c| c.host_span_context());
            let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
                .as_ref()
                .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
                .unwrap_or_default();

            let mut call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
                .with_collectors(collector_arcs);

            if let Some(host_ctx) = host_ctx {
                call_ctx = call_ctx.with_host_ctx(host_ctx);
            }

            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, decoded, call_ctx.build(), rt))
        })();

        let (runtime, decoded, call_ctx, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return Ok(Buffer::from(bridge_cffi::error_to_outbound(e))),
        };

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary and error/panic routing) lives in
        // bridge_cffi; we just return the encoded envelope bytes for the TS
        // decoder to surface.
        let bytes = rt.block_on(bridge_cffi::call_and_encode(
            runtime,
            function_name,
            decoded.kwargs,
            call_ctx,
        ));

        Ok(Buffer::from(bytes))
    }

    /// Call a BAML function asynchronously.
    #[napi(ts_return_type = "Promise<Buffer>")]
    pub fn call_function<'e>(
        &self,
        env: &'e Env,
        function_name: String,
        args_proto: Buffer,
        ctx: Option<&HostSpanManager>,
        collectors: Option<Vec<&Collector>>,
    ) -> napi::Result<PromiseRaw<'e, Buffer>> {
        let prepared = (|| -> std::result::Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let decoded = decode_args(args_proto.as_ref(), &function_name)?;
            let host_ctx = ctx.and_then(|c| c.host_span_context());
            let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
                .as_ref()
                .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
                .unwrap_or_default();

            let mut call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
                .with_collectors(collector_arcs);

            if let Some(host_ctx) = host_ctx {
                call_ctx = call_ctx.with_host_ctx(host_ctx);
            }

            Ok((runtime, decoded, call_ctx.build()))
        })();

        // Same shared call_and_encode as the sync + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes for the TS decoder.
        env.spawn_future(async move {
            let bytes = match prepared {
                Ok((runtime, decoded, call_ctx)) => {
                    bridge_cffi::call_and_encode(runtime, function_name, decoded.kwargs, call_ctx)
                        .await
                }
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

/// Decode protobuf-encoded function arguments into `BexArgs`.
fn decode_args(
    args_proto: &[u8],
    _function_name: &str,
) -> std::result::Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    let args = bridge_ctypes::baml_core::cffi::CallFunctionArgs::decode(args_proto)
        .map_err(bridge_ctypes::CtypesError::from)?;

    if args.call_id == 0 {
        return Err(bridge_cffi::BridgeError::InvalidCallId);
    }

    let call_id = bex_project::CallId(args.call_id);
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    Ok(DecodedCallArgs {
        kwargs: kwargs.into(),
        call_id,
    })
}
