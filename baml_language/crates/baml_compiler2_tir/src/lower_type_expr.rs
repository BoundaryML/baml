//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};

use crate::{
    infer_context::TirTypeError,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, PrimitiveType, QualifiedTypeName, Ty},
};

/// A diagnostic sink: each lowering diagnostic is forwarded to this callback
/// in source-walk order. Internal lowering threads a `DiagSink` instead of
/// collecting into a throwaway `Vec`, so callers can route each diagnostic
/// directly to its final report target (or discard it) without an
/// intermediate drain. The public `Vec`-taking entry points are thin shims
/// over the sink-based cores.
pub type DiagSink<'a> = &'a mut dyn FnMut(TirTypeError);

/// The read-only triple threaded through the recursive lowering cores: the
/// salsa database, the resolved package items to resolve names against, and the
/// namespace context (e.g. `["fs"]`) that unqualified paths resolve relative
/// to first. All three are sourced from the same salsa db, so they share one
/// `'db` lifetime. The per-recursion-varying inputs (`generic_params` /
/// `bindings`) are deliberately kept as separate explicit parameters.
#[derive(Clone, Copy)]
pub struct LoweringCtx<'db> {
    pub db: &'db dyn crate::Db,
    pub package_items: &'db PackageItems<'db>,
    pub ns_context: &'db [baml_base::Name],
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

/// Sink-based variant of [`lower_type_expr_in_ns`]: forwards each diagnostic
/// to `sink` in source-walk order instead of collecting into a `Vec`.
pub fn lower_type_expr_in_ns_into(
    db: &dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    sink: DiagSink<'_>,
) -> Ty {
    let ctx = LoweringCtx {
        db,
        package_items,
        ns_context,
    };
    lower_in_ns(ctx, type_expr, generic_params, sink)
}

/// BEP-044: walk `type_expr` and replace any `Self` reference with
/// `replacement`. Used by signature lowering to pre-resolve `Self` to
/// the enclosing class/interface's type expression before regular
/// resolution runs.
pub fn substitute_self_in(type_expr: &TypeExpr, replacement: &TypeExpr) -> TypeExpr {
    substitute_in(type_expr, &|segments, generic_args| {
        (segments.len() == 1 && generic_args.is_empty() && segments[0].as_str() == "Self")
            .then(|| replacement.clone())
    })
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
    substitute_in(type_expr, &|segments, generic_args| {
        if segments.len() == 1 && generic_args.is_empty() {
            subst.get(&segments[0]).cloned()
        } else {
            None
        }
    })
}

