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

use std::sync::{Arc, Once, OnceLock};

use bex_project::{BexArgs, BexExternalAdt, CallId, MediaKind, MediaValue, RuntimeTy};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, kwargs_to_bex_values};
use indexmap::IndexMap;
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteArray, JClass, JString, JValue},
    sys::{jboolean, jint, jlong},
};
use prost::Message;

/// Decoded `CallFunctionArgs` — the JVM analog of `bridge_python`'s
/// `DecodedCallArgs`.
struct DecodedCallArgs {
    kwargs: BexArgs,
    call_id: CallId,
    target: bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget,
    /// Semantic projection of the authored function. Direct is protobuf's
    /// default so older generated SDKs remain wire-compatible.
    operation: bex_project::FunctionOperation,
    /// Explicit, named `TypeVar` bindings for a generic call, in De Bruijn
    /// order (empty for non-generic calls). See `CallFunctionArgs.type_args`.
    type_args: IndexMap<String, RuntimeTy>,
    type_defs: IndexMap<String, bex_project::PortableTypeDef>,
}

/// Decode protobuf-encoded `CallFunctionArgs` bytes into `BexArgs`.
///
/// Returns a `BridgeError` (not a thrown exception) so the byte-returning
/// call site can route the failure through `bridge_cffi::error_to_outbound`
/// into the structured `BamlOutboundResult` envelope, exactly like
/// `bridge_python` does — the Java side then decodes + raises uniformly.
fn decode_args(args_proto: &[u8]) -> Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget;

    let args = bridge_ctypes::baml_bridge::cffi::CallFunctionArgs::decode(args_proto)
        .map_err(bridge_ctypes::CtypesError::from)?;

    if args.call_id == 0 {
        return Err(bridge_cffi::BridgeError::InvalidCallId);
    }

    let call_id = CallId(args.call_id);
    let operation = bridge_cffi::decode_function_operation(args.operation)?;
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
        operation,
        type_args: type_args.type_args,
        type_defs: type_args.type_defs,
    })
}

/// Shared synchronous call body. Mirrors `bridge_python`'s
/// `call_function_sync`: pre-call host-boundary failures (uninitialized
/// runtime, malformed args, no tokio runtime) are encoded into the
/// `BamlOutboundResult` envelope rather than thrown, so the returned bytes
/// decode + raise uniformly on the Java side. The `catch_unwind` + engine
/// error handling already lives in `bridge_cffi::call_and_encode`.
fn call_sync_to_bytes(args_proto: &[u8]) -> Vec<u8> {
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
        .with_type_defs(decoded.type_defs)
        .build();

    // Block on the shared multi-thread tokio runtime, like the pyo3 sync path
    // (`rt.block_on(...)`). Returns the encoded `BamlOutboundResult` bytes.
    match decoded.target {
        bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionName(
            function_name,
        ) => rt.block_on(bridge_cffi::call_operation_and_encode(
            runtime,
            function_name,
            decoded.operation,
            decoded.kwargs,
            call_ctx,
        )),
        bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionHandle(
            handle_key,
        ) => rt.block_on(bridge_cffi::call_handle_operation_and_encode(
            runtime,
            handle_key,
            decoded.operation,
            decoded.kwargs,
            call_ctx,
        )),
    }
}

