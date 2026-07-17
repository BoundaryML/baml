//! Versioned function-table entry point for dynamically loaded host bridges.
//!
//! Hosts resolve only [`baml_get_api_v1`] with the platform loader. Every
//! callable used by that ABI version is then obtained from this table, which
//! makes completeness and compatibility validation explicit.

use bex_project::HostReleaseFn;

use crate::{
    BamlBridgeInfoV1, Buffer,
    ffi::{callbacks::CallbackFn, handle::BamlCffiStatus, host_value::HostDispatchFn},
};

/// First version of the shared BAML C API.
///
/// Fields may only be appended. Existing fields must retain their order,
/// signatures, and semantics for the lifetime of ABI version 1.
#[repr(C)]
pub struct BamlApiV1 {
    /// ABI version represented by this table. Always `1` for this type.
    pub abi_version: u32,
    /// Size of the table in bytes, allowing hosts to reject truncated tables.
    pub struct_size: usize,
    /// Return the canonical BAML product version.
    pub version: extern "C" fn() -> Buffer,
    /// Replace the process-wide runtime with a serialized BAML program.
    pub initialize_runtime_from_bytecode:
        extern "C" fn(bytecode: *const u8, length: usize) -> Buffer,
    /// Release a buffer allocated by the runtime.
    pub free_buffer: extern "C" fn(buffer: Buffer),
    /// Register the host callback that receives completed calls.
    pub register_callback: extern "C" fn(callback: CallbackFn),
    /// Begin a BAML function call.
    pub call_function: extern "C" fn(
        function_name: *const libc::c_char,
        encoded_args: *const u8,
        length: usize,
        callback_id: u32,
    ),
    /// Allocate a process-unique function-call identifier.
    pub new_function_call: extern "C" fn() -> u64,
    /// Cancel a function call. Zero means success.
    pub cancel_function_call: extern "C" fn(id: u64) -> i32,
    /// Register the host callback used when BAML invokes a host value.
    pub register_host_dispatch_callback: extern "C" fn(callback: HostDispatchFn),
    /// Register the callback used to release host-language values.
    pub register_host_release_callback: extern "C" fn(callback: HostReleaseFn),
    /// Complete one host-value invocation.
    pub complete_host_call:
        extern "C" fn(call_id: u32, is_error: i32, content: *const i8, length: usize),
    /// Clone an owned CFFI handle.
    pub handle_clone: unsafe extern "C" fn(key: u64, out_key: *mut u64) -> BamlCffiStatus,
    /// Release an owned CFFI handle.
    pub handle_release: unsafe extern "C" fn(key: u64) -> BamlCffiStatus,
    /// Construct a media handle backed by a URL.
    pub media_from_url: unsafe extern "C" fn(
        media_kind: i32,
        url: *const libc::c_char,
        mime_type_or_null: *const libc::c_char,
        out_key: *mut u64,
        out_handle_type: *mut i32,
    ) -> BamlCffiStatus,
    /// Construct a media handle backed by a local file.
    pub media_from_file: unsafe extern "C" fn(
        media_kind: i32,
        path: *const libc::c_char,
        mime_type_or_null: *const libc::c_char,
        out_key: *mut u64,
        out_handle_type: *mut i32,
    ) -> BamlCffiStatus,
    /// Construct a media handle backed by base64 data.
    pub media_from_base64: unsafe extern "C" fn(
        media_kind: i32,
        base64: *const libc::c_char,
        mime_type_or_null: *const libc::c_char,
        out_key: *mut u64,
        out_handle_type: *mut i32,
    ) -> BamlCffiStatus,
    /// Read the URL of a media handle.
    pub media_url:
        unsafe extern "C" fn(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus,
    /// Read the local path of a media handle.
    pub media_file:
        unsafe extern "C" fn(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus,
    /// Read the base64 contents of a media handle.
    pub media_base64:
        unsafe extern "C" fn(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus,
    /// Read the MIME type of a media handle.
    pub media_mime_type:
        unsafe extern "C" fn(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus,
    /// Register the calling bridge and require an exact release-version match.
    ///
    /// An empty buffer means compatible. A non-empty buffer is an owned UTF-8
    /// diagnostic that must be released with [`crate::free_buffer`].
    pub register_bridge: unsafe extern "C" fn(info: *const BamlBridgeInfoV1) -> Buffer,
    /// Flush the process event sink before a host exits or completes a test.
    pub flush_events: extern "C" fn(),
}

static BAML_API_V1: BamlApiV1 = BamlApiV1 {
    abi_version: 1,
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
    flush_events: crate::flush_events,
};

/// Return the immutable version-1 BAML C API function table.
///
/// This is the only symbol a manually loaded host bridge needs to resolve.
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
        assert_eq!(api.abi_version, 1);
        assert_eq!(api.struct_size, std::mem::size_of::<BamlApiV1>());
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
        assert_same_function!(api.flush_events, crate::flush_events);
    }
}
