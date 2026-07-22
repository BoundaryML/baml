//! Runtime management FFI functions.

use std::{collections::HashMap, ffi::CStr, panic::AssertUnwindSafe, sync::OnceLock};

use super::super::panic::ffi_safe_ptr;
use crate::{
    Buffer, initialize_runtime,
    initialize_runtime_from_bytecode as initialize_runtime_from_bytecode_impl,
};

/// Returns the BAML version as a Buffer containing raw UTF-8 bytes.
/// Caller must free with free_buffer().
#[unsafe(no_mangle)]
pub extern "C" fn version() -> Buffer {
    Buffer::from(baml_version::CANONICAL_VERSION.as_bytes().to_vec())
}

/// Require an exact match between a host SDK and this native runtime.
///
/// BAML releases are kept in lockstep while the public bridge contracts are
/// evolving. Compatibility is therefore deliberately stricter than SemVer:
/// the SDK and runtime must report the same canonical release version.
pub fn ensure_version_compatible(expected_version: &str) -> Result<(), String> {
    let actual_version = baml_version::CANONICAL_VERSION;
    if expected_version == actual_version {
        Ok(())
    } else {
        Err(format!(
            "BAML runtime version mismatch: SDK requires {expected_version}, but library reports {actual_version}"
        ))
    }
}

/// Stable identity of an official host-language bridge.
///
/// Discriminants are part of the C ABI and may never be reused. The C-facing
/// registration struct stores the raw value as a `u32`, so an unknown value
/// can be rejected without constructing an invalid Rust enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BridgeLanguage {
    NodeJs = 1,
    Python = 2,
    Go = 3,
    Rust = 4,
    CSharp = 5,
    Cpp = 6,
    Java = 7,
    Swift = 8,
}

impl BridgeLanguage {
    pub const fn telemetry_name(self) -> &'static str {
        match self {
            Self::NodeJs => "nodejs",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::CSharp => "csharp",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Swift => "swift",
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::NodeJs => "Node.js",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Rust => "Rust",
            Self::CSharp => "C#",
            Self::Cpp => "C++",
            Self::Java => "Java",
            Self::Swift => "Swift",
        }
    }
}

impl TryFrom<u32> for BridgeLanguage {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::NodeJs),
            2 => Ok(Self::Python),
            3 => Ok(Self::Go),
            4 => Ok(Self::Rust),
            5 => Ok(Self::CSharp),
            6 => Ok(Self::Cpp),
            7 => Ok(Self::Java),
            8 => Ok(Self::Swift),
            _ => Err(format!("unknown BAML bridge language ID {value}")),
        }
    }
}

/// Host bridge metadata retained by the runtime for diagnostics and telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeInfo {
    pub language: BridgeLanguage,
    pub sdk_version: String,
}

/// Version-1 C representation of bridge registration metadata.
///
/// Fields may only be appended. Existing fields must retain their order,
/// types, and semantics for the lifetime of ABI version 1. The `language`
/// field is a raw `uint32_t` at the C boundary and is validated before it is
/// interpreted as a `BamlBridgeLanguage` value. Consumers set `struct_size` to
/// the size they provide. `sdk_version` is borrowed for `sdk_version_len` bytes
/// only during `register_bridge`; it is copied before that function returns.
/// A zero length permits a null pointer. The version is UTF-8 and identifies
/// the BAML product release, independently of the table's ABI version.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BamlBridgeInfoV1 {
    pub struct_size: usize,
    pub language: u32,
    pub sdk_version: *const u8,
    pub sdk_version_len: usize,
}

struct BridgeRegistry {
    info: OnceLock<BridgeInfo>,
}

impl BridgeRegistry {
    const fn new() -> Self {
        Self {
            info: OnceLock::new(),
        }
    }

    fn register(&self, requested: BridgeInfo) -> Result<&BridgeInfo, String> {
        if let Some(existing) = self.info.get() {
            return compatible_registration(existing, &requested).map(|()| existing);
        }

        if let Err(requested) = self.info.set(requested) {
            let existing = self
                .info
                .get()
                .expect("bridge registration was initialized concurrently");
            return compatible_registration(existing, &requested).map(|()| existing);
        }
        Ok(self
            .info
            .get()
            .expect("bridge registration was just initialized"))
    }
}

