//! Runtime management FFI functions.

use std::{collections::HashMap, ffi::CStr, panic::AssertUnwindSafe};

use super::super::panic::ffi_safe_ptr;
use crate::{
    BridgeInfo, BridgeLanguage, Buffer, initialize_runtime,
    initialize_runtime_from_bytecode as initialize_runtime_from_bytecode_impl, register_bridge,
};

/// Returns the BAML version as a Buffer containing raw UTF-8 bytes.
/// Caller must free with free_buffer().
#[unsafe(no_mangle)]
pub extern "C" fn version() -> Buffer {
    Buffer::from(baml_version::CANONICAL_VERSION.as_bytes().to_vec())
}

/// Version-1 C representation of bridge registration metadata.
///
/// Fields may only be appended. Existing fields must retain their order,
/// types, and semantics for the lifetime of ABI version 1. The `language`
/// field is a raw `uint32_t` at the C boundary and is validated before it is
/// interpreted as a `BamlBridgeLanguage` value. Consumers set `struct_size` to
/// the size they provide. Each string pointer is borrowed for its corresponding
/// length only during `register_bridge`; all strings are copied before that
/// function returns. A zero length permits a null pointer. `sdk_version` is
/// UTF-8 and identifies the canonical BAML toolchain required by the bridge,
/// independently of the table's ABI version.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BamlBridgeInfoV1 {
    pub struct_size: usize,
    pub language: u32,
    pub sdk_version: *const u8,
    pub sdk_version_len: usize,
    pub bridge_runtime_name: *const u8,
    pub bridge_runtime_name_len: usize,
    pub bridge_runtime_version: *const u8,
    pub bridge_runtime_version_len: usize,
}

/// Register the calling bridge and check SDK/runtime compatibility.
///
/// An empty buffer means success. Otherwise the returned buffer contains a
/// shared UTF-8 diagnostic and must be released with [`crate::free_buffer`].
///
/// # Safety
///
/// `info` must be null or point to an aligned C object whose readable extent
/// is at least the value of its `struct_size` field. Any non-null
/// string pointer must be readable for its corresponding length.
pub unsafe extern "C" fn register_bridge_ffi(info: *const BamlBridgeInfoV1) -> Buffer {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if info.is_null() {
            return Err("BAML bridge registration pointer is null".to_string());
        }
        // Read only the leading size field before trusting that the caller's
        // allocation is large enough for the rest of the version-1 struct.
        let struct_size = unsafe { std::ptr::addr_of!((*info).struct_size).read() };
        let legacy_size = std::mem::offset_of!(BamlBridgeInfoV1, bridge_runtime_name);
        if struct_size < legacy_size {
            return Err(format!(
                "truncated BAML bridge registration: got {} bytes, need {}",
                struct_size, legacy_size,
            ));
        }
        let language =
            BridgeLanguage::try_from(unsafe { std::ptr::addr_of!((*info).language).read() })?;
        let sdk_version = unsafe {
            read_borrowed_utf8(
                std::ptr::addr_of!((*info).sdk_version).read(),
                std::ptr::addr_of!((*info).sdk_version_len).read(),
                "BAML SDK version",
            )
        }?;
        let (bridge_runtime_name, bridge_runtime_version) =
            if struct_size >= std::mem::size_of::<BamlBridgeInfoV1>() {
                let name = unsafe {
                    read_borrowed_utf8(
                        std::ptr::addr_of!((*info).bridge_runtime_name).read(),
                        std::ptr::addr_of!((*info).bridge_runtime_name_len).read(),
                        "BAML bridge runtime name",
                    )
                }?;
                let version = unsafe {
                    read_borrowed_utf8(
                        std::ptr::addr_of!((*info).bridge_runtime_version).read(),
                        std::ptr::addr_of!((*info).bridge_runtime_version_len).read(),
                        "BAML bridge runtime version",
                    )
                }?;
                (name.to_string(), version.to_string())
            } else if struct_size == legacy_size {
                (
                    language.legacy_runtime_name().to_string(),
                    sdk_version.to_string(),
                )
            } else {
                return Err(format!(
                    "truncated appended BAML bridge registration: got {} bytes, need {}",
                    struct_size,
                    std::mem::size_of::<BamlBridgeInfoV1>(),
                ));
            };
        register_bridge(BridgeInfo {
            language,
            bridge_runtime_name,
            bridge_runtime_version,
            toolchain_version: sdk_version.to_string(),
        })
        .map(|_| ())
    }));

    match result {
        Ok(Ok(())) => Buffer::from(Vec::new()),
        Ok(Err(error)) => Buffer::from(error.into_bytes()),
        Err(_) => Buffer::from(b"panic while registering BAML bridge".to_vec()),
    }
}