/// `baml_bridge.BamlFfi.nativeInitFromBytecode(byte[] bytecode, String metadata, String runtimeVersion, String toolchainVersion)`.
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
    class: JClass<'_>,
    bytecode: JByteArray<'_>,
    embedded_baml_toml: JString<'_>,
    bridge_runtime_version: JString<'_>,
    toolchain_version: JString<'_>,
) {
    // Capture the JVM + `BamlFfi` class ref (idempotent) so the host-dispatch
    // and host-release trampolines can call back into Java from an engine
    // thread — host callables can fire during a *sync* call (no
    // `nativeCallAsync` ever runs), so the route must be wired at init, not
    // lazily on first async call. Reuses the async completion route's capture.
    if let Err(e) = ensure_completion_route(&mut env, &class) {
        throw_runtime_exception(
            &mut env,
            &format!("failed to wire host-callback route: {e}"),
        );
        return;
    }
    // Install the host-value dispatch + release callbacks once (bridge_cffi is
    // first-call-wins, but the release side logs on a second registration — so
    // guard with a `Once` to keep re-init quiet). These make BAML→host callable
    // dispatch and GC-driven release land on the static `BamlFfi.hostDispatch` /
    // `BamlFfi.hostRelease` methods.
    REGISTER_HOST_CALLBACKS.call_once(|| {
        bridge_cffi::register_host_dispatch_callback(host_dispatch_trampoline);
        bridge_cffi::register_host_release_callback(host_release_trampoline);
        bridge_cffi::register_unhandled_spawn_error_callback(unhandled_spawn_error_trampoline);
    });

    let bridge_runtime_version: String = match env.get_string(&bridge_runtime_version) {
        Ok(value) => value.into(),
        Err(e) => {
            throw_runtime_exception(
                &mut env,
                &format!("failed to read bridge runtime version: {e}"),
            );
            return;
        }
    };
    let toolchain_version: String = match env.get_string(&toolchain_version) {
        Ok(value) => value.into(),
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read toolchain version: {e}"));
            return;
        }
    };
    // Register the same stamped versions exposed by BamlFfi's public getters.
    if let Err(e) = bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Java,
        bridge_runtime_name: BRIDGE_RUNTIME_NAME.to_string(),
        bridge_runtime_version,
        toolchain_version,
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
    let embedded_baml_toml: Option<String> = if embedded_baml_toml.is_null() {
        None
    } else {
        match env.get_string(&embedded_baml_toml) {
            Ok(value) => Some(value.into()),
            Err(e) => {
                throw_runtime_exception(
                    &mut env,
                    &format!("failed to read embedded baml.toml: {e}"),
                );
                return;
            }
        }
    };

    if let Err(e) =
        bridge_cffi::initialize_runtime_from_bytecode(&bytes, embedded_baml_toml.as_deref())
    {
        throw_runtime_exception_exact(&mut env, &e.to_string());
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeShutdownRuntime(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    let result = bridge_cffi::get_tokio_runtime()
        .and_then(|runtime| runtime.block_on(bridge_cffi::shutdown_runtime()));
    if let Err(error) = result {
        throw_runtime_exception(&mut env, &format!("runtime shutdown failed: {error}"));
    }
}

/// `baml_bridge.BamlFfi.nativeCallSync(byte[] encodedCallFunctionArgs) -> byte[]`.
///
/// Decode/dispatch exactly like `bridge_python`'s sync call: parse the
/// `CallFunctionArgs` bytes, run the function (blocking), and return the
/// serialized `BamlOutboundResult` bytes. Engine errors/panics are carried
/// inside those bytes, not thrown here — the Java side inspects the envelope.
/// A `RuntimeException` is thrown only for JNI-glue failures (array read/alloc failure).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeCallSync<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    encoded_args: JByteArray<'local>,
) -> JByteArray<'local> {
    let args_proto = match env.convert_byte_array(&encoded_args) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read args array: {e}"));
            return JByteArray::default();
        }
    };

    let out = call_sync_to_bytes(&args_proto);

    match env.byte_array_from_slice(&out) {
        Ok(arr) => arr,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to allocate result array: {e}"));
            JByteArray::default()
        }
    }
}

// ===========================================================================
// Async call path — the JVM analog of bridge_cffi's `call_function` C ABI
// (`lib_native.rs`) and bridge_python's async `call_function` (`runtime.rs`).
//
// `nativeCallAsync` spawns the engine call on the shared tokio runtime and
// returns immediately; the completing engine thread attaches to the JVM and
// invokes the static `baml_bridge.BamlFfi.completeCall(long, byte[])`, resolving
// the `CompletableFuture` the Java side registered under the same call id. The
// delivered bytes are the *identical* `BamlOutboundResult` envelope the sync
// path returns, so an async result decodes + raises through the exact same
// `ProtoReader.decodeOutboundResult` path (see `BamlFfi.decodeResult`) — ok,
// error and panic arms alike. There is no separate error/panic channel.
// ===========================================================================

/// The host JVM, captured once (from the first `nativeCallAsync`). A tokio
/// worker thread must attach to this before it can call back into Java.
static JVM: OnceLock<JavaVM> = OnceLock::new();

/// A global reference to the `baml_bridge.BamlFfi` class, captured once from a
/// JVM application thread. A bare `FindClass` on an attached tokio worker would
/// resolve against the system class loader and miss this application class, so
/// the class object the JVM hands to every static native method on `BamlFfi` is
/// promoted to a `GlobalRef` here and reused for the completion callback.
static BAML_FFI_CLASS: OnceLock<GlobalRef> = OnceLock::new();

/// The Java completion route: `static void completeCall(long, byte[])`.
const COMPLETE_CALL_METHOD: &str = "completeCall";
const COMPLETE_CALL_SIG: &str = "(J[B)V";

