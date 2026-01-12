//! Lower HIR `TypeRef` to TIR Ty.
//!
//! This module converts syntactic type references (`TypeRef`) from HIR
//! into semantic types (Ty) in TIR. This involves:
//! - Resolving named types to their definitions (classes, enums)
//! - Converting type constructors (Optional, List, Union)
//! - Handling primitive type names
//! - Validating that named types exist (when `known_types` is provided)

use std::collections::HashSet;

use baml_base::{Name, Span};
use baml_compiler_diagnostics::TypeError;
use baml_compiler_hir::TypeRef;

use crate::{LiteralValue, Ty};

/// Context for type lowering with validation.
///
/// When `known_types` is provided, unknown type names will produce errors
/// and return `Ty::Error` to suppress downstream type mismatches.
pub struct TypeLoweringContext<'a> {
    /// Set of known type names (classes, enums, type aliases).
    /// If None, no validation is performed.
    pub known_types: Option<&'a HashSet<Name>>,
    /// Span to use for error reporting. If None, a default span is used.
    pub span: Option<Span>,
    /// Accumulated errors during lowering.
    pub errors: Vec<TypeError<Ty>>,
}

impl<'a> TypeLoweringContext<'a> {
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
}

impl Default for TypeLoweringContext<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Lower a `TypeRef` to a Ty with validation AND resolution of class/enum types.
///
/// This combines validation (checking types exist) with resolution (converting
/// Named types to Class/Enum). This is the preferred function for type checking
/// contexts where you need fully resolved types.
///
/// Returns the lowered type and any errors encountered.
pub fn lower_type_ref_validated_resolved(
    type_ref: &TypeRef,
    known_types: &HashSet<Name>,
    class_names: &HashSet<Name>,
    enum_names: &HashSet<Name>,
    span: Span,
) -> (Ty, Vec<TypeError<Ty>>) {
    let mut ctx = TypeLoweringContextResolved::new(known_types, class_names, enum_names, span);
    let ty = lower_type_ref_resolved_with_ctx(&mut ctx, type_ref);
    (ty, ctx.errors)
}

/// Context for type lowering with validation and resolution.
struct TypeLoweringContextResolved<'a> {
    known_types: &'a HashSet<Name>,
    class_names: &'a HashSet<Name>,
    enum_names: &'a HashSet<Name>,
    span: Span,
    errors: Vec<TypeError<Ty>>,
}

impl<'a> TypeLoweringContextResolved<'a> {
    fn new(
        known_types: &'a HashSet<Name>,
        class_names: &'a HashSet<Name>,
        enum_names: &'a HashSet<Name>,
        span: Span,
    ) -> Self {
        Self {
            known_types,
            class_names,
            enum_names,
            span,
            errors: Vec::new(),
        }
    }

    fn is_known_type(&self, name: &Name) -> bool {
        self.known_types.contains(name)
    }

    fn unknown_type_error(&mut self, name: &Name) -> Ty {
        self.errors.push(TypeError::UnknownType {
            name: name.to_string(),
            span: self.span,
        });
        Ty::Error
    }

    fn resolve_name(&self, name: &Name) -> Option<Ty> {
        if self.class_names.contains(name) {
            Some(Ty::Class(name.clone()))
        } else if self.enum_names.contains(name) {
            Some(Ty::Enum(name.clone()))
        } else {
            None
        }
    }
}

/// Lower a `TypeRef` with validation and resolution context.
fn lower_type_ref_resolved_with_ctx(
    ctx: &mut TypeLoweringContextResolved<'_>,
    type_ref: &TypeRef,
) -> Ty {
    match type_ref {
        // Primitives
        TypeRef::Int => Ty::Int,
        TypeRef::Float => Ty::Float,
        TypeRef::String => Ty::String,
        TypeRef::Bool => Ty::Bool,
        TypeRef::Null => Ty::Null,

        // Media types
        TypeRef::Media(kind) => Ty::Media(kind.clone()),

        // Named type via path
        TypeRef::Path(path) => lower_path_type_resolved_with_ctx(ctx, path),

        // Type constructors
        TypeRef::Optional(inner) => {
            let inner_ty = lower_type_ref_resolved_with_ctx(ctx, inner);
            Ty::Optional(Box::new(inner_ty))
        }

        TypeRef::List(inner) => {
            let inner_ty = lower_type_ref_resolved_with_ctx(ctx, inner);
            Ty::List(Box::new(inner_ty))
        }

        TypeRef::Map { key, value } => {
            let key_ty = lower_type_ref_resolved_with_ctx(ctx, key);
            let value_ty = lower_type_ref_resolved_with_ctx(ctx, value);
            Ty::Map {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }

        TypeRef::Union(types) => {
            let tys: Vec<Ty> = types
                .iter()
                .map(|t| lower_type_ref_resolved_with_ctx(ctx, t))
                .collect();
            normalize_union(tys)
        }

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

/// Lower a path-based type with validation and resolution.
fn lower_path_type_resolved_with_ctx(
    ctx: &mut TypeLoweringContextResolved<'_>,
    path: &baml_compiler_hir::Path,
) -> Ty {
    match path.segments.len() {
        1 => {
            let name = &path.segments[0];
            match name.as_str() {
                // Primitive type names
                "int" => Ty::Int,
                "float" => Ty::Float,
                "string" => Ty::String,
                "bool" => Ty::Bool,
                "null" => Ty::Null,
                "image" => Ty::Media(baml_base::MediaKind::Image),
                "audio" => Ty::Media(baml_base::MediaKind::Audio),
                "video" => Ty::Media(baml_base::MediaKind::Video),
                "pdf" => Ty::Media(baml_base::MediaKind::Pdf),
                // User-defined type - resolve to Class/Enum or validate
                _ => {
                    // Skip validation for complex type expressions
                    if !is_simple_type_name(name.as_str()) {
                        return Ty::Named(name.clone());
                    }

                    // Try to resolve to Class/Enum
                    if let Some(resolved) = ctx.resolve_name(name) {
                        return resolved;
                    }

                    // Check if it's a known type (could be a type alias)
                    if ctx.is_known_type(name) {
                        Ty::Named(name.clone())
                    } else {
                        ctx.unknown_type_error(name)
                    }
                }
            }
        }
        _ => {
            let full_path = path
                .segments
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>()
                .join(".");
            let name = Name::new(&full_path);
            if !is_simple_type_name(&full_path) || ctx.is_known_type(&name) {
                Ty::Named(name)
            } else {
                ctx.unknown_type_error(&name)
            }
        }
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
