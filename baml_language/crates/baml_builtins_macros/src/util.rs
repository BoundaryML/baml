//! Shared utility functions for case conversion, type inspection, and
//! type-to-`TypePattern` mapping.

use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{GenericArgument, Ident, PathArguments, ReturnType, Type};

use crate::parse::DslType;

/// Convert an identifier from camelCase/PascalCase to `SCREAMING_SNAKE_CASE`.
pub(crate) fn to_screaming_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_uppercase());
    }
    result
}

/// Convert an identifier from camelCase/PascalCase to `snake_case`.
pub(crate) fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

/// Convert a `snake_case` identifier to `PascalCase`.
///
/// E.g., `"baml_fs_open"` → `"BamlFsOpen"`.
pub(crate) fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.collect::<String>()
                }
            }
        })
        .collect()
}

/// Derive the Rust struct identifier from a builtin DSL path.
///
/// Strips the `baml.` prefix, `PascalCases` each remaining segment, and joins.
/// E.g., `"baml.http.Response"` → `HttpResponse`, `"baml.fs.File"` → `FsFile`.
pub(crate) fn path_to_rust_ident(path: &str) -> Ident {
    let without_baml = path
        .strip_prefix("baml.")
        .unwrap_or_else(|| panic!("builtin path '{path}' should start with 'baml.'"));
    let ident_str: String = without_baml
        .split('.')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(c) => {
                    let mut s = c.to_uppercase().to_string();
                    s.extend(chars);
                    s
                }
                None => String::new(),
            }
        })
        .collect();
    format_ident!("{}", ident_str)
}

