//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};

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
    generic_params: &[baml_base::Name],
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    lower_type_expr_in_ns(
        db,
        type_expr,
        package_items,
        &[],
        generic_params,
        diagnostics,
    )
}

/// Like [`lower_type_expr`], but resolves unqualified names relative to
/// `ns_context` first (e.g. `["fs"]`), falling back to the package root.
///
/// Use this when lowering type expressions from a function/class signature
/// that lives in a sub-namespace of its package. For example, `File` in
/// `baml/fs.baml` (namespace `["fs"]`) resolves via `lookup_type(&["fs", "File"])`.
pub fn lower_type_expr_in_ns(
    db: &dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    match type_expr {
        TypeExpr::Path(segments) => {
            // When we have a namespace context, try the qualified path first.
            // e.g. for ns_context=["fs"], segments=["File"], try ["fs", "File"].
            let resolved = if !ns_context.is_empty() {
                let qualified: Vec<baml_base::Name> =
                    ns_context.iter().chain(segments.iter()).cloned().collect();
                package_items
                    .lookup_type(&qualified)
                    .or_else(|| package_items.lookup_type(segments))
            } else {
                package_items.lookup_type(segments)
            };
            // Cross-package fallback: if not found in the current package and
            // the first segment is a known package name, look in that package
            // using the remaining segments. This supports synthetic type
            // references like `baml.llm.PromptAst` in companion functions.
            let resolved = resolved.or_else(|| {
                if segments.len() >= 2 {
                    if segments[0].as_str() == "root" {
                        package_items.lookup_type(&segments[1..])
                    } else {
                        let pkg_id = PackageId::new(db, segments[0].clone());
                        let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                        pkg.lookup_type(&segments[1..])
                    }
                } else {
                    None
                }
            });

            if let Some(def) = resolved {
                let short = segments.last().expect("non-empty path");
                match def {
                    Definition::Class(_) => Ty::Class(qualify_def(db, def, short)),
                    Definition::Enum(_) => Ty::Enum(qualify_def(db, def, short)),
                    Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, short)),
                    // Let bindings are values, not types — produce Unknown in a type position.
                    _ => Ty::Unknown,
                }
            } else {
                // Check if this is a generic type parameter (e.g. T, K, V).
                if segments.len() == 1 {
                    if generic_params.iter().any(|p| *p == segments[0]) {
                        return Ty::TypeVar(segments[0].clone());
                    }
                }
                let name = segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
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
        TypeExpr::Optional(inner) => Ty::Optional(Box::new(lower_type_expr_in_ns(
            db,
            inner,
            package_items,
            ns_context,
            generic_params,
            diagnostics,
        ))),
        TypeExpr::List(inner) => Ty::List(Box::new(lower_type_expr_in_ns(
            db,
            inner,
            package_items,
            ns_context,
            generic_params,
            diagnostics,
        ))),
        TypeExpr::Map { key, value } => Ty::Map(
            Box::new(lower_type_expr_in_ns(
                db,
                key,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            )),
            Box::new(lower_type_expr_in_ns(
                db,
                value,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            )),
        ),
        TypeExpr::Union(members) => Ty::Union(
            members
                .iter()
                .map(|m| {
                    lower_type_expr_in_ns(
                        db,
                        m,
                        package_items,
                        ns_context,
                        generic_params,
                        diagnostics,
                    )
                })
                .collect(),
        ),
        TypeExpr::Function { params, ret } => Ty::Function {
            params: params
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        lower_type_expr_in_ns(
                            db,
                            &p.ty,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        ),
                    )
                })
                .collect(),
            ret: Box::new(lower_type_expr_in_ns(
                db,
                ret,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            )),
        },
        TypeExpr::Literal(lit) => Ty::Literal(lit.clone(), Freshness::Regular),
        TypeExpr::BuiltinUnknown => Ty::BuiltinUnknown,
        TypeExpr::Error | TypeExpr::Unknown => Ty::Unknown,
        // Dedicated Ty::Type variant — see ty.rs doc comment for design rationale.
        TypeExpr::Type => Ty::Type,
        // `$rust_type` — opaque Rust-managed state field type.
        TypeExpr::Rust => Ty::RustType,
    }
}

/// Derive the qualified name for a type from its Definition's file location.
pub fn qualify_def(
    db: &dyn crate::Db,
    def: Definition,
    name: &baml_base::Name,
) -> QualifiedTypeName {
    let file = def.file(db);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    QualifiedTypeName::new(pkg_info.package, pkg_info.namespace_path, name.clone())
}
