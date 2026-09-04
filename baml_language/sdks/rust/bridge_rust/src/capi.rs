//! The BAML engine's C ABI, as consumed by this crate.
//!
//! The engine is never linked into the host binary: like the other
//! dylib-loader clients, every call goes through [`Api`] — a table of C
//! symbols resolved at runtime from the `bridge_cffi` shared library
//! acquired by [`crate::loader`]. One call path in development and in
//! production. The type definitions here mirror `bridge_cffi`'s exported
//! ABI exactly.

use std::ffi::{c_char, c_void};

use crate::{
    SdkError,
    loader::{self, LoaderError, log},
};

/// The engine's result-delivery callback. `content` is borrowed only for
/// the synchronous duration of the call — implementations must copy.
pub(crate) type CallbackFn = extern "C" fn(call_id: u32, content: *const c_char, length: usize);

/// The engine's BAML→host dispatch callback: BAML invoked a host-owned
/// callable. `args` (a protobuf `BamlToHostCall`) is borrowed only for the
/// synchronous duration of the call — implementations must copy, return
/// promptly, and eventually complete via [`Api::complete_host_call`].
pub(crate) type HostDispatchFn =
    extern "C" fn(host_value_key: u64, call_id: u32, args: *const u8, length: usize);

/// The engine's host-value release callback: the engine dropped its last
/// reference to a host-owned value.
pub(crate) type HostReleaseFn = extern "C" fn(host_value_key: u64);

/// Owned byte buffer returned by the engine. Must be released exactly once
/// with [`Api::free_buffer`]. Layout-identical to `bridge_cffi::Buffer`.
#[repr(C)]
pub(crate) struct Buffer {
    pub(crate) ptr: *const c_char,
    pub(crate) len: usize,
}

/// Function table over the engine's C ABI. Holds exactly the symbols the
/// bridge calls today; entries are added alongside the features that use
/// them.
pub(crate) struct Api {
    pub(crate) create_runtime: unsafe extern "C" fn(*const u8, usize, *mut u64) -> Buffer,
    pub(crate) unregister_runtime: unsafe extern "C" fn(u64) -> Buffer,
    pub(crate) register_program:
        unsafe extern "C" fn(u64, *const u8, usize, *const c_char) -> Buffer,
    pub(crate) call_function_for_runtime: unsafe extern "C" fn(u64, *const u8, usize, u32),
    pub(crate) create_baml_runtime:
        unsafe extern "C" fn(*const c_char, *const c_char) -> *const c_void,
    /// Returns a status buffer: empty on success, otherwise a UTF-8 error
    /// message. Read it with [`Api::take_status`].
    pub(crate) initialize_runtime_from_bytecode: unsafe extern "C" fn(*const u8, usize) -> Buffer,
    pub(crate) initialize_runtime_from_bytecode_with_metadata:
        unsafe extern "C" fn(*const u8, usize, *const c_char) -> Buffer,
    pub(crate) register_callback: unsafe extern "C" fn(CallbackFn),
    pub(crate) new_function_call: unsafe extern "C" fn() -> u64,
    pub(crate) call_function: unsafe extern "C" fn(*const u8, usize, u32),
    pub(crate) handle_clone: unsafe extern "C" fn(u64, *mut u64) -> u32,
    pub(crate) handle_release: unsafe extern "C" fn(u64) -> u32,
    pub(crate) free_buffer: unsafe extern "C" fn(Buffer),
    pub(crate) register_host_dispatch_callback: unsafe extern "C" fn(HostDispatchFn),
    pub(crate) register_host_release_callback: unsafe extern "C" fn(HostReleaseFn),
    /// Complete one outstanding BAML→host call. `is_error` is 0 or 1;
    /// `content` is a protobuf `InboundValue`, borrowed only for the call.
    /// An empty (`length == 0`) error payload is the bridge-failure signal:
    /// the engine surfaces it as an SDK panic instead of a catchable throw.
    pub(crate) complete_host_call: unsafe extern "C" fn(u32, i32, *const c_char, usize),
}

