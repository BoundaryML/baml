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

use std::sync::LazyLock;

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
    /// Type variable - binds to actual type during pattern matching.
    /// E.g., `Var("T")` in `Array<T>.push(item: T)` binds to the element type.
    Var(&'static str),
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

/// Macro to define builtins with path constants.
///
/// This ensures path strings are defined exactly once and generates:
/// 1. A `pub mod paths` with constants like `ARRAY_LENGTH`, `STRING_TRIM`, etc.
/// 2. The `BUILTINS` static with all signatures using those constants.
/// 3. A `for_all_builtins!` macro that can be used to iterate over all builtin names.
macro_rules! define_builtins {
    (
        $(
            $const_name:ident = $path:literal {
                receiver: $receiver:expr,
                params: [$($param:expr),* $(,)?],
                returns: $returns:expr $(,)?
            }
        ),* $(,)?
    ) => {
        /// Path constants for all builtins.
        ///
        /// Use these constants instead of raw strings to avoid typos.
        pub mod paths {
            $(
                pub const $const_name: &str = $path;
            )*

            /// All builtin paths as a slice.
            pub const ALL: &[&str] = &[$($path),*];
        }

        /// Invoke a macro with all builtin constant names.
        ///
        /// Usage:
        /// ```ignore
        /// baml_builtins::for_all_builtins!(my_macro);
        /// // Expands to: my_macro!(ARRAY_LENGTH, ARRAY_PUSH, STRING_LENGTH, ...);
        /// ```
        #[macro_export]
        macro_rules! for_all_builtins {
            ($callback:ident) => {
                $callback!($($const_name),*)
            };
        }

        /// All built-in function signatures.
        static BUILTINS: LazyLock<Vec<BuiltinSignature>> = LazyLock::new(|| {
            vec![
                $(
                    BuiltinSignature {
                        path: paths::$const_name,
                        receiver: $receiver,
                        params: vec![$($param),*],
                        returns: $returns,
                    },
                )*
            ]
        });
    };
}

define_builtins! {
    // =========================================================================
    // Array methods
    // =========================================================================
    ARRAY_LENGTH = "baml.Array.length" {
        receiver: Some(TypePattern::Array(Box::new(TypePattern::Var("T")))),
        params: [],
        returns: TypePattern::Int,
    },
    ARRAY_PUSH = "baml.Array.push" {
        receiver: Some(TypePattern::Array(Box::new(TypePattern::Var("T")))),
        params: [("item", TypePattern::Var("T"))],
        returns: TypePattern::Null,
    },

    // =========================================================================
    // String methods
    // =========================================================================
    STRING_LENGTH = "baml.String.length" {
        receiver: Some(TypePattern::String),
        params: [],
        returns: TypePattern::Int,
    },
    STRING_TO_LOWER_CASE = "baml.String.toLowerCase" {
        receiver: Some(TypePattern::String),
        params: [],
        returns: TypePattern::String,
    },
    STRING_TO_UPPER_CASE = "baml.String.toUpperCase" {
        receiver: Some(TypePattern::String),
        params: [],
        returns: TypePattern::String,
    },
    STRING_TRIM = "baml.String.trim" {
        receiver: Some(TypePattern::String),
        params: [],
        returns: TypePattern::String,
    },
    STRING_INCLUDES = "baml.String.includes" {
        receiver: Some(TypePattern::String),
        params: [("search", TypePattern::String)],
        returns: TypePattern::Bool,
    },
    STRING_STARTS_WITH = "baml.String.startsWith" {
        receiver: Some(TypePattern::String),
        params: [("prefix", TypePattern::String)],
        returns: TypePattern::Bool,
    },
    STRING_ENDS_WITH = "baml.String.endsWith" {
        receiver: Some(TypePattern::String),
        params: [("suffix", TypePattern::String)],
        returns: TypePattern::Bool,
    },
    STRING_SPLIT = "baml.String.split" {
        receiver: Some(TypePattern::String),
        params: [("delimiter", TypePattern::String)],
        returns: TypePattern::Array(Box::new(TypePattern::String)),
    },
    STRING_SUBSTRING = "baml.String.substring" {
        receiver: Some(TypePattern::String),
        params: [("start", TypePattern::Int), ("end", TypePattern::Int)],
        returns: TypePattern::String,
    },
    STRING_REPLACE = "baml.String.replace" {
        receiver: Some(TypePattern::String),
        params: [("search", TypePattern::String), ("replacement", TypePattern::String)],
        returns: TypePattern::String,
    },

    // =========================================================================
    // Map methods
    // =========================================================================
    MAP_LENGTH = "baml.Map.length" {
        receiver: Some(TypePattern::Map {
            key: Box::new(TypePattern::Var("K")),
            value: Box::new(TypePattern::Var("V")),
        }),
        params: [],
        returns: TypePattern::Int,
    },
    MAP_HAS = "baml.Map.has" {
        receiver: Some(TypePattern::Map {
            key: Box::new(TypePattern::String),
            value: Box::new(TypePattern::Var("V")),
        }),
        params: [("key", TypePattern::String)],
        returns: TypePattern::Bool,
    },

    // =========================================================================
    // Free functions
    // =========================================================================
    ENV_GET = "env.get" {
        receiver: None,
        params: [("key", TypePattern::String)],
        returns: TypePattern::String,
    },
    DEEP_COPY = "baml.deep_copy" {
        receiver: None,
        params: [("value", TypePattern::Var("T"))],
        returns: TypePattern::Var("T"),
    },
    DEEP_EQUALS = "baml.deep_equals" {
        receiver: None,
        params: [("a", TypePattern::Var("T")), ("b", TypePattern::Var("T"))],
        returns: TypePattern::Bool,
    },
    UNSTABLE_STRING = "baml.unstable.string" {
        receiver: None,
        params: [("value", TypePattern::Var("T"))],
        returns: TypePattern::String,
    },
}

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
}