/// The Java host-dispatch route: `static void hostDispatch(long, long, byte[])`.
const HOST_DISPATCH_METHOD: &str = "hostDispatch";
const HOST_DISPATCH_SIG: &str = "(JJ[B)V";
/// The Java host-release route: `static void hostRelease(long)`.
const HOST_RELEASE_METHOD: &str = "hostRelease";
const HOST_RELEASE_SIG: &str = "(J)V";
const UNHANDLED_SPAWN_ERROR_METHOD: &str = "unhandledSpawnError";
const UNHANDLED_SPAWN_ERROR_SIG: &str = "([BZ)V";

/// Guards the one-time `register_host_{dispatch,release}_callback` install so a
/// runtime re-init (`nativeInitFromBytecode` replaces the runtime) does not
/// re-register — `register_host_release_callback` logs a diagnostic on a second
/// call, which would be spurious noise on every re-init.
static REGISTER_HOST_CALLBACKS: Once = Once::new();
const BRIDGE_RUNTIME_NAME: &str = "com.boundaryml:baml-bridge";

/// Capture the JVM handle and a `GlobalRef` to the `BamlFfi` class (idempotent,
/// first-call-wins). `class` is the `baml_bridge.BamlFfi` class object the JVM
/// passes to this static native method, so no `FindClass` is needed and the
/// application class loader is implicitly correct.
fn ensure_completion_route(
    env: &mut JNIEnv<'_>,
    class: &JClass<'_>,
) -> Result<(), jni::errors::Error> {
    if JVM.get().is_none() {
        let _ = JVM.set(env.get_java_vm()?);
    }
    if BAML_FFI_CLASS.get().is_none() {
        let _ = BAML_FFI_CLASS.set(env.new_global_ref(class)?);
    }
    Ok(())
}

/// `baml_bridge.BamlFfi.nativeCallAsync(long callId, byte[] encodedCallFunctionArgs)`.
///
/// Non-blocking sibling of [`Java_baml_1bridge_BamlFfi_nativeCallSync`]: it
/// decodes/dispatches the identical `CallFunctionArgs` payload but spawns the
/// engine call on the shared tokio runtime and returns at once. `call_id` is
/// passed explicitly (it is also embedded in the encoded args) so the completion
/// route stays keyed even if the args fail to decode. Completion — ok value,
/// thrown error, panic, or a pre-call host-boundary failure — is delivered as
/// one `BamlOutboundResult` envelope to `completeCall(call_id, bytes)`. A
/// `RuntimeException` is thrown only for JNI-glue failures while reading the
/// argument array before the call is handed off.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeCallAsync<'local>(
    mut env: JNIEnv<'local>,
    class: JClass<'local>,
    call_id: jlong,
    encoded_args: JByteArray<'local>,
) {
    if let Err(e) = ensure_completion_route(&mut env, &class) {
        throw_runtime_exception(
            &mut env,
            &format!("failed to wire async completion route: {e}"),
        );
        return;
    }

    let args_proto = match env.convert_byte_array(&encoded_args) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read args array: {e}"));
            return;
        }
    };

    spawn_async_call(call_id as u64, args_proto);
}

/// Spawn the engine call and route its `BamlOutboundResult` envelope back to
/// `completeCall`. Mirrors `bridge_cffi::lib_native::call_function_inner`, but
/// delivers over JNI instead of the C result callback. Pre-call host-boundary
/// failures (uninitialized runtime, malformed args, no tokio runtime) are
/// encoded into the same envelope and delivered immediately, so they decode +
/// raise identically to a sync pre-call failure.
fn spawn_async_call(call_id: u64, args_proto: Vec<u8>) {
    let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
        let runtime = bridge_cffi::get_runtime()?;
        let decoded = decode_args(&args_proto)?;
        let rt = bridge_cffi::get_tokio_runtime()?;
        Ok((runtime, decoded, rt))
    })();

    let (runtime, decoded, rt) = match prepared {
        Ok(v) => v,
        Err(e) => {
            // Same envelope bytes the sync path returns, delivered on this
            // (JVM) thread — the Java future is already registered.
            deliver_completion(call_id, bridge_cffi::error_to_outbound(e));
            return;
        }
    };

    let DecodedCallArgs {
        kwargs,
        call_id: engine_call_id,
        target,
        operation,
        type_args,
        type_defs,
    } = decoded;
    let call_ctx = bridge_cffi::function_call_context_builder(engine_call_id)
        .with_type_args(type_args)
        .with_type_defs(type_defs)
        .build();

    rt.spawn(async move {
        // Inner task so a panic during result *encoding* is caught (via the
        // JoinError) and still delivered as an SdkPanic envelope, rather than
        // silently dropping the task and hanging the future. `call_and_encode`
        // already turns an engine-call panic into that envelope itself; this
        // guards the rarer encode-time panic, exactly as the C-ABI path does.
        let inner = tokio::spawn(async move {
            match target {
                bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionName(function_name) => {
                    bridge_cffi::call_operation_and_encode(runtime, function_name, operation, kwargs, call_ctx).await
                }
                bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionHandle(handle_key) => {
                    bridge_cffi::call_handle_operation_and_encode(runtime, handle_key, operation, kwargs, call_ctx).await
                }
            }
        });
        let bytes = match inner.await {
            Ok(bytes) => bytes,
            Err(join_err) => encode_task_failure(join_err),
        };
        deliver_completion(call_id, bytes);
    });
}