impl Api {
    /// Consume an engine status buffer: empty means success, anything else
    /// is the engine's UTF-8 error message. Frees the buffer either way.
    pub(crate) fn take_status(&self, buffer: Buffer) -> Result<(), String> {
        let message = self.copy_and_free(buffer);
        if message.is_empty() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&message).into_owned())
        }
    }

    /// Copy an engine-owned buffer's bytes out and free it exactly once.
    fn copy_and_free(&self, buffer: Buffer) -> Vec<u8> {
        // SAFETY: the engine hands over an owned buffer of `len` bytes that
        // we must free exactly once; the bytes are copied before the free.
        #[expect(unsafe_code)]
        unsafe {
            let bytes = if buffer.ptr.is_null() || buffer.len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len).to_vec()
            };
            (self.free_buffer)(buffer);
            bytes
        }
    }
}

/// The one loaded engine per process. A load failure is cached too:
/// retrying could observe half-installed state, and the Go loader
/// likewise fails once and for all.
static LOADED: std::sync::OnceLock<Result<Api, LoaderError>> = std::sync::OnceLock::new();

/// Report whether the engine library has already been loaded (or
/// terminally failed to load).
pub(crate) fn engine_loaded() -> bool {
    LOADED.get().is_some()
}

/// The process-wide API table, loading the engine on first use.
pub(crate) fn api() -> Result<&'static Api, SdkError> {
    LOADED
        .get_or_init(load)
        .as_ref()
        .map_err(|e| SdkError::new(e.to_string()))
}

/// Load on the current thread, except on a multi-threaded async
/// executor: there, `block_in_place` hands the worker's duties off so a
/// first-call library download cannot stall the executor.
fn load() -> Result<Api, LoaderError> {
    if let Ok(handle) = tokio::runtime::Handle::try_current()
        && matches!(
            handle.runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread
        )
    {
        return tokio::task::block_in_place(|| load_inner(&loader::LoaderEnv::from_process()));
    }
    load_inner(&loader::LoaderEnv::from_process())
}

