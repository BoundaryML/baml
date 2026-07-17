//! Staticlib wrapper around `bridge_cffi` for the Swift SDK.
//!
//! The Swift runtime consumes the canonical versioned C ABI: it
//! resolves `baml_get_api_v1()` (carried into this staticlib by the
//! dependency link) and calls every native entry point through the
//! returned `BamlApiV1` function table. The re-export below
//! additionally exposes the Rust API so future Swift-specific glue
//! (if any) can call it without going through the C ABI. No
//! Swift-specific Rust logic should live here — the Swift side of the
//! bridge is `sdks/swift/Sources/BamlBridge`.
pub use bridge_cffi::*;
