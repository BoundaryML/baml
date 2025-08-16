pub mod async_runtime;
pub mod callbacks;
pub mod functions;
pub mod objects;
pub mod runtime;
pub mod utils;
pub mod value;

// Common imports used across FFI modules
pub use std::ffi::{CStr, CString};

#[cfg(not(target_arch = "wasm32"))]
pub use libc::{c_char, c_int, c_void};
#[cfg(target_arch = "wasm32")]
pub type c_char = i8;
#[cfg(target_arch = "wasm32")]
pub type c_int = i32;
#[cfg(target_arch = "wasm32")]
pub type c_void = std::ffi::c_void;

pub use value::*;
pub use async_runtime::AsyncRuntime;

pub use crate::ctypes::*;