/// Convert a Rust type to a `TypePattern` token stream.
pub(crate) fn type_to_pattern(
    ty: &Type,
    generic_params: &[String],
    builtin_types: &HashMap<String, String>,
    builtin_enums: &HashMap<String, String>,
) -> TokenStream2 {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident = &segment.ident;
            let ident_str = ident.to_string();

            // Check if it's a generic type parameter
            if generic_params.contains(&ident_str) {
                let lit = syn::LitStr::new(&ident_str, ident.span());
                return quote!(TypePattern::Var(#lit));
            }

            match ident_str.as_str() {
                "String" => quote!(TypePattern::String),
                "i64" => quote!(TypePattern::Int),
                "f64" => quote!(TypePattern::Float),
                "bool" => quote!(TypePattern::Bool),
                "Media" => quote!(TypePattern::Media),
                "ResourceHandle" => quote!(TypePattern::Resource),
                "Unknown" => quote!(TypePattern::BuiltinUnknown),
                "Type" => quote!(TypePattern::Type),
                "Option" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let inner_pattern = type_to_pattern(
                                inner,
                                generic_params,
                                builtin_types,
                                builtin_enums,
                            );
                            return quote!(TypePattern::Optional(Box::new(#inner_pattern)));
                        }
                    }
                    quote!(TypePattern::Optional(Box::new(TypePattern::Null)))
                }
                "Array" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let inner_pattern = type_to_pattern(
                                inner,
                                generic_params,
                                builtin_types,
                                builtin_enums,
                            );
                            return quote!(TypePattern::Array(Box::new(#inner_pattern)));
                        }
                    }
                    quote!(TypePattern::Array(Box::new(TypePattern::Null)))
                }
                "Map" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        let mut iter = args.args.iter();
                        let key = iter
                            .next()
                            .and_then(|a| {
                                if let GenericArgument::Type(t) = a {
                                    Some(t)
                                } else {
                                    None
                                }
                            })
                            .map(|t| {
                                type_to_pattern(t, generic_params, builtin_types, builtin_enums)
                            })
                            .unwrap_or_else(|| quote!(TypePattern::String));
                        let value = iter
                            .next()
                            .and_then(|a| {
                                if let GenericArgument::Type(t) = a {
                                    Some(t)
                                } else {
                                    None
                                }
                            })
                            .map(|t| {
                                type_to_pattern(t, generic_params, builtin_types, builtin_enums)
                            })
                            .unwrap_or_else(|| quote!(TypePattern::Null));
                        return quote!(TypePattern::Map {
                            key: Box::new(#key),
                            value: Box::new(#value),
                        });
                    }
                    quote!(TypePattern::Map {
                        key: Box::new(TypePattern::String),
                        value: Box::new(TypePattern::Null),
                    })
                }
                _ => {
                    // Check if it's a builtin enum
                    if let Some(full_path) = builtin_enums.get(&ident_str) {
                        return quote!(TypePattern::Enum(#full_path));
                    }
                    // Check if it's a builtin type (struct)
                    if let Some(full_path) = builtin_types.get(&ident_str) {
                        return quote!(TypePattern::Builtin(#full_path));
                    }
                    // Unknown type - treat as a type variable
                    let lit = syn::LitStr::new(&ident_str, ident.span());
                    quote!(TypePattern::Var(#lit))
                }
            }
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => {
            quote!(TypePattern::Null)
        }
        Type::BareFn(fn_ty) => {
            let params: Vec<TokenStream2> = fn_ty
                .inputs
                .iter()
                .map(|arg| type_to_pattern(&arg.ty, generic_params, builtin_types, builtin_enums))
                .collect();
            let ret = match &fn_ty.output {
                ReturnType::Default => quote!(TypePattern::Null),
                ReturnType::Type(_, ty) => {
                    type_to_pattern(ty, generic_params, builtin_types, builtin_enums)
                }
            };
            quote!(TypePattern::Function {
                params: vec![#(#params),*],
                ret: Box::new(#ret),
            })
        }
        _ => {
            quote!(TypePattern::Null)
        }
    }
}

/// Get the simple type name from a `Type` (for native fn generation).
pub(crate) fn type_to_simple_name(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident_str = segment.ident.to_string();

            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                let inner_types: Vec<String> = args
                    .args
                    .iter()
                    .filter_map(|arg| {
                        if let GenericArgument::Type(t) = arg {
                            Some(type_to_simple_name(t))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !inner_types.is_empty() {
                    return format!("{}<{}>", ident_str, inner_types.join(", "));
                }
            }
            ident_str
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => "()".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Check if a type is a generic type parameter.
pub(crate) fn is_generic_type(ty: &Type, generic_params: &[String]) -> bool {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident_str = segment.ident.to_string();
            generic_params.contains(&ident_str)
        }
        _ => false,
    }
}

/// Check if a type is `Result<T>` and return the inner type if so.
///
/// Returns (`inner_type`, `is_result`) where `inner_type` is `T` from `Result<T>`
/// or the original type.
pub(crate) fn unwrap_result_type(ty: &Type) -> (&Type, bool) {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last().unwrap();
        if segment.ident == "Result" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return (inner, true);
                }
            }
        }
    }
    (ty, false)
}

// ============================================================================
// DslType-aware wrappers
// ============================================================================

/// Convert a `DslType` to a `TypePattern` token stream.
pub(crate) fn dsl_type_to_pattern(
    dsl_ty: &DslType,
    generic_params: &[String],
    builtin_types: &HashMap<String, String>,
    builtin_enums: &HashMap<String, String>,
) -> TokenStream2 {
    match dsl_ty {
        DslType::Syn(ty) => type_to_pattern(ty, generic_params, builtin_types, builtin_enums),
        DslType::StringLiteral(s) => {
            quote!(TypePattern::StringLiteral(#s))
        }
        DslType::Union(variants) => {
            let patterns: Vec<TokenStream2> = variants
                .iter()
                .map(|v| dsl_type_to_pattern(v, generic_params, builtin_types, builtin_enums))
                .collect();
            quote!(TypePattern::Union(vec![#(#patterns),*]))
        }
    }
}

/// Get the simple type name from a `DslType` (for native fn generation).
pub(crate) fn dsl_type_to_simple_name(dsl_ty: &DslType) -> String {
    match dsl_ty {
        DslType::Syn(ty) => type_to_simple_name(ty),
        DslType::StringLiteral(_) => "String".to_string(),
        DslType::Union(variants) => {
            if variants
                .iter()
                .all(|v| matches!(v, DslType::StringLiteral(_)))
            {
                "String".to_string()
            } else {
                dsl_type_to_simple_name(variants.first().expect("empty union"))
            }
        }
    }
}

/// Check if a `DslType` is a generic type parameter.
pub(crate) fn dsl_is_generic_type(dsl_ty: &DslType, generic_params: &[String]) -> bool {
    match dsl_ty {
        DslType::Syn(ty) => is_generic_type(ty, generic_params),
        DslType::StringLiteral(_) | DslType::Union(_) => false,
    }
}

/// Check if a `DslType` is `Result<T>` and return the inner type if so.
///
/// String literal and union types are never `Result`.
/// Returns owned values since the inner type may need wrapping.
pub(crate) fn dsl_unwrap_result_type(dsl_ty: &DslType) -> (DslType, bool) {
    match dsl_ty {
        DslType::Syn(ty) => {
            let (inner, is_result) = unwrap_result_type(ty);
            if is_result {
                (DslType::Syn(inner.clone()), true)
            } else {
                (dsl_ty.clone(), false)
            }
        }
        DslType::StringLiteral(_) | DslType::Union(_) => (dsl_ty.clone(), false),
    }
}
