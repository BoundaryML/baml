//! Lower HIR `TypeRef` to THIR Ty.
//!
//! This module converts syntactic type references (`TypeRef`) from HIR
//! into semantic types (Ty) in THIR. This involves:
//! - Resolving named types to their definitions (classes, enums)
//! - Converting type constructors (Optional, List, Union)
//! - Handling primitive type names

use baml_hir::TypeRef;

use crate::{LiteralValue, Ty};

/// Lower a `TypeRef` to a Ty.
///
/// This function converts syntactic type references into semantic types.
/// Named types are resolved to their definitions where possible.
pub fn lower_type_ref<'db>(_db: &'db dyn baml_hir::Db, type_ref: &TypeRef) -> Ty<'db> {
    TyLowering::lower(type_ref)
}

/// Type lowering context.
// In the future, this will hold database reference for name resolution
pub(crate) struct TyLowering;

impl TyLowering {
    /// Lower a `TypeRef` to a Ty.
    pub(crate) fn lower<'db>(type_ref: &TypeRef) -> Ty<'db> {
        match type_ref {
            // Primitives
            TypeRef::Int => Ty::Int,
            TypeRef::Float => Ty::Float,
            TypeRef::String => Ty::String,
            TypeRef::Bool => Ty::Bool,
            TypeRef::Null => Ty::Null,

            // Media types
            TypeRef::Image => Ty::Image,
            TypeRef::Audio => Ty::Audio,
            TypeRef::Video => Ty::Video,
            TypeRef::Pdf => Ty::Pdf,

            // Named type via path
            TypeRef::Path(path) => TyLowering::lower_path_type(path),

            // Type constructors
            TypeRef::Optional(inner) => {
                let inner_ty = TyLowering::lower(inner);
                // Flatten nested optionals: T?? = T? since (T | null) | null = T | null
                match inner_ty {
                    Ty::Optional(_) => inner_ty, // Already optional, don't double-wrap
                    _ => Ty::Optional(Box::new(inner_ty)),
                }
            }

            TypeRef::List(inner) => {
                let inner_ty = TyLowering::lower(inner);
                Ty::List(Box::new(inner_ty))
            }

            TypeRef::Map { key, value } => {
                let key_ty = TyLowering::lower(key);
                let value_ty = TyLowering::lower(value);
                Ty::Map {
                    key: Box::new(key_ty),
                    value: Box::new(value_ty),
                }
            }

            TypeRef::Union(types) => {
                let tys: Vec<Ty<'db>> = types.iter().map(TyLowering::lower).collect();
                normalize_union(tys)
            }

            // Literal types - preserve the literal values for exhaustiveness checking
            TypeRef::StringLiteral(s) => Ty::Literal(LiteralValue::String(s.clone())),
            TypeRef::IntLiteral(i) => Ty::Literal(LiteralValue::Int(*i)),
            TypeRef::FloatLiteral(f) => Ty::Literal(LiteralValue::Float(f.clone())),
            TypeRef::BoolLiteral(b) => Ty::Literal(LiteralValue::Bool(*b)),

            // Generics - not yet supported
            TypeRef::Generic { .. } => Ty::Unknown,
            TypeRef::TypeParam(_) => Ty::Unknown,

            // Error/Unknown
            TypeRef::Error => Ty::Error,
            TypeRef::Unknown => Ty::Unknown,
        }
    }

    /// Lower a path-based type reference (named type).
    fn lower_path_type<'db>(path: &baml_hir::Path) -> Ty<'db> {
        // For simple paths (single segment), check if it's a primitive type name
        match path.segments.len() {
            1 => {
                let name = &path.segments[0];
                match name.as_str() {
                    "int" => Ty::Int,
                    "float" => Ty::Float,
                    "string" => Ty::String,
                    "bool" => Ty::Bool,
                    "null" => Ty::Null,
                    "image" => Ty::Image,
                    "audio" => Ty::Audio,
                    "video" => Ty::Video,
                    "pdf" => Ty::Pdf,
                    // User-defined type - return as Named for now
                    _ => Ty::Named(name.clone()),
                }
            }
            // For qualified paths, join them with :: and return as Named
            _ => {
                let full_path = path
                    .segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                Ty::Named(baml_base::Name::new(&full_path))
            }
        }
    }
}

/// Normalize a union type by flattening nested unions and removing duplicates.
fn normalize_union(types: Vec<Ty<'_>>) -> Ty<'_> {
    let mut normalized = Vec::new();

    for ty in types {
        match ty {
            // Flatten nested unions
            Ty::Union(inner) => {
                for inner_ty in inner {
                    if !normalized.contains(&inner_ty) {
                        normalized.push(inner_ty);
                    }
                }
            }
            // Add non-union types, avoiding duplicates
            _ => {
                if !normalized.contains(&ty) {
                    normalized.push(ty);
                }
            }
        }
    }

    // Simplify
    match normalized.len() {
        0 => Ty::Unknown, // Empty union becomes Unknown (could be Never in a more complete type system)
        1 => normalized.pop().unwrap(),
        _ => Ty::Union(normalized),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_union_empty() {
        let result: Ty<'_> = normalize_union(vec![]);
        assert_eq!(result, Ty::Unknown);
    }

    #[test]
    fn test_normalize_union_single() {
        let result: Ty<'_> = normalize_union(vec![Ty::Int]);
        assert_eq!(result, Ty::Int);
    }

    #[test]
    fn test_normalize_union_removes_duplicates() {
        let result: Ty<'_> = normalize_union(vec![Ty::Int, Ty::String, Ty::Int]);
        assert_eq!(result, Ty::Union(vec![Ty::Int, Ty::String]));
    }

    #[test]
    fn test_normalize_union_flattens() {
        let inner: Ty<'_> = Ty::Union(vec![Ty::Int, Ty::Float]);
        let result: Ty<'_> = normalize_union(vec![inner, Ty::String]);
        assert_eq!(result, Ty::Union(vec![Ty::Int, Ty::Float, Ty::String]));
    }
}
