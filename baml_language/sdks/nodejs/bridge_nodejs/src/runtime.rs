//! BamlRuntime napi class.
//!
//! A zero-sized handle: the single source of truth for the `Arc<dyn Bex>`
//! singleton is `bridge_cffi`, fetched via `bridge_cffi::engine::get_runtime()`
//! at each call site (mirrors `bridge_python` after 31e-phase4), so this no
//! longer caches its own clone.

use std::sync::Arc;

use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use prost::Message;

use crate::{
    abort_controller::AbortController,
    errors::{bridge_error_to_napi, invalid_argument_error},
    types::{HostSpanManager, collector::Collector},
};

/// The main BAML runtime. A zero-sized handle (see module docs).
#[napi]
pub struct BamlRuntime {}

#[napi]
impl BamlRuntime {
    /// Create a runtime from in-memory BAML source files.
    #[napi(factory)]
    pub fn from_files(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> napi::Result<Self> {
        // `initialize_runtime` stores the `Arc<dyn Bex>` in bridge_cffi's
        // singleton; we don't keep our own copy.
        match bridge_cffi::engine::initialize_runtime(&root_path, files) {
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
        abort_controller: Option<&AbortController>,
    ) -> napi::Result<Buffer> {
        let runtime = bridge_cffi::engine::get_runtime().map_err(bridge_error_to_napi)?;
        let kwargs = decode_args(args_proto.as_ref(), &function_name)?;
        let host_ctx = ctx.and_then(|c| c.host_span_context());
        let cancel = abort_controller
            .map(AbortController::token)
            .unwrap_or_default();
        let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
            .as_ref()
            .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
            .unwrap_or_default();

        let call_id = bex_project::CallId::next();
        let mut call_ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_collectors(collector_arcs)
            .with_cancel_token(cancel);

        if let Some(host_ctx) = host_ctx {
            call_ctx = call_ctx.with_host_ctx(host_ctx);
        }

        let call_ctx = call_ctx.build();

        let rt = bridge_cffi::engine::get_tokio_runtime().map_err(bridge_error_to_napi)?;

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary and error/panic routing) lives in
        // bridge_cffi; we just return the encoded envelope bytes for the TS
        // decoder to surface.
        let bytes = rt.block_on(bridge_cffi::call_and_encode(
            runtime,
            function_name,
            kwargs,
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
        abort_controller: Option<&AbortController>,
    ) -> napi::Result<PromiseRaw<'e, Buffer>> {
        let runtime = bridge_cffi::engine::get_runtime().map_err(bridge_error_to_napi)?;
        let kwargs = decode_args(args_proto.as_ref(), &function_name)?;
        let host_ctx = ctx.and_then(|c| c.host_span_context());
        let cancel = abort_controller
            .map(AbortController::token)
            .unwrap_or_default();
        let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
            .as_ref()
            .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
            .unwrap_or_default();

        let call_id = bex_project::CallId::next();
        let mut call_ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_collectors(collector_arcs)
            .with_cancel_token(cancel);

        if let Some(host_ctx) = host_ctx {
            call_ctx = call_ctx.with_host_ctx(host_ctx);
        }

        let call_ctx = call_ctx.build();

        // Same shared call_and_encode as the sync + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes for the TS decoder.
        env.spawn_future(async move {
            let bytes =
                bridge_cffi::call_and_encode(runtime, function_name, kwargs, call_ctx).await;
            Ok(Buffer::from(bytes))
        })
    }
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
fn decode_args(args_proto: &[u8], function_name: &str) -> napi::Result<bex_project::BexArgs> {
    let args =
        bridge_ctypes::baml_core::cffi::CallFunctionArgs::decode(args_proto).map_err(|e| {
            invalid_argument_error(format!(
                "Failed to decode arguments for function '{function_name}': {e}"
            ))
        })?;

    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE).map_err(|e| {
        invalid_argument_error(format!(
            "Failed to convert arguments for function '{function_name}': {e}"
        ))
    })?;

    Ok(kwargs.into())
}
