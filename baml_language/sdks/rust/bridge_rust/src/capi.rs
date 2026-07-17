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
    pub(crate) create_baml_runtime:
        unsafe extern "C" fn(*const c_char, *const c_char) -> *const c_void,
    /// Returns a status buffer: empty on success, otherwise a UTF-8 error
    /// message. Read it with [`Api::take_status`].
    pub(crate) initialize_runtime_from_bytecode: unsafe extern "C" fn(*const u8, usize) -> Buffer,
    pub(crate) register_callback: unsafe extern "C" fn(CallbackFn),
    pub(crate) new_function_call: unsafe extern "C" fn() -> u64,
    pub(crate) call_function: unsafe extern "C" fn(*const c_char, *const u8, usize, u32),
    pub(crate) free_buffer: unsafe extern "C" fn(Buffer),
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

/// The leading fields of `bridge_cffi`'s versioned `BamlApiV1` table (returned
/// by its `baml_get_api_v1` entry point) that this bridge reads. Its
/// `#[repr(C)]` layout and field order mirror `BamlApiV1` exactly, so reading
/// it through the real (possibly longer) table is sound; `struct_size` guards
/// against a shorter or older table. Extend this in ABI order as more of the
/// table is used.
///
/// `create_baml_runtime` is intentionally NOT in `BamlApiV1` (it is a legacy
/// direct export) and is resolved separately.
#[repr(C)]
struct BamlApiV1 {
    abi_version: u32,
    struct_size: usize,
    version: unsafe extern "C" fn() -> Buffer,
    initialize_runtime_from_bytecode: unsafe extern "C" fn(*const u8, usize) -> Buffer,
    free_buffer: unsafe extern "C" fn(Buffer),
    register_callback: unsafe extern "C" fn(CallbackFn),
    call_function: unsafe extern "C" fn(*const c_char, *const u8, usize, u32),
    new_function_call: unsafe extern "C" fn() -> u64,
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
    let table = unsafe { &*get_api_v1() };
    if table.abi_version != 1 || table.struct_size < std::mem::size_of::<BamlApiV1>() {
        return Err(LoaderError::LoadLibrary(format!(
            "{} exposes an incompatible BAML C API (abi_version {}, {} bytes; \
             baml_bridge needs v1 with at least {} bytes)",
            path.display(),
            table.abi_version,
            table.struct_size,
            std::mem::size_of::<BamlApiV1>()
        )));
    }

    let api = Api {
        // Not part of BamlApiV1 (a legacy direct export); resolved directly.
        create_baml_runtime: sym(&library, b"create_baml_runtime\0")?,
        initialize_runtime_from_bytecode: table.initialize_runtime_from_bytecode,
        register_callback: table.register_callback,
        new_function_call: table.new_function_call,
        call_function: table.call_function,
        free_buffer: table.free_buffer,
    };

    // SAFETY: `version` returns an engine-owned buffer freed via the same
    // ABI; `copy_and_free` copies the bytes out before freeing it.
    #[expect(unsafe_code)]
    let version_buffer = unsafe { (table.version)() };
    let loaded_version = String::from_utf8_lossy(&api.copy_and_free(version_buffer)).into_owned();
    let expected = crate::get_version();
    if loaded_version != expected {
        // Dropping `library` here unloads it (Go parity).
        return Err(LoaderError::VersionMismatch(format!(
            "baml_bridge expects {expected}, but loaded library {} reports {loaded_version}",
            path.display()
        )));
    }

    log::info(&format!("BAML (v{loaded_version}) loaded"));
    log::debug(&format!("Library path: {}", path.display()));

    // The Api table's fn pointers point into the mapped library; keep it
    // mapped for the process lifetime.
    std::mem::forget(library);
    Ok(api)
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
}
