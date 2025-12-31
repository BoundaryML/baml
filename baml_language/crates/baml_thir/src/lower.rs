//! Lower HIR `TypeRef` to THIR Ty.
//!
//! This module converts syntactic type references (`TypeRef`) from HIR
//! into semantic types (Ty) in THIR. This involves:
//! - Resolving named types to their definitions (classes, enums)
//! - Converting type constructors (Optional, List, Union)
//! - Handling primitive type names
//! - Validating that named types exist (when `known_types` is provided)

use std::collections::HashSet;

use baml_base::{Name, Span};
use baml_diagnostics::compiler_error::TypeError;
use baml_hir::TypeRef;

use crate::{LiteralValue, Ty};

/// Context for type lowering with validation.
///
/// When `known_types` is provided, unknown type names will produce errors
/// and return `Ty::Error` to suppress downstream type mismatches.
pub struct TypeLoweringContext<'a, 'db> {
    /// Set of known type names (classes, enums, type aliases).
    /// If None, no validation is performed.
    pub known_types: Option<&'a HashSet<Name>>,
    /// Span to use for error reporting. If None, a default span is used.
    pub span: Option<Span>,
    /// Accumulated errors during lowering.
    pub errors: Vec<TypeError<Ty<'db>>>,
}

impl<'a, 'db> TypeLoweringContext<'a, 'db> {
    /// Create a new context without validation.
    pub fn new() -> Self {
        TypeLoweringContext {
            known_types: None,
            span: None,
            errors: Vec::new(),
        }
    }

    /// Create a new context with type name validation.
    pub fn with_validation(known_types: &'a HashSet<Name>, span: Span) -> Self {
        TypeLoweringContext {
            known_types: Some(known_types),
            span: Some(span),
            errors: Vec::new(),
        }
    }

    /// Check if a type name is known (exists in the project).
    fn is_known_type(&self, name: &Name) -> bool {
        match &self.known_types {
            Some(known) => known.contains(name),
            None => true, // If no validation, assume all types are valid
        }
    }

    /// Record an unknown type error and return `Ty::Error`.
    fn unknown_type_error(&mut self, name: &Name) -> Ty<'db> {
        let span = self.span.unwrap_or_default();
        self.errors.push(TypeError::UnknownType {
            name: name.to_string(),
            span,
        });
        Ty::Error
    }
}

impl Default for TypeLoweringContext<'_, '_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Lower a `TypeRef` to a Ty without validation.
///
/// This function converts syntactic type references into semantic types.
/// Named types are NOT validated - use `lower_type_ref_validated` for validation.
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

/// Lower a `TypeRef` to a Ty with validation against known types.
///
/// Returns the lowered type and any errors encountered.
/// Unknown type names will return `Ty::Error` to suppress downstream errors.
pub fn lower_type_ref_validated<'db>(
    type_ref: &TypeRef,
    known_types: &HashSet<Name>,
    span: Span,
) -> (Ty<'db>, Vec<TypeError<Ty<'db>>>) {
    let mut ctx = TypeLoweringContext::with_validation(known_types, span);
    let ty = lower_type_ref_with_ctx(&mut ctx, type_ref);
    (ty, ctx.errors)
}

/// Lower a `TypeRef` to a Ty using the provided context.
fn lower_type_ref_with_ctx<'db>(
    ctx: &mut TypeLoweringContext<'_, 'db>,
    type_ref: &TypeRef,
) -> Ty<'db> {
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
        TypeRef::Path(path) => lower_path_type_with_ctx(ctx, path),

        // Type constructors
        TypeRef::Optional(inner) => {
            let inner_ty = lower_type_ref_with_ctx(ctx, inner);
            Ty::Optional(Box::new(inner_ty))
        }

        TypeRef::List(inner) => {
            let inner_ty = lower_type_ref_with_ctx(ctx, inner);
            Ty::List(Box::new(inner_ty))
        }

        TypeRef::Map { key, value } => {
            let key_ty = lower_type_ref_with_ctx(ctx, key);
            let value_ty = lower_type_ref_with_ctx(ctx, value);
            Ty::Map {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }

        TypeRef::Union(types) => {
            let tys: Vec<Ty<'db>> = types
                .iter()
                .map(|t| lower_type_ref_with_ctx(ctx, t))
                .collect();
            normalize_union(tys)
        }

        // Literal types - treat as their base type for now
        TypeRef::StringLiteral(_) => Ty::String,
        TypeRef::IntLiteral(_) => Ty::Int,
        TypeRef::FloatLiteral(_) => Ty::Float,
        TypeRef::BoolLiteral(_) => Ty::Bool,

        // Generics - not yet supported
        TypeRef::Generic { .. } => Ty::Unknown,
        TypeRef::TypeParam(_) => Ty::Unknown,

        // Error/Unknown
        TypeRef::Error => Ty::Error,
        TypeRef::Unknown => Ty::Unknown,
    }
}

/// Check if a type name looks like a simple identifier (not a complex type expression).
///
/// Complex type expressions like `map<string, int>` or `int[]?` come through from
/// the simplified HIR type parser as raw strings. We should not validate these
/// as named types since they require proper parsing.
///
/// Returns false for:
/// - Empty strings (parsing errors)
/// - Complex type expressions with special characters
fn is_simple_type_name(name: &str) -> bool {
    // Empty strings are not valid type names (usually from parsing errors)
    if name.is_empty() {
        return false;
    }
    // Simple type names contain only alphanumeric characters and underscores
    // Complex types contain: < > [ ] | ? , etc.
    !name.contains(['<', '>', '[', ']', '|', '?', ','])
}

/// Lower a path-based type reference (named type) with validation.
fn lower_path_type_with_ctx<'db>(
    ctx: &mut TypeLoweringContext<'_, 'db>,
    path: &baml_hir::Path,
) -> Ty<'db> {
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
                // User-defined type - validate if known_types is provided
                _ => {
                    // Only validate simple type names (not complex expressions like `map<K,V>`)
                    // Complex expressions need proper parsing, not name validation
                    if !is_simple_type_name(name.as_str()) || ctx.is_known_type(name) {
                        Ty::Named(name.clone())
                    } else {
                        ctx.unknown_type_error(name)
                    }
                }
            }
        }
        // For qualified paths, join them and validate the full path
        _ => {
            let full_path = path
                .segments
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>()
                .join(".");
            let name = Name::new(&full_path);
            // Only validate simple type names
            if !is_simple_type_name(&full_path) || ctx.is_known_type(&name) {
                Ty::Named(name)
            } else {
                ctx.unknown_type_error(&name)
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