/// Shared recursion for the `substitute_*` walkers. `replace` inspects each
/// `Path`'s segments/generic-args and returns a replacement to swap in, or
/// `None` to recurse into it normally. All non-`Path` variants are rebuilt
/// identically regardless of the replacement decision.
fn substitute_in(
    ty: &TypeExpr,
    replace: &impl Fn(&[baml_base::Name], &[TypeExpr]) -> Option<TypeExpr>,
) -> TypeExpr {
    match ty {
        TypeExpr::Path {
            segments,
            generic_args,
            attrs,
        } => {
            if let Some(replacement) = replace(segments, generic_args) {
                return replacement;
            }
            TypeExpr::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_in(a, replace))
                    .collect(),
                attrs: attrs.clone(),
            }
        }
        TypeExpr::List { inner, attrs } => TypeExpr::List {
            inner: Box::new(substitute_in(inner, replace)),
            attrs: attrs.clone(),
        },
        TypeExpr::Optional { inner, attrs } => TypeExpr::Optional {
            inner: Box::new(substitute_in(inner, replace)),
            attrs: attrs.clone(),
        },
        TypeExpr::Map { key, value, attrs } => TypeExpr::Map {
            key: Box::new(substitute_in(key, replace)),
            value: Box::new(substitute_in(value, replace)),
            attrs: attrs.clone(),
        },
        TypeExpr::Union { variants, attrs } => TypeExpr::Union {
            variants: variants.iter().map(|v| substitute_in(v, replace)).collect(),
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
                .map(|bound| bound.as_ref().map(|bound| substitute_in(bound, replace)))
                .collect(),
            params: params
                .iter()
                .map(|param| {
                    let mut param = param.clone();
                    param.ty = substitute_in(&param.ty, replace);
                    param
                })
                .collect(),
            ret: Box::new(substitute_in(ret, replace)),
            throws: throws
                .as_ref()
                .map(|ty| Box::new(substitute_in(ty, replace))),
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
    ctx: LoweringCtx<'db>,
    path: &[baml_base::Name],
    item: &baml_base::Name,
) -> Option<Definition<'db>> {
    let seg_ns = &path[..path.len() - 1];
    // When we have a namespace context, try the qualified path first.
    // e.g. for ns_context=["fs"], path=["File"], try namespace ["fs"] item "File".
    let resolved = if !ctx.ns_context.is_empty() {
        let ns: Vec<baml_base::Name> = ctx
            .ns_context
            .iter()
            .chain(seg_ns.iter())
            .cloned()
            .collect();
        ctx.package_items.lookup_type(&ns, item)
    } else {
        ctx.package_items.lookup_type(seg_ns, item)
    };
    // Cross-package fallback: if not found in the current package and
    // the first segment is a known package name, look in that package
    // using the remaining segments. This supports synthetic type
    // references like `baml.llm.PromptAst` in companion functions.
    resolved.or_else(|| {
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                ctx.package_items
                    .lookup_type(&path[1..path.len() - 1], item)
            } else {
                let pkg_id = PackageId::new(ctx.db, path[0].clone());
                let pkg = baml_compiler2_ppir::package_items(ctx.db, pkg_id);
                pkg.lookup_type(&path[1..path.len() - 1], item)
            }
        } else {
            None
        }
    })
}

/// Lower each generic arg in turn, surfacing any nested diagnostics.
fn lower_args(
    ctx: LoweringCtx<'_>,
    generic_args: &[TypeExpr],
    generic_params: &[baml_base::Name],
    sink: DiagSink<'_>,
) -> Vec<Ty> {
    generic_args
        .iter()
        .map(|ga| lower_in_ns(ctx, ga, generic_params, sink))
        .collect()
}

/// Lower a non-generic type reference (enum / type alias): lower any generic
/// args for their own diagnostics, flag them as not-generic, then build the
/// `Ty` via `make_ty`.
#[allow(clippy::too_many_arguments)]
fn lower_non_generic(
    ctx: LoweringCtx<'_>,
    def: Definition,
    item: &baml_base::Name,
    generic_args: &[TypeExpr],
    generic_params: &[baml_base::Name],
    sink: DiagSink<'_>,
    kind: &'static str,
    make_ty: impl FnOnce(QualifiedTypeName) -> Ty,
) -> Ty {
    lower_args(ctx, generic_args, generic_params, sink);
    // Flag generic args supplied for a non-generic type (enum / type alias).
    // The args were lowered just above so their own diagnostics still surface.
    if !generic_args.is_empty() {
        sink(TirTypeError::TypeIsNotGeneric {
            type_name: item.clone(),
            kind,
        });
    }
    make_ty(qualify_def(ctx.db, def, item))
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
    let mut push = |e| diagnostics.push(e);
    let ctx = LoweringCtx {
        db,
        package_items,
        ns_context,
    };
    lower_in_ns(ctx, type_expr, generic_params, &mut push)
}

