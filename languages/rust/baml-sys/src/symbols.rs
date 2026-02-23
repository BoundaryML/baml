//! FFI symbol loading and storage.

use libc::{c_char, c_int, c_void, size_t};
use libloading::Symbol;
use once_cell::sync::OnceCell;

use crate::{
    error::{BamlSysError, Result},
    loader::{get_library, LoadedLibrary, VERSION},
};

/// Callback function type for results.
pub type CallbackFn =
    extern "C" fn(call_id: u32, is_done: c_int, content: *const c_char, length: size_t);

/// Callback function type for streaming ticks.
pub type OnTickCallbackFn = extern "C" fn(call_id: u32);

/// Buffer returned from object operations.
#[repr(C)]
#[allow(missing_docs)] // FFI struct fields are self-explanatory
pub struct Buffer {
    /// Pointer to the buffer data.
    pub ptr: *const c_char,
    /// Length of the buffer.
    pub len: size_t,
}

// Type aliases for FFI function signatures
type VersionFn = unsafe extern "C" fn() -> Buffer;
type RegisterCallbacksFn = unsafe extern "C" fn(CallbackFn, CallbackFn, OnTickCallbackFn);
type CreateBamlRuntimeFn =
    unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *const c_void;
type DestroyBamlRuntimeFn = unsafe extern "C" fn(*const c_void);
type InvokeRuntimeCliFn = unsafe extern "C" fn(*const *const c_char) -> c_int;
type CallFunctionFromCFn =
    unsafe extern "C" fn(*const c_void, *const c_char, *const c_char, size_t, u32) -> Buffer;
type CancelFunctionCallFn = unsafe extern "C" fn(u32) -> Buffer;
type CallObjectConstructorFn = unsafe extern "C" fn(*const c_char, size_t) -> Buffer;
type CallObjectMethodFn = unsafe extern "C" fn(*const c_void, *const c_char, size_t) -> Buffer;
type FreeBufferFn = unsafe extern "C" fn(Buffer);

/// Loaded symbols from the dynamic library.
#[allow(missing_docs)] // FFI symbol fields match their C function names
pub struct Symbols {
    pub(crate) version: Symbol<'static, VersionFn>,
    pub(crate) register_callbacks: Symbol<'static, RegisterCallbacksFn>,
    pub(crate) create_baml_runtime: Symbol<'static, CreateBamlRuntimeFn>,
    pub(crate) destroy_baml_runtime: Symbol<'static, DestroyBamlRuntimeFn>,
    pub(crate) invoke_runtime_cli: Symbol<'static, InvokeRuntimeCliFn>,
    pub(crate) call_function_from_c: Symbol<'static, CallFunctionFromCFn>,
    pub(crate) call_function_stream_from_c: Symbol<'static, CallFunctionFromCFn>,
    pub(crate) call_function_parse_from_c: Symbol<'static, CallFunctionFromCFn>,
    pub(crate) build_request_from_c: Symbol<'static, CallFunctionFromCFn>,
    pub(crate) cancel_function_call: Symbol<'static, CancelFunctionCallFn>,
    pub(crate) call_object_constructor: Symbol<'static, CallObjectConstructorFn>,
    pub(crate) call_object_method: Symbol<'static, CallObjectMethodFn>,
    pub(crate) free_buffer: Symbol<'static, FreeBufferFn>,
}

/// Global symbols instance.
static SYMBOLS: OnceCell<Symbols> = OnceCell::new();

/// Get the loaded symbols, initializing if necessary.
pub fn get_symbols() -> Result<&'static Symbols> {
    SYMBOLS.get_or_try_init(|| {
        let lib = get_library()?;
        load_symbols(lib)
    })
}

/// Load all symbols from the library.
fn load_symbols(lib: &'static LoadedLibrary) -> Result<Symbols> {
    // Safety: We're loading symbols from a dynamic library that should
    // have been built with the matching C ABI.
    #[allow(unsafe_code)]
    unsafe {
        // Load free_buffer first so we can clean up the version buffer
        let free_buffer: Symbol<FreeBufferFn> = load_symbol(&lib.library, "free_buffer")?;
        let version: Symbol<VersionFn> = load_symbol(&lib.library, "version")?;

        // Verify version matches - version() now returns Buffer
        let version_buf = version();
        let lib_version = if !version_buf.ptr.is_null() && version_buf.len > 0 {
            let bytes = std::slice::from_raw_parts(version_buf.ptr.cast::<u8>(), version_buf.len);
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            "unknown".to_string()
        };
        free_buffer(version_buf); // Clean up immediately

        if lib_version != VERSION {
            return Err(BamlSysError::VersionMismatch {
                expected: VERSION.to_string(),
                actual: lib_version.to_string(),
            });
        }

        Ok(Symbols {
            version,
            register_callbacks: load_symbol(&lib.library, "register_callbacks")?,
            create_baml_runtime: load_symbol(&lib.library, "create_baml_runtime")?,
            destroy_baml_runtime: load_symbol(&lib.library, "destroy_baml_runtime")?,
            invoke_runtime_cli: load_symbol(&lib.library, "invoke_runtime_cli")?,
            call_function_from_c: load_symbol(&lib.library, "call_function_from_c")?,
            call_function_stream_from_c: load_symbol(&lib.library, "call_function_stream_from_c")?,
            call_function_parse_from_c: load_symbol(&lib.library, "call_function_parse_from_c")?,
            build_request_from_c: load_symbol(&lib.library, "build_request_from_c")?,
            cancel_function_call: load_symbol(&lib.library, "cancel_function_call")?,
            call_object_constructor: load_symbol(&lib.library, "call_object_constructor")?,
            call_object_method: load_symbol(&lib.library, "call_object_method")?,
            free_buffer,
        })
    }
}

/// Load a single symbol from the library.
#[allow(unsafe_code)]
unsafe fn load_symbol<T>(
    library: &'static libloading::Library,
    name: &'static str,
) -> Result<Symbol<'static, T>> {
    unsafe {
        library
            .get(name.as_bytes())
            .map_err(|e| BamlSysError::SymbolNotFound {
                symbol: name,
                source: e,
            })
    }
}
