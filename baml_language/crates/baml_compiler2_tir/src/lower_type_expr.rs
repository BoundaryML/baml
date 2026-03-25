//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::{SpannedTypeExpr, SpannedTypeExprKind, TypeExpr};
use baml_compiler2_hir::{contributions::Definition, package::PackageItems};
use text_size::TextRange;

use crate::{
    infer_context::TirTypeError,
    ty::{Freshness, PrimitiveType, QualifiedTypeName, Ty},
};

/// Resolve an AST `TypeExpr` to a `Ty` using package-level name resolution.
///
/// Names are resolved against `package_items`: classes, enums, and type aliases
/// are looked up in the type namespace. Unresolved names become `Ty::Unknown`
/// and push an `UnresolvedType` diagnostic to `diagnostics`.
/// The package for each resolved type is derived from the **definition's** file,
/// not the referencing file.
pub fn lower_type_expr(
    db: &dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'_>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    match type_expr {
        TypeExpr::Path(segments) => {
            let names: Vec<baml_base::Name> = segments.clone();
            if let Some(def) = package_items.lookup_type(&names) {
                let short = segments.last().expect("non-empty path");
                match def {
                    Definition::Class(_) => Ty::Class(qualify_def(db, def, short)),
                    Definition::Enum(_) => Ty::Enum(qualify_def(db, def, short)),
                    Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, short)),
                    _ => Ty::Unknown,
                }
            } else {
                // Not found in type namespace — unresolved
                let name = segments
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                diagnostics.push(TirTypeError::UnresolvedType {
                    name: baml_base::Name::new(&name),
                });
                Ty::Unknown
            }
        }
        TypeExpr::Int => Ty::Primitive(PrimitiveType::Int),
        TypeExpr::Float => Ty::Primitive(PrimitiveType::Float),
        TypeExpr::String => Ty::Primitive(PrimitiveType::String),
        TypeExpr::Bool => Ty::Primitive(PrimitiveType::Bool),
        TypeExpr::Null => Ty::Primitive(PrimitiveType::Null),
        TypeExpr::Never => Ty::Never,
        TypeExpr::Media(kind) => Ty::Primitive(match kind {
            baml_base::MediaKind::Image => PrimitiveType::Image,
            baml_base::MediaKind::Audio => PrimitiveType::Audio,
            baml_base::MediaKind::Video => PrimitiveType::Video,
            baml_base::MediaKind::Pdf => PrimitiveType::Pdf,
            // Generic media — treated as unknown for type resolution purposes
            baml_base::MediaKind::Generic => return Ty::Unknown,
        }),
        TypeExpr::Optional(inner) => Ty::Optional(Box::new(lower_type_expr(
            db,
            inner,
            package_items,
            diagnostics,
        ))),
        TypeExpr::List(inner) => Ty::List(Box::new(lower_type_expr(
            db,
            inner,
            package_items,
            diagnostics,
        ))),
        TypeExpr::Map { key, value } => Ty::Map(
            Box::new(lower_type_expr(db, key, package_items, diagnostics)),
            Box::new(lower_type_expr(db, value, package_items, diagnostics)),
        ),
        TypeExpr::Union(members) => Ty::Union(
            members
                .iter()
                .map(|m| lower_type_expr(db, m, package_items, diagnostics))
                .collect(),
        ),
        TypeExpr::Function { params, ret } => Ty::Function {
            params: params
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        lower_type_expr(db, &p.ty, package_items, diagnostics),
                    )
                })
                .collect(),
            ret: Box::new(lower_type_expr(db, ret, package_items, diagnostics)),
        },
        TypeExpr::Literal(lit) => Ty::Literal(lit.clone(), Freshness::Regular),
        TypeExpr::BuiltinUnknown => Ty::BuiltinUnknown,
        TypeExpr::Error | TypeExpr::Unknown => Ty::Unknown,
        TypeExpr::Type => Ty::Unknown,
        // `$rust_type` — opaque Rust-managed state field type.
        TypeExpr::Rust => Ty::RustType,
    }
}

/// Build a qualified type name from package and short name.
pub fn qualify(pkg: &str, name: &baml_base::Name) -> QualifiedTypeName {
    QualifiedTypeName::new(baml_base::Name::new(pkg), name.clone())
}

