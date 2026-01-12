//! Type signatures for BAML built-in functions.
//!
//! This crate provides compile-time type information for built-in functions,
//! used by the type checker (`baml_compiler_tir`). It does NOT include
//! runtime implementations - those live in `baml_vm`.
//!
//! This separation allows the type checker to avoid depending on the VM.
//!
//! # Adding a new builtin
//!
//! Add a new entry in the `define_builtins!` macro invocation below.
//! This generates both the path constant and the signature in one place.

/// Type pattern for matching/constructing types with type variables.
///
/// Used for generic builtin functions like `Array<T>.push(item: T)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePattern {
    Int,
    Float,
    String,
    Bool,
    Null,
    Array(Box<TypePattern>),
    Map {
        key: Box<TypePattern>,
        value: Box<TypePattern>,
    },
    Media,
    Optional(Box<TypePattern>),
    /// Type variable - binds to actual type during pattern matching.
    /// E.g., `Var("T")` in `Array<T>.push(item: T)` binds to the element type.
    Var(&'static str),
}

impl TypePattern {
    #[must_use]
    pub fn optional(self) -> Self {
        match self {
            Self::Optional(inner) => inner.optional(),
            _ => Self::Optional(Box::new(self)),
        }
    }
}

/// A built-in function's type signature (compile-time only).
///
/// This contains everything needed for type checking, but NOT runtime execution.
/// The VM links these signatures to native function implementations separately.
pub struct BuiltinSignature {
    /// Full path, e.g., "baml.Array.length" or "env.get".
    pub path: &'static str,

    /// If Some, this is a method callable as `receiver.method_name()`.
    /// If None, this is a free function callable as `path()`.
    pub receiver: Option<TypePattern>,

    /// Parameters (excluding self for methods).
    pub params: Vec<(&'static str, TypePattern)>,

    /// Return type.
    pub returns: TypePattern,
}

impl BuiltinSignature {
    /// Get the method name from the path.
    /// E.g., "baml.Array.length" -> Some("length")
    /// E.g., "env.get" -> None (it's a free function)
    pub fn method_name(&self) -> Option<&str> {
        self.receiver.as_ref()?;
        self.path.rsplit('.').next()
    }

    /// Arity including self for methods.
    pub fn arity(&self) -> usize {
        self.params.len() + usize::from(self.receiver.is_some())
    }
}

/// Macro containing all builtin definitions.
///
/// This is used by both `baml_builtins` and `baml_vm` to ensure consistency.
/// The macro takes a callback that will receive the definitions.
#[macro_export]
macro_rules! with_builtins {
    ($callback:path) => {
        $callback! {
            mod baml {
                // =====================================================================
                // Array methods
                // =====================================================================
                struct Array<T> {
                    fn length(self: Array<T>) -> i64;
                    fn push(self: mut Array<T>, item: T);
                    fn at(self: Array<T>, index: i64) -> Result<T>;
                    fn concat(self: Array<T>, other: Array<T>) -> Array<T>;
                }

                // =====================================================================
                // String methods
                // =====================================================================
                struct String {
                    fn length(self: String) -> i64;
                    fn toLowerCase(self: String) -> String;
                    fn toUpperCase(self: String) -> String;
                    fn trim(self: String) -> String;
                    fn includes(self: String, search: String) -> bool;
                    fn startsWith(self: String, prefix: String) -> bool;
                    fn endsWith(self: String, suffix: String) -> bool;
                    #[uses(vm)]
                    fn split(self: String, delimiter: String) -> Array<String>;
                    fn substring(self: String, start: i64, end: i64) -> String;
                    fn replace(self: String, search: String, replacement: String) -> String;
                }

                // =====================================================================
                // Map methods
                // =====================================================================
                struct Map<K, V> {
                    fn length(self: Map<K, V>) -> i64;
                }
                // Map.has only works on string-keyed maps, so we define it separately
                // with only V as generic (String in the signature is the concrete type)
                struct Map<V> {
                    fn has(self: Map<String, V>, key: String) -> bool;
                }

                // =====================================================================
                // Free functions
                // =====================================================================
                #[uses(vm)]
                fn deep_copy<T>(value: T) -> Result<T>;
                #[uses(vm)]
                fn deep_equals<T>(a: T, b: T) -> bool;

                mod unstable {
                    #[uses(vm)]
                    fn string<T>(value: T) -> Result<String>;
                }

                // =====================================================================
                // Media methods
                // =====================================================================
                struct Media {
                    fn as_url(self: Media) -> Option<String>;
                    fn as_base64(self: Media) -> Option<String>;
                    fn as_file(self: Media) -> Option<String>;
                    fn mime_type(self: Media) -> Option<String>;
                }
            }

            mod env {
                #[uses(vm)]
                fn get(key: String) -> Result<String>;
            }
        }
    };
}

// Define all builtins using ergonomic Rust-like syntax.
// The macro generates:
// - `pub mod paths` with constants like `BAML_ARRAY_LENGTH`, `ENV_GET`, etc.
// - `for_all_builtins!` macro for iterating over all builtin names
// - `BUILTINS` static with all `BuiltinSignature` instances
with_builtins!(baml_builtins_macros::define_builtins);

/// Get all built-in function signatures.
pub fn builtins() -> &'static [BuiltinSignature] {
    &BUILTINS
}