/// Encode a spawned-task failure (a panic escaping result encoding, or — once
/// cancellation is wired — a cancelled task) as `BamlOutboundResult` SdkPanic
/// envelope bytes, so even this path decodes + raises uniformly on the Java side.
fn encode_task_failure(err: tokio::task::JoinError) -> Vec<u8> {
    if err.is_panic() {
        bridge_cffi::baml_to_host::panic_to_outbound(err.into_panic().as_ref())
    } else {
        let msg = String::from("BAML async call task ended without producing a result");
        bridge_cffi::baml_to_host::panic_to_outbound(&msg)
    }
}

/// Resolve the Java `CompletableFuture` registered under `call_id` with the
/// encoded `BamlOutboundResult` envelope. Attaches the current (engine) thread
/// to the JVM as a daemon — a cheap no-op on a thread that is already attached
/// (e.g. the JVM caller thread for a pre-call failure), and self-detaching when
/// a reused tokio worker exits — then invokes the static `completeCall`.
fn deliver_completion(call_id: u64, bytes: Vec<u8>) {
    if let Err(e) = deliver_completion_inner(call_id, &bytes) {
        // The only failure modes here (attach failure, unwired route) leave the
        // future unresolved, which can be neither retried nor surfaced across
        // the already-returned native call. Log so it is at least diagnosable.
        eprintln!("bridge_java: failed to deliver async completion for call {call_id}: {e}");
    }
}

fn with_java_callback(
    context: &str,
    callback: impl FnOnce(&mut JNIEnv<'_>, &GlobalRef) -> Result<(), String>,
) -> Result<(), String> {
    let vm = JVM
        .get()
        .ok_or_else(|| format!("JavaVM was not captured before {context}"))?;
    let class = BAML_FFI_CLASS
        .get()
        .ok_or_else(|| format!("BamlFfi class ref was not captured before {context}"))?;
    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|error| format!("attach failed: {error}"))?;
    let result = callback(&mut env, class);
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result
}

fn deliver_completion_inner(call_id: u64, bytes: &[u8]) -> Result<(), String> {
    with_java_callback("completion", |env, class| {
        let payload = env
            .byte_array_from_slice(bytes)
            .map_err(|error| format!("result array alloc failed: {error}"))?;
        env.call_static_method(
            class,
            COMPLETE_CALL_METHOD,
            COMPLETE_CALL_SIG,
            &[JValue::Long(call_id as jlong), JValue::from(&payload)],
        )
        .map(|_| ())
        .map_err(|error| format!("completeCall invocation failed: {error}"))
    })
}

extern "C" fn unhandled_spawn_error_trampoline(content: *const i8, length: usize, cancelled: i32) {
    let bytes = if content.is_null() || length == 0 {
        Vec::new()
    } else {
        // SAFETY: bridge_cffi keeps the borrowed callback buffer valid until return.
        unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) }.to_vec()
    };
    if let Err(error) = deliver_unhandled_spawn_error(&bytes, cancelled != 0) {
        eprintln!("bridge_java: failed to deliver unhandled spawn error: {error}");
    }
}

