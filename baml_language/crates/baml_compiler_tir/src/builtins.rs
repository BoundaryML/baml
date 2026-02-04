//! Built-in function type checking.
//!
//! This module provides utilities for matching built-in function signatures
//! against concrete types and substituting type variables.

use std::collections::HashMap;

use baml_base::{Name, baml_debug};
use baml_builtins::{BuiltinSignature, TypePattern};
use baml_compiler_hir::QualifiedName;

use crate::Ty;

/// Parse a builtin path string like "baml.http.Response" into an FQN.
///
/// The path is expected to start with "baml." and have at least one segment after.
pub fn parse_builtin_path(path: &str) -> QualifiedName {
    assert!(
        path.starts_with("baml."),
        "builtin path must start with 'baml.'"
    );
    // Strip "baml." prefix if present
    let without_prefix = path.strip_prefix("baml.").unwrap_or(path);
    let segments: Vec<&str> = without_prefix.split('.').collect();

    assert!(
        !segments.is_empty(),
        "builtin path must have at least one segment"
    );

    if segments.len() == 1 {
        // Just a name with no path, e.g., "Array"
        QualifiedName::builtin_primitive(Name::new(segments[0]))
    } else {
        // Multiple segments: all but last are path, last is name
        let (path_segments, name) = segments.split_at(segments.len() - 1);
        let path: Vec<Name> = path_segments.iter().map(|s| Name::new(*s)).collect();
        QualifiedName::builtin(path, Name::new(name[0]))
    }
}

/// Type variable bindings from pattern matching.
///
/// Maps type variable names (e.g., "T", "K", "V") to their bound types.
pub type Bindings = HashMap<&'static str, Ty>;

/// Match a `TypePattern` against a concrete `Ty`, extracting type variable bindings.
///
/// Returns `Some(bindings)` if the pattern matches, `None` otherwise.
///
/// # Examples
///
/// ```ignore
/// // Pattern: Array<T>
/// // Concrete: int[]
/// // Result: Some({"T" => int})
///
/// let pattern = TypePattern::Array(Box::new(TypePattern::Var("T")));
/// let concrete = Ty::List(Box::new(Ty::Int));
/// let bindings = match_pattern(&pattern, &concrete);
/// assert_eq!(bindings.unwrap().get("T"), Some(&Ty::Int));
/// ```
pub fn match_pattern(pattern: &TypePattern, ty: &Ty) -> Option<Bindings> {
    let mut bindings = HashMap::new();
    if match_pattern_inner(pattern, ty, &mut bindings) {
        Some(bindings)
    } else {
        None
    }
}

fn match_pattern_inner(pattern: &TypePattern, ty: &Ty, bindings: &mut Bindings) -> bool {
    match (pattern, ty) {
        // Type variable: bind or check consistency
        (TypePattern::Var(name), ty) => {
            if let Some(existing) = bindings.get(name) {
                // Already bound - check it matches
                existing == ty
            } else {
                // Bind the variable
                bindings.insert(name, ty.clone());
                true
            }
        }

        // Primitive types must match exactly
        (TypePattern::Int, Ty::Int) => true,
        (TypePattern::Float, Ty::Float) => true,
        (TypePattern::String, Ty::String) => true,
        (TypePattern::Bool, Ty::Bool) => true,
        (TypePattern::Null, Ty::Null) => true,

        // Array/List: match element type
        (TypePattern::Array(elem_pat), Ty::List(elem_ty)) => {
            match_pattern_inner(elem_pat, elem_ty, bindings)
        }

        // Map: match key and value types
        (
            TypePattern::Map {
                key: key_pat,
                value: value_pat,
            },
            Ty::Map {
                key: key_ty,
                value: value_ty,
            },
        ) => {
            match_pattern_inner(key_pat, key_ty, bindings)
                && match_pattern_inner(value_pat, value_ty, bindings)
        }
        (TypePattern::Media, Ty::Media(_)) => true,

        // Builtin types match exactly by path - they are represented as Ty::Class now
        // Use full display path (e.g., "baml.fs.File") for comparison
        (TypePattern::Builtin(pattern_path), Ty::Class(fqn)) => *pattern_path == fqn.display(),

        // Unknown in Ty matches any pattern (for error recovery)
        (_, Ty::Unknown) => true,

        // BuiltinUnknown accepts any type (for builtins that need heterogeneous values)
        (TypePattern::BuiltinUnknown, _) => true,

        // No match
        _ => false,
    }
}

