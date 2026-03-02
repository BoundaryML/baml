//! Lower HIR `TypeRef` to TIR Ty.
//!
//! This module converts syntactic type references (`TypeRef`) from HIR
//! into semantic types (Ty) in TIR. This involves:
//! - Resolving named types to their definitions (classes, enums)
//! - Converting type constructors (Optional, List, Union)
//! - Handling primitive type names
//! - Validating that named types exist (when `type_alias_names` is provided)

use std::collections::{HashMap, HashSet};

use baml_base::{Name, TyAttr};
use baml_compiler_diagnostics::TypeError;
use baml_compiler_hir::{ErrorLocation, TypeRef};

use crate::{LiteralValue, TirTypeError, Ty};

/// Lower a `TypeRef` to a `Ty`, validating and resolving type names.
///
/// This combines validation (checking types exist) with resolution (converting
/// Named types to Class/Enum). This is the preferred function for type checking
/// contexts where you need fully resolved types.
///
/// The `location` parameter can be either:
/// - A `Span` for direct span-based error reporting
/// - An `ErrorLocation` for position-independent error locations (used by cached queries)
///
/// Returns the lowered type and any errors encountered.
pub(crate) fn lower_type_ref(
    type_ref: &TypeRef,
    type_alias_names: &HashSet<Name>,
    class_names: &HashMap<Name, baml_compiler_hir::QualifiedName>,
    enum_names: &HashMap<Name, baml_compiler_hir::QualifiedName>,
    location: impl Into<ErrorLocation>,
) -> (Ty, Vec<TirTypeError>) {
    let mut ctx = TypeLoweringContextResolved::new(
        type_alias_names,
        class_names,
        enum_names,
        location.into(),
    );
    let ty = lower_type_ref_resolved_with_ctx(&mut ctx, type_ref);
    (ty, ctx.errors)
}

/// Context for type lowering with validation and resolution.
struct TypeLoweringContextResolved<'a> {
    type_alias_names: &'a HashSet<Name>,
    class_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,
    enum_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,
    /// Base error location (e.g., `TypeAliasType` with `alias_name`)
    base_location: ErrorLocation,
    /// Current path within nested type constructors (for `TypeAliasType`)
    current_path: Vec<usize>,
    errors: Vec<TirTypeError>,
}