fn deliver_unhandled_spawn_error(bytes: &[u8], cancelled: bool) -> Result<(), String> {
    with_java_callback("unhandled spawn error", |env, class| {
        let payload = env
            .byte_array_from_slice(bytes)
            .map_err(|error| format!("error array allocation failed: {error}"))?;
        env.call_static_method(
            class,
            UNHANDLED_SPAWN_ERROR_METHOD,
            UNHANDLED_SPAWN_ERROR_SIG,
            &[
                JValue::from(&payload),
                JValue::Bool(if cancelled { 1 } else { 0 }),
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("unhandledSpawnError invocation failed: {error}"))
    })
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

/// `baml_bridge.BamlFfi.nativeCancelFunctionCall(long callId) -> boolean`.
///
/// Cancel an in-flight function call by id via
/// [`bridge_cffi::cancel_function_call_by_id`], the same entry point
/// `bridge_python`'s `BamlCallContext.abort` funnels through. Returns `true`
/// when the runtime accepted the cancel, `false` otherwise (unknown /
/// already-completed id, id 0, or an uninitialized runtime). Never throws:
/// both `BamlCallContext.abort()` and a host `future.cancel(true)` fire it and
/// tolerate a `false`, so there is no envelope or exception to surface.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeCancelFunctionCall(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    call_id: jlong,
) -> jboolean {
    // `call_id` round-trips u64 → i64 → u64; `cancel_function_call_by_id`
    // rejects a 0 id and an uninitialized runtime, returning false.
    jboolean::from(bridge_cffi::cancel_function_call_by_id(call_id as u64))
}

// ===========================================================================
// Host-callable dispatch + release — the JVM analog of bridge_python's
// `host_dispatch_callback` / `host_release_callback` (host_value.rs).
//
// When BAML invokes a host callable, the engine's `call_host_value` sysop fires
// the registered dispatch callback on a runtime thread; the trampoline attaches
// that thread to the JVM and hands (host_value_key, call_id, BamlToHostCall
// bytes) to the static `BamlFfi.hostDispatch`, which submits the decode / invoke
// / complete work to a Java executor and returns promptly (the api.rs
// "return promptly; dispatches may be concurrent" contract — the Java executor
// is the async boundary, so the Rust hop is a bounded inline JNI call rather
// than a tokio spawn). The Java side reports the result via
// `nativeCompleteHostCall` → `bridge_cffi::complete_host_call`. Release fires
// when the engine drops the last `HostValueArc`; the trampoline forwards the key
// to `BamlFfi.hostRelease`.
//
// The registry (callable + opaque-throwable objects) lives entirely Java-side
// (BamlFfi.HOST_VALUES); Rust is a pure router, so identity is a plain JVM
// reference (no GlobalRef bookkeeping) and `assertSame` round-trips for free.
// ===========================================================================

/// Dispatch a BAML→host callable invocation into Java. Copies the wire bytes,
/// attaches the (engine) thread to the JVM, and calls the static
/// `BamlFfi.hostDispatch(hostValueKey, callId, bamlToHostCall)`. On any failure
/// to reach Java, completes the in-flight call with an empty-payload error so
/// the engine surfaces a `BridgeFailure` (→ `SdkPanic`) instead of awaiting
/// forever.
///
/// The registered `BamlHostDispatchCallback` derefs `args` inside an `unsafe`
/// block; pointer validity is the engine's contract (documented on the callback
/// type in `bridge_cffi::api`). This fn is private, so the public-only
/// `not_unsafe_ptr_arg_deref` lint does not apply.
extern "C" fn host_dispatch_trampoline(
    host_value_key: u64,
    call_id: u32,
    args: *const u8,
    length: usize,
) {
    // Copy the wire bytes: the dispatch task outlives this stack frame (the Java
    // executor runs the callable asynchronously), and `complete_host_call`
    // reads from a Java-owned array anyway.
    let bytes: Vec<u8> = if length == 0 || args.is_null() {
        Vec::new()
    } else {
        // SAFETY: the engine guarantees `args` is valid for `length` bytes for
        // the duration of this call (see `sys_native::host_dispatch`).
        unsafe { std::slice::from_raw_parts(args, length) }.to_vec()
    };

    if let Err(e) = dispatch_into_java(host_value_key, call_id, &bytes) {
        eprintln!("bridge_java: host dispatch for key {host_value_key} call {call_id} failed: {e}");
        // Never leave the engine awaiting: an empty-payload error decodes to a
        // BridgeFailure engine-side (ffi/host_value.rs), the correct routing for
        // a bridge-layer fault (we could not even reach the callable).
        complete_host_call_bridge_failure(call_id);
    }
}

fn dispatch_into_java(host_value_key: u64, call_id: u32, bytes: &[u8]) -> Result<(), String> {
    with_java_callback("host dispatch", |env, class| {
        let payload = env
            .byte_array_from_slice(bytes)
            .map_err(|error| format!("args array alloc failed: {error}"))?;
        env.call_static_method(
            class,
            HOST_DISPATCH_METHOD,
            HOST_DISPATCH_SIG,
            &[
                JValue::Long(host_value_key as jlong),
                JValue::Long(u64::from(call_id) as jlong),
                JValue::from(&payload),
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("hostDispatch invocation failed: {error}"))
    })
}

/// Notify Java that a host-value key can be released (the engine dropped the
/// last `HostValueArc`). Best-effort — a failure to reach Java only delays the
/// registry entry's removal (a leak until process exit), matching the
/// `xfail`/`@Disabled` release semantics across bridges.
extern "C" fn host_release_trampoline(host_value_key: u64) {
    if let Err(e) = release_into_java(host_value_key) {
        eprintln!("bridge_java: host release for key {host_value_key} failed: {e}");
    }
}

fn release_into_java(host_value_key: u64) -> Result<(), String> {
    with_java_callback("host release", |env, class| {
        env.call_static_method(
            class,
            HOST_RELEASE_METHOD,
            HOST_RELEASE_SIG,
            &[JValue::Long(host_value_key as jlong)],
        )
        .map(|_| ())
        .map_err(|error| format!("hostRelease invocation failed: {error}"))
    })
}

/// Complete an in-flight host call with an empty error payload, which
/// `bridge_cffi::complete_host_call` maps to a `BridgeFailure` — the routing for
/// a bridge-layer fault (missing callable, unreachable JVM).
fn complete_host_call_bridge_failure(call_id: u32) {
    bridge_cffi::complete_host_call(call_id, 1, std::ptr::null(), 0);
}

/// `baml_bridge.BamlFfi.nativeCompleteHostCall(long callId, boolean isError, byte[] content)`.
///
/// Forwards a completed host-callable result (or thrown value) to
/// `bridge_cffi::complete_host_call`. `content` is a protobuf-encoded
/// `InboundValue` (host→engine direction); an empty error payload is the
/// bridge-failure signal. `complete_host_call` reads `content` synchronously, so
/// the Java-owned array stays valid for the call.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeCompleteHostCall<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    call_id: jlong,
    is_error: jboolean,
    content: JByteArray<'local>,
) {
    let bytes = match env.convert_byte_array(&content) {
        Ok(b) => b,
        Err(e) => {
            // Could not read the payload — still complete the call (with a
            // bridge failure) so the engine does not hang on this id.
            eprintln!("bridge_java: nativeCompleteHostCall failed to read content: {e}");
            complete_host_call_bridge_failure(call_id as u32);
            return;
        }
    };
    bridge_cffi::complete_host_call(
        call_id as u32,
        i32::from(is_error != 0),
        bytes.as_ptr() as *const i8,
        bytes.len(),
    );
}

// ===========================================================================
// Handle lifecycle + media (baml.media.{Image,Audio,Video,Pdf}).
//
// The JVM analog of `bridge_python`'s `media.rs` / `py_handle.rs`: media values
// are minted as `Adt(Media)` rows in the shared `HANDLE_TABLE` and referenced
// across the JNI boundary by their `u64` key (returned as a `long`). Encode
// clones a fresh key for the wire so the engine can `drain` it while the Java
// object keeps its own row; decode rehydrates a wrapper class from the key +
// `handle_type` tag. Mirrors `bridge_cffi::ffi::handle` — we call the same
// underlying `MediaValue` / `HANDLE_TABLE` API in-process rather than the
// C-string ABI, since the JNI layer already owns `String` conversion.
// ===========================================================================

/// Map a proto `MediaTypeEnum` discriminant (as passed from Java) to a
/// `MediaKind`. Mirrors `bridge_cffi::ffi::handle::media_kind_from_proto`.
fn media_kind_from_proto(kind: jint) -> Option<MediaKind> {
    use bridge_ctypes::baml_bridge::cffi::MediaTypeEnum;
    match kind {
        x if x == MediaTypeEnum::Image as jint => Some(MediaKind::Image),
        x if x == MediaTypeEnum::Audio as jint => Some(MediaKind::Audio),
        x if x == MediaTypeEnum::Pdf as jint => Some(MediaKind::Pdf),
        x if x == MediaTypeEnum::Video as jint => Some(MediaKind::Video),
        x if x == MediaTypeEnum::Other as jint => Some(MediaKind::Generic),
        _ => None,
    }
}

/// Read a required `JString` argument into an owned `String`, throwing (and
/// returning `None`) on a null pointer or invalid UTF-8.
fn read_required_string(env: &mut JNIEnv<'_>, s: &JString<'_>, ctx: &str) -> Option<String> {
    if s.as_raw().is_null() {
        throw_runtime_exception(env, &format!("{ctx}: required string argument was null"));
        return None;
    }
    match env.get_string(s) {
        Ok(js) => Some(String::from(js)),
        Err(e) => {
            throw_runtime_exception(env, &format!("{ctx}: failed to read string argument: {e}"));
            None
        }
    }
}

/// Read an optional `JString` (Java `null` → `None`), throwing on invalid UTF-8.
fn read_optional_string(
    env: &mut JNIEnv<'_>,
    s: &JString<'_>,
    ctx: &str,
) -> Result<Option<String>, ()> {
    if s.as_raw().is_null() {
        return Ok(None);
    }
    match env.get_string(s) {
        Ok(js) => Ok(Some(String::from(js))),
        Err(e) => {
            throw_runtime_exception(env, &format!("{ctx}: failed to read mime string: {e}"));
            Err(())
        }
    }
}

/// Convert an `Option<String>` accessor result into a Java `String` (or Java
/// `null` for `None`), throwing on allocation failure.
fn optional_string_to_jstring<'local>(
    env: &mut JNIEnv<'local>,
    value: Option<String>,
    ctx: &str,
) -> JString<'local> {
    match value {
        None => JString::default(), // Java null — the accessor field is absent.
        Some(s) => match env.new_string(s) {
            Ok(js) => js,
            Err(e) => {
                throw_runtime_exception(env, &format!("{ctx}: failed to allocate string: {e}"));
                JString::default()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::decode_args;

    #[test]
    fn call_argument_decoder_preserves_stream_operation() {
        use bridge_ctypes::baml_bridge::cffi::{
            CallFunctionArgs, FunctionOperation, call_function_args::CallTarget,
        };

        let encoded = CallFunctionArgs {
            kwargs: Vec::new(),
            call_id: 1,
            type_args: Vec::new(),
            operation: FunctionOperation::Stream as i32,
            call_target: Some(CallTarget::FunctionName("user.Extract".to_string())),
        }
        .encode_to_vec();

        let decoded = decode_args(&encoded).expect("decode stream call");
        assert_eq!(decoded.operation, bex_project::FunctionOperation::Stream);
    }
}

/// Resolve a live media row by key, or `None` when the key does not identify an
/// `Adt(Media)` entry (invalid key or wrong handle kind).
fn resolve_media(key: jlong) -> Option<Arc<MediaValue>> {
    let entry = HANDLE_TABLE.resolve(key as u64)?;
    match &*entry {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(media)) => Some(media.clone()),
        _ => None,
    }
}

/// `nativeMediaFromUrl(int kind, String url, String mimeType) -> long`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaFromUrl<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    kind: jint,
    url: JString<'local>,
    mime: JString<'local>,
) -> jlong {
    media_from(
        &mut env,
        kind,
        url,
        mime,
        "nativeMediaFromUrl",
        MediaValue::from_url,
    )
}