/// Derive the qualified name for a type from its Definition's file location.
pub fn qualify_def(
    db: &dyn crate::Db,
    def: Definition,
    name: &baml_base::Name,
) -> QualifiedTypeName {
    let file = def.file(db);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    qualify(pkg_info.package.as_str(), name)
}

/// Resolve a recursive `SpannedTypeExpr` to a `Ty`, collecting errors with
/// per-node spans. Each error is paired with the `TextRange` of the specific
/// sub-expression that caused it (e.g. just the `sring` part of `sring | image`).
pub fn lower_spanned_type_expr(
    db: &dyn crate::Db,
    spanned: &SpannedTypeExpr,
    package_items: &PackageItems<'_>,
    diagnostics: &mut Vec<(TirTypeError, TextRange)>,
) -> Ty {
    match &spanned.kind {
        SpannedTypeExprKind::Path(segments) => {
            let names: Vec<baml_base::Name> = segments.clone();
            if let Some(def) = package_items.lookup_type(&names) {
                let short = segments.last().expect("non-empty path");
                match def {
                    Definition::Class(_) => Ty::Class(qualify_def(db, def, short)),
                    Definition::Enum(_) => Ty::Enum(qualify_def(db, def, short)),
                    Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, short)),
                    _ => Ty::Unknown,
                }
            } else {
                let name = segments
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                diagnostics.push((
                    TirTypeError::UnresolvedType {
                        name: baml_base::Name::new(&name),
                    },
                    spanned.span,
                ));
                Ty::Unknown
            }
        }
        SpannedTypeExprKind::Int => Ty::Primitive(PrimitiveType::Int),
        SpannedTypeExprKind::Float => Ty::Primitive(PrimitiveType::Float),
        SpannedTypeExprKind::String => Ty::Primitive(PrimitiveType::String),
        SpannedTypeExprKind::Bool => Ty::Primitive(PrimitiveType::Bool),
        SpannedTypeExprKind::Null => Ty::Primitive(PrimitiveType::Null),
        SpannedTypeExprKind::Never => Ty::Never,
        SpannedTypeExprKind::Media(kind) => Ty::Primitive(match kind {
            baml_base::MediaKind::Image => PrimitiveType::Image,
            baml_base::MediaKind::Audio => PrimitiveType::Audio,
            baml_base::MediaKind::Video => PrimitiveType::Video,
            baml_base::MediaKind::Pdf => PrimitiveType::Pdf,
            baml_base::MediaKind::Generic => return Ty::Unknown,
        }),
        SpannedTypeExprKind::Optional(inner) => Ty::Optional(Box::new(lower_spanned_type_expr(
            db,
            inner,
            package_items,
            diagnostics,
        ))),
        SpannedTypeExprKind::List(inner) => Ty::List(Box::new(lower_spanned_type_expr(
            db,
            inner,
            package_items,
            diagnostics,
        ))),
        SpannedTypeExprKind::Map { key, value } => Ty::Map(
            Box::new(lower_spanned_type_expr(db, key, package_items, diagnostics)),
            Box::new(lower_spanned_type_expr(
                db,
                value,
                package_items,
                diagnostics,
            )),
        ),
        SpannedTypeExprKind::Union(members) => Ty::Union(
            members
                .iter()
                .map(|m| lower_spanned_type_expr(db, m, package_items, diagnostics))
                .collect(),
        ),
        SpannedTypeExprKind::Function { params, ret } => Ty::Function {
            params: params
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        lower_spanned_type_expr(db, &p.ty, package_items, diagnostics),
                    )
                })
                .collect(),
            ret: Box::new(lower_spanned_type_expr(db, ret, package_items, diagnostics)),
        },
        SpannedTypeExprKind::Literal(lit) => Ty::Literal(lit.clone(), Freshness::Regular),
        SpannedTypeExprKind::BuiltinUnknown => Ty::BuiltinUnknown,
        SpannedTypeExprKind::Error | SpannedTypeExprKind::Unknown => Ty::Unknown,
        SpannedTypeExprKind::Type => Ty::Unknown,
        SpannedTypeExprKind::Rust => Ty::RustType,
    }
}
