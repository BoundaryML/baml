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

use std::sync::{Arc, OnceLock};

use bex_project::{BexArgs, BexExternalAdt, CallId, MediaKind, MediaValue, RuntimeTy};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, kwargs_to_bex_values};
use indexmap::IndexMap;
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteArray, JClass, JString, JValue},
    sys::{jint, jlong},
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

/// `baml_bridge.BamlFfi.nativeCallAsync(long callId, String fqn, byte[] encodedCallFunctionArgs)`.
///
/// Non-blocking sibling of [`Java_baml_1bridge_BamlFfi_nativeCallSync`]: it
/// decodes/dispatches the identical `CallFunctionArgs` payload but spawns the
/// engine call on the shared tokio runtime and returns at once. `call_id` is
/// passed explicitly (it is also embedded in the encoded args) so the completion
/// route stays keyed even if the args fail to decode. Completion — ok value,
/// thrown error, panic, or a pre-call host-boundary failure — is delivered as
/// one `BamlOutboundResult` envelope to `completeCall(call_id, bytes)`. A
/// `RuntimeException` is thrown only for JNI-glue failures (bad UTF-8 fqn, array
/// read) that happen before the call is handed off.
#[unsafe(no_mangle)]
pub extern "system" fn Java_baml_1bridge_BamlFfi_nativeCallAsync<'local>(
    mut env: JNIEnv<'local>,
    class: JClass<'local>,
    call_id: jlong,
    fqn: JString<'local>,
    encoded_args: JByteArray<'local>,
) {
    if let Err(e) = ensure_completion_route(&mut env, &class) {
        throw_runtime_exception(
            &mut env,
            &format!("failed to wire async completion route: {e}"),
        );
        return;
    }

    let function_name: String = match env.get_string(&fqn) {
        Ok(js) => String::from(js),
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read fqn string: {e}"));
            return;
        }
    };

    let args_proto = match env.convert_byte_array(&encoded_args) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("failed to read args array: {e}"));
            return;
        }
    };

    spawn_async_call(call_id as u64, function_name, args_proto);
}

/// Spawn the engine call and route its `BamlOutboundResult` envelope back to
/// `completeCall`. Mirrors `bridge_cffi::lib_native::call_function_inner`, but
/// delivers over JNI instead of the C result callback. Pre-call host-boundary
/// failures (uninitialized runtime, malformed args, no tokio runtime) are
/// encoded into the same envelope and delivered immediately, so they decode +
/// raise identically to a sync pre-call failure.
fn spawn_async_call(call_id: u64, function_name: String, args_proto: Vec<u8>) {
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
        type_args,
    } = decoded;
    let call_ctx = bridge_cffi::function_call_context_builder(engine_call_id)
        .with_type_args(type_args)
        .build();

    rt.spawn(async move {
        // Inner task so a panic during result *encoding* is caught (via the
        // JoinError) and still delivered as an SdkPanic envelope, rather than
        // silently dropping the task and hanging the future. `call_and_encode`
        // already turns an engine-call panic into that envelope itself; this
        // guards the rarer encode-time panic, exactly as the C-ABI path does.
        let inner = tokio::spawn(bridge_cffi::call_and_encode(
            runtime,
            function_name,
            kwargs,
            call_ctx,
        ));
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

fn deliver_completion_inner(call_id: u64, bytes: &[u8]) -> Result<(), String> {
    let vm = JVM
        .get()
        .ok_or("JavaVM was not captured before completion")?;
    let class = BAML_FFI_CLASS
        .get()
        .ok_or("BamlFfi class ref was not captured before completion")?;

    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|e| format!("attach failed: {e}"))?;

    let payload = env
        .byte_array_from_slice(bytes)
        .map_err(|e| format!("result array alloc failed: {e}"))?;

    let outcome = env.call_static_method(
        class,
        COMPLETE_CALL_METHOD,
        COMPLETE_CALL_SIG,
        &[JValue::Long(call_id as jlong), JValue::from(&payload)],
    );

    if let Err(e) = outcome {
        // `completeCall` is written never to throw; clear any pending exception
        // so the (reused) daemon worker thread is left in a clean state.
        let _ = env.exception_clear();
        return Err(format!("completeCall invocation failed: {e}"));
    }
    Ok(())
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

/// Throw an unchecked `java.lang.RuntimeException`, ignoring a failure to
/// throw (nothing more can be done at that point). All messages are prefixed
/// so they are attributable to this bridge.
fn throw_runtime_exception(env: &mut JNIEnv<'_>, message: &str) {
    let _ = env.throw_new(
        "java/lang/RuntimeException",
        format!("bridge_java: {message}"),
    );
}
