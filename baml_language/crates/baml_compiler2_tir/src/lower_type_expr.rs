//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};

use crate::{
    infer_context::TirTypeError,
    ty::{
        Freshness, FunctionParamMode, FunctionParamTy, PrimitiveType, QualifiedTypeName, Ty, TyAttr,
    },
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

/// BEP-044: walk `type_expr` and replace any `Self` reference with
/// `replacement`. Used by signature lowering to pre-resolve `Self` to
/// the enclosing class/interface's type expression before regular
/// resolution runs.
pub fn substitute_self_in(type_expr: &TypeExpr, replacement: &TypeExpr) -> TypeExpr {
    match type_expr {
        TypeExpr::Path {
            segments,
            generic_args,
            attrs,
        } => {
            if segments.len() == 1 && generic_args.is_empty() && segments[0].as_str() == "Self" {
                return replacement.clone();
            }
            TypeExpr::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_self_in(a, replacement))
                    .collect(),
                attrs: attrs.clone(),
            }
        }
        TypeExpr::List { inner, attrs } => TypeExpr::List {
            inner: Box::new(substitute_self_in(inner, replacement)),
            attrs: attrs.clone(),
        },
        TypeExpr::Optional { inner, attrs } => TypeExpr::Optional {
            inner: Box::new(substitute_self_in(inner, replacement)),
            attrs: attrs.clone(),
        },
        TypeExpr::Map { key, value, attrs } => TypeExpr::Map {
            key: Box::new(substitute_self_in(key, replacement)),
            value: Box::new(substitute_self_in(value, replacement)),
            attrs: attrs.clone(),
        },
        TypeExpr::Union { variants, attrs } => TypeExpr::Union {
            variants: variants
                .iter()
                .map(|v| substitute_self_in(v, replacement))
                .collect(),
            attrs: attrs.clone(),
        },
        TypeExpr::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attrs,
        } => TypeExpr::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|bound| {
                    bound
                        .as_ref()
                        .map(|bound| substitute_self_in(bound, replacement))
                })
                .collect(),
            params: params
                .iter()
                .map(|param| {
                    let mut param = param.clone();
                    param.ty = substitute_self_in(&param.ty, replacement);
                    param
                })
                .collect(),
            ret: Box::new(substitute_self_in(ret, replacement)),
            throws: throws
                .as_ref()
                .map(|ty| Box::new(substitute_self_in(ty, replacement))),
            attrs: attrs.clone(),
        },
        _ => type_expr.clone(),
    }
}

/// BEP-044 generic bounds: walk `type_expr` and replace any single-
/// segment `Path` whose name appears in `subst` with the matching
/// replacement `TypeExpr`. Used at MIR lowering to substitute bounded
/// type-vars (`T extends Named`) with their bound (`Named`) so the
/// runtime sees a concrete type to dispatch on.
pub fn substitute_paths_in(
    type_expr: &TypeExpr,
    subst: &std::collections::HashMap<baml_base::Name, TypeExpr>,
) -> TypeExpr {
    if subst.is_empty() {
        return type_expr.clone();
    }
    substitute_paths_walk(type_expr, subst)
}