/// `nativeMediaFromFile(int kind, String path, String mimeType) -> long`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaFromFile<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    kind: jint,
    path: JString<'local>,
    mime: JString<'local>,
) -> jlong {
    media_from(
        &mut env,
        kind,
        path,
        mime,
        "nativeMediaFromFile",
        MediaValue::from_file,
    )
}

/// `nativeMediaFromBase64(int kind, String base64, String mimeType) -> long`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaFromBase64<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    kind: jint,
    base64: JString<'local>,
    mime: JString<'local>,
) -> jlong {
    media_from(
        &mut env,
        kind,
        base64,
        mime,
        "nativeMediaFromBase64",
        MediaValue::from_base64,
    )
}

/// Shared body for the three media constructors: validate the kind, read the
/// value + optional mime strings, then mint the row and return its key. Returns
/// `0` after throwing on any input failure (a thrown JNI method's return value
/// is ignored by the JVM).
fn media_from<'local>(
    env: &mut JNIEnv<'local>,
    kind: jint,
    value: JString<'local>,
    mime: JString<'local>,
    ctx: &str,
    make: fn(MediaKind, &str, Option<&str>) -> Arc<MediaValue>,
) -> jlong {
    let Some(media_kind) = media_kind_from_proto(kind) else {
        throw_runtime_exception(env, &format!("{ctx}: unsupported media kind {kind}"));
        return 0;
    };
    let Some(value) = read_required_string(env, &value, ctx) else {
        return 0;
    };
    let mime = match read_optional_string(env, &mime, ctx) {
        Ok(m) => m,
        Err(()) => return 0,
    };
    let media = make(media_kind, &value, mime.as_deref());
    HANDLE_TABLE.insert(CffiHandleTableEntry::Adt(BexExternalAdt::Media(media))) as jlong
}