unsafe fn read_borrowed_utf8<'a>(
    pointer: *const u8,
    length: usize,
    field: &str,
) -> Result<&'a str, String> {
    if pointer.is_null() && length != 0 {
        return Err(format!("{field} pointer is null but length is nonzero"));
    }
    if length > isize::MAX as usize {
        return Err(format!("{field} length {length} exceeds isize::MAX"));
    }
    let bytes = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(pointer, length) }
    };
    std::str::from_utf8(bytes).map_err(|error| format!("{field} is not valid UTF-8: {error}"))
}

/// Create/initialize the BAML runtime (global BexEngine).
///
/// # Arguments
/// * `root_path` - Root path for BAML files (C string)
/// * `src_files_json` - JSON-encoded HashMap<String, String> of file contents
///
/// # Returns
/// Non-null pointer on success (value is opaque, not used), null on failure.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn create_baml_runtime(
    root_path: *const libc::c_char,
    src_files_json: *const libc::c_char,
) -> *const libc::c_void {
    ffi_safe_ptr(|| -> Result<*const libc::c_void, String> {
        // Parse root_path
        let root_path_str = unsafe {
            CStr::from_ptr(root_path)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in root_path: {e}"))?
        };

        // Parse src_files JSON
        let src_files_str = unsafe {
            CStr::from_ptr(src_files_json)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in src_files_json: {e}"))?
        };
        let src_files: HashMap<String, String> = serde_json::from_str(src_files_str)
            .map_err(|e| format!("Failed to parse src_files JSON: {e}"))?;

        // Initialize global runtime
        initialize_runtime(root_path_str, src_files)
            .map_err(|e| format!("Failed to initialize runtime: {e}"))?;

        // Return non-null pointer to indicate success
        // The actual value doesn't matter since we use global engine
        Ok(std::ptr::dangling::<libc::c_void>())
    })
}

/// Initialize the process-global BAML runtime from serialized bytecode.
///
/// An empty returned buffer means success. On failure, the buffer contains a
/// UTF-8 error message. The caller owns every returned buffer and must release
/// it with [`crate::free_buffer`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn initialize_runtime_from_bytecode(bytecode: *const u8, length: usize) -> Buffer {
    if bytecode.is_null() && length != 0 {
        return Buffer::from(b"bytecode pointer is null but length is nonzero".to_vec());
    }
    if let Some(error) = bytecode_preflight_failure(length) {
        return error;
    }
    initialize_runtime_from_bytecode_inner(bytecode, length, None)
}

/// Initialize generated bytecode after validating its embedded `baml.toml`.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn initialize_runtime_from_bytecode_with_metadata(
    bytecode: *const u8,
    length: usize,
    baml_toml: *const libc::c_char,
) -> Buffer {
    if bytecode.is_null() && length != 0 {
        return Buffer::from(b"bytecode pointer is null but length is nonzero".to_vec());
    }
    if let Some(error) = bytecode_preflight_failure(length) {
        return error;
    }
    let manifest = if baml_toml.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(baml_toml) }.to_str() {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                return Buffer::from(
                    format!(
                        "BAML startup failed: generation metadata is invalid.\n\nThe embedded `baml.toml` is not valid UTF-8: {error}"
                    )
                    .into_bytes(),
                );
            }
        }
    };
    initialize_runtime_from_bytecode_inner(bytecode, length, manifest)
}

fn bytecode_preflight_failure(length: usize) -> Option<Buffer> {
    crate::validate_bytecode_startup_preconditions(length == 0)
        .err()
        .map(|error| Buffer::from(error.to_string().into_bytes()))
}

fn initialize_runtime_from_bytecode_inner(
    bytecode: *const u8,
    length: usize,
    embedded_baml_toml: Option<&str>,
) -> Buffer {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if bytecode.is_null() && length != 0 {
            return Err("bytecode pointer is null but length is nonzero".to_string());
        }

        let bytecode = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytecode, length) }
        };
        initialize_runtime_from_bytecode_impl(bytecode, embedded_baml_toml)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }));

    match result {
        Ok(Ok(())) => Buffer::from(Vec::new()),
        Ok(Err(error)) => Buffer::from(error.into_bytes()),
        Err(panic_info) => {
            let message = panic_info
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| panic_info.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Buffer::from(format!("panic while initializing BAML bytecode: {message}").into_bytes())
        }
    }
}

/// Destroy the process-wide BAML runtime after its spawned work settles.
#[unsafe(no_mangle)]
pub extern "C" fn destroy_baml_runtime(_runtime: *const libc::c_void) {
    crate::free_buffer(shutdown_runtime());
}

#[unsafe(no_mangle)]
pub extern "C" fn shutdown_runtime() -> Buffer {
    let result = std::panic::catch_unwind(shutdown_runtime_on_fresh_thread);
    match result {
        Ok(Ok(())) => Buffer::from(Vec::new()),
        Ok(Err(error)) => Buffer::from(error.into_bytes()),
        Err(_) => Buffer::from(b"panic while shutting down BAML runtime".to_vec()),
    }
}