fn substitute_paths_walk(
    ty: &TypeExpr,
    subst: &std::collections::HashMap<baml_base::Name, TypeExpr>,
) -> TypeExpr {
    match ty {
        TypeExpr::Path {
            segments,
            generic_args,
            attrs,
        } => {
            if segments.len() == 1
                && generic_args.is_empty()
                && let Some(replacement) = subst.get(&segments[0])
            {
                return replacement.clone();
            }
            TypeExpr::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_paths_walk(a, subst))
                    .collect(),
                attrs: attrs.clone(),
            }
        }
        TypeExpr::List { inner, attrs } => TypeExpr::List {
            inner: Box::new(substitute_paths_walk(inner, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Optional { inner, attrs } => TypeExpr::Optional {
            inner: Box::new(substitute_paths_walk(inner, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Map { key, value, attrs } => TypeExpr::Map {
            key: Box::new(substitute_paths_walk(key, subst)),
            value: Box::new(substitute_paths_walk(value, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Union { variants, attrs } => TypeExpr::Union {
            variants: variants
                .iter()
                .map(|v| substitute_paths_walk(v, subst))
                .collect(),
            attrs: attrs.clone(),
        },
        TypeExpr::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attrs,
        } => TypeExpr::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|bound| {
                    bound
                        .as_ref()
                        .map(|bound| substitute_paths_walk(bound, subst))
                })
                .collect(),
            params: params
                .iter()
                .map(|param| {
                    let mut param = param.clone();
                    param.ty = substitute_paths_walk(&param.ty, subst);
                    param
                })
                .collect(),
            ret: Box::new(substitute_paths_walk(ret, subst)),
            throws: throws
                .as_ref()
                .map(|ty| Box::new(substitute_paths_walk(ty, subst))),
            attrs: attrs.clone(),
        },
        _ => ty.clone(),
    }
}

/// Build a `TypeExpr::Path` referring to a single named type, so
/// `substitute_self_in` can swap `Self` for the enclosing class /
/// interface name.
pub fn type_expr_for_name(name: baml_base::Name) -> TypeExpr {
    TypeExpr::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        attrs: Vec::new(),
    }
}

/// Resolve a type path against `package_items`, trying the `ns_context`-qualified
/// path first, then the cross-package fallback (`root.*` or a package-named first
/// segment). Returns the resolved [`Definition`], if any.
fn resolve_type<'db>(
    db: &'db dyn crate::Db,
    package_items: &PackageItems<'db>,
    ns_context: &[baml_base::Name],
    path: &[baml_base::Name],
    item: &baml_base::Name,
) -> Option<Definition<'db>> {
    let seg_ns = &path[..path.len() - 1];
    // When we have a namespace context, try the qualified path first.
    // e.g. for ns_context=["fs"], path=["File"], try namespace ["fs"] item "File".
    let resolved = if !ns_context.is_empty() {
        let ns: Vec<baml_base::Name> = ns_context.iter().chain(seg_ns.iter()).cloned().collect();
        package_items.lookup_type(&ns, item)
    } else {
        package_items.lookup_type(seg_ns, item)
    };
    // Cross-package fallback: if not found in the current package and
    // the first segment is a known package name, look in that package
    // using the remaining segments. This supports synthetic type
    // references like `baml.llm.PromptAst` in companion functions.
    resolved.or_else(|| {
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                package_items.lookup_type(&path[1..path.len() - 1], item)
            } else {
                let pkg_id = PackageId::new(db, path[0].clone());
                let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                pkg.lookup_type(&path[1..path.len() - 1], item)
            }
        } else {
            None
        }
    })
}

/// Lower each generic arg in turn, surfacing any nested diagnostics.
fn lower_args(
    db: &dyn crate::Db,
    generic_args: &[TypeExpr],
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    diagnostics: &mut Vec<TirTypeError>,
) -> Vec<Ty> {
    generic_args
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
        .collect()
}

