//! BamlRuntime napi class — wraps `Arc<dyn Bex>`.

use std::sync::Arc;

use bex_project::Bex;
use bridge_ctypes::{HANDLE_TABLE, external_to_outbound, kwargs_to_bex_values};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use prost::Message;

use crate::{
    abort_controller::AbortController,
    errors::{bridge_error_to_napi, invalid_argument_error, runtime_error_to_napi},
    types::{HostSpanManager, collector::Collector},
};

/// The main BAML runtime, wrapping a `dyn Bex` instance.
#[napi]
pub struct BamlRuntime {
    bex: Arc<dyn Bex>,
}

#[napi]
impl BamlRuntime {
    /// Create a runtime from in-memory BAML source files.
    #[napi(factory)]
    pub fn from_files(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> napi::Result<Self> {
        match bridge_cffi::engine::initialize_runtime(&root_path, files) {
            Ok(bex) => Ok(BamlRuntime { bex }),
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
        let bex = self.bex.clone();
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

        let result = rt
            .block_on(bex.call_function(&function_name, kwargs, call_ctx))
            .map_err(runtime_error_to_napi)?;

        // FIXME: Uses invalid_argument_error for a post-execution serialization failure, which
        // surfaces as BamlInvalidArgumentError to JS callers. Legacy engine/ had no equivalent
        // (returned NAPI classes directly, no encode step). bridge_python has the same
        // misclassification. Leaving as-is for parity; fix in both bridges together.
        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_in_process();
        let baml_value = external_to_outbound(&result, &handle_options)
            .map_err(|e| invalid_argument_error(format!("Failed to encode result: {e}")))?;

        Ok(Buffer::from(baml_value.encode_to_vec()))
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
        let bex = self.bex.clone();
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

        env.spawn_future(async move {
            let result = bex
                .call_function(&function_name, kwargs, call_ctx)
                .await
                .map_err(runtime_error_to_napi)?;

            // FIXME: Same misclassification as the sync path above.
            let handle_options = bridge_ctypes::CffiHandleTableOptions::for_in_process();
            let baml_value = external_to_outbound(&result, &handle_options)
                .map_err(|e| invalid_argument_error(format!("Failed to encode result: {e}")))?;

            Ok(Buffer::from(baml_value.encode_to_vec()))
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