/// Substitute type variable bindings into a `TypePattern` to get a concrete `Ty`.
///
/// # Panics
///
/// Panics if a type variable in the pattern has no binding. This should not happen
/// if `match_pattern` succeeded for the receiver type.
///
/// # Examples
///
/// ```ignore
/// // Pattern: Var("T")
/// // Bindings: {"T" => int}
/// // Result: int
///
/// let pattern = TypePattern::Var("T");
/// let mut bindings = HashMap::new();
/// bindings.insert("T", Ty::Int);
/// let result = substitute(&pattern, &bindings);
/// assert_eq!(result, Ty::Int);
/// ```
pub fn substitute(pattern: &TypePattern, bindings: &Bindings) -> Ty {
    match pattern {
        TypePattern::Var(name) => bindings
            .get(name)
            .cloned()
            // Fall back to Unknown for unbound type variables (e.g., "Any")
            .unwrap_or(Ty::Unknown),

        TypePattern::Int => Ty::Int,
        TypePattern::Float => Ty::Float,
        TypePattern::String => Ty::String,
        TypePattern::Bool => Ty::Bool,
        TypePattern::Null => Ty::Null,

        TypePattern::Array(elem) => Ty::List(Box::new(substitute(elem, bindings))),

        TypePattern::Map { key, value } => Ty::Map {
            key: Box::new(substitute(key, bindings)),
            value: Box::new(substitute(value, bindings)),
        },
        TypePattern::Media => Ty::Media(baml_base::MediaKind::Generic),
        TypePattern::Optional(inner) => Ty::Optional(Box::new(substitute(inner, bindings))),
        TypePattern::Builtin(path) => Ty::Class(parse_builtin_path(path)),
        TypePattern::Function { params, ret } => Ty::Function {
            params: params.iter().map(|p| substitute(p, bindings)).collect(),
            ret: Box::new(substitute(ret, bindings)),
        },
        TypePattern::BuiltinUnknown => Ty::BuiltinUnknown,
    }
}

/// Substitute type patterns to types, using `Ty::Unknown` for unbound type variables.
///
/// This is useful for builtin function calls where we don't have concrete type bindings
/// (e.g., `baml.Array.length(arr)` where we don't know the element type yet).
pub fn substitute_unknown(pattern: &TypePattern) -> Ty {
    match pattern {
        TypePattern::Var(_) => Ty::Unknown,
        TypePattern::Int => Ty::Int,
        TypePattern::Float => Ty::Float,
        TypePattern::String => Ty::String,
        TypePattern::Bool => Ty::Bool,
        TypePattern::Null => Ty::Null,
        TypePattern::Array(elem) => Ty::List(Box::new(substitute_unknown(elem))),
        TypePattern::Map { key, value } => Ty::Map {
            key: Box::new(substitute_unknown(key)),
            value: Box::new(substitute_unknown(value)),
        },
        TypePattern::Media => Ty::Media(baml_base::MediaKind::Generic),
        TypePattern::Optional(inner) => Ty::Optional(Box::new(substitute_unknown(inner))),
        TypePattern::Builtin(path) => Ty::Class(parse_builtin_path(path)),
        TypePattern::Function { params, ret } => Ty::Function {
            params: params.iter().map(substitute_unknown).collect(),
            ret: Box::new(substitute_unknown(ret)),
        },
        TypePattern::BuiltinUnknown => Ty::BuiltinUnknown,
    }
}

/// Find a matching built-in method for a receiver type and method name.
///
/// Returns the `BuiltinSignature` and type variable bindings if found.
///
/// # Examples
///
/// ```ignore
/// // Looking up: arr.length() where arr: int[]
/// let result = lookup_method(&Ty::List(Box::new(Ty::Int)), "length");
/// // result = Some((BuiltinSignature for Array.length, {"T" => int}))
/// // Return type: substitute(&def.returns, &bindings) => Ty::Int
/// ```
pub fn lookup_method(
    receiver_ty: &Ty,
    method_name: &str,
) -> Option<(&'static BuiltinSignature, Bindings)> {
    baml_debug!("Looking up method: {:?}.{}", receiver_ty, method_name);

    for def in baml_builtins::find_method(method_name) {
        if let Some(ref receiver_pattern) = def.receiver {
            baml_debug!("  Trying receiver pattern: {:?}", receiver_pattern);
            if let Some(bindings) = match_pattern(receiver_pattern, receiver_ty) {
                baml_debug!("  Found method: {} with bindings {:?}", def.path, bindings);
                return Some((def, bindings));
            }
        }
    }

    baml_debug!("  No method found for {:?}.{}", receiver_ty, method_name);
    None
}

/// Look up a built-in free function by path (functions without a receiver).
///
/// Returns the `BuiltinSignature` if found.
pub fn lookup_function(path: &str) -> Option<&'static BuiltinSignature> {
    baml_builtins::find_function(path)
}

/// Look up any built-in by path (including methods).
///
/// This is useful for direct builtin calls like `baml.Array.length(arr)`.
pub fn lookup_builtin_by_path(path: &str) -> Option<&'static BuiltinSignature> {
    baml_builtins::find_builtin_by_path(path)
}