impl<'a> TypeLoweringContextResolved<'a> {
    fn new(
        type_alias_names: &'a HashSet<Name>,
        class_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,
        enum_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,
        location: ErrorLocation,
    ) -> Self {
        Self {
            type_alias_names,
            class_names,
            enum_names,
            base_location: location,
            current_path: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Get the current error location, incorporating the path for `TypeAliasType`.
    fn current_location(&self) -> ErrorLocation {
        match &self.base_location {
            ErrorLocation::TypeAliasType { alias_name, .. } => ErrorLocation::TypeAliasType {
                alias_name: alias_name.clone(),
                path: self.current_path.clone(),
            },
            other => other.clone(),
        }
    }

    fn is_type_alias_name(&self, name: &Name) -> bool {
        self.type_alias_names.contains(name)
    }

    fn unknown_type_error(&mut self, name: &Name) -> Ty {
        self.errors.push(TypeError::UnknownType {
            name: name.to_string(),
            location: self.current_location(),
        });
        Ty::Error {
            attr: TyAttr::default(),
        }
    }

    fn resolve_name(&self, name: &Name) -> Option<Ty> {
        if let Some(qn) = self.class_names.get(name) {
            return Some(Ty::Class(qn.clone(), TyAttr::default()));
        }
        if let Some(qn) = self.enum_names.get(name) {
            return Some(Ty::Enum(qn.clone(), TyAttr::default()));
        }
        None
    }
}

/// Lower a `TypeRef` with validation and resolution context.
fn lower_type_ref_resolved_with_ctx(
    ctx: &mut TypeLoweringContextResolved<'_>,
    type_ref: &TypeRef,
) -> Ty {
    match type_ref {
        // Primitives
        TypeRef::Int => Ty::Int {
            attr: TyAttr::default(),
        },
        TypeRef::Float => Ty::Float {
            attr: TyAttr::default(),
        },
        TypeRef::String => Ty::String {
            attr: TyAttr::default(),
        },
        TypeRef::Bool => Ty::Bool {
            attr: TyAttr::default(),
        },
        TypeRef::Null => Ty::Null {
            attr: TyAttr::default(),
        },

        // Media types
        TypeRef::Media(kind) => Ty::Media(*kind, TyAttr::default()),

        // Named type via path
        TypeRef::Path(path) => lower_path_type_resolved_with_ctx(ctx, path),

        // Type constructors - track path for error location
        TypeRef::Optional(inner) => {
            ctx.current_path.push(0); // Optional inner is at index 0
            let inner_ty = lower_type_ref_resolved_with_ctx(ctx, inner);
            ctx.current_path.pop();
            Ty::Optional(Box::new(inner_ty), TyAttr::default())
        }

        TypeRef::List(inner) => {
            ctx.current_path.push(0); // List element is at index 0
            let inner_ty = lower_type_ref_resolved_with_ctx(ctx, inner);
            ctx.current_path.pop();
            Ty::List(Box::new(inner_ty), TyAttr::default())
        }

        TypeRef::Map { key, value } => {
            ctx.current_path.push(0); // Map key is at index 0
            let key_ty = lower_type_ref_resolved_with_ctx(ctx, key);
            ctx.current_path.pop();

            ctx.current_path.push(1); // Map value is at index 1
            let value_ty = lower_type_ref_resolved_with_ctx(ctx, value);
            ctx.current_path.pop();

            Ty::Map {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
                attr: TyAttr::default(),
            }
        }

        TypeRef::Union(types) => {
            let tys: Vec<Ty> = types
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    ctx.current_path.push(i); // Union variant is at its index
                    let ty = lower_type_ref_resolved_with_ctx(ctx, t);
                    ctx.current_path.pop();
                    ty
                })
                .collect();
            normalize_union(tys)
        }

        TypeRef::StringLiteral(s) => {
            Ty::Literal(LiteralValue::String(s.clone()), TyAttr::default())
        }
        TypeRef::IntLiteral(i) => Ty::Literal(LiteralValue::Int(*i), TyAttr::default()),
        TypeRef::FloatLiteral(f) => Ty::Literal(LiteralValue::Float(f.clone()), TyAttr::default()),
        TypeRef::BoolLiteral(b) => Ty::Literal(LiteralValue::Bool(*b), TyAttr::default()),

        // Function types: (x: int, y: int) -> bool
        TypeRef::Function { params, ret } => {
            let param_tys: Vec<(Option<Name>, Ty)> = params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    ctx.current_path.push(i);
                    let ty = lower_type_ref_resolved_with_ctx(ctx, &p.ty);
                    ctx.current_path.pop();
                    (p.name.clone(), ty)
                })
                .collect();
            ctx.current_path.push(params.len());
            let ret_ty = lower_type_ref_resolved_with_ctx(ctx, ret);
            ctx.current_path.pop();
            Ty::Function {
                params: param_tys,
                ret: Box::new(ret_ty),
                attr: TyAttr::default(),
            }
        }

        // Generics - not yet supported
        TypeRef::Generic { .. } => Ty::Unknown {
            attr: TyAttr::default(),
        },
        TypeRef::TypeParam(_) => Ty::Unknown {
            attr: TyAttr::default(),
        },

        // Error/Unknown
        TypeRef::Error => Ty::Error {
            attr: TyAttr::default(),
        },
        TypeRef::Unknown => Ty::Unknown {
            attr: TyAttr::default(),
        },

        // BuiltinUnknown - the `unknown` type keyword for builtin functions
        TypeRef::BuiltinUnknown => Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        },

        // Type - the `type` keyword for the meta-type
        TypeRef::Type => Ty::Type {
            attr: TyAttr::default(),
        },
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
                "int" => Ty::Int {
                    attr: TyAttr::default(),
                },
                "float" => Ty::Float {
                    attr: TyAttr::default(),
                },
                "string" => Ty::String {
                    attr: TyAttr::default(),
                },
                "bool" => Ty::Bool {
                    attr: TyAttr::default(),
                },
                "null" => Ty::Null {
                    attr: TyAttr::default(),
                },
                "image" => Ty::Media(baml_base::MediaKind::Image, TyAttr::default()),
                "audio" => Ty::Media(baml_base::MediaKind::Audio, TyAttr::default()),
                "video" => Ty::Media(baml_base::MediaKind::Video, TyAttr::default()),
                "pdf" => Ty::Media(baml_base::MediaKind::Pdf, TyAttr::default()),
                // map with wrong arity - the arity error is already reported by HIR,
                // so we don't report an unknown type error here
                "map" => Ty::Error {
                    attr: TyAttr::default(),
                },
                // User-defined type - resolve to Class/Enum or validate
                _ => {
                    use baml_compiler_hir::QualifiedName;

                    // Skip validation for complex type expressions
                    if !is_simple_type_name(name.as_str()) {
                        return Ty::TypeAlias(
                            QualifiedName::local(name.clone()),
                            TyAttr::default(),
                        );
                    }

                    // Try to resolve to Class/Enum
                    if let Some(resolved) = ctx.resolve_name(name) {
                        return resolved;
                    }

                    // Check if it's a type alias
                    if ctx.is_type_alias_name(name) {
                        Ty::TypeAlias(QualifiedName::local(name.clone()), TyAttr::default())
                    } else {
                        ctx.unknown_type_error(name)
                    }
                }
            }
        }
        _ => {
            use baml_compiler_hir::QualifiedName;

            let full_path = path
                .segments
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>()
                .join(".");
            let name = Name::new(&full_path);

            if !is_simple_type_name(&full_path) {
                return Ty::TypeAlias(QualifiedName::local(name), TyAttr::default());
            }

            // Resolve as class/enum (builtin types like "baml.http.Request" are in class_names)
            if let Some(resolved) = ctx.resolve_name(&name) {
                return resolved;
            }

            if ctx.is_type_alias_name(&name) {
                Ty::TypeAlias(QualifiedName::local(name), TyAttr::default())
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
///
/// Uses `TyAttr::default()` for the output union because this is only called during
/// type lowering from `TypeRef` syntax nodes, which construct fresh types rather than
/// transforming existing ones — there is no source `TyAttr` to preserve.
fn normalize_union(types: Vec<Ty>) -> Ty {
    let mut normalized = Vec::new();

    for ty in types {
        match ty {
            // Flatten nested unions
            Ty::Union(inner, _) => {
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
        0 => Ty::Unknown {
            attr: TyAttr::default(),
        }, // Empty union becomes Unknown (could be Never in a more complete type system)
        1 => normalized.pop().unwrap(),
        _ => Ty::Union(normalized, TyAttr::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> TyAttr {
        TyAttr::default()
    }

    #[test]
    fn test_normalize_union_empty() {
        let result = normalize_union(vec![]);
        assert_eq!(result, Ty::Unknown { attr: d() });
    }

    #[test]
    fn test_normalize_union_single() {
        let result = normalize_union(vec![Ty::Int { attr: d() }]);
        assert_eq!(result, Ty::Int { attr: d() });
    }

    #[test]
    fn test_normalize_union_removes_duplicates() {
        let result = normalize_union(vec![
            Ty::Int { attr: d() },
            Ty::String { attr: d() },
            Ty::Int { attr: d() },
        ]);
        assert_eq!(
            result,
            Ty::Union(vec![Ty::Int { attr: d() }, Ty::String { attr: d() }], d())
        );
    }

    #[test]
    fn test_normalize_union_flattens() {
        let inner = Ty::Union(vec![Ty::Int { attr: d() }, Ty::Float { attr: d() }], d());
        let result = normalize_union(vec![inner, Ty::String { attr: d() }]);
        assert_eq!(
            result,
            Ty::Union(
                vec![
                    Ty::Int { attr: d() },
                    Ty::Float { attr: d() },
                    Ty::String { attr: d() }
                ],
                d()
            )
        );
    }
}
