//! BamlRuntime napi class.
//!
//! Each handle routes through its uint64 registration in the process-wide CFFI registry.

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
    target: bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget,
    /// Explicit TypeVar bindings for a generic call (`CallFunctionArgs.type_args`),
    /// as a name-keyed map in wire (De Bruijn) order. Seeded into the entry
    /// frame's `type_args` slot. Empty for non-generic calls. Mirrors
    /// `bridge_python`'s `DecodedCallArgs::type_args`.
    type_args: indexmap::IndexMap<String, bex_project::RuntimeTy>,
    type_defs: indexmap::IndexMap<String, bex_project::PortableTypeDef>,
}

/// A handle to one uint64 runtime registration in the shared CFFI library.
#[napi]
pub struct BamlRuntime {
    key: u64,
}

#[napi]
impl BamlRuntime {
    #[napi(getter)]
    pub fn runtime_key(&self) -> BigInt {
        self.key.into()
    }

    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        bridge_cffi::unregister_runtime(self.key)
            .map(|_| ())
            .map_err(bridge_error_to_napi)
    }

    /// Create an independent dynamic registration from BAML source files.
    #[napi(factory, js_name = "initializeRuntime")]
    pub fn initialize_runtime(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> napi::Result<Self> {
        // CFFI owns the registration; this handle owns its routing key.
        match bridge_cffi::initialize_runtime(&root_path, files) {
            Ok(bex) => Ok(BamlRuntime {
                key: bridge_cffi::runtime_key(&bex).map_err(bridge_error_to_napi)?,
            }),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }

    /// Create a dynamic runtime, or register a generated uint64 identity, from precompiled BAML bytecode.
    #[napi(factory, js_name = "initializeRuntimeFromBytecode")]
    pub fn initialize_runtime_from_bytecode(
        bytecode: Buffer,
        embedded_baml_toml: Option<String>,
        runtime_key: Option<BigInt>,
    ) -> napi::Result<Self> {
        let result = match runtime_key {
            Some(key) => bridge_cffi::register_runtime_from_bytecode(
                checked_key(key)?,
                bytecode.as_ref(),
                embedded_baml_toml.as_deref(),
            ),
            None => bridge_cffi::initialize_runtime_from_bytecode(
                bytecode.as_ref(),
                embedded_baml_toml.as_deref(),
            ),
        };
        match result {
            Ok(bex) => Ok(BamlRuntime {
                key: bridge_cffi::runtime_key(&bex).map_err(bridge_error_to_napi)?,
            }),
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
            let runtime = bridge_cffi::runtime_for_encoded_call(self.key, args_proto.as_ref())?;
            let decoded = decode_args(args_proto.as_ref())?;
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
            .with_type_defs(decoded.type_defs)
            .build();

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary and error/panic routing) lives in
        // bridge_cffi; we just return the encoded envelope bytes for the TS
        // decoder to surface.
        let bytes = match decoded.target {
            bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionName(
                function_name,
            ) => rt.block_on(bridge_cffi::call_and_encode(
                runtime,
                function_name,
                decoded.kwargs,
                call_ctx,
            )),
            bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionHandle(
                handle_key,
            ) => rt.block_on(bridge_cffi::call_handle_and_encode(
                runtime,
                handle_key,
                decoded.kwargs,
                call_ctx,
            )),
        };

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
        let prepared = (|| -> std::result::Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::runtime_for_encoded_call(self.key, args_proto.as_ref())?;
            let decoded = decode_args(args_proto.as_ref())?;
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
                        .with_type_defs(decoded.type_defs)
                        .build();
                    match decoded.target {
                        bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionName(function_name) => {
                            bridge_cffi::call_and_encode(runtime, function_name, decoded.kwargs, call_ctx).await
                        }
                        bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionHandle(handle_key) => {
                            bridge_cffi::call_handle_and_encode(runtime, handle_key, decoded.kwargs, call_ctx).await
                        }
                    }
                }
                Err(e) => bridge_cffi::error_to_outbound(e),
            };
            Ok(Buffer::from(bytes))
        })
    }
}

/// Resolve an explicit uint64 registration, or the only registered runtime.
#[napi(js_name = "getRuntime")]
pub fn get_runtime(runtime_key: Option<BigInt>) -> napi::Result<BamlRuntime> {
    let runtime = match runtime_key { Some(key) => bridge_cffi::get_runtime_by_key(checked_key(key)?), None => bridge_cffi::get_runtime() }.map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized => napi::Error::new(
            napi::Status::GenericFailure,
            "BamlError: BAML runtime has not been initialized — call BamlRuntime.initializeRuntime first.",
        ),
        other => bridge_error_to_napi(other),
    })?;
    Ok(BamlRuntime {
        key: bridge_cffi::runtime_key(&runtime).map_err(bridge_error_to_napi)?,
    })
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
fn decode_args(
    args_proto: &[u8],
) -> std::result::Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget;

    let args = bridge_ctypes::baml_bridge::cffi::CallFunctionArgs::decode(args_proto)
        .map_err(bridge_ctypes::CtypesError::from)?;

    if args.call_id == 0 {
        return Err(bridge_cffi::BridgeError::InvalidCallId);
    }

    let call_id = bex_project::CallId(args.call_id);
    let target = args
        .call_target
        .ok_or(bridge_cffi::BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !args.type_args.is_empty() {
        return Err(bridge_cffi::BridgeError::FunctionHandleTypeArgs);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    Ok(DecodedCallArgs {
        kwargs: kwargs.into(),
        call_id,
        target,
        type_args: type_args.type_args,
        type_defs: type_args.type_defs,
    })
}

fn checked_key(key: BigInt) -> napi::Result<u64> {
    let (negative, value, lossless) = key.get_u64();
    if negative || !lossless {
        return Err(napi::Error::from_reason(
            "runtime key must be a uint64 bigint",
        ));
    }
    Ok(value)
}