/// Get the return type of a built-in method for a specific receiver type.
///
/// This combines `lookup_method`, pattern matching, and substitution.
///
/// # Examples
///
/// ```ignore
/// // arr.length() where arr: int[] => int
/// let return_ty = method_return_type(&Ty::List(Box::new(Ty::Int)), "length");
/// assert_eq!(return_ty, Some(Ty::Int));
///
/// // arr.push(x) where arr: int[] => null (but validates x: int)
/// let return_ty = method_return_type(&Ty::List(Box::new(Ty::Int)), "push");
/// assert_eq!(return_ty, Some(Ty::Null));
/// ```
pub fn method_return_type(receiver_ty: &Ty, method_name: &str) -> Option<Ty> {
    let (def, bindings) = lookup_method(receiver_ty, method_name)?;
    Some(substitute(&def.returns, &bindings))
}

/// Get the expected parameter types of a built-in method for a specific receiver type.
///
/// Returns the parameter types after substituting type variables.
pub fn method_param_types(receiver_ty: &Ty, method_name: &str) -> Option<Vec<(&'static str, Ty)>> {
    let (def, bindings) = lookup_method(receiver_ty, method_name)?;
    let params = def
        .params
        .iter()
        .map(|(name, pattern)| (*name, substitute(pattern, &bindings)))
        .collect();
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_primitive() {
        assert!(match_pattern(&TypePattern::Int, &Ty::Int).is_some());
        assert!(match_pattern(&TypePattern::String, &Ty::String).is_some());
        assert!(match_pattern(&TypePattern::Int, &Ty::String).is_none());
    }

    #[test]
    fn test_match_type_var() {
        let bindings = match_pattern(&TypePattern::Var("T"), &Ty::Int).unwrap();
        assert_eq!(bindings.get("T"), Some(&Ty::Int));
    }

    #[test]
    fn test_match_array() {
        let pattern = TypePattern::Array(Box::new(TypePattern::Var("T")));
        let ty = Ty::List(Box::new(Ty::String));

        let bindings = match_pattern(&pattern, &ty).unwrap();
        assert_eq!(bindings.get("T"), Some(&Ty::String));
    }

    #[test]
    fn test_match_map() {
        let pattern = TypePattern::Map {
            key: Box::new(TypePattern::Var("K")),
            value: Box::new(TypePattern::Var("V")),
        };
        let ty = Ty::Map {
            key: Box::new(Ty::String),
            value: Box::new(Ty::Int),
        };

        let bindings = match_pattern(&pattern, &ty).unwrap();
        assert_eq!(bindings.get("K"), Some(&Ty::String));
        assert_eq!(bindings.get("V"), Some(&Ty::Int));
    }

    #[test]
    fn test_substitute_var() {
        let mut bindings = HashMap::new();
        bindings.insert("T", Ty::Int);

        let result = substitute(&TypePattern::Var("T"), &bindings);
        assert_eq!(result, Ty::Int);
    }

    #[test]
    fn test_substitute_array() {
        let mut bindings = HashMap::new();
        bindings.insert("T", Ty::String);

        let pattern = TypePattern::Array(Box::new(TypePattern::Var("T")));
        let result = substitute(&pattern, &bindings);
        assert_eq!(result, Ty::List(Box::new(Ty::String)));
    }

    #[test]
    fn test_lookup_array_length() {
        let arr_ty = Ty::List(Box::new(Ty::Int));
        let result = lookup_method(&arr_ty, "length");

        assert!(result.is_some());
        let (def, bindings) = result.unwrap();
        assert_eq!(def.path, "baml.Array.length");
        assert_eq!(bindings.get("T"), Some(&Ty::Int));

        // Return type should be int
        let return_ty = substitute(&def.returns, &bindings);
        assert_eq!(return_ty, Ty::Int);
    }

    #[test]
    fn test_lookup_string_length() {
        let result = lookup_method(&Ty::String, "length");

        assert!(result.is_some());
        let (def, _bindings) = result.unwrap();
        assert_eq!(def.path, "baml.String.length");
    }

    #[test]
    fn test_method_return_type() {
        let arr_ty = Ty::List(Box::new(Ty::Float));

        // length returns int regardless of element type
        assert_eq!(method_return_type(&arr_ty, "length"), Some(Ty::Int));

        // push returns null
        assert_eq!(method_return_type(&arr_ty, "push"), Some(Ty::Null));
    }

    #[test]
    fn test_method_param_types() {
        let arr_ty = Ty::List(Box::new(Ty::String));

        // push takes the element type
        let params = method_param_types(&arr_ty, "push").unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], ("item", Ty::String));
    }

    #[test]
    fn test_no_such_method() {
        let arr_ty = Ty::List(Box::new(Ty::Int));
        assert!(lookup_method(&arr_ty, "nonexistent").is_none());
    }
}