/// The `bridge_cffi` versioned `BamlApiV1` table returned by
/// `baml_get_api_v1`. Its `#[repr(C)]` layout and field order mirror
/// `BamlApiV1` exactly; `struct_size` guards against a shorter or older table.
/// Raw slots remain nullable until the loader validates every required
/// operation.
///
/// `create_baml_runtime` is intentionally NOT in `BamlApiV1` (it is a legacy
/// direct export) and is resolved separately.
#[repr(C)]
struct BamlApiV1 {
    abi_version: u32,
    struct_size: usize,
    version: Option<unsafe extern "C" fn() -> Buffer>,
    initialize_runtime_from_bytecode: Option<unsafe extern "C" fn(*const u8, usize) -> Buffer>,
    free_buffer: Option<unsafe extern "C" fn(Buffer)>,
    register_callback: Option<unsafe extern "C" fn(CallbackFn)>,
    call_function: Option<unsafe extern "C" fn(*const u8, usize, u32)>,
    new_function_call: Option<unsafe extern "C" fn() -> u64>,
    /// Layout placeholder: sits between `new_function_call` and the
    /// host-value entries in ABI order. Unused until cancellation lands.
    cancel_function_call: Option<unsafe extern "C" fn(u64) -> i32>,
    register_host_dispatch_callback: Option<unsafe extern "C" fn(HostDispatchFn)>,
    register_host_release_callback: Option<unsafe extern "C" fn(HostReleaseFn)>,
    complete_host_call: Option<unsafe extern "C" fn(u32, i32, *const c_char, usize)>,
    handle_clone: Option<unsafe extern "C" fn(u64, *mut u64) -> u32>,
    handle_release: Option<unsafe extern "C" fn(u64) -> u32>,
    media_from_url:
        Option<unsafe extern "C" fn(i32, *const c_char, *const c_char, *mut u64, *mut i32) -> u32>,
    media_from_file:
        Option<unsafe extern "C" fn(i32, *const c_char, *const c_char, *mut u64, *mut i32) -> u32>,
    media_from_base64:
        Option<unsafe extern "C" fn(i32, *const c_char, *const c_char, *mut u64, *mut i32) -> u32>,
    media_url: Option<unsafe extern "C" fn(u64, i32, *mut Buffer) -> u32>,
    media_file: Option<unsafe extern "C" fn(u64, i32, *mut Buffer) -> u32>,
    media_base64: Option<unsafe extern "C" fn(u64, i32, *mut Buffer) -> u32>,
    media_mime_type: Option<unsafe extern "C" fn(u64, i32, *mut Buffer) -> u32>,
    register_bridge: Option<unsafe extern "C" fn(*const c_void) -> Buffer>,
    register_unhandled_spawn_error_callback:
        Option<unsafe extern "C" fn(extern "C" fn(*const c_char, usize, i32))>,
    shutdown_runtime: Option<unsafe extern "C" fn() -> Buffer>,
    initialize_runtime_from_bytecode_with_metadata:
        Option<unsafe extern "C" fn(*const u8, usize, *const c_char) -> Buffer>,
    register_program: Option<unsafe extern "C" fn(u64, *const u8, usize, *const c_char) -> Buffer>,
    create_runtime: Option<unsafe extern "C" fn(*const u8, usize, *mut u64) -> Buffer>,
    unregister_runtime: Option<unsafe extern "C" fn(u64) -> Buffer>,
    call_function_for_runtime: Option<unsafe extern "C" fn(u64, *const u8, usize, u32)>,
    program_key: Option<unsafe extern "C" fn(*const u8, usize, *mut u64) -> Buffer>,
    release_function_call: Option<unsafe extern "C" fn(u64)>,
}

#[repr(C)]
struct BamlBridgeInfoV1 {
    struct_size: usize,
    language: u32,
    sdk_version: *const u8,
    sdk_version_len: usize,
    bridge_runtime_name: *const u8,
    bridge_runtime_name_len: usize,
    bridge_runtime_version: *const u8,
    bridge_runtime_version_len: usize,
}

