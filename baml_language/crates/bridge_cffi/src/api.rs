//! Versioned public C ABI for dynamically loaded host bridges.
//!
//! This module is the source consumed by cbindgen to produce
//! `include/baml_cffi.h`. Keep public C-facing declarations here so the Rust
//! implementation and the checked-in header cannot evolve independently.

use super::{BamlBridgeInfoV1, ffi::handle::BamlCffiStatus};
use crate::Buffer;

/// ABI revision represented by `BamlApiV1`.
///
/// This identifies the function-table contract, not the BAML product release.
///
/// Revision 2 changes the `call_function` slot from the legacy four-argument
/// name-plus-payload signature to the unified three-argument payload signature.
/// Hosts and runtimes built against different revisions must reject one another
/// before reading that slot.
pub const BAML_API_V1_ABI_VERSION: u32 = 2;

/// Valid `media_kind` values for media constructors.
///
/// These values are shared with the V1 protobuf `MediaTypeEnum` wire contract.
/// Zero is not a constructible media kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum BamlCffiMediaKind {
    Unspecified = 0,
    Image = 1,
    Audio = 2,
    Pdf = 3,
    Video = 4,
    Generic = 5,
}

/// Handle-type values returned by media constructors and carried on the wire.
///
/// Values 3 and 4 are permanently reserved. Host-value handle keys use their
/// own registry and release callback rather than `handle_release`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum BamlCffiHandleType {
    Unspecified = 0,
    UntaggedRustData = 1,
    UntaggedBexHeap = 2,
    FunctionRef = 5,
    MediaImage = 6,
    MediaAudio = 7,
    MediaVideo = 8,
    MediaPdf = 9,
    MediaGeneric = 10,
    PromptAst = 11,
    Collector = 12,
    Type = 13,
    TaggedHeapHandle = 14,
    HostValueCallable = 15,
    HostValueOpaque = 16,
    FunctionSpec = 17,
    RuntimeValue = 18,
}

/// Receives the completed result of `call_function`.
///
/// `content` points to a protobuf-encoded `BamlOutboundResult`. It is borrowed,
/// read-only, and valid only until the callback returns. A zero length permits
/// a null pointer. Copy any bytes that must outlive the callback. Calls for
/// different IDs may arrive concurrently, and the callback must not unwind or
/// throw across the C boundary.
pub type BamlResultCallback = extern "C" fn(call_id: u32, content: *const i8, length: usize);

/// Receives a BAML invocation of a host-owned value.
///
/// `args` points to a protobuf-encoded argument payload. It is borrowed,
/// read-only, and valid only until the callback returns. A zero length permits
/// a null pointer. The callback must copy retained bytes, return promptly, and
/// eventually call `complete_host_call` exactly once unless the BAML call is
/// cancelled. Dispatches may be concurrent. Synchronous, blocking re-entry
/// into BAML is unsupported. The callback must not unwind or throw across the
/// C boundary.
pub type BamlHostDispatchCallback =
    extern "C" fn(host_value_key: u64, call_id: u32, args: *const u8, length: usize);

/// Notifies the host that a host-owned value key can be released.
///
/// Notifications are deferred until an engine safepoint, may occur on any
/// runtime thread, and may be concurrent. The callback must not unwind or
/// throw across the C boundary.
pub type BamlHostReleaseCallback = extern "C" fn(host_value_key: u64);

/// Receives an error from spawned work whose future became unreachable.
///
/// `content` follows the same borrowed `BamlOutboundResult` contract as
/// `BamlResultCallback`. `cancelled` is nonzero when shutdown cancellation
/// caused the error. The callback must not unwind or throw across the C
/// boundary.
pub type BamlUnhandledSpawnErrorCallback =
    extern "C" fn(content: *const i8, length: usize, cancelled: i32);

pub type BamlVersionFn = extern "C" fn() -> Buffer;
pub type BamlInitializeRuntimeFromBytecodeFn =
    extern "C" fn(bytecode: *const u8, length: usize) -> Buffer;
