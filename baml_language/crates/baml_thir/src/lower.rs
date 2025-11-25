//! Lower HIR TypeRef to THIR Ty.
//!
//! This module converts syntactic type references (TypeRef) from HIR
//! into semantic types (Ty) in THIR. This involves:
//! - Resolving named types to their definitions (classes, enums)
//! - Converting type constructors (Optional, List, Union)
//! - Handling primitive type names

use baml_base::Name;
use baml_hir::TypeRef;

use crate::Ty;

/// Lower a TypeRef to a Ty.
///
/// This function converts syntactic type references into semantic types.
/// Named types are resolved to their definitions where possible.
pub fn lower_type_ref(db: &dyn salsa::Database, type_ref: &TypeRef) -> Ty {
    let mut lowering = TyLowering::new(db);
    lowering.lower(type_ref)
}

/// Type lowering context.
pub(crate) struct TyLowering<'db> {
    #[allow(dead_code)]
    db: &'db dyn salsa::Database,
}

impl<'db> TyLowering<'db> {
    pub(crate) fn new(db: &'db dyn salsa::Database) -> Self {
        Self { db }
    }

    /// Lower a TypeRef to a Ty.
    pub(crate) fn lower(&mut self, type_ref: &TypeRef) -> Ty {
        match type_ref {
            TypeRef::Named(name) => self.lower_named_type(name),

            TypeRef::Optional(inner) => {
                let inner_ty = self.lower(inner);
                // Optional<T> is represented as T | null
                Ty::Union(vec![inner_ty, Ty::Null])
            }

            TypeRef::List(inner) => {
                let inner_ty = self.lower(inner);
                Ty::List(Box::new(inner_ty))
            }

            TypeRef::Union(types) => {
                let tys: Vec<Ty> = types.iter().map(|t| self.lower(t)).collect();
                normalize_union(tys)
            }

            TypeRef::Unknown => Ty::Unknown,
        }
    }

    /// Lower a named type reference.
    fn lower_named_type(&self, name: &Name) -> Ty {
        // Check for primitive types first
        match name.as_str() {
            "int" | "Int" => Ty::Int,
            "float" | "Float" => Ty::Float,
            "string" | "String" => Ty::String,
            "bool" | "Bool" | "boolean" | "Boolean" => Ty::Bool,
            "null" | "Null" | "None" => Ty::Null,
            "image" | "Image" => Ty::Image,
            "audio" | "Audio" => Ty::Audio,
            "video" | "Video" => Ty::Video,
            "pdf" | "Pdf" | "PDF" => Ty::Pdf,
            _ => {
                // Try to resolve as a user-defined type
                // For now, we don't have access to the full project context,
                // so we'll create a placeholder class type
                // TODO: Implement proper name resolution once we have project-wide queries
                self.resolve_user_type(name)
            }
        }
    }

    /// Resolve a user-defined type by name.
    fn resolve_user_type(&self, _name: &Name) -> Ty {
        // For now, we return Unknown since we don't have project-wide name resolution yet.
        // In a full implementation, this would:
        // 1. Look up the name in the current file's items
        // 2. Look up the name in imported modules
        // 3. Return Ty::Class(ClassId) or Ty::Enum(EnumId) if found
        // 4. Return Ty::Error and emit a diagnostic if not found

        // We could potentially use a ClassId/EnumId with a synthetic file ID,
        // but for now, Unknown is safer
        Ty::Unknown
    }
}

/// Normalize a union type by flattening nested unions and removing duplicates.
fn normalize_union(types: Vec<Ty>) -> Ty {
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
        let result = normalize_union(vec![]);
        assert_eq!(result, Ty::Unknown);
    }

    #[test]
    fn test_normalize_union_single() {
        let result = normalize_union(vec![Ty::Int]);
        assert_eq!(result, Ty::Int);
    }

    #[test]
    fn test_normalize_union_removes_duplicates() {
        let result = normalize_union(vec![Ty::Int, Ty::String, Ty::Int]);
        assert_eq!(result, Ty::Union(vec![Ty::Int, Ty::String]));
    }

    #[test]
    fn test_normalize_union_flattens() {
        let inner = Ty::Union(vec![Ty::Int, Ty::Float]);
        let result = normalize_union(vec![inner, Ty::String]);
        assert_eq!(result, Ty::Union(vec![Ty::Int, Ty::Float, Ty::String]));
    }
}
