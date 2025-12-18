use libc::{c_char, c_int, c_void, size_t};

/// Callback function type for results
pub type CallbackFn = extern "C" fn(call_id: u32, is_done: c_int, content: *const i8, length: size_t);

/// Callback function type for streaming ticks
pub type OnTickCallbackFn = extern "C" fn(call_id: u32);

/// Buffer returned from object operations
#[repr(C)]
pub struct Buffer {
    pub ptr: *const i8,
    pub len: size_t,
}

unsafe extern "C" {
    // Version
    pub fn version() -> *const c_char;

    // Callback registration - MUST be called before any other FFI calls
    pub fn register_callbacks(
        callback_fn: CallbackFn,
        error_callback_fn: CallbackFn,
        on_tick_callback_fn: OnTickCallbackFn,
    );

    // Runtime lifecycle
    pub fn create_baml_runtime(
        root_path: *const c_char,
        src_files_json: *const c_char,
        env_vars_json: *const c_char,
    ) -> *const c_void;

    pub fn destroy_baml_runtime(runtime: *const c_void);

    // CLI invocation - useful for smoke testing FFI linkage
    pub fn invoke_runtime_cli(args: *const *const c_char) -> c_int;

    // Function calls
    pub fn call_function_from_c(
        runtime: *const c_void,
        function_name: *const c_char,
        encoded_args: *const c_char,
        length: size_t,
        id: u32,
    ) -> *const c_void;

    pub fn call_function_stream_from_c(
        runtime: *const c_void,
        function_name: *const c_char,
        encoded_args: *const c_char,
        length: size_t,
        id: u32,
    ) -> *const c_void;

    pub fn call_function_parse_from_c(
        runtime: *const c_void,
        function_name: *const c_char,
        encoded_args: *const c_char,
        length: size_t,
        id: u32,
    ) -> *const c_void;

    pub fn cancel_function_call(id: u32) -> *const c_void;

    // Object operations
    pub fn call_object_constructor(
        encoded_invocation: *const c_char,
        length: size_t,
    ) -> Buffer;

    pub fn call_object_method(
        runtime: *const c_void,
        encoded_invocation: *const c_char,
        length: size_t,
    ) -> Buffer;

    pub fn free_buffer(buf: Buffer);
}
