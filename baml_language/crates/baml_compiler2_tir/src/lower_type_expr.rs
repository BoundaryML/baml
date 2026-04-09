//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};

use crate::{
    infer_context::TirTypeError,
    ty::{Freshness, PrimitiveType, QualifiedTypeName, Ty, TyAttr},
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
        TypeExpr::Path {
            segments,
            generic_args,
            ..
        } => {
            let item = segments.last().expect("non-empty path");
            let seg_ns = &segments[..segments.len() - 1];
            // When we have a namespace context, try the qualified path first.
            // e.g. for ns_context=["fs"], segments=["File"], try namespace ["fs"] item "File".
            let resolved = if !ns_context.is_empty() {
                let ns: Vec<baml_base::Name> =
                    ns_context.iter().chain(seg_ns.iter()).cloned().collect();
                package_items.lookup_type(&ns, item)
            } else {
                package_items.lookup_type(seg_ns, item)
            };
            // Cross-package fallback: if not found in the current package and
            // the first segment is a known package name, look in that package
            // using the remaining segments. This supports synthetic type
            // references like `baml.llm.PromptAst` in companion functions.
            let resolved = resolved.or_else(|| {
                if segments.len() >= 2 {
                    if segments[0].as_str() == "root" {
                        package_items.lookup_type(&segments[1..segments.len() - 1], item)
                    } else {
                        let pkg_id = PackageId::new(db, segments[0].clone());
                        let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                        pkg.lookup_type(&segments[1..segments.len() - 1], item)
                    }
                } else {
                    None
                }
            });

            if let Some(def) = resolved {
                let short = segments.last().expect("non-empty path");
                match def {
                    Definition::Class(class_loc) => {
                        // Collect and lower generic args, storing them in Ty::Class.
                        let lowered_args: Vec<Ty> = generic_args
                            .iter()
                            .map(|ga| {
                                lower_type_expr_in_ns(
                                    db,
                                    ga,
                                    package_items,
                                    ns_context,
                                    generic_params,
                                    diagnostics,
                                )
                            })
                            .collect();

                        // Validate arity against the class's declared generic params.
                        let file = class_loc.file(db);
                        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
                        let class_data = &item_tree[class_loc.id(db)];
                        let expected = class_data.generic_params.len();
                        let got = lowered_args.len();

                        if got != expected {
                            diagnostics.push(TirTypeError::WrongNumberOfTypeArgs {
                                class_name: short.clone(),
                                expected,
                                got,
                            });
                        }

                        Ty::Class(qualify_def(db, def, short), lowered_args, TyAttr::default())
                    }
                    Definition::Enum(_) => {
                        // Enums are not generic — validate-and-discard any args.
                        for ga in generic_args {
                            let _ = lower_type_expr_in_ns(
                                db,
                                ga,
                                package_items,
                                ns_context,
                                generic_params,
                                diagnostics,
                            );
                        }
                        Ty::Enum(qualify_def(db, def, short), TyAttr::default())
                    }
                    Definition::TypeAlias(_) => {
                        // Type aliases are not generic — validate-and-discard any args.
                        for ga in generic_args {
                            let _ = lower_type_expr_in_ns(
                                db,
                                ga,
                                package_items,
                                ns_context,
                                generic_params,
                                diagnostics,
                            );
                        }
                        Ty::TypeAlias(qualify_def(db, def, short), TyAttr::default())
                    }
                    // Let bindings are values, not types — produce Unknown in a type position.
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                }
            } else {
                // Check if this is a generic type parameter (e.g. T, K, V).
                if segments.len() == 1 {
                    if generic_params.iter().any(|p| *p == segments[0]) {
                        return Ty::TypeVar(segments[0].clone(), TyAttr::default());
                    }
                }
                let name_str = segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                // Scan all namespaces for the item name to build "did you mean" suggestions.
                // Only do this for single-segment bare names — multi-segment paths already
                // encode the intended namespace.
                let mut suggestions = Vec::new();
                if segments.len() == 1 {
                    for (ns_path, ns_items) in &package_items.namespaces {
                        if ns_items.types.contains_key(item) {
                            if ns_path.is_empty() {
                                suggestions.push(format!("root.{item}"));
                            } else {
                                let ns_str = ns_path
                                    .iter()
                                    .map(smol_str::SmolStr::as_str)
                                    .collect::<Vec<_>>()
                                    .join(".");
                                suggestions.push(format!("root.{ns_str}.{item}"));
                            }
                        }
                    }
                    suggestions.sort();
                }
                diagnostics.push(TirTypeError::UnresolvedType {
                    name: baml_base::Name::new(&name_str),
                    suggestions,
                });
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
        TypeExpr::Int { .. } => Ty::Primitive(PrimitiveType::Int, TyAttr::default()),
        TypeExpr::Float { .. } => Ty::Primitive(PrimitiveType::Float, TyAttr::default()),
        TypeExpr::String { .. } => Ty::Primitive(PrimitiveType::String, TyAttr::default()),
        TypeExpr::Bool { .. } => Ty::Primitive(PrimitiveType::Bool, TyAttr::default()),
        TypeExpr::Null { .. } => Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
        TypeExpr::Never { .. } => Ty::Never {
            attr: TyAttr::default(),
        },
        TypeExpr::Void { .. } => Ty::Void {
            attr: TyAttr::default(),
        },
        TypeExpr::Uint8Array { .. } => Ty::Primitive(PrimitiveType::Uint8Array, TyAttr::default()),
        TypeExpr::Media { kind, .. } => Ty::Primitive(
            match kind {
                baml_base::MediaKind::Image => PrimitiveType::Image,
                baml_base::MediaKind::Audio => PrimitiveType::Audio,
                baml_base::MediaKind::Video => PrimitiveType::Video,
                baml_base::MediaKind::Pdf => PrimitiveType::Pdf,
                // Generic media — treated as unknown for type resolution purposes
                baml_base::MediaKind::Generic => {
                    return Ty::Unknown {
                        attr: TyAttr::default(),
                    };
                }
            },
            TyAttr::default(),
        ),
        TypeExpr::Optional { inner, .. } => Ty::Optional(
            Box::new(lower_type_expr_in_ns(
                db,
                inner,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::List { inner, .. } => Ty::List(
            Box::new(lower_type_expr_in_ns(
                db,
                inner,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::Map { key, value, .. } => Ty::Map(
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
            TyAttr::default(),
        ),
        TypeExpr::Union {
            variants: members, ..
        } => Ty::Union(
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
            TyAttr::default(),
        ),
        TypeExpr::Function { params, ret, .. } => Ty::Function {
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
            attr: TyAttr::default(),
        },
        TypeExpr::Literal { value: lit, .. } => {
            Ty::Literal(lit.clone(), Freshness::Regular, TyAttr::default())
        }
        TypeExpr::BuiltinUnknown { .. } => Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        },
        TypeExpr::Error { .. } | TypeExpr::Unknown { .. } => Ty::Unknown {
            attr: TyAttr::default(),
        },
        // Dedicated Ty::Type variant — see ty.rs doc comment for design rationale.
        TypeExpr::Type { .. } => Ty::Type {
            attr: TyAttr::default(),
        },
        // `$rust_type` — opaque Rust-managed state field type.
        TypeExpr::Rust { .. } => Ty::RustType {
            attr: TyAttr::default(),
        },
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