/// `nativeMediaUrl(long key) -> String` (Java `null` when the media has no URL).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaUrl<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    key: jlong,
) -> JString<'local> {
    match resolve_media(key) {
        Some(media) => optional_string_to_jstring(&mut env, media.url(), "nativeMediaUrl"),
        None => {
            throw_runtime_exception(&mut env, "nativeMediaUrl: invalid media handle key");
            JString::default()
        }
    }
}

/// `nativeMediaFile(long key) -> String` (Java `null` when not file-backed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaFile<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    key: jlong,
) -> JString<'local> {
    match resolve_media(key) {
        Some(media) => optional_string_to_jstring(&mut env, media.file(), "nativeMediaFile"),
        None => {
            throw_runtime_exception(&mut env, "nativeMediaFile: invalid media handle key");
            JString::default()
        }
    }
}

/// `nativeMediaBase64(long key) -> String` (never null; empty when unavailable).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaBase64<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    key: jlong,
) -> JString<'local> {
    match resolve_media(key) {
        Some(media) => {
            optional_string_to_jstring(&mut env, Some(media.base64()), "nativeMediaBase64")
        }
        None => {
            throw_runtime_exception(&mut env, "nativeMediaBase64: invalid media handle key");
            JString::default()
        }
    }
}

/// `nativeMediaMimeType(long key) -> String` (Java `null` when no mime is set).
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeMediaMimeType<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    key: jlong,
) -> JString<'local> {
    match resolve_media(key) {
        Some(media) => {
            optional_string_to_jstring(&mut env, media.mime_type(), "nativeMediaMimeType")
        }
        None => {
            throw_runtime_exception(&mut env, "nativeMediaMimeType: invalid media handle key");
            JString::default()
        }
    }
}

