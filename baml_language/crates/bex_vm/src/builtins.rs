//! Built-in functions for VM execution.
//!
//! This module links builtin type signatures (from `baml_builtins`)
//! to their native function implementations via the generated `NativeFunctions` trait.
//!
//! The `NativeFunctions` trait (generated in `OUT_DIR/nativefunctions_generated.rs`) provides
//! `VmNatives::get_native_fn(path)` as a direct match lookup — no `LazyLock`, no macros.

// Re-export type signatures from baml_builtins.
// Type checker uses these directly without depending on bex_vm.
pub use baml_builtins::{BuiltinSignature, TypePattern, paths};
use indexmap::IndexMap;

use crate::native::{NativeFunction, NativeFunctions, VmNatives};

/// Get the native function for a builtin path.
pub fn get_native_fn(path: &str) -> Option<NativeFunction> {
    VmNatives::get_native_fn(path)
}

/// Generate the functions map for VM registration.
///
/// Uses signatures from `baml_builtins` and links them to native implementations.
pub fn functions() -> IndexMap<String, (NativeFunction, usize)> {
    baml_builtins::builtins()
        .iter()
        .filter_map(|sig| {
            let native_fn = get_native_fn(sig.path)?;
            Some((sig.path.to_string(), (native_fn, sig.arity())))
        })
        .collect()
}