pub type BamlInitializeRuntimeFromBytecodeWithMetadataFn =
    extern "C" fn(bytecode: *const u8, length: usize, baml_toml: *const libc::c_char) -> Buffer;
pub type BamlFreeBufferFn = extern "C" fn(buffer: Buffer);
pub type BamlRegisterCallbackFn = extern "C" fn(callback: BamlResultCallback);
pub type BamlCallFunctionFn =
    extern "C" fn(encoded_args: *const u8, length: usize, callback_id: u32);
pub type BamlNewFunctionCallFn = extern "C" fn() -> u64;
pub type BamlCancelFunctionCallFn = extern "C" fn(id: u64) -> i32;
pub type BamlRegisterHostDispatchCallbackFn = extern "C" fn(callback: BamlHostDispatchCallback);
pub type BamlRegisterHostReleaseCallbackFn = extern "C" fn(callback: BamlHostReleaseCallback);
pub type BamlRegisterUnhandledSpawnErrorCallbackFn =
    extern "C" fn(callback: BamlUnhandledSpawnErrorCallback);
pub type BamlShutdownRuntimeFn = extern "C" fn() -> Buffer;
pub type BamlCompleteHostCallFn =
    extern "C" fn(call_id: u32, is_error: i32, content: *const i8, length: usize);