/// Find methods by method name.
///
/// Returns an iterator over all builtin signatures that are methods with the given name.
pub fn find_method(method_name: &str) -> impl Iterator<Item = &'static BuiltinSignature> {
    builtins()
        .iter()
        .filter(move |def| def.method_name() == Some(method_name))
}

/// Find a free function by path (functions without a receiver).
pub fn find_function(path: &str) -> Option<&'static BuiltinSignature> {
    builtins()
        .iter()
        .find(|def| def.receiver.is_none() && def.path == path)
}

/// Find any builtin by path (including methods).
///
/// This is useful for direct builtin calls like `baml.Array.length(arr)`.
pub fn find_builtin_by_path(path: &str) -> Option<&'static BuiltinSignature> {
    builtins().iter().find(|def| def.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_name() {
        let array_len = builtins()
            .iter()
            .find(|d| d.path == "baml.Array.length")
            .unwrap();
        assert_eq!(array_len.method_name(), Some("length"));

        let env_get = builtins().iter().find(|d| d.path == "env.get").unwrap();
        assert_eq!(env_get.method_name(), None);
    }

    #[test]
    fn test_arity() {
        let array_len = builtins()
            .iter()
            .find(|d| d.path == "baml.Array.length")
            .unwrap();
        assert_eq!(array_len.arity(), 1); // self only

        let array_push = builtins()
            .iter()
            .find(|d| d.path == "baml.Array.push")
            .unwrap();
        assert_eq!(array_push.arity(), 2); // self + item

        let env_get = builtins().iter().find(|d| d.path == "env.get").unwrap();
        assert_eq!(env_get.arity(), 1); // key only
    }

    #[test]
    fn test_find_method() {
        let methods: Vec<_> = find_method("length").collect();
        assert!(methods.len() >= 2); // Array.length and String.length at minimum
    }

    #[test]
    fn test_find_function() {
        let env_get = find_function("env.get");
        assert!(env_get.is_some());
        assert_eq!(env_get.unwrap().path, "env.get");
    }

    #[test]
    fn test_find_builtin_by_path() {
        assert!(find_builtin_by_path("baml.Array.length").is_some());
        assert!(find_builtin_by_path("env.get").is_some());
        assert!(find_builtin_by_path("nonexistent").is_none());
    }

    #[test]
    fn test_path_constants() {
        // Verify path constants are generated correctly
        assert_eq!(paths::BAML_ARRAY_LENGTH, "baml.Array.length");
        assert_eq!(paths::BAML_STRING_TO_LOWER_CASE, "baml.String.toLowerCase");
        assert_eq!(paths::ENV_GET, "env.get");
        assert_eq!(paths::BAML_DEEP_COPY, "baml.deep_copy");
        assert_eq!(paths::BAML_UNSTABLE_STRING, "baml.unstable.string");
    }
}