fn shutdown_runtime_on_fresh_thread() -> Result<(), String> {
    // Process-exit hooks can run after the caller thread's Tokio context has
    // been destroyed. A fresh thread has valid thread-local state for block_on.
    std::thread::spawn(|| {
        crate::get_tokio_runtime()
            .map_err(|error| error.to_string())?
            .block_on(crate::shutdown_runtime())
            .map_err(|error| error.to_string())
    })
    .join()
    .map_err(|_| "panic while shutting down BAML runtime".to_string())?
}

/// Invoke the BAML CLI.
/// Currently returns 1 (error) as CLI is not implemented for bridge.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn invoke_runtime_cli(_args: *const *const libc::c_char) -> libc::c_int {
    // TODO: Implement CLI invocation if needed
    eprintln!("invoke_runtime_cli not implemented in bridge_cffi");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_version_is_compatible() {
        assert!(crate::ensure_version_compatible(baml_version::CANONICAL_VERSION).is_ok());
    }

    #[test]
    fn any_nonidentical_version_is_incompatible() {
        let error = crate::ensure_version_compatible("999.0.0").unwrap_err();
        assert!(error.contains("requires toolchain 999.0.0"));
        assert!(error.contains(baml_version::CANONICAL_VERSION));
    }

    #[test]
    fn bridge_language_selects_one_process_wide_inbound_policy() {
        use bex_project::InboundUnionAmbiguityPolicy::{Reject, SelectDefault};

        assert_eq!(
            BridgeLanguage::NodeJs.inbound_union_ambiguity_policy(),
            SelectDefault
        );
        assert_eq!(
            BridgeLanguage::Python.inbound_union_ambiguity_policy(),
            SelectDefault
        );
        for language in [
            BridgeLanguage::Go,
            BridgeLanguage::Rust,
            BridgeLanguage::CSharp,
            BridgeLanguage::Cpp,
        ] {
            assert_eq!(language.inbound_union_ambiguity_policy(), Reject);
        }
    }

    fn ffi_info(language: u32, version: &[u8]) -> BamlBridgeInfoV1 {
        BamlBridgeInfoV1 {
            struct_size: std::mem::size_of::<BamlBridgeInfoV1>(),
            language,
            sdk_version: version.as_ptr(),
            sdk_version_len: version.len(),
            bridge_runtime_name: b"test-bridge".as_ptr(),
            bridge_runtime_name_len: b"test-bridge".len(),
            bridge_runtime_version: version.as_ptr(),
            bridge_runtime_version_len: version.len(),
        }
    }

    #[test]
    fn ffi_registration_rejects_unknown_language_without_undefined_behavior() {
        let expected = baml_version::CANONICAL_VERSION.as_bytes();
        let info = ffi_info(u32::MAX, expected);
        let buffer = unsafe { register_bridge_ffi(&info) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        let message = std::str::from_utf8(bytes).unwrap();
        assert_eq!(message, "unknown BAML bridge language ID 4294967295");
        crate::free_buffer(buffer);
    }

    #[test]
    fn ffi_registration_rejects_truncated_metadata() {
        let info = BamlBridgeInfoV1 {
            struct_size: std::mem::size_of::<usize>(),
            language: BridgeLanguage::Rust as u32,
            sdk_version: std::ptr::null(),
            sdk_version_len: 0,
            bridge_runtime_name: std::ptr::null(),
            bridge_runtime_name_len: 0,
            bridge_runtime_version: std::ptr::null(),
            bridge_runtime_version_len: 0,
        };
        let buffer = unsafe { register_bridge_ffi(&info) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        let message = std::str::from_utf8(bytes).unwrap();
        assert!(message.starts_with("truncated BAML bridge registration:"));
        crate::free_buffer(buffer);
    }

    #[test]
    fn ffi_registration_rejects_impossible_version_length() {
        let mut info = ffi_info(
            BridgeLanguage::Rust as u32,
            baml_version::CANONICAL_VERSION.as_bytes(),
        );
        info.sdk_version = std::ptr::dangling();
        info.sdk_version_len = usize::MAX;
        let buffer = unsafe { register_bridge_ffi(&info) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        let message = std::str::from_utf8(bytes).unwrap();
        assert!(message.contains("exceeds isize::MAX"));
        crate::free_buffer(buffer);
    }

    #[test]
    fn bytecode_initializer_rejects_null_pointer_with_nonzero_length() {
        let buffer = initialize_runtime_from_bytecode(std::ptr::null(), 1);
        let message = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        assert_eq!(
            std::str::from_utf8(message).unwrap(),
            "bytecode pointer is null but length is nonzero"
        );
        crate::free_buffer(buffer);
    }
}
