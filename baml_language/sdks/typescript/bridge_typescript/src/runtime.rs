//! BamlRuntime napi class.
//!
//! The handle carries a u32 key. The corresponding `Arc<dyn Bex>` lives in
//! bridge_cffi's registry and is fetched at each call site.

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
    /// Explicit TypeVar bindings for a generic call (`CallFunctionArgs.type_args`),
    /// as a name-keyed map in wire (De Bruijn) order. Seeded into the entry
    /// frame's `type_args` slot. Empty for non-generic calls. Mirrors
    /// `bridge_python`'s `DecodedCallArgs::type_args`.
    type_args: indexmap::IndexMap<String, bex_project::RuntimeTy>,
}

/// The main BAML runtime. Its engine is stored in bridge_cffi's registry.
#[napi]
pub struct BamlRuntime {
    runtime_key: u32,
}

#[napi]
impl BamlRuntime {
    /// Initialize a registered runtime from in-memory BAML source files.
    /// An explicit key replaces that registry entry; without a key, a new
    /// nonzero, non-`u32::MAX` key is allocated. Renamed from `fromFiles` for parity with
    /// `bridge_python`'s `initialize_runtime` constructor.
    #[napi(factory, js_name = "initializeRuntime")]
    pub fn initialize_runtime(
        root_path: String,
        files: std::collections::HashMap<String, String>,
        runtime_key: Option<u32>,
    ) -> napi::Result<Self> {
        match bridge_cffi::initialize_runtime(&root_path, files, runtime_key) {
            Ok(runtime_key) => Ok(BamlRuntime { runtime_key }),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Initialize a registered runtime from precompiled BAML bytecode.
    #[napi(factory, js_name = "initializeRuntimeFromBytecode")]
    pub fn initialize_runtime_from_bytecode(
        bytecode: Buffer,
        runtime_key: Option<u32>,
    ) -> napi::Result<Self> {
        match bridge_cffi::initialize_runtime_from_bytecode(bytecode.as_ref(), runtime_key) {
            Ok(runtime_key) => Ok(BamlRuntime { runtime_key }),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    #[napi(getter)]
    pub fn runtime_key(&self) -> u32 {
        self.runtime_key
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
            let runtime = bridge_cffi::get_runtime(self.runtime_key)?;
            let decoded = decode_args(args_proto.as_ref(), &function_name)?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, decoded, rt))
        })();
        let _ = (&ctx, &collectors);

        let (runtime, decoded, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return Ok(Buffer::from(bridge_cffi::error_to_outbound(e))),
        };
        let call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
            .with_type_args(decoded.type_args)
            .build();

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
            let runtime = bridge_cffi::get_runtime(self.runtime_key)?;
            let decoded = decode_args(args_proto.as_ref(), &function_name)?;
            Ok((runtime, decoded))
        })();
        let _ = (&ctx, &collectors);

        // Same shared call_and_encode as the sync + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes for the TS decoder.
        env.spawn_future(async move {
            let bytes = match prepared {
                Ok((runtime, decoded)) => {
                    let call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
                        .with_type_args(decoded.type_args)
                        .build();
                    bridge_cffi::call_and_encode(runtime, function_name, decoded.kwargs, call_ctx)
                        .await
                }
                Err(e) => bridge_cffi::error_to_outbound(e),
            };
            Ok(Buffer::from(bytes))
        })
    }
}

/// Return the `BamlRuntime` registered under `runtime_key`, or a
/// `BamlError`-shaped `napi::Error` if that key has not been initialized.
#[napi(js_name = "getRuntime")]
pub fn get_runtime(runtime_key: Option<u32>) -> napi::Result<BamlRuntime> {
    let runtime_key = runtime_key.unwrap_or(0);
    bridge_cffi::get_runtime(runtime_key).map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized
        | bridge_cffi::BridgeError::RuntimeNotFound(_) => napi::Error::new(
            napi::Status::GenericFailure,
            "BamlError: BAML runtime has not been initialized — call BamlRuntime.initializeRuntime first.",
        ),
        other => bridge_error_to_napi(other),
    })?;
    Ok(BamlRuntime { runtime_key })
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
fn decode_args(
    args_proto: &[u8],
    _function_name: &str,
) -> std::result::Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    let args = bridge_ctypes::baml_bridge::cffi::CallFunctionArgs::decode(args_proto)
        .map_err(bridge_ctypes::CtypesError::from)?;

    if args.call_id == 0 {
        return Err(bridge_cffi::BridgeError::InvalidCallId);
    }

    let call_id = bex_project::CallId(args.call_id);
    let type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    Ok(DecodedCallArgs {
        kwargs: kwargs.into(),
        call_id,
        type_args,
    })
}
