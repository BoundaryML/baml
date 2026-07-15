//! Staticlib wrapper around `bridge_cffi` for the Swift SDK.
//!
//! The `#[no_mangle] extern "C"` symbols defined in `bridge_cffi`
//! (`call_function`, `register_callback`,
//! `initialize_runtime_from_bytecode`, the handle/media ops, …) are
//! carried into this crate's staticlib output by the dependency link;
//! the re-export below additionally exposes the Rust API so future
//! Swift-specific glue (if any) can call it without going through the
//! C ABI. No Swift-specific Rust logic should live here — the Swift
//! side of the bridge is `sdks/swift/Sources/BamlBridge`.
pub use bridge_cffi::*;