/// Emit a `TypeIsNotGeneric` diagnostic if `generic_args` were supplied for a
/// non-generic type (enum / type alias). The args are lowered separately by the
/// caller so their own diagnostics still surface.
fn not_generic(
    generic_args: &[TypeExpr],
    type_name: &baml_base::Name,
    kind: &'static str,
    diagnostics: &mut Vec<TirTypeError>,
) {
    if !generic_args.is_empty() {
        diagnostics.push(TirTypeError::TypeIsNotGeneric {
            type_name: type_name.clone(),
            kind,
        });
    }
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
            let resolved = resolve_type(db, package_items, ns_context, segments, item);

            if let Some(def) = resolved {
                match def {
                    Definition::Class(_) => {
                        // Collect and lower generic args, storing them in Ty::Class.
                        let lowered_args = lower_args(
                            db,
                            generic_args,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        );

                        // BEP-034: `baml.future.Future<T, E>` resolves to the
                        // dedicated `Ty::Future` variant rather than the
                        // generic `Ty::Class`. The class declaration exists
                        // as a regular `.baml` file (for method dispatch via
                        // the standard PackageBaml path), but `spawn` /
                        // `await` keys off `Ty::Future` directly. Mirrors
                        // how `int[]` resolves to `Ty::List` even though
                        // `class Array<T>` is a regular class declaration.
                        let qtn = qualify_def(db, def, item);
                        if qtn.name().as_str() == "Future"
                            && qtn.package().as_str() == baml_base::BAML_PACKAGE
                            && qtn.namespace().len() == 1
                            && qtn.namespace()[0].as_str() == "future"
                            && lowered_args.len() == 2
                        {
                            return Ty::Future(
                                Box::new(lowered_args[0].clone()),
                                Box::new(lowered_args[1].clone()),
                                TyAttr::default(),
                            );
                        }

                        // Class arity mismatches are not flagged here — downstream
                        // checks (pattern subtype check, generic substitution,
                        // assignment checks) already produce a clearer diagnostic
                        // (e.g. `expected Box<int>, got Box`) when the arity
                        // mismatch actually matters at the use site.
                        Ty::Class(qtn, lowered_args, TyAttr::default())
                    }
                    Definition::Interface(_) => {
                        // Same generic-arg handling as `Class` — interface
                        // parameters are valid in the same positions.
                        let lowered_args = lower_args(
                            db,
                            generic_args,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        );
                        Ty::Interface(qualify_def(db, def, item), lowered_args, TyAttr::default())
                    }
                    Definition::Enum(_) => {
                        // Enums are not generic — lower args (for their own
                        // diagnostics) but flag any as not-generic.
                        lower_args(
                            db,
                            generic_args,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        );
                        not_generic(generic_args, item, "enum", diagnostics);
                        Ty::Enum(qualify_def(db, def, item), TyAttr::default())
                    }
                    Definition::TypeAlias(_) => {
                        // Type aliases are not generic — same handling as enums.
                        lower_args(
                            db,
                            generic_args,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        );
                        not_generic(generic_args, item, "type alias", diagnostics);
                        Ty::TypeAlias(qualify_def(db, def, item), TyAttr::default())
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
                // Enum-variant fallback: a path like `Status.Active` (or
                // `pkg.ns.Status.Active`) won't resolve as a type — `Active`
                // isn't a type, it's a variant. Try interpreting the last
                // segment as the variant and the rest as the enum's path.
                if segments.len() >= 2 {
                    let (variant, enum_path) = segments.split_last().unwrap();
                    let enum_short = enum_path.last().unwrap();
                    let enum_resolved =
                        resolve_type(db, package_items, ns_context, enum_path, enum_short);
                    if let Some(def @ Definition::Enum(enum_loc)) = enum_resolved {
                        // Verify the variant actually exists on the enum;
                        // otherwise `Status.Typo` would silently produce a
                        // bogus `Ty::EnumVariant` and downstream code would
                        // never see `UnresolvedType`.
                        let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
                        let enum_data = &item_tree[enum_loc.id(db)];
                        if enum_data.variants.iter().any(|v| v.name == *variant) {
                            return Ty::EnumVariant(
                                qualify_def(db, def, enum_short),
                                variant.clone(),
                                TyAttr::default(),
                            );
                        }
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
        TypeExpr::Bigint { .. } => Ty::Primitive(PrimitiveType::Bigint, TyAttr::default()),
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
        TypeExpr::Function {
            generic_params: function_generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            let mut all_generic_params = generic_params.to_vec();
            all_generic_params.extend(function_generic_params.iter().cloned());
            Ty::Function {
                generic_params: function_generic_params.clone(),
                generic_param_bounds: generic_param_bounds
                    .iter()
                    .map(|bound| {
                        bound.as_ref().map(|bound| {
                            lower_type_expr_in_ns(
                                db,
                                bound,
                                package_items,
                                ns_context,
                                &all_generic_params,
                                diagnostics,
                            )
                        })
                    })
                    .collect(),
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: lower_type_expr_in_ns(
                            db,
                            &p.ty,
                            package_items,
                            ns_context,
                            &all_generic_params,
                            diagnostics,
                        ),
                        mode: if p.optional {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(lower_type_expr_in_ns(
                    db,
                    ret,
                    package_items,
                    ns_context,
                    &all_generic_params,
                    diagnostics,
                )),
                throws: Box::new(
                    throws
                        .as_deref()
                        .map(|throws| {
                            lower_type_expr_in_ns(
                                db,
                                throws,
                                package_items,
                                ns_context,
                                &all_generic_params,
                                diagnostics,
                            )
                        })
                        .unwrap_or(Ty::Never {
                            attr: TyAttr::default(),
                        }),
                ),
                attr: TyAttr::default(),
            }
        }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use baml_base::Name;
    use baml_compiler2_ast::{FunctionTypeParam, TypeExpr};
    use baml_compiler2_hir::package::PackageItems;
    use baml_workspace::Project;
    use rustc_hash::FxHashMap;

    use super::*;

    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        project: Option<Project>,
    }

    impl TestDb {
        fn init(&mut self) {
            self.project = Some(Project::new(self, PathBuf::from("."), Vec::new()));
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl baml_workspace::Db for TestDb {
        fn project(&self) -> Project {
            self.project.expect("TestDb not initialized")
        }
    }

    #[salsa::db]
    impl baml_compiler2_hir::Db for TestDb {}

    #[salsa::db]
    impl baml_compiler2_ppir::Db for TestDb {}

    #[salsa::db]
    impl crate::Db for TestDb {}

    fn path(name: &str) -> TypeExpr {
        TypeExpr::Path {
            segments: vec![Name::new(name)],
            generic_args: vec![],
            attrs: vec![],
        }
    }

    #[test]
    fn substitute_self_recurses_into_function_type() {
        let type_expr = TypeExpr::Function {
            generic_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            params: vec![FunctionTypeParam {
                name: Some(Name::new("value")),
                optional: false,
                ty: path("Self"),
            }],
            ret: Box::new(path("Self")),
            throws: Some(Box::new(path("Self"))),
            attrs: vec![],
        };
        let replacement = TypeExpr::Path {
            segments: vec![Name::new("Named")],
            generic_args: vec![TypeExpr::Int { attrs: vec![] }],
            attrs: vec![],
        };

        let TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } = substitute_self_in(&type_expr, &replacement)
        else {
            panic!("expected function type");
        };

        assert_eq!(params[0].ty, replacement);
        assert_eq!(*ret, replacement);
        assert_eq!(throws.map(|ty| *ty), Some(replacement));
    }

    #[test]
    fn substitute_paths_recurses_into_function_type() {
        let type_expr = TypeExpr::Function {
            generic_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            params: vec![FunctionTypeParam {
                name: Some(Name::new("value")),
                optional: false,
                ty: path("T"),
            }],
            ret: Box::new(path("T")),
            throws: Some(Box::new(path("E"))),
            attrs: vec![],
        };
        let mut subst = std::collections::HashMap::new();
        subst.insert(Name::new("T"), TypeExpr::String { attrs: vec![] });
        subst.insert(Name::new("E"), TypeExpr::Bool { attrs: vec![] });

        let TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } = substitute_paths_in(&type_expr, &subst)
        else {
            panic!("expected function type");
        };

        assert!(matches!(params[0].ty, TypeExpr::String { .. }));
        assert!(matches!(*ret, TypeExpr::String { .. }));
        assert!(matches!(throws.as_deref(), Some(TypeExpr::Bool { .. })));
    }

    #[test]
    fn lower_function_type_preserves_parameter_optionality() {
        let mut db = TestDb::default();
        db.init();
        let package_items = PackageItems {
            namespaces: FxHashMap::default(),
            extra: None,
        };
        let type_expr = TypeExpr::Function {
            generic_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            params: vec![
                FunctionTypeParam {
                    name: Some(Name::new("query")),
                    optional: false,
                    ty: TypeExpr::String { attrs: vec![] },
                },
                FunctionTypeParam {
                    name: Some(Name::new("limit")),
                    optional: true,
                    ty: TypeExpr::Int { attrs: vec![] },
                },
            ],
            ret: Box::new(TypeExpr::Bool { attrs: vec![] }),
            throws: None,
            attrs: vec![],
        };
        let mut diagnostics = Vec::new();

        let ty = lower_type_expr_in_ns(&db, &type_expr, &package_items, &[], &[], &mut diagnostics);

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:?}"
        );
        let Ty::Function { params, .. } = ty else {
            panic!("expected function type, got {ty:?}");
        };
        let params: Vec<&FunctionParamTy> = params.iter().collect();

        assert_eq!(params[0].name.as_deref(), Some("query"));
        assert_eq!(params[0].mode, FunctionParamMode::Required);
        assert_eq!(params[1].name.as_deref(), Some("limit"));
        assert_eq!(params[1].mode, FunctionParamMode::Optional);
    }
}