fn load_inner(env: &loader::LoaderEnv) -> Result<Api, LoaderError> {
    let path = loader::resolve_library_path(env)?;
    log::debug(&format!("Loading BAML library from {}", path.display()));

    // SAFETY: loading a shared library runs its initializers; the
    // resolved file is the engine artifact this loader exists to load.
    #[expect(unsafe_code)]
    let library = unsafe { libloading::Library::new(&path) }.map_err(|e| {
        let text = e.to_string();
        let arch_hint = [
            "wrong architecture",
            "wrong ELF class",
            "is not a valid Win32 application",
        ]
        .iter()
        .any(|needle| text.contains(needle));
        LoaderError::LoadLibrary(format!(
            "failed to load {}: {text}{}",
            path.display(),
            if arch_hint {
                " (possible architecture mismatch)"
            } else {
                ""
            }
        ))
    })?;

    // `bridge_cffi` exposes its production surface through one versioned entry
    // point, `baml_get_api_v1`; its individual symbols are legacy exports the
    // ABI wants hosts to stop resolving. Read the table and reject an
    // incompatible or truncated one.
    let get_api_v1: unsafe extern "C" fn() -> *const BamlApiV1 =
        sym(&library, b"baml_get_api_v1\0")?;
    // SAFETY: the entry point returns a pointer to a `'static` table owned by
    // the library, valid for as long as it stays mapped (we forget it below).
    #[expect(unsafe_code)]
    let table_ptr = unsafe { get_api_v1() };
    if table_ptr.is_null() {
        return Err(LoaderError::LoadLibrary(format!(
            "{} returned a null BAML C API table",
            path.display()
        )));
    }
    // SAFETY: the null pointer case was rejected above, and the entry point
    // contract returns a process-lifetime table.
    #[expect(unsafe_code)]
    let table = unsafe { &*table_ptr };
    if table.abi_version != 2 || table.struct_size < std::mem::size_of::<BamlApiV1>() {
        return Err(LoaderError::LoadLibrary(format!(
            "{} exposes an incompatible BAML C API (abi_version {}, {} bytes; \
             baml_bridge needs ABI revision 2 with at least {} bytes)",
            path.display(),
            table.abi_version,
            table.struct_size,
            std::mem::size_of::<BamlApiV1>()
        )));
    }

    let version = required_slot(table.version, "version", &path)?;
    let initialize_runtime_from_bytecode = required_slot(
        table.initialize_runtime_from_bytecode,
        "initialize_runtime_from_bytecode",
        &path,
    )?;
    let initialize_runtime_from_bytecode_with_metadata = required_slot(
        table.initialize_runtime_from_bytecode_with_metadata,
        "initialize_runtime_from_bytecode_with_metadata",
        &path,
    )?;
    let free_buffer = required_slot(table.free_buffer, "free_buffer", &path)?;
    let register_callback = required_slot(table.register_callback, "register_callback", &path)?;
    let call_function = required_slot(table.call_function, "call_function", &path)?;
    let new_function_call = required_slot(table.new_function_call, "new_function_call", &path)?;
    required_slot(table.cancel_function_call, "cancel_function_call", &path)?;
    let register_host_dispatch_callback = required_slot(
        table.register_host_dispatch_callback,
        "register_host_dispatch_callback",
        &path,
    )?;
    let register_host_release_callback = required_slot(
        table.register_host_release_callback,
        "register_host_release_callback",
        &path,
    )?;
    let complete_host_call = required_slot(table.complete_host_call, "complete_host_call", &path)?;
    let handle_clone = required_slot(table.handle_clone, "handle_clone", &path)?;
    let handle_release = required_slot(table.handle_release, "handle_release", &path)?;
    required_slot(table.media_from_url, "media_from_url", &path)?;
    required_slot(table.media_from_file, "media_from_file", &path)?;
    required_slot(table.media_from_base64, "media_from_base64", &path)?;
    required_slot(table.media_url, "media_url", &path)?;
    required_slot(table.media_file, "media_file", &path)?;
    required_slot(table.media_base64, "media_base64", &path)?;
    required_slot(table.media_mime_type, "media_mime_type", &path)?;
    let register_bridge = required_slot(table.register_bridge, "register_bridge", &path)?;
    required_slot(
        table.register_unhandled_spawn_error_callback,
        "register_unhandled_spawn_error_callback",
        &path,
    )?;
    required_slot(table.shutdown_runtime, "shutdown_runtime", &path)?;

    let api = Api {
        create_runtime: required_slot(table.create_runtime, "create_runtime", &path)?,
        unregister_runtime: required_slot(table.unregister_runtime, "unregister_runtime", &path)?,
        register_program: required_slot(table.register_program, "register_program", &path)?,
        call_function_for_runtime: required_slot(
            table.call_function_for_runtime,
            "call_function_for_runtime",
            &path,
        )?,
        // Not part of BamlApiV1 (a legacy direct export); resolved directly.
        create_baml_runtime: sym(&library, b"create_baml_runtime\0")?,
        initialize_runtime_from_bytecode,
        initialize_runtime_from_bytecode_with_metadata,
        register_callback,
        new_function_call,
        call_function,
        handle_clone,
        handle_release,
        free_buffer,
        register_host_dispatch_callback,
        register_host_release_callback,
        complete_host_call,
    };

    // SAFETY: `version` returns an engine-owned buffer freed via the same
    // ABI; `copy_and_free` copies the bytes out before freeing it.
    #[expect(unsafe_code)]
    let version_buffer = unsafe { version() };
    let loaded_version = String::from_utf8_lossy(&api.copy_and_free(version_buffer)).into_owned();
    let expected = crate::get_version();
    if loaded_version != expected {
        // Dropping `library` here unloads it (Go parity).
        return Err(LoaderError::VersionMismatch(format!(
            "baml_bridge expects {expected}, but loaded library {} reports {loaded_version}",
            path.display()
        )));
    }

    let runtime_name = crate::version::BRIDGE_RUNTIME_NAME.as_bytes();
    let runtime_version = crate::get_bridge_runtime_version().as_bytes();
    let toolchain_version = crate::get_toolchain_version().as_bytes();
    let bridge_info = BamlBridgeInfoV1 {
        struct_size: std::mem::size_of::<BamlBridgeInfoV1>(),
        language: 4,
        sdk_version: toolchain_version.as_ptr(),
        sdk_version_len: toolchain_version.len(),
        bridge_runtime_name: runtime_name.as_ptr(),
        bridge_runtime_name_len: runtime_name.len(),
        bridge_runtime_version: runtime_version.as_ptr(),
        bridge_runtime_version_len: runtime_version.len(),
    };
    #[expect(unsafe_code)]
    let registration = unsafe { register_bridge((&raw const bridge_info).cast()) };
    api.take_status(registration)
        .map_err(LoaderError::VersionMismatch)?;

    log::info(&format!("BAML (v{loaded_version}) loaded"));
    log::debug(&format!("Library path: {}", path.display()));

    // The Api table's fn pointers point into the mapped library; keep it
    // mapped for the process lifetime.
    std::mem::forget(library);
    Ok(api)
}

