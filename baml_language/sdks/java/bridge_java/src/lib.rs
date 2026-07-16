//! bridge_java — JNI bindings for BAML using `bex_engine` (via `bridge_cffi`).
//!
//! In-process JVM analog of `bridge_python` (pyo3) / `bridge_nodejs` (napi):
//! a `cdylib` that links `bridge_cffi` and speaks the same
//! `baml_bridge.cffi.v1` protobuf envelopes. Because the engine is linked
//! in-process there is a true synchronous call path (block on the shared
//! tokio runtime) rather than synthesizing sync over an async-only C ABI.
//!
//! The JNI layer is deliberately thin — bytes in, bytes out. All protobuf
//! encoding/decoding of `CallFunctionArgs` / `BamlOutboundResult` happens on
//! the Java side (`baml_bridge.internal.Proto*`); here we only reuse
//! `bridge_cffi`'s existing byte-oriented entry points, mirroring
//! `bridge_python/src/runtime.rs::call_function_sync` almost line for line.
//!
//! The exported symbols correspond to native methods on the Java class
//! `baml_bridge.BamlFfi`. JNI mangles the package `baml_bridge` as
//! `baml_1bridge` (an underscore in a Java name is escaped `_1`).

use bex_project::{BexArgs, CallId, RuntimeTy};
use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use indexmap::IndexMap;
use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JString},
    sys::jlong,
};
use prost::Message;

/// Decoded `CallFunctionArgs` — the JVM analog of `bridge_python`'s
/// `DecodedCallArgs`.
struct DecodedCallArgs {
    kwargs: BexArgs,
    call_id: CallId,
    /// Explicit, named `TypeVar` bindings for a generic call, in De Bruijn
    /// order (empty for non-generic calls). See `CallFunctionArgs.type_args`.
    type_args: IndexMap<String, RuntimeTy>,
}

/// Decode protobuf-encoded `CallFunctionArgs` bytes into `BexArgs`.
///
/// Returns a `BridgeError` (not a thrown exception) so the byte-returning
/// call site can route the failure through `bridge_cffi::error_to_outbound`
/// into the structured `BamlOutboundResult` envelope, exactly like
/// `bridge_python` does — the Java side then decodes + raises uniformly.
fn decode_args(args_proto: &[u8]) -> Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    let args = bridge_ctypes::baml_bridge::cffi::CallFunctionArgs::decode(args_proto)
        .map_err(bridge_ctypes::CtypesError::from)?;

    if args.call_id == 0 {
        return Err(bridge_cffi::BridgeError::InvalidCallId);
    }

    let call_id = CallId(args.call_id);
    let type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    Ok(DecodedCallArgs {
        kwargs: kwargs.into(),
        call_id,
        type_args,
    })
}

/// Shared synchronous call body. Mirrors `bridge_python`'s
/// `call_function_sync`: pre-call host-boundary failures (uninitialized
/// runtime, malformed args, no tokio runtime) are encoded into the
/// `BamlOutboundResult` envelope rather than thrown, so the returned bytes
/// decode + raise uniformly on the Java side. The `catch_unwind` + engine
/// error handling already lives in `bridge_cffi::call_and_encode`.
fn call_sync_to_bytes(function_name: String, args_proto: &[u8]) -> Vec<u8> {
    let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
        let runtime = bridge_cffi::get_runtime()?;
        let decoded = decode_args(args_proto)?;
        let rt = bridge_cffi::get_tokio_runtime()?;
        Ok((runtime, decoded, rt))
    })();

    let (runtime, decoded, rt) = match prepared {
        Ok(v) => v,
        Err(e) => return bridge_cffi::error_to_outbound(e),
    };

    let call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
        .with_type_args(decoded.type_args)
        .build();

    // Block on the shared multi-thread tokio runtime, like the pyo3 sync path
    // (`rt.block_on(...)`). Returns the encoded `BamlOutboundResult` bytes.
    rt.block_on(bridge_cffi::call_and_encode(
        runtime,
        function_name,
        decoded.kwargs,
        call_ctx,
    ))
}

/// `baml_bridge.BamlFfi.nativeInitFromBytecode(byte[] bytecode)`.
///
/// Initialize the process-global runtime from serialized BAML bytecode
/// (`bridge_cffi::initialize_runtime_from_bytecode`, the same path
/// `bridge_python` uses). Idempotent in the same sense as Python: the
/// single-slot singleton is replaced, so a second call swaps the runtime.
/// A setup failure is thrown as an unchecked `RuntimeException` (this is a
/// handle-returning site with no envelope to ride, like Python's
/// `initialize_runtime_from_bytecode` raising).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeInitFromBytecode(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    bytecode: JByteArray<'_>,
) {
    // Register this bridge with the versioned ABI (idempotent; mirrors
    // bridge_python). A canonical-version mismatch is a real deployment
    // error and surfaces as a Java exception via the panic handler.
    if let Err(e) = bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Java,
        sdk_version: baml_version::CANONICAL_VERSION.to_string(),
    }) {
        throw_runtime_exception(&mut env, &format!("BAML bridge registration failed: {e}"));
        return;
    }
    let bytes = match env.convert_byte_array(&bytecode) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read bytecode array: {e}"));
            return;
        }
    };

    if let Err(e) = bridge_cffi::initialize_runtime_from_bytecode(&bytes) {
        throw_runtime_exception(&mut env, &format!("runtime initialization failed: {e}"));
    }
}

/// `baml_bridge.BamlFfi.nativeCallSync(String fqn, byte[] encodedCallFunctionArgs) -> byte[]`.
///
/// Decode/dispatch exactly like `bridge_python`'s sync call: parse the
/// `CallFunctionArgs` bytes, run the function (blocking), and return the
/// serialized `BamlOutboundResult` bytes. Engine errors/panics are carried
/// inside those bytes, not thrown here — the Java side inspects the envelope.
/// A `RuntimeException` is thrown only for JNI-glue failures (bad UTF-8 fqn,
/// array read/alloc failure).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeCallSync<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    fqn: JString<'local>,
    encoded_args: JByteArray<'local>,
) -> JByteArray<'local> {
    let function_name: String = match env.get_string(&fqn) {
        Ok(js) => String::from(js),
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read fqn string: {e}"));
            return JByteArray::default();
        }
    };

    let args_proto = match env.convert_byte_array(&encoded_args) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read args array: {e}"));
            return JByteArray::default();
        }
    };

    let out = call_sync_to_bytes(function_name, &args_proto);

    match env.byte_array_from_slice(&out) {
        Ok(arr) => arr,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to allocate result array: {e}"));
            JByteArray::default()
        }
    }
}

/// `baml_bridge.BamlFfi.nativeNewCallId() -> long`.
///
/// Mint a process-unique function-call id via the same
/// `bridge_cffi::new_function_call_id()` counter `bridge_python`'s
/// `new_function_call()` exposes, so a nonzero `call_id` can be stamped onto
/// `CallFunctionArgs` (a zero `call_id` is rejected engine-side).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeNewCallId(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jlong {
    // Call ids fit u64; JNI `long` is i64. The counter starts at 1 and the
    // low 63 bits are what the engine keys on, so the bit cast is faithful.
    bridge_cffi::new_function_call_id() as jlong
}

/// Throw an unchecked `java.lang.RuntimeException`, ignoring a failure to
/// throw (nothing more can be done at that point). All messages are prefixed
/// so they are attributable to this bridge.
fn throw_runtime_exception(env: &mut JNIEnv<'_>, message: &str) {
    let _ = env.throw_new(
        "java/lang/RuntimeException",
        format!("bridge_java: {message}"),
    );
}
