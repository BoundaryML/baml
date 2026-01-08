//! Built-in functions for VM execution.
//!
//! This module links builtin type signatures (from `baml_builtins`)
//! to their native function implementations. Type signatures are the
//! single source of truth in `baml_builtins`.
//!
//! # Naming Convention
//!
//! Native functions are automatically mapped by converting the constant name to lowercase:
//! - `ARRAY_LENGTH` → `native::array_length`
//! - `STRING_TO_UPPER_CASE` → `native::string_to_upper_case`
//!
//! If you add a new builtin in `baml_builtins::define_builtins!`, you must add
//! a corresponding function in `native.rs` with the lowercase name.

use std::sync::LazyLock;

// Re-export type signatures from baml_builtins.
// Type checker uses these directly without depending on baml_vm.
pub use baml_builtins::{BuiltinSignature, TypePattern, paths};
use indexmap::IndexMap;
use paste::paste;

use crate::native::{self, NativeFunction};

/// Maps path constants to native functions by naming convention.
///
/// For each builtin constant like `ARRAY_LENGTH`, this macro:
/// 1. Looks up `paths::ARRAY_LENGTH` for the path string
/// 2. Looks up `native::array_length` for the implementation
///
/// # Compile Error?
///
/// If you see "cannot find function `xyz` in module `native`", you need to
/// add `pub fn xyz(...)` to `native.rs` matching the lowercase constant name.
macro_rules! make_native_fn_entries {
    ($($name:ident),*) => {
        paste! {
            &[
                $({
                    // If this line fails, add this function to native.rs:
                    //   pub fn [<$name:lower>](vm: &mut Vm, args: &[Value]) -> Result<Value, VmError>
                    #[allow(clippy::unnecessary_cast)]
                    let f: NativeFunction = native::[<$name:lower>];
                    (paths::$name, f)
                },)*
            ]
        }
    };
}

/// Map from builtin path to native function implementation.
static NATIVE_FUNCTIONS: LazyLock<IndexMap<&'static str, NativeFunction>> = LazyLock::new(|| {
    // Native function implementations for each builtin.
    //
    // Automatically generated from `baml_builtins::for_all_builtins!`.
    // Maps each path constant to its native function by naming convention.
    let native_fns: &[(&str, NativeFunction)] =
        baml_builtins::for_all_builtins!(make_native_fn_entries);

    native_fns
        .iter()
        .map(|(path, fn_ptr)| (*path, *fn_ptr))
        .collect()
});

/// Get the native function for a builtin path.
pub fn get_native_fn(path: &str) -> Option<NativeFunction> {
    NATIVE_FUNCTIONS.get(path).copied()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_signatures_have_native_fns() {
        // Every signature in baml_builtins must have a native implementation
        for sig in baml_builtins::builtins() {
            assert!(
                get_native_fn(sig.path).is_some(),
                "Missing native function for builtin: {}",
                sig.path
            );
        }
    }

    #[test]
    fn test_all_native_fns_have_signatures() {
        // Every native function must have a signature in baml_builtins
        for path in NATIVE_FUNCTIONS.keys() {
            assert!(
                baml_builtins::find_builtin_by_path(path).is_some(),
                "Native function has no signature: {path}"
            );
        }
    }

    #[test]
    fn test_functions_map() {
        let fns = functions();
        assert!(fns.contains_key(paths::ARRAY_LENGTH));
        assert!(fns.contains_key(paths::ENV_GET));
        assert_eq!(fns.get(paths::ARRAY_LENGTH).unwrap().1, 1);
    }
}