/// Sink-based core of [`lower_type_expr_in_ns`]. The public `Vec`-taking entry
/// points are thin shims over this function.
fn lower_in_ns(
    ctx: LoweringCtx<'_>,
    type_expr: &TypeExpr,
    generic_params: &[baml_base::Name],
    sink: DiagSink<'_>,
) -> Ty {
    match type_expr {
        TypeExpr::Path {
            segments,
            generic_args,
            ..
        } => {
            let item = segments.last().expect("non-empty path");
            let resolved = resolve_type(ctx, segments, item);

            if let Some(def) = resolved {
                match def {
                    Definition::Class(_) => {
                        let lowered_args = lower_args(ctx, generic_args, generic_params, sink);

                        // BEP-034: `baml.future.Future<T, E>` resolves to the
                        // dedicated `Ty::Future` variant rather than the
                        // generic `Ty::Class`. The class declaration exists
                        // as a regular `.baml` file (for method dispatch via
                        // the standard PackageBaml path), but `spawn` /
                        // `await` keys off `Ty::Future` directly. Mirrors
                        // how `int[]` resolves to `Ty::List` even though
                        // `class Array<T>` is a regular class declaration.
                        let qtn = qualify_def(ctx.db, def, item);
                        if qtn.is_builtin_future() && lowered_args.len() == 2 {
                            return Ty::Future(
                                Box::new(lowered_args[0].clone()),
                                Box::new(lowered_args[1].clone()),
                            );
                        }

                        // Class arity mismatches are not flagged here — downstream
                        // checks (pattern subtype check, generic substitution,
                        // assignment checks) already produce a clearer diagnostic
                        // (e.g. `expected Box<int>, got Box`) when the arity
                        // mismatch actually matters at the use site.
                        Ty::Class(qtn, lowered_args)
                    }
                    Definition::Interface(_) => {
                        let lowered_args = lower_args(ctx, generic_args, generic_params, sink);
                        Ty::Interface(qualify_def(ctx.db, def, item), lowered_args)
                    }
                    Definition::Enum(_) => lower_non_generic(
                        ctx,
                        def,
                        item,
                        generic_args,
                        generic_params,
                        sink,
                        "enum",
                        Ty::Enum,
                    ),
                    Definition::TypeAlias(_) => lower_non_generic(
                        ctx,
                        def,
                        item,
                        generic_args,
                        generic_params,
                        sink,
                        "type alias",
                        Ty::TypeAlias,
                    ),
                    _ => Ty::Unknown,
                }
            } else {
                if segments.len() == 1 {
                    if generic_params.iter().any(|p| *p == segments[0]) {
                        return Ty::TypeVar(segments[0].clone());
                    }
                }
                // Enum-variant fallback: a path like `Status.Active` (or
                // `pkg.ns.Status.Active`) won't resolve as a type — `Active`
                // isn't a type, it's a variant. Try interpreting the last
                // segment as the variant and the rest as the enum's path.
                if segments.len() >= 2 {
                    let (variant, enum_path) = segments.split_last().unwrap();
                    let enum_short = enum_path.last().unwrap();
                    let enum_resolved = resolve_type(ctx, enum_path, enum_short);
                    if let Some(def @ Definition::Enum(enum_loc)) = enum_resolved {
                        // Verify the variant actually exists on the enum;
                        // otherwise `Status.Typo` would silently produce a
                        // bogus `Ty::EnumVariant` and downstream code would
                        // never see `UnresolvedType`.
                        let item_tree =
                            baml_compiler2_ppir::file_item_tree(ctx.db, enum_loc.file(ctx.db));
                        let enum_data = &item_tree[enum_loc.id(ctx.db)];
                        if enum_data.variants.iter().any(|v| v.name == *variant) {
                            return Ty::EnumVariant(
                                qualify_def(ctx.db, def, enum_short),
                                variant.clone(),
                            );
                        }
                    }
                }
                let name_str = dotted(segments);
                // Scan all namespaces for the item name to build "did you mean" suggestions.
                // Only do this for single-segment bare names — multi-segment paths already
                // encode the intended namespace.
                let mut suggestions = Vec::new();
                if segments.len() == 1 {
                    for (ns_path, ns_items) in &ctx.package_items.namespaces {
                        if ns_items.types.contains_key(item) {
                            if ns_path.is_empty() {
                                suggestions.push(format!("root.{item}"));
                            } else {
                                let ns_str = dotted(ns_path);
                                suggestions.push(format!("root.{ns_str}.{item}"));
                            }
                        }
                    }
                    suggestions.sort();
                }
                sink(TirTypeError::UnresolvedType {
                    name: baml_base::Name::new(&name_str),
                    suggestions,
                });
                Ty::Unknown
            }
        }
        TypeExpr::Int { .. } => Ty::Primitive(PrimitiveType::Int),
        TypeExpr::Bigint { .. } => Ty::Primitive(PrimitiveType::Bigint),
        TypeExpr::Float { .. } => Ty::Primitive(PrimitiveType::Float),
        TypeExpr::String { .. } => Ty::Primitive(PrimitiveType::String),
        TypeExpr::Bool { .. } => Ty::Primitive(PrimitiveType::Bool),
        TypeExpr::Null { .. } => Ty::Primitive(PrimitiveType::Null),
        TypeExpr::Never { .. } => Ty::Never,
        TypeExpr::Void { .. } => Ty::Void,
        TypeExpr::Uint8Array { .. } => Ty::Primitive(PrimitiveType::Uint8Array),
        TypeExpr::Media { kind, .. } => Ty::Primitive(match kind {
            baml_base::MediaKind::Image => PrimitiveType::Image,
            baml_base::MediaKind::Audio => PrimitiveType::Audio,
            baml_base::MediaKind::Video => PrimitiveType::Video,
            baml_base::MediaKind::Pdf => PrimitiveType::Pdf,
            // Generic media — treated as unknown for type resolution purposes
            baml_base::MediaKind::Generic => {
                return Ty::Unknown;
            }
        }),
        TypeExpr::Optional { inner, .. } => {
            Ty::Optional(Box::new(lower_in_ns(ctx, inner, generic_params, sink)))
        }
        TypeExpr::List { inner, .. } => {
            Ty::List(Box::new(lower_in_ns(ctx, inner, generic_params, sink)))
        }
        TypeExpr::Map { key, value, .. } => Ty::Map(
            Box::new(lower_in_ns(ctx, key, generic_params, sink)),
            Box::new(lower_in_ns(ctx, value, generic_params, sink)),
        ),
        TypeExpr::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| lower_in_ns(ctx, m, generic_params, sink))
                .collect(),
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
                        bound
                            .as_ref()
                            .map(|bound| lower_in_ns(ctx, bound, &all_generic_params, sink))
                    })
                    .collect(),
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: lower_in_ns(ctx, &p.ty, &all_generic_params, sink),
                        mode: if p.optional {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(lower_in_ns(ctx, ret, &all_generic_params, sink)),
                throws: Box::new(
                    throws
                        .as_deref()
                        .map(|throws| lower_in_ns(ctx, throws, &all_generic_params, sink))
                        .unwrap_or(Ty::Never),
                ),
            }
        }
        TypeExpr::Literal { value: lit, .. } => Ty::Literal(lit.clone(), Freshness::Regular),
        TypeExpr::BuiltinUnknown { .. } => Ty::BuiltinUnknown,
        TypeExpr::Error { .. } | TypeExpr::Unknown { .. } => Ty::Unknown,
        // Dedicated Ty::Type variant — see ty.rs doc comment for design rationale.
        TypeExpr::Type { .. } => Ty::Type,
        // `$rust_type` — opaque Rust-managed state field type.
        TypeExpr::Rust { .. } => Ty::RustType,
    }
}

/// Join name segments with `.` for diagnostic messages.
fn dotted(parts: &[baml_base::Name]) -> String {
    parts
        .iter()
        .map(smol_str::SmolStr::as_str)
        .collect::<Vec<_>>()
        .join(".")
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