pub type BamlHandleCloneFn = unsafe extern "C" fn(key: u64, out_key: *mut u64) -> BamlCffiStatus;
pub type BamlHandleReleaseFn = unsafe extern "C" fn(key: u64) -> BamlCffiStatus;
/// Construct media using a raw `BamlCffiMediaKind` value.
pub type BamlMediaConstructorFn = unsafe extern "C" fn(
    media_kind: i32,
    value: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus;
/// Read media using a raw `BamlCffiHandleType` value; zero accepts any type.
pub type BamlMediaAccessorFn =
    unsafe extern "C" fn(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus;
pub type BamlRegisterBridgeFn = unsafe extern "C" fn(info: *const BamlBridgeInfoV1) -> Buffer;
pub type BamlRegisterProgramFn = unsafe extern "C" fn(
    key: u64,
    bytecode: *const u8,
    length: usize,
    metadata: *const libc::c_char,
) -> Buffer;
pub type BamlCreateRuntimeFn =
    unsafe extern "C" fn(bytecode: *const u8, length: usize, out_key: *mut u64) -> Buffer;
pub type BamlUnregisterRuntimeFn = extern "C" fn(key: u64) -> Buffer;
pub type BamlCallFunctionForRuntimeFn =
    extern "C" fn(key: u64, encoded_args: *const u8, length: usize, callback_id: u32);
pub type BamlGetApiV1Fn = extern "C" fn() -> *const BamlApiV1;

/// First version of the shared BAML C API.
///
/// The table is immutable runtime-owned storage and remains valid until the
/// native library is unloaded. All function pointers are required and use the
/// C calling convention. The functions are safe to invoke from multiple
/// threads subject to the per-function ownership rules below; callbacks may be
/// invoked concurrently.
///
/// A host must not unload the native library while a returned buffer, owned
/// handle, registered callback, or asynchronous call can still reach it. The
/// The ABI has no callback-unregistration operation in V1. Hosts must call
/// `shutdown_runtime` before unloading the native library.
///
/// No Rust panic may unwind across this ABI. Operations with a diagnostic or
/// callback result channel catch and translate expected implementation panics.
/// An otherwise unexpected panic aborts the process rather than crossing into
/// foreign frames. Likewise, host callbacks must not unwind or throw into Rust.
///
/// V1 is append-only: existing fields may never be reordered, removed, or
/// change type or semantics. New fields may only be appended. Before reading a
/// field, consumers must verify that `struct_size` reaches the end of that
/// field. `baml_api_v1_is_compatible` performs the check for the original V1
/// prefix. A larger unknown size is compatible; a truncated prefix is not.
#[repr(C)]
pub struct BamlApiV1 {
    /// Function-table ABI version. Always `BAML_API_V1_ABI_VERSION`.
    pub abi_version: u32,
    /// Actual byte size of this runtime's table.
    pub struct_size: usize,
    /// Return the canonical BAML product version as owned UTF-8 bytes.
    /// Release the result exactly once with this table's `free_buffer`.
    pub version: BamlVersionFn,
    /// Replace the process-wide runtime from borrowed serialized bytecode.
    ///
    /// The input is read only during the call; zero length permits null. The
    /// returned owned buffer is empty on success or a UTF-8 diagnostic on
    /// failure and must always be passed once to `free_buffer`. Concurrent
    /// calls are serialized only while replacing the global runtime; calls
    /// already in progress retain their previous runtime instance.
    pub initialize_runtime_from_bytecode: BamlInitializeRuntimeFromBytecodeFn,
    /// Release exactly one runtime-owned buffer returned through this table.
    ///
    /// Do not use a different library instance's function, release a copied
    /// buffer twice, or pass host-allocated memory. A zero-length buffer may
    /// have either a null or non-null pointer and must still be released.
    pub free_buffer: BamlFreeBufferFn,
    /// Install the process-global result callback; the first call wins.
    ///
    /// Later registrations are ignored. The function pointer must remain valid
    /// until the native library is unloaded.
    pub register_callback: BamlRegisterCallbackFn,
    /// Enqueue a BAML function call and return immediately.
    ///
    /// `encoded_args` is a borrowed protobuf-encoded `CallFunctionArgs` whose
    /// `call_target` selects either a function name or an owned function handle.
    /// It is borrowed for `length` bytes and need remain valid only for this
    /// call. Completion is delivered later to the registered result callback
    /// using `callback_id`.
    pub call_function: BamlCallFunctionFn,
    /// Allocate a nonzero process-unique BAML function-call identifier.
    /// Dispatch it once or release it with `release_function_call` if abandoned.
    pub new_function_call: BamlNewFunctionCallFn,
    /// Request cancellation of a BAML function call.
    ///
    /// Returns 0 on success and 1 when the ID is zero, unknown, completed, or
    /// the runtime is unavailable.
    pub cancel_function_call: BamlCancelFunctionCallFn,
    /// Install the process-global host-dispatch callback; the first call wins.
    /// Later registrations are ignored. The pointer remains borrowed until
    /// the native library is unloaded.
    pub register_host_dispatch_callback: BamlRegisterHostDispatchCallbackFn,
    /// Install the process-global host-release callback; the first call wins.
    /// Later registrations are ignored after logging a diagnostic. The pointer
    /// remains borrowed until the native library is unloaded.
    pub register_host_release_callback: BamlRegisterHostReleaseCallbackFn,
    /// Complete one outstanding host-value call synchronously.
    ///
    /// `is_error` must be exactly 0 or 1. `content` is borrowed only for this
    /// call and may be null only when `length` is zero. The payload is a
    /// protobuf-encoded `InboundValue`; an empty success means BAML null, while
    /// an empty error is rejected as a bridge failure. Unknown or cancelled
    /// call IDs are ignored after a diagnostic.
    pub complete_host_call: BamlCompleteHostCallFn,
    /// Clone an owned engine handle into `out_key`.
    ///
    /// On `BAML_CFFI_STATUS_OK`, the host owns the new key and must release it
    /// exactly once with `handle_release`. `out_key` must be writable.
    pub handle_clone: BamlHandleCloneFn,
    /// Release one owned engine handle key. Host-value keys are instead
    /// released in response to the host-release callback.
    pub handle_release: BamlHandleReleaseFn,
    /// Create an owned media handle from a borrowed NUL-terminated URL.
    ///
    /// The optional MIME type may be null. On success, both output pointers are
    /// written and the returned key must be released with `handle_release`.
    pub media_from_url: BamlMediaConstructorFn,
    /// Create an owned media handle from a borrowed NUL-terminated file path.
    /// Ownership and output rules match `media_from_url`.
    pub media_from_file: BamlMediaConstructorFn,
    /// Create an owned media handle from borrowed NUL-terminated base64 data.
    /// Ownership and output rules match `media_from_url`.
    pub media_from_base64: BamlMediaConstructorFn,
    /// Read a media URL into `out`.
    ///
    /// On success, `out` receives an owned buffer. An absent optional value is
    /// represented by a zero-length buffer. Release every successful output
    /// once with `free_buffer`.
    pub media_url: BamlMediaAccessorFn,
    /// Read a media file path. Ownership rules match `media_url`.
    pub media_file: BamlMediaAccessorFn,
    /// Read media base64 data. Ownership rules match `media_url`.
    pub media_base64: BamlMediaAccessorFn,
    /// Read a media MIME type. Ownership rules match `media_url`.
    pub media_mime_type: BamlMediaAccessorFn,
    /// Register the calling bridge and require an exact toolchain-version match.
    ///
    /// `info` and its version bytes are borrowed only for this call. The
    /// returned owned buffer is empty on success or a UTF-8 diagnostic on
    /// failure and must always be released with `free_buffer`. Registration is
    /// process-global, thread-safe, first-successful-call-wins, and idempotent
    /// only for an identical language/version pair.
    pub register_bridge: BamlRegisterBridgeFn,
    /// Install the process-global unhandled-spawn callback; the first call wins.
    pub register_unhandled_spawn_error_callback: BamlRegisterUnhandledSpawnErrorCallbackFn,
    /// Wait for spawned work, report unreachable errors, and release the runtime.
    pub shutdown_runtime: BamlShutdownRuntimeFn,
    /// Replace the runtime from bytecode after validating embedded generation metadata.
    pub initialize_runtime_from_bytecode_with_metadata:
        BamlInitializeRuntimeFromBytecodeWithMetadataFn,
    /// Append-only keyed API. Generated keys are process-owned and idempotent for identical contents.
    pub register_program: BamlRegisterProgramFn,
    /// Allocate an independent dynamic registration; out_key is written only on success.
    pub create_runtime: BamlCreateRuntimeFn,
    /// Remove a dynamic registration. In-flight calls retain their engine.
    pub unregister_runtime: BamlUnregisterRuntimeFn,
    /// Route a call using all 64 bits of its originating registration key.
    pub call_function_for_runtime: BamlCallFunctionForRuntimeFn,
    /// Compute a generated identity from canonical program contents.
    pub program_key: BamlCreateRuntimeFn,
    /// Release a call ID abandoned before dispatch. Safe after completion.
    pub release_function_call: extern "C" fn(u64),
}

static BAML_API_V1: BamlApiV1 = BamlApiV1 {
    program_key: crate::program_key_ffi,
    release_function_call: crate::release_function_call,
    register_program: crate::register_program_ffi,
    create_runtime: crate::create_runtime_ffi,
    unregister_runtime: crate::unregister_runtime_ffi,
    call_function_for_runtime: crate::call_function_for_runtime,
    abi_version: BAML_API_V1_ABI_VERSION,
    struct_size: std::mem::size_of::<BamlApiV1>(),
    version: crate::version,
    initialize_runtime_from_bytecode: crate::initialize_runtime_from_bytecode_ffi,
    free_buffer: crate::free_buffer,
    register_callback: crate::register_callback,
    call_function: crate::call_function,
    new_function_call: crate::new_function_call,
    cancel_function_call: crate::cancel_function_call,
    register_host_dispatch_callback: crate::register_host_dispatch_callback,
    register_host_release_callback: crate::register_host_release_callback,
    complete_host_call: crate::complete_host_call,
    handle_clone: crate::baml_handle_clone,
    handle_release: crate::baml_handle_release,
    media_from_url: crate::baml_media_from_url,
    media_from_file: crate::baml_media_from_file,
    media_from_base64: crate::baml_media_from_base64,
    media_url: crate::baml_media_url,
    media_file: crate::baml_media_file,
    media_base64: crate::baml_media_base64,
    media_mime_type: crate::baml_media_mime_type,
    register_bridge: crate::register_bridge_ffi,
    register_unhandled_spawn_error_callback: crate::register_unhandled_spawn_error_callback,
    shutdown_runtime: crate::shutdown_runtime_ffi,
    initialize_runtime_from_bytecode_with_metadata:
        crate::initialize_runtime_from_bytecode_with_metadata,
};

/// Return the immutable version-1 BAML C API function table.
///
/// The returned pointer is non-null, owned by the runtime, and valid until the
/// dynamic library is unloaded. It must not be modified or freed. This is the
/// only symbol a dynamically loaded host bridge needs to resolve.
#[unsafe(no_mangle)]
pub extern "C" fn baml_get_api_v1() -> *const BamlApiV1 {
    &BAML_API_V1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_v1_reports_its_exact_layout() {
        let api = unsafe { &*baml_get_api_v1() };
        assert_eq!(api.abi_version, BAML_API_V1_ABI_VERSION);
        assert_eq!(api.struct_size, std::mem::size_of::<BamlApiV1>());
    }

    #[test]
    fn unified_call_target_uses_a_new_abi_revision() {
        assert_eq!(BAML_API_V1_ABI_VERSION, 2);
    }

    #[test]
    fn api_v1_contains_the_complete_production_cffi_surface() {
        macro_rules! assert_same_function {
            ($actual:expr, $expected:path) => {
                assert_eq!($actual as *const (), $expected as *const ());
            };
        }

        let api = unsafe { &*baml_get_api_v1() };
        assert_same_function!(api.version, crate::version);
        assert_same_function!(
            api.initialize_runtime_from_bytecode,
            crate::initialize_runtime_from_bytecode_ffi
        );
        assert_same_function!(api.free_buffer, crate::free_buffer);
        assert_same_function!(api.register_callback, crate::register_callback);
        assert_same_function!(api.call_function, crate::call_function);
        assert_same_function!(api.new_function_call, crate::new_function_call);
        assert_same_function!(api.cancel_function_call, crate::cancel_function_call);
        assert_same_function!(
            api.register_host_dispatch_callback,
            crate::register_host_dispatch_callback
        );
        assert_same_function!(
            api.register_host_release_callback,
            crate::register_host_release_callback
        );
        assert_same_function!(api.complete_host_call, crate::complete_host_call);
        assert_same_function!(api.handle_clone, crate::baml_handle_clone);
        assert_same_function!(api.handle_release, crate::baml_handle_release);
        assert_same_function!(api.media_from_url, crate::baml_media_from_url);
        assert_same_function!(api.media_from_file, crate::baml_media_from_file);
        assert_same_function!(api.media_from_base64, crate::baml_media_from_base64);
        assert_same_function!(api.media_url, crate::baml_media_url);
        assert_same_function!(api.media_file, crate::baml_media_file);
        assert_same_function!(api.media_base64, crate::baml_media_base64);
        assert_same_function!(api.media_mime_type, crate::baml_media_mime_type);
        assert_same_function!(api.register_bridge, crate::register_bridge_ffi);
        assert_same_function!(
            api.register_unhandled_spawn_error_callback,
            crate::register_unhandled_spawn_error_callback
        );
        assert_same_function!(api.shutdown_runtime, crate::shutdown_runtime_ffi);
        assert_same_function!(
            api.initialize_runtime_from_bytecode_with_metadata,
            crate::initialize_runtime_from_bytecode_with_metadata
        );
    }

    #[test]
    fn every_v1_field_retains_its_declared_function_type() {
        let api = unsafe { &*baml_get_api_v1() };
        let _: BamlVersionFn = api.version;
        let _: BamlInitializeRuntimeFromBytecodeFn = api.initialize_runtime_from_bytecode;
        let _: BamlFreeBufferFn = api.free_buffer;
        let _: BamlRegisterCallbackFn = api.register_callback;
        let _: BamlCallFunctionFn = api.call_function;
        let _: BamlNewFunctionCallFn = api.new_function_call;
        let _: BamlCancelFunctionCallFn = api.cancel_function_call;
        let _: BamlRegisterHostDispatchCallbackFn = api.register_host_dispatch_callback;
        let _: BamlRegisterHostReleaseCallbackFn = api.register_host_release_callback;
        let _: BamlCompleteHostCallFn = api.complete_host_call;
        let _: BamlHandleCloneFn = api.handle_clone;
        let _: BamlHandleReleaseFn = api.handle_release;
        let _: BamlMediaConstructorFn = api.media_from_url;
        let _: BamlMediaConstructorFn = api.media_from_file;
        let _: BamlMediaConstructorFn = api.media_from_base64;
        let _: BamlMediaAccessorFn = api.media_url;
        let _: BamlMediaAccessorFn = api.media_file;
        let _: BamlMediaAccessorFn = api.media_base64;
        let _: BamlMediaAccessorFn = api.media_mime_type;
        let _: BamlRegisterBridgeFn = api.register_bridge;
        let _: BamlRegisterUnhandledSpawnErrorCallbackFn =
            api.register_unhandled_spawn_error_callback;
        let _: BamlShutdownRuntimeFn = api.shutdown_runtime;
        let _: BamlInitializeRuntimeFromBytecodeWithMetadataFn =
            api.initialize_runtime_from_bytecode_with_metadata;
        let _: BamlGetApiV1Fn = baml_get_api_v1;
    }

    #[test]
    fn public_media_and_handle_discriminants_match_the_wire_contract() {
        use bridge_ctypes::baml_bridge::cffi::{BamlHandleType, MediaTypeEnum};

        let media_pairs = [
            (
                BamlCffiMediaKind::Unspecified,
                MediaTypeEnum::MediaTypeUnspecified,
            ),
            (BamlCffiMediaKind::Image, MediaTypeEnum::Image),
            (BamlCffiMediaKind::Audio, MediaTypeEnum::Audio),
            (BamlCffiMediaKind::Pdf, MediaTypeEnum::Pdf),
            (BamlCffiMediaKind::Video, MediaTypeEnum::Video),
            (BamlCffiMediaKind::Generic, MediaTypeEnum::Other),
        ];
        for (public, wire) in media_pairs {
            assert_eq!(public as i32, wire as i32);
        }

        let handle_pairs = [
            (
                BamlCffiHandleType::Unspecified,
                BamlHandleType::HandleUnspecified,
            ),
            (
                BamlCffiHandleType::UntaggedRustData,
                BamlHandleType::UntaggedRustData,
            ),
            (
                BamlCffiHandleType::UntaggedBexHeap,
                BamlHandleType::UntaggedBexHeap,
            ),
            (BamlCffiHandleType::FunctionRef, BamlHandleType::FunctionRef),
            (
                BamlCffiHandleType::MediaImage,
                BamlHandleType::AdtMediaImage,
            ),
            (
                BamlCffiHandleType::MediaAudio,
                BamlHandleType::AdtMediaAudio,
            ),
            (
                BamlCffiHandleType::MediaVideo,
                BamlHandleType::AdtMediaVideo,
            ),
            (BamlCffiHandleType::MediaPdf, BamlHandleType::AdtMediaPdf),
            (
                BamlCffiHandleType::MediaGeneric,
                BamlHandleType::AdtMediaGeneric,
            ),
            (BamlCffiHandleType::PromptAst, BamlHandleType::AdtPromptAst),
            (BamlCffiHandleType::Collector, BamlHandleType::AdtCollector),
            (BamlCffiHandleType::Type, BamlHandleType::AdtType),
            (
                BamlCffiHandleType::TaggedHeapHandle,
                BamlHandleType::AdtTaggedHeapHandle,
            ),
            (
                BamlCffiHandleType::HostValueCallable,
                BamlHandleType::HostValueCallable,
            ),
            (
                BamlCffiHandleType::HostValueOpaque,
                BamlHandleType::HostValueOpaque,
            ),
            (
                BamlCffiHandleType::FunctionSpec,
                BamlHandleType::AdtFunctionSpec,
            ),
            (
                BamlCffiHandleType::RuntimeValue,
                BamlHandleType::AdtRuntimeValue,
            ),
        ];
        for (public, wire) in handle_pairs {
            assert_eq!(public as i32, wire as i32);
        }
    }
}