fn compatible_registration(existing: &BridgeInfo, requested: &BridgeInfo) -> Result<(), String> {
    if existing == requested {
        return Ok(());
    }
    Err(format!(
        "BAML native runtime is already registered by the {} SDK {}; cannot also register the {} SDK {}",
        existing.language.display_name(),
        existing.sdk_version,
        requested.language.display_name(),
        requested.sdk_version,
    ))
}

static REGISTERED_BRIDGE: BridgeRegistry = BridgeRegistry::new();

/// Return the registered bridge identity for telemetry and diagnostics.
pub fn registered_bridge() -> Option<&'static BridgeInfo> {
    REGISTERED_BRIDGE.info.get()
}

/// Register a host bridge and require an exact canonical release match.
pub fn register_bridge(info: BridgeInfo) -> Result<&'static BridgeInfo, String> {
    if ensure_version_compatible(&info.sdk_version).is_err() {
        return Err(format!(
            "BAML {} SDK {} cannot use native runtime {}. Install the matching native runtime or update the {} SDK",
            info.language.display_name(),
            info.sdk_version,
            baml_version::CANONICAL_VERSION,
            info.language.display_name(),
        ));
    }
    REGISTERED_BRIDGE.register(info)
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
/// `sdk_version` must be readable for `sdk_version_len` bytes.
pub unsafe extern "C" fn register_bridge_ffi(info: *const BamlBridgeInfoV1) -> Buffer {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if info.is_null() {
            return Err("BAML bridge registration pointer is null".to_string());
        }
        // Read only the leading size field before trusting that the caller's
        // allocation is large enough for the rest of the version-1 struct.
        let struct_size = unsafe { std::ptr::addr_of!((*info).struct_size).read() };
        if struct_size < std::mem::size_of::<BamlBridgeInfoV1>() {
            return Err(format!(
                "truncated BAML bridge registration: got {} bytes, need {}",
                struct_size,
                std::mem::size_of::<BamlBridgeInfoV1>(),
            ));
        }
        let info = unsafe { info.read() };
        let language = BridgeLanguage::try_from(info.language)?;
        if info.sdk_version.is_null() && info.sdk_version_len != 0 {
            return Err("BAML SDK version pointer is null but length is nonzero".to_string());
        }
        if info.sdk_version_len > isize::MAX as usize {
            return Err(format!(
                "BAML SDK version length {} exceeds isize::MAX",
                info.sdk_version_len,
            ));
        }

        let bytes = if info.sdk_version_len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(info.sdk_version, info.sdk_version_len) }
        };
        let sdk_version = std::str::from_utf8(bytes)
            .map_err(|error| format!("BAML SDK version is not valid UTF-8: {error}"))?;
        register_bridge(BridgeInfo {
            language,
            sdk_version: sdk_version.to_string(),
        })
        .map(|_| ())
    }));

    match result {
        Ok(Ok(())) => Buffer::from(Vec::new()),
        Ok(Err(error)) => Buffer::from(error.into_bytes()),
        Err(_) => Buffer::from(b"panic while registering BAML bridge".to_vec()),
    }
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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if bytecode.is_null() && length != 0 {
            return Err("bytecode pointer is null but length is nonzero".to_string());
        }

        let bytecode = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytecode, length) }
        };
        initialize_runtime_from_bytecode_impl(bytecode)
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
    let result = std::panic::catch_unwind(|| {
        crate::get_tokio_runtime()
            .map_err(|error| error.to_string())?
            .block_on(crate::shutdown_runtime())
            .map_err(|error| error.to_string())
    });
    match result {
        Ok(Ok(())) => Buffer::from(Vec::new()),
        Ok(Err(error)) => Buffer::from(error.into_bytes()),
        Err(_) => Buffer::from(b"panic while shutting down BAML runtime".to_vec()),
    }
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
        assert!(ensure_version_compatible(baml_version::CANONICAL_VERSION).is_ok());
    }

    #[test]
    fn any_nonidentical_version_is_incompatible() {
        let error = ensure_version_compatible("999.0.0").unwrap_err();
        assert!(error.contains("SDK requires 999.0.0"));
        assert!(error.contains(baml_version::CANONICAL_VERSION));
    }

    #[test]
    fn bridge_registry_is_idempotent_for_identical_registration() {
        let registry = BridgeRegistry::new();
        let info = BridgeInfo {
            language: BridgeLanguage::NodeJs,
            sdk_version: baml_version::CANONICAL_VERSION.to_string(),
        };
        assert_eq!(registry.register(info.clone()).unwrap(), &info);
        assert_eq!(registry.register(info.clone()).unwrap(), &info);
    }

    #[test]
    fn bridge_registry_rejects_conflicting_registration() {
        let registry = BridgeRegistry::new();
        registry
            .register(BridgeInfo {
                language: BridgeLanguage::NodeJs,
                sdk_version: baml_version::CANONICAL_VERSION.to_string(),
            })
            .unwrap();
        let error = registry
            .register(BridgeInfo {
                language: BridgeLanguage::Python,
                sdk_version: baml_version::CANONICAL_VERSION.to_string(),
            })
            .unwrap_err();
        assert!(error.contains("already registered by the Node.js SDK"));
        assert!(error.contains("cannot also register the Python SDK"));
    }

    #[test]
    fn ffi_registration_returns_shared_language_aware_diagnostic() {
        let expected = b"999.0.0";
        let info = BamlBridgeInfoV1 {
            struct_size: std::mem::size_of::<BamlBridgeInfoV1>(),
            language: BridgeLanguage::Python as u32,
            sdk_version: expected.as_ptr(),
            sdk_version_len: expected.len(),
        };
        let buffer = unsafe { register_bridge_ffi(&info) };
        assert!(!buffer.is_empty());
        let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        let message = std::str::from_utf8(bytes).unwrap();
        assert!(message.contains("BAML Python SDK 999.0.0 cannot use native runtime"));
        assert!(message.contains("Install the matching native runtime"));
        crate::free_buffer(buffer);
    }

    #[test]
    fn ffi_registration_records_bridge_identity_for_telemetry() {
        let expected = baml_version::CANONICAL_VERSION.as_bytes();
        let info = BamlBridgeInfoV1 {
            struct_size: std::mem::size_of::<BamlBridgeInfoV1>(),
            language: BridgeLanguage::Rust as u32,
            sdk_version: expected.as_ptr(),
            sdk_version_len: expected.len(),
        };

        let first = unsafe { register_bridge_ffi(&info) };
        assert!(first.is_empty());
        crate::free_buffer(first);
        let second = unsafe { register_bridge_ffi(&info) };
        assert!(second.is_empty());
        crate::free_buffer(second);

        let registered = registered_bridge().unwrap();
        assert_eq!(registered.language, BridgeLanguage::Rust);
        assert_eq!(registered.language.telemetry_name(), "rust");
        assert_eq!(registered.sdk_version, baml_version::CANONICAL_VERSION);
    }

    #[test]
    fn ffi_registration_rejects_unknown_language_without_undefined_behavior() {
        let expected = baml_version::CANONICAL_VERSION.as_bytes();
        let info = BamlBridgeInfoV1 {
            struct_size: std::mem::size_of::<BamlBridgeInfoV1>(),
            language: u32::MAX,
            sdk_version: expected.as_ptr(),
            sdk_version_len: expected.len(),
        };
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
        };
        let buffer = unsafe { register_bridge_ffi(&info) };
        let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        let message = std::str::from_utf8(bytes).unwrap();
        assert!(message.starts_with("truncated BAML bridge registration:"));
        crate::free_buffer(buffer);
    }

    #[test]
    fn ffi_registration_rejects_impossible_version_length() {
        let info = BamlBridgeInfoV1 {
            struct_size: std::mem::size_of::<BamlBridgeInfoV1>(),
            language: BridgeLanguage::Rust as u32,
            sdk_version: std::ptr::dangling(),
            sdk_version_len: usize::MAX,
        };
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