fn required_slot<T: Copy>(
    slot: Option<T>,
    name: &str,
    path: &std::path::Path,
) -> Result<T, LoaderError> {
    slot.ok_or_else(|| {
        LoaderError::LoadLibrary(format!(
            "{} exposes a null required BAML C API operation `{name}`",
            path.display()
        ))
    })
}

/// Resolve one symbol and copy its address out of the borrow on
/// `library`.
fn sym<T: Copy>(library: &libloading::Library, name: &'static [u8]) -> Result<T, LoaderError> {
    // SAFETY: `T` is instantiated only with `extern "C"` fn-pointer types
    // mirroring the engine ABI.
    #[expect(unsafe_code)]
    let symbol = unsafe { library.get::<T>(name) }.map_err(|e| {
        LoaderError::LoadLibrary(format!(
            "symbol lookup failed for {}: {e}",
            String::from_utf8_lossy(&name[..name.len() - 1])
        ))
    })?;
    Ok(*symbol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::LoaderEnv;

    #[test]
    fn loading_a_non_library_file_fails_loudly() {
        let path = std::env::temp_dir().join(format!(
            "baml_bridge_not_a_library_{}.dylib",
            std::process::id()
        ));
        std::fs::write(&path, b"definitely not a shared library").unwrap();
        let env = LoaderEnv {
            explicit_path: Some(path.clone()),
            env_path: None,
            cache_dir_override: None,
            user_cache_dir: None,
            disable_download: true,
            download_base: None,
            system_paths: Vec::new(),
            version: crate::get_version().to_string(),
        };
        let Err(err) = load_inner(&env) else {
            panic!("expected loading a non-library file to fail")
        };
        let _ = std::fs::remove_file(&path);
        let msg = err.to_string();
        assert!(matches!(err, LoaderError::LoadLibrary(_)), "{msg}");
        assert!(msg.contains("baml_bridge_not_a_library"), "{msg}");
    }

    #[test]
    fn null_required_api_slot_fails_loudly() {
        let path = std::path::Path::new("/tmp/libbaml_cffi.dylib");
        let err = required_slot::<unsafe extern "C" fn(*const u8, usize, u32)>(
            None,
            "call_function",
            path,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, LoaderError::LoadLibrary(_)), "{msg}");
        assert!(msg.contains("call_function"), "{msg}");
        assert!(msg.contains("null required"), "{msg}");
    }
}
