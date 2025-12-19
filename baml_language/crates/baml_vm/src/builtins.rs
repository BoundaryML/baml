//! Built-in functions and their type signatures.
//!
//! This is the single source of truth for all built-in functions.
//! Both type checking and codegen use this.

use std::sync::LazyLock;

use crate::native::{self, NativeFunction};

/// Type pattern for matching/constructing types with type variables.
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

/// A built-in function definition.
///
/// This contains everything needed for both type checking and VM execution.
pub struct FunctionDef {
    /// Full path, e.g., "baml.Array.length" or "env.get".
    pub path: &'static str,

    /// If Some, this is a method callable as `receiver.method_name()`.
    /// If None, this is a free function callable as `path()`.
    pub receiver: Option<TypePattern>,

    /// Parameters (excluding self for methods).
    pub params: Vec<(&'static str, TypePattern)>,

    /// Return type.
    pub returns: TypePattern,

    /// Native function implementation.
    pub native_fn: NativeFunction,
}

impl FunctionDef {
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

/// All built-in functions.
///
/// Access via `builtins()` function.
static BUILTINS: LazyLock<Vec<FunctionDef>> = LazyLock::new(|| {
    vec![
        // =====================================================================
        // Array methods
        // =====================================================================
        FunctionDef {
            path: "baml.Array.length",
            receiver: Some(TypePattern::Array(Box::new(TypePattern::Var("T")))),
            params: vec![],
            returns: TypePattern::Int,
            native_fn: native::array_len,
        },
        FunctionDef {
            path: "baml.Array.push",
            receiver: Some(TypePattern::Array(Box::new(TypePattern::Var("T")))),
            params: vec![("item", TypePattern::Var("T"))],
            returns: TypePattern::Null,
            native_fn: native::array_push,
        },
        // =====================================================================
        // String methods
        // =====================================================================
        FunctionDef {
            path: "baml.String.length",
            receiver: Some(TypePattern::String),
            params: vec![],
            returns: TypePattern::Int,
            native_fn: native::string_len,
        },
        FunctionDef {
            path: "baml.String.toLowerCase",
            receiver: Some(TypePattern::String),
            params: vec![],
            returns: TypePattern::String,
            native_fn: native::string_to_lower_case,
        },
        FunctionDef {
            path: "baml.String.toUpperCase",
            receiver: Some(TypePattern::String),
            params: vec![],
            returns: TypePattern::String,
            native_fn: native::string_to_upper_case,
        },
        FunctionDef {
            path: "baml.String.trim",
            receiver: Some(TypePattern::String),
            params: vec![],
            returns: TypePattern::String,
            native_fn: native::string_trim,
        },
        FunctionDef {
            path: "baml.String.includes",
            receiver: Some(TypePattern::String),
            params: vec![("search", TypePattern::String)],
            returns: TypePattern::Bool,
            native_fn: native::string_includes,
        },
        FunctionDef {
            path: "baml.String.startsWith",
            receiver: Some(TypePattern::String),
            params: vec![("prefix", TypePattern::String)],
            returns: TypePattern::Bool,
            native_fn: native::string_starts_with,
        },
        FunctionDef {
            path: "baml.String.endsWith",
            receiver: Some(TypePattern::String),
            params: vec![("suffix", TypePattern::String)],
            returns: TypePattern::Bool,
            native_fn: native::string_ends_with,
        },
        FunctionDef {
            path: "baml.String.split",
            receiver: Some(TypePattern::String),
            params: vec![("delimiter", TypePattern::String)],
            returns: TypePattern::Array(Box::new(TypePattern::String)),
            native_fn: native::string_split,
        },
        FunctionDef {
            path: "baml.String.substring",
            receiver: Some(TypePattern::String),
            params: vec![("start", TypePattern::Int), ("end", TypePattern::Int)],
            returns: TypePattern::String,
            native_fn: native::string_substring,
        },
        FunctionDef {
            path: "baml.String.replace",
            receiver: Some(TypePattern::String),
            params: vec![
                ("search", TypePattern::String),
                ("replacement", TypePattern::String),
            ],
            returns: TypePattern::String,
            native_fn: native::string_replace,
        },
        // =====================================================================
        // Map methods
        // =====================================================================
        FunctionDef {
            path: "baml.Map.length",
            receiver: Some(TypePattern::Map {
                key: Box::new(TypePattern::Var("K")),
                value: Box::new(TypePattern::Var("V")),
            }),
            params: vec![],
            returns: TypePattern::Int,
            native_fn: native::map_len,
        },
        FunctionDef {
            path: "baml.Map.has",
            receiver: Some(TypePattern::Map {
                key: Box::new(TypePattern::String),
                value: Box::new(TypePattern::Var("V")),
            }),
            params: vec![("key", TypePattern::String)],
            returns: TypePattern::Bool,
            native_fn: native::map_has,
        },
        // =====================================================================
        // Free functions
        // =====================================================================
        FunctionDef {
            path: "env.get",
            receiver: None,
            params: vec![("key", TypePattern::String)],
            returns: TypePattern::String,
            native_fn: native::env_get,
        },
        FunctionDef {
            path: "baml.deep_copy",
            receiver: None,
            params: vec![("value", TypePattern::Var("T"))],
            returns: TypePattern::Var("T"),
            native_fn: native::deep_copy_object,
        },
        FunctionDef {
            path: "baml.deep_equals",
            receiver: None,
            params: vec![("a", TypePattern::Var("T")), ("b", TypePattern::Var("T"))],
            returns: TypePattern::Bool,
            native_fn: native::deep_equals,
        },
        FunctionDef {
            path: "baml.unstable.string",
            receiver: None,
            params: vec![("value", TypePattern::Var("T"))],
            returns: TypePattern::String,
            native_fn: native::any_value_to_string,
        },
    ]
});

/// Get all built-in function definitions.
pub fn builtins() -> &'static [FunctionDef] {
    &BUILTINS
}

/// Find a method by receiver type pattern match and method name.
pub fn find_method(method_name: &str) -> impl Iterator<Item = &'static FunctionDef> {
    builtins()
        .iter()
        .filter(move |def| def.method_name() == Some(method_name))
}

/// Find a free function by path (functions without a receiver).
pub fn find_function(path: &str) -> Option<&'static FunctionDef> {
    builtins()
        .iter()
        .find(|def| def.receiver.is_none() && def.path == path)
}

/// Find any builtin by path (including methods).
///
/// This is useful for direct builtin calls like `baml.Array.length(arr)`.
pub fn find_builtin_by_path(path: &str) -> Option<&'static FunctionDef> {
    builtins().iter().find(|def| def.path == path)
}

/// Generate the functions map for VM registration.
pub fn functions() -> indexmap::IndexMap<String, (NativeFunction, usize)> {
    builtins()
        .iter()
        .map(|def| (def.path.to_string(), (def.native_fn, def.arity())))
        .collect()
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
    fn test_functions_map() {
        let fns = functions();
        assert!(fns.contains_key("baml.Array.length"));
        assert!(fns.contains_key("env.get"));
        assert_eq!(fns.get("baml.Array.length").unwrap().1, 1);
    }
}
