//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_base::Name;
use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};

use crate::{
    infer_context::TirTypeError,
    ty::{Freshness, PrimitiveType, QualifiedTypeName, Ty, TyAttr},
};

/// Context for lowering function types — determines how an omitted `throws`
/// clause is interpreted.
///
/// - `DefaultClosed`: omitted throws ⇒ `Ty::Never` (pure by default).
///   Used for class fields, type aliases, return types, locals, and nested
///   function types inside parameter/return positions.
/// - `DirectParamRoot { param_name }`: omitted throws ⇒ fresh effect `TypeVar`
///   named `__throws_<param_name>`.  Used only for the *outermost* function
///   type of a direct callback parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FnTypeLoweringContext {
    /// Omitted throws ⇒ `Ty::Never`.
    DefaultClosed,
    /// Omitted throws ⇒ fresh effect `TypeVar` named `__throws_<param_name>`.
    DirectParamRoot { param_name: Name },
}

/// Lower a function-typed `TypeExpr` with implicit effect polymorphism.
///
/// When `ctx` is `DirectParamRoot { param_name }` and the function type has no
/// explicit `throws` clause, a fresh `TypeVar` named `__throws_<param_name>` is
/// generated and recorded in `synthetic_effect_vars`.  All nested positions
/// (params, return type, nested function types) are lowered with
/// `DefaultClosed`.
///
/// Returns the lowered `Ty`.  Any generated effect var names are pushed into
/// `synthetic_effect_vars` so the caller can include them in the generic
/// binding set.
#[allow(clippy::too_many_arguments)]
pub fn lower_type_expr_with_fn_context(
    db: &dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'_>,
    ns_context: &[Name],
    generic_params: &[Name],
    diagnostics: &mut Vec<TirTypeError>,
    ctx: &FnTypeLoweringContext,
    synthetic_effect_vars: &mut Vec<Name>,
) -> Ty {
    match type_expr {
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            // Nested function types inside params/return are always DefaultClosed.
            let param_tys: Vec<(Option<Name>, Ty)> = params
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
                .collect();

            let ret_ty = lower_type_expr_in_ns(
                db,
                ret,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            );

            let throws_ty = match throws {
                Some(t) => {
                    // Explicit throws clause — lower normally.
                    lower_type_expr_in_ns(
                        db,
                        t,
                        package_items,
                        ns_context,
                        generic_params,
                        diagnostics,
                    )
                }
                None => match ctx {
                    FnTypeLoweringContext::DirectParamRoot { param_name } => {
                        // No explicit throws + direct param ⇒ fresh effect TypeVar.
                        let var_name = Name::new(format!("__throws_{param_name}"));
                        synthetic_effect_vars.push(var_name.clone());
                        Ty::TypeVar(var_name, TyAttr::default())
                    }
                    FnTypeLoweringContext::DefaultClosed => Ty::Never {
                        attr: TyAttr::default(),
                    },
                },
            };

            Ty::Function {
                params: param_tys,
                ret: Box::new(ret_ty),
                throws: Box::new(throws_ty),
                attr: TyAttr::default(),
            }
        }
        // Non-function types fall through to normal lowering.
        _ => lower_type_expr_in_ns(
            db,
            type_expr,
            package_items,
            ns_context,
            generic_params,
            diagnostics,
        ),
    }
}

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
            type_args,
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
                let lowered_type_args: Vec<Ty> = type_args
                    .iter()
                    .map(|arg| {
                        lower_type_expr_in_ns(
                            db,
                            arg,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        )
                    })
                    .collect();
                match def {
                    Definition::Class(_) => {
                        Ty::Class(qualify_def(db, def, short).into(), TyAttr::default())
                            .with_nominal_type_args(lowered_type_args)
                    }
                    Definition::Enum(_) => {
                        Ty::Enum(qualify_def(db, def, short).into(), TyAttr::default())
                            .with_nominal_type_args(lowered_type_args)
                    }
                    Definition::TypeAlias(_) => {
                        Ty::TypeAlias(qualify_def(db, def, short).into(), TyAttr::default())
                            .with_nominal_type_args(lowered_type_args)
                    }
                    // Let bindings are values, not types — produce Unknown in a type position.
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                }
            } else {
                // Check if this is a generic type parameter (e.g. T, K, V).
                if segments.len() == 1 && type_args.is_empty() {
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
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => Ty::Function {
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
            throws: Box::new(
                throws
                    .as_ref()
                    .map(|t| {
                        lower_type_expr_in_ns(
                            db,
                            t,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        )
                    })
                    .unwrap_or_else(|| Ty::Never {
                        attr: TyAttr::default(),
                    }),
            ),
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