/// `nativeHandleClone(long key) -> long`. Mint a new owned key pointing at the
/// same underlying row (used to hand a fresh key to the engine on the inbound
/// wire so the Java object keeps its own). Throws on an invalid key.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeHandleClone(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: jlong,
) -> jlong {
    match HANDLE_TABLE.clone_handle(key as u64) {
        Some(new_key) => new_key as jlong,
        None => {
            throw_runtime_exception(
                &mut env,
                &format!("nativeHandleClone: invalid handle key {key}"),
            );
            0
        }
    }
}

/// `nativeHandleRelease(long key)`. Release one owned key. Best-effort: an
/// invalid/stale key (double release, JVM teardown race) is silently ignored,
/// matching `bridge_python`'s `BamlPyHandle::drop`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeHandleRelease(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: jlong,
) {
    let _ = HANDLE_TABLE.release(key as u64);
}

/// `nativeEnvSet(String name, String value)`. Mutate the **process**
/// environment so the in-process engine observes the change: the engine reads
/// env via Rust `std::env::var`, which is backed by the same process `environ`
/// this writes. This is the JVM analog of `bridge_python` relying on Python's
/// `os.environ[...] = ...` (which calls `setenv(3)`) being visible to the
/// pyo3-linked engine — JVM-side `System.getenv` caching / junit-pioneer patch
/// the JVM's cached view only, never native `getenv`. Used by the replay-harness
/// tests to point the env-driven `StreamStub` client at the local server.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeEnvSet<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    name: JString<'local>,
    value: JString<'local>,
) {
    let Some(name) = read_required_string(&mut env, &name, "nativeEnvSet") else {
        return;
    };
    let Some(value) = read_required_string(&mut env, &value, "nativeEnvSet") else {
        return;
    };
    // SAFETY: not thread-safe against concurrent getenv (why it is `unsafe` on
    // edition 2024), but a test-harness env mutation — the exact parity of
    // Python's `os.environ[...] = ...`. The replay tests serialize their env
    // set/unset around the call they drive.
    unsafe { std::env::set_var(name, value) };
}

/// `nativeEnvUnset(String name)`. Remove a process environment variable — the
/// teardown half of [`Java_baml_1bridge_BamlFfi_nativeEnvSet`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeEnvUnset<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    name: JString<'local>,
) {
    let Some(name) = read_required_string(&mut env, &name, "nativeEnvUnset") else {
        return;
    };
    // SAFETY: see `nativeEnvSet`.
    unsafe { std::env::remove_var(name) };
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

fn throw_runtime_exception_exact(env: &mut JNIEnv<'_>, message: &str) {
    let _ = env.throw_new("java/lang/RuntimeException", message);
}
