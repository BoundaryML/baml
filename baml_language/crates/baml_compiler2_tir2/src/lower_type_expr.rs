//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};
use rustc_hash::FxHashSet;

use crate::{
    infer_context::TirTypeError,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, MediaKind, QualifiedTypeName, Ty, TyAttr},
};

/// The scope a `TypeExpr` is lowered in — everything that varies between a class
/// signature, an instantiated callee, a type alias, or a method body. Lowering
/// (`lower_type_expr`) is a pure recursion over the `TypeExpr`; the context supplies
/// the scope-specific resolutions and nothing else.
///
/// A type variable always lowers to `Ty::TypeVar(name)` — filling it with a concrete
/// argument (e.g. instantiating `Array::at` at `int[]`) is a separate [`substitute_ty`]
/// pass over the already-lowered `Ty`, never part of lowering. So the context never
/// substitutes; it only reports whether a name *is* a type variable (and its bounds).
///
/// [`substitute_ty`]: crate::generics::substitute_ty
pub trait TypeExprContext<'db> {
    fn db(&self) -> &'db dyn crate::Db;

    /// Resolve a type name/path to its definition in this scope (the implementor owns
    /// namespace-awareness). `Err` carries the "did you mean" suggestions for the
    /// [`TirTypeError::UnresolvedType`] diagnostic (each a fully qualified path).
    fn resolve_type(
        &self,
        segments: &[baml_base::Name],
    ) -> Result<Definition<'db>, Box<[baml_base::Name]>>;

    /// What `Self` lowers to here, or `None` where `Self` is not in scope
    /// (free functions, type aliases).
    fn lower_self(&self) -> Option<Ty>;

    /// If `name` is an in-scope type variable: its interface bounds (empty = unbounded,
    /// used to resolve a `T.member` projection). It lowers to `Ty::TypeVar(name)`.
    /// `None` = `name` is not a type variable here; resolve it as a type name instead.
    fn type_var_bounds(&self, name: &baml_base::Name) -> Option<Box<[baml_type::Interface]>>;
}

/// "Did you mean" candidates for an unresolved single-segment name: every namespace in
/// `package_items` that declares `item`, each as a `root.…` path. Empty for multi-segment paths.
fn type_suggestions(
    package_items: &PackageItems<'_>,
    segments: &[baml_base::Name],
) -> Box<[baml_base::Name]> {
    // Only single-segment bare names get suggestions — multi-segment paths already
    // encode the intended namespace.
    if segments.len() != 1 {
        return Box::new([]);
    }
    let item = &segments[0];
    let mut suggestions: Vec<String> = Vec::new();
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
    suggestions
        .iter()
        .map(String::as_str)
        .map(baml_base::Name::new)
        .collect()
}

/// The general lowering scope: a package's items, a namespace, the in-scope type-variable
/// names, and their bounds. Used by every `lower_type_expr_in_ns[_bounded]` wrapper.
pub(crate) struct ScopeCtx<'a, 'db> {
    pub db: &'db dyn crate::Db,
    pub package_items: &'a PackageItems<'db>,
    pub ns_context: &'a [baml_base::Name],
    pub generic_params: &'a [baml_base::Name],
    /// Each in-scope type variable's interface bound, as a *constraint*
    /// ([`baml_type::Interface`], which may pin only some associated types) — never a
    /// [`Ty::Interface`] existential, which would have to specify them all.
    pub bounds: &'a rustc_hash::FxHashMap<baml_base::Name, baml_type::Interface>,
    /// What `Self` lowers to in this scope, or `None` where `Self` isn't valid.
    pub self_ty: Option<Ty>,
}

impl<'db> TypeExprContext<'db> for ScopeCtx<'_, 'db> {
    fn db(&self) -> &'db dyn crate::Db {
        self.db
    }

    fn resolve_type(
        &self,
        segments: &[baml_base::Name],
    ) -> Result<Definition<'db>, Box<[baml_base::Name]>> {
        resolve_type_in(self.db, self.package_items, self.ns_context, segments)
            .ok_or_else(|| type_suggestions(self.package_items, segments))
    }

    fn lower_self(&self) -> Option<Ty> {
        self.self_ty.clone()
    }

    fn type_var_bounds(&self, name: &baml_base::Name) -> Option<Box<[baml_type::Interface]>> {
        self.generic_params
            .contains(name)
            .then(|| self.bounds.get(name).cloned().into_iter().collect())
    }
}

/// Like [`lower_type_expr`], but resolves unqualified names relative to
/// `ns_context` first (e.g. `["fs"]`), falling back to the package root.
///
/// Use this when lowering type expressions from a function/class signature
/// that lives in a sub-namespace of its package. For example, `File` in
/// `baml/fs.baml` (namespace `["fs"]`) resolves via `lookup_type(&["fs", "File"])`.
fn projection_chain_from_root(
    root: TypeExpr,
    members: &[baml_base::Name],
    attrs: &[baml_compiler2_ast::RawAttribute],
) -> TypeExpr {
    members
        .iter()
        .enumerate()
        .fold(root, |base, (idx, member)| {
            TypeExpr::AssociatedTypeProjection {
                base: Box::new(base),
                interface: None,
                member: member.clone(),
                attrs: if idx + 1 == members.len() {
                    attrs.to_vec()
                } else {
                    Vec::new()
                },
            }
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
            associated_type_bindings,
            attrs,
        } => {
            if generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && let Some(replacement) = subst.get(&segments[0])
            {
                if segments.len() == 1 {
                    return replacement.clone();
                }
                return projection_chain_from_root(replacement.clone(), &segments[1..], attrs);
            }
            TypeExpr::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_paths_walk(a, subst))
                    .collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(substitute_paths_walk(&binding.ty, subst)),
                    })
                    .collect(),
                attrs: attrs.clone(),
            }
        }
        TypeExpr::AssociatedTypeProjection {
            base,
            interface,
            member,
            attrs,
        } => TypeExpr::AssociatedTypeProjection {
            base: Box::new(substitute_paths_walk(base, subst)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute_paths_walk(interface, subst))),
            member: member.clone(),
            attrs: attrs.clone(),
        },
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
            params,
            ret,
            throws,
            attrs,
        } => TypeExpr::Function {
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

/// Build a `TypeExpr::Path` referring to a single named type — used to desugar the bare
/// `self` receiver to a `Self` path so it flows through normal parameter lowering.
pub fn type_expr_for_name(name: baml_base::Name) -> TypeExpr {
    TypeExpr::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
}

fn can_be_associated_type_projection_base(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Class(..)
            | Ty::Interface(..)
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::AssociatedTypeProjection { .. }
    )
}

pub fn lower_type_expr_in_ns(
    db: &dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    lower_type_expr_in_ns_bounded(
        db,
        type_expr,
        package_items,
        ns_context,
        generic_params,
        &rustc_hash::FxHashMap::default(),
        diagnostics,
    )
}

/// Resolve a type name/path to its definition, with `ns_context` as the namespace
/// prefix. Tries the namespace-qualified own-package lookup, then a cross-package lookup
/// (`root.ns.Name` or `pkg.ns.Name`), then the `$stream` companion (whose base class/alias
/// the caller re-qualifies under the `$stream` name). `None` when unresolved.
pub(crate) fn resolve_type_in<'db>(
    db: &'db dyn crate::Db,
    package_items: &PackageItems<'db>,
    ns_context: &[baml_base::Name],
    segments: &[baml_base::Name],
) -> Option<Definition<'db>> {
    let item = segments.last().expect("non-empty path");
    let seg_ns = &segments[..segments.len() - 1];
    let resolved = if !ns_context.is_empty() {
        let ns: Vec<baml_base::Name> = ns_context.iter().chain(seg_ns.iter()).cloned().collect();
        package_items.lookup_type(&ns, item)
    } else {
        package_items.lookup_type(seg_ns, item)
    };
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
    resolved.or_else(|| {
        let base = item.as_str().strip_suffix("$stream")?;
        let base_name = baml_base::Name::new(base);
        let base_def = if !ns_context.is_empty() {
            let ns: Vec<baml_base::Name> =
                ns_context.iter().chain(seg_ns.iter()).cloned().collect();
            package_items.lookup_type(&ns, &base_name)
        } else {
            package_items.lookup_type(seg_ns, &base_name)
        }
        .or_else(|| {
            if segments.len() >= 2 {
                if segments[0].as_str() == "root" {
                    package_items.lookup_type(&segments[1..segments.len() - 1], &base_name)
                } else {
                    let pkg_id = PackageId::new(db, segments[0].clone());
                    let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                    pkg.lookup_type(&segments[1..segments.len() - 1], &base_name)
                }
            } else {
                None
            }
        })?;
        // Only classes and aliases get a `$stream` companion.
        matches!(base_def, Definition::Class(_) | Definition::TypeAlias(_)).then_some(base_def)
    })
}

/// Resolve an AST `TypeExpr` to a `Ty`, driven entirely by `ctx` (name
/// resolution, `Self`, and type-variable bounds). Lowering is a pure recursion
/// over the `TypeExpr`: the scope-specific decisions all funnel through
/// [`TypeExprContext`]. Unresolved names become `Ty::Unknown` and push an
/// `UnresolvedType` diagnostic to `diagnostics`.
pub fn lower_type_expr(
    type_expr: &TypeExpr,
    ctx: &dyn TypeExprContext<'_>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    let db = ctx.db();
    match type_expr {
        TypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            // `Self` / `Self.Member…`: resolve the receiver through the context, but only
            // when `Self` is in scope (`lower_self` is `Some`). A bare `Self` is the
            // receiver type; `Self.Item` is an associated-type projection rooted at it.
            // Falls through otherwise, so a stray `Self` still takes the unresolved path.
            if segments[0].as_str() == "Self"
                && generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && let Some(self_ty) = ctx.lower_self()
            {
                if segments.len() == 1 {
                    return self_ty;
                }
                let mut ty = self_ty;
                for member in &segments[1..] {
                    let lowered = crate::builder::associated_projection::lower_projection(
                        ctx,
                        ty,
                        None,
                        member.clone(),
                    );
                    diagnostics.extend(lowered.diagnostics);
                    ty = lowered.ty;
                }
                return ty;
            }
            match ctx.resolve_type(segments) {
                Ok(def) => {
                    let short = segments.last().expect("non-empty path");
                    match def {
                        Definition::Class(class_loc) => {
                            // Collect and lower generic args, storing them in Ty::Class.
                            let lowered_args: Vec<Ty> = generic_args
                                .iter()
                                .map(|ga| lower_type_expr(ga, ctx, diagnostics))
                                .collect();

                            let class_tree =
                                baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
                            let expected_type_args = class_tree
                                .classes
                                .get(&class_loc.id(db))
                                .map(|class| class.generic_params.len())
                                .unwrap_or(0);
                            // A bare generic name (`generic_args.is_empty()`) is left
                            // unchecked here: it is a deliberate wildcard in several
                            // positions (`reflect.type_of<Box>()`, construction
                            // `Box { .. }` where args infer from fields, an interface's
                            // own `Self` type). Object construction that cannot infer
                            // its args is reported by `infer_object_expr`
                            // (`CannotInferTypeParameter`) instead.
                            if !generic_args.is_empty() && generic_args.len() != expected_type_args
                            {
                                diagnostics.push(TirTypeError::WrongNumberOfTypeArgs {
                                    type_name: short.clone(),
                                    expected: expected_type_args,
                                    got: generic_args.len(),
                                });
                            }

                            // BEP-034: `baml.future.Future<T, E>` resolves to the
                            // dedicated `Ty::Future` variant rather than the
                            // generic `Ty::Class`. The class declaration exists
                            // as a regular `.baml` file (for method dispatch via
                            // the standard PackageBaml path), but `spawn` /
                            // `await` keys off `Ty::Future` directly. Mirrors
                            // how `int[]` resolves to `Ty::List` even though
                            // `class Array<T>` is a regular class declaration.
                            let qtn = qualify_def(db, def, short);
                            if qtn.name().as_str() == "Future"
                                && qtn.package().as_str() == "baml"
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

                            Ty::Class(qtn, lowered_args, TyAttr::default())
                        }
                        Definition::Interface(iface_loc) => {
                            // Same generic-arg handling as `Class` — interface
                            // parameters are valid in the same positions.
                            let lowered_args: Vec<Ty> = generic_args
                                .iter()
                                .map(|ga| lower_type_expr(ga, ctx, diagnostics))
                                .collect();
                            let iface_tree =
                                baml_compiler2_ppir::file_item_tree(db, iface_loc.file(db));
                            let expected_type_args = iface_tree
                                .interfaces
                                .get(&iface_loc.id(db))
                                .map(|iface| iface.generic_params.len())
                                .unwrap_or(0);
                            if !generic_args.is_empty() && generic_args.len() != expected_type_args
                            {
                                diagnostics.push(TirTypeError::WrongNumberOfTypeArgs {
                                    type_name: short.clone(),
                                    expected: expected_type_args,
                                    got: generic_args.len(),
                                });
                            }
                            let known_associated_types: FxHashSet<baml_base::Name> = iface_tree
                                .interfaces
                                .get(&iface_loc.id(db))
                                .map(|iface| {
                                    iface
                                        .associated_types
                                        .iter()
                                        .map(|assoc| assoc.name.clone())
                                        .collect()
                                })
                                .unwrap_or_default();
                            let mut seen_associated_bindings = FxHashSet::default();
                            let lowered_associated_bindings: Vec<(baml_base::Name, Ty)> =
                                associated_type_bindings
                                    .iter()
                                    .map(|binding| {
                                        if !known_associated_types.contains(&binding.name) {
                                            diagnostics.push(TirTypeError::UnresolvedType {
                                                name: binding.name.clone(),
                                                suggestions: known_associated_types
                                                    .iter()
                                                    .cloned()
                                                    .collect(),
                                            });
                                        }
                                        if !seen_associated_bindings.insert(binding.name.clone()) {
                                            diagnostics.push(TirTypeError::TypeMismatch {
                                                expected: Ty::Unknown {
                                                    attr: TyAttr::default(),
                                                },
                                                got: Ty::Unknown {
                                                    attr: TyAttr::default(),
                                                },
                                            });
                                        }
                                        (
                                            binding.name.clone(),
                                            lower_type_expr(&binding.ty, ctx, diagnostics),
                                        )
                                    })
                                    .collect();
                            Ty::Interface(
                                qualify_def(db, def, short),
                                lowered_args,
                                lowered_associated_bindings,
                                TyAttr::default(),
                            )
                        }
                        Definition::Enum(_) => {
                            // Enums are not generic — validate args and emit a diagnostic if any were supplied.
                            for ga in generic_args {
                                let _ = lower_type_expr(ga, ctx, diagnostics);
                            }
                            if !generic_args.is_empty() {
                                diagnostics.push(TirTypeError::TypeIsNotGeneric {
                                    type_name: short.clone(),
                                    kind: "enum",
                                });
                            }
                            Ty::Enum(qualify_def(db, def, short), TyAttr::default())
                        }
                        Definition::TypeAlias(_) => {
                            // Type aliases are not generic — validate args and emit a diagnostic if any were supplied.
                            for ga in generic_args {
                                let _ = lower_type_expr(ga, ctx, diagnostics);
                            }
                            if !generic_args.is_empty() {
                                diagnostics.push(TirTypeError::TypeIsNotGeneric {
                                    type_name: short.clone(),
                                    kind: "type alias",
                                });
                            }
                            Ty::TypeAlias(qualify_def(db, def, short), TyAttr::default())
                        }
                        // Let bindings are values, not types — produce Unknown in a type position.
                        _ => Ty::Unknown {
                            attr: TyAttr::default(),
                        },
                    }
                }
                Err(suggestions) => {
                    // A single-segment name that is an in-scope type variable
                    // (e.g. T, K, V) lowers to `Ty::TypeVar`, not an error.
                    if segments.len() == 1 && ctx.type_var_bounds(&segments[0]).is_some() {
                        return Ty::TypeVar(segments[0].clone(), TyAttr::default());
                    }
                    // Enum-variant fallback: a path like `Status.Active` (or
                    // `pkg.ns.Status.Active`) won't resolve as a type — `Active`
                    // isn't a type, it's a variant. Try interpreting the last
                    // segment as the variant and the rest as the enum's path.
                    if segments.len() >= 2 {
                        let (variant, enum_path) = segments.split_last().unwrap();
                        let enum_short = enum_path.last().unwrap();
                        if let Ok(def @ Definition::Enum(enum_loc)) = ctx.resolve_type(enum_path) {
                            // Verify the variant actually exists on the enum;
                            // otherwise `Status.Typo` would silently produce a
                            // bogus `Ty::EnumVariant` and downstream code would
                            // never see `UnresolvedType`.
                            let item_tree =
                                baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
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
                    // Associated type projection fallback: after ordinary type
                    // paths and enum variants have had first refusal, treat
                    // `Base.Member` as shorthand for an associated type projection.
                    // This preserves enum disambiguation (`Status.Active`) and
                    // still accepts aliases, type variables, concrete classes,
                    // interfaces, and nested projections as projection bases.
                    if segments.len() >= 2
                        && generic_args.is_empty()
                        && associated_type_bindings.is_empty()
                    {
                        let base_expr = TypeExpr::Path {
                            segments: segments[..segments.len() - 1].to_vec(),
                            generic_args: Vec::new(),
                            associated_type_bindings: Vec::new(),
                            attrs: Vec::new(),
                        };
                        let mut base_diags = Vec::new();
                        let base_ty = lower_type_expr(&base_expr, ctx, &mut base_diags);
                        if base_diags.is_empty() && can_be_associated_type_projection_base(&base_ty)
                        {
                            let member = segments.last().expect("non-empty path").clone();
                            let lowered = crate::builder::associated_projection::lower_projection(
                                ctx, base_ty, None, member,
                            );
                            diagnostics.extend(lowered.diagnostics);
                            return lowered.ty;
                        }
                    }
                    let name_str = segments
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    diagnostics.push(TirTypeError::UnresolvedType {
                        name: baml_base::Name::new(&name_str),
                        suggestions,
                    });
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            }
        }
        TypeExpr::Int { .. } => Ty::Int {
            attr: TyAttr::default(),
        },
        TypeExpr::Bigint { .. } => Ty::Bigint {
            attr: TyAttr::default(),
        },
        TypeExpr::Float { .. } => Ty::Float {
            attr: TyAttr::default(),
        },
        TypeExpr::String { .. } => Ty::String {
            attr: TyAttr::default(),
        },
        TypeExpr::Bool { .. } => Ty::Bool {
            attr: TyAttr::default(),
        },
        TypeExpr::Null { .. } => Ty::Null {
            attr: TyAttr::default(),
        },
        TypeExpr::Never { .. } => Ty::Never {
            attr: TyAttr::default(),
        },
        TypeExpr::Void { .. } => Ty::Void {
            attr: TyAttr::default(),
        },
        TypeExpr::Uint8Array { .. } => Ty::Uint8Array {
            attr: TyAttr::default(),
        },
        TypeExpr::Media { kind, .. } => match kind {
            baml_base::MediaKind::Image => Ty::Media(MediaKind::Image, TyAttr::default()),
            baml_base::MediaKind::Audio => Ty::Media(MediaKind::Audio, TyAttr::default()),
            baml_base::MediaKind::Video => Ty::Media(MediaKind::Video, TyAttr::default()),
            baml_base::MediaKind::Pdf => Ty::Media(MediaKind::Pdf, TyAttr::default()),
            // Generic media — treated as unknown for type resolution purposes
            baml_base::MediaKind::Generic => Ty::Unknown {
                attr: TyAttr::default(),
            },
        },
        // `T?` is sugar for `T | null` — lower it directly to a nullable union.
        TypeExpr::Optional { inner, .. } => Ty::optional(lower_type_expr(inner, ctx, diagnostics)),
        TypeExpr::List { inner, .. } => Ty::List(
            Box::new(lower_type_expr(inner, ctx, diagnostics)),
            TyAttr::default(),
        ),
        TypeExpr::Map { key, value, .. } => Ty::Map {
            key: Box::new(lower_type_expr(key, ctx, diagnostics)),
            value: Box::new(lower_type_expr(value, ctx, diagnostics)),
            attr: TyAttr::default(),
        },
        TypeExpr::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| lower_type_expr(m, ctx, diagnostics))
                .collect(),
            TyAttr::default(),
        ),
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            // A function type carries no generics of its own; its type variables
            // come from the enclosing context.
            Ty::Function {
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: lower_type_expr(&p.ty, ctx, diagnostics),
                        mode: if p.optional {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(lower_type_expr(ret, ctx, diagnostics)),
                throws: Box::new(
                    throws
                        .as_deref()
                        .map(|throws| lower_type_expr(throws, ctx, diagnostics))
                        .unwrap_or(Ty::Never {
                            attr: TyAttr::default(),
                        }),
                ),
                attr: TyAttr::default(),
            }
        }
        TypeExpr::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => {
            let base_ty = lower_type_expr(base, ctx, diagnostics);
            let explicit_interface = interface
                .as_ref()
                .map(|interface| lower_type_expr(interface, ctx, diagnostics));
            let lowered = crate::builder::associated_projection::lower_projection(
                ctx,
                base_ty,
                explicit_interface,
                member.clone(),
            );
            diagnostics.extend(lowered.diagnostics);
            lowered.ty
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

/// Like [`lower_type_expr_in_ns`], but threads the in-scope type-variable
/// `bounds` (`T extends Named`) through to associated-type-projection
/// resolution, so a projection `T.member` on a bounded type variable can find
/// `T`'s bound interface. A thin wrapper that packages its scope into a
/// [`ScopeCtx`] and delegates to [`lower_type_expr`].
pub fn lower_type_expr_in_ns_bounded(
    db: &dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    bounds: &rustc_hash::FxHashMap<baml_base::Name, baml_type::Interface>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    let ctx = ScopeCtx {
        db,
        package_items,
        ns_context,
        generic_params,
        bounds,
        self_ty: None,
    };
    lower_type_expr(type_expr, &ctx, diagnostics)
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

/// Lower a declaration's generic-parameter interface bounds to `(name, constraint)` pairs,
/// keyed by parameter name with unbounded parameters omitted. Delegates the per-bound
/// lowering to [`crate::builder::lower_generic_param_bounds`] (every sibling parameter in
/// scope, so a bound naming another parameter resolves); bound-lowering diagnostics are
/// discarded — the declaration's bounds are checked where the declaration is, not here.
///
/// Each bound is kept as a [`baml_type::Interface`] *constraint* (a bare `T extends Iterator`
/// pins no associated types), never a [`Ty::Interface`] existential; a bound that does not
/// lower to an interface is dropped.
fn lower_decl_generic_param_bounds(
    db: &dyn crate::Db,
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    generic_param_bounds: &[Option<TypeExpr>],
) -> Vec<(baml_base::Name, baml_type::Interface)> {
    let mut diagnostics = Vec::new();
    crate::builder::lower_generic_param_bounds(
        db,
        generic_param_bounds,
        package_items,
        ns_context,
        generic_params,
        None,
        &mut diagnostics,
    )
    .into_iter()
    .zip(generic_params)
    .filter_map(|(bound_ty, name)| {
        bound_ty
            .and_then(|bound_ty| bound_ty.as_interface())
            .map(|constraint| (name.clone(), constraint))
    })
    .collect()
}

/// A class's generic-parameter interface bounds, keyed by parameter name — lets a
/// projection `T.member` on a class type variable find `T`'s bound interface.
#[salsa::tracked(returns(ref))]
pub fn class_generic_param_bounds<'db>(
    db: &'db dyn crate::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
) -> Vec<(baml_base::Name, baml_type::Interface)> {
    let file = class_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let Some(class) = item_tree.classes.get(&class_loc.id(db)) else {
        return Vec::new();
    };
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let package_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));
    lower_decl_generic_param_bounds(
        db,
        package_items,
        &pkg_info.namespace_path,
        &class.generic_params,
        &class.generic_param_bounds,
    )
}

/// An interface's generic-parameter interface bounds, keyed by parameter name.
#[salsa::tracked(returns(ref))]
pub fn interface_generic_param_bounds<'db>(
    db: &'db dyn crate::Db,
    interface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Vec<(baml_base::Name, baml_type::Interface)> {
    let file = interface_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let Some(interface) = item_tree.interfaces.get(&interface_loc.id(db)) else {
        return Vec::new();
    };
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let package_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));
    lower_decl_generic_param_bounds(
        db,
        package_items,
        &pkg_info.namespace_path,
        &interface.generic_params,
        &interface.generic_param_bounds,
    )
}

/// Every generic-parameter interface bound in scope for a function's signature or body: the
/// enclosing class or interface's parameters (when the function is a method), followed by
/// the function's own parameters. Keyed by parameter name; own parameters are appended last
/// so a map built by insertion lets them shadow an enclosing parameter of the same name.
#[salsa::tracked(returns(ref))]
pub fn function_in_scope_generic_param_bounds<'db>(
    db: &'db dyn crate::Db,
    function_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Vec<(baml_base::Name, baml_type::Interface)> {
    let file = function_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let function_id = function_loc.id(db);
    let mut bounds = Vec::new();
    for (class_id, class) in &item_tree.classes {
        if class.methods.contains(&function_id) {
            bounds.extend(
                class_generic_param_bounds(
                    db,
                    baml_compiler2_hir::loc::ClassLoc::new(db, file, *class_id),
                )
                .iter()
                .cloned(),
            );
        }
    }
    for (interface_id, interface) in &item_tree.interfaces {
        if interface.default_methods.contains(&function_id) {
            bounds.extend(
                interface_generic_param_bounds(
                    db,
                    baml_compiler2_hir::loc::InterfaceLoc::new(db, file, *interface_id),
                )
                .iter()
                .cloned(),
            );
        }
    }
    if let Some(function) = item_tree.functions.get(&function_id) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let package_items =
            baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));
        bounds.extend(lower_decl_generic_param_bounds(
            db,
            package_items,
            &pkg_info.namespace_path,
            &function.generic_params,
            &function.generic_param_bounds,
        ));
    }
    bounds
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
        path_segments(&[name])
    }

    fn path_segments(names: &[&str]) -> TypeExpr {
        TypeExpr::Path {
            segments: names.iter().map(|name| Name::new(*name)).collect(),
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
    }

    /// Compile a single-file `.baml` source into a `TestDb`. It's a *user* file (builtin
    /// paths are filtered out of `compiler2_all_files`), so it lands in package `user`,
    /// root namespace. Lets tir2 tests exercise the real HIR/PPIR pipeline end-to-end
    /// rather than hand-constructing `Ty` values.
    fn compile(source: &str) -> TestDb {
        let mut db = TestDb::default();
        let file = baml_base::SourceFile::new(
            &db,
            source.to_string(),
            PathBuf::from("test.baml"),
            baml_base::FileId::new(0),
        );
        db.project = Some(Project::new(&db, PathBuf::from("."), vec![file]));
        db
    }

    #[test]
    fn source_fixture_compiles_an_interface() {
        let db = compile("interface Iterator {\n  type Item\n}\n");
        let items = baml_compiler2_ppir::package_items(&db, PackageId::new(&db, Name::new("user")));
        assert!(
            items.lookup_type(&[], &Name::new("Iterator")).is_some(),
            "Iterator interface should resolve in package_items",
        );
    }

    // `Self.Item` (an associated-type projection on the receiver) lowers through the
    // context: `Self` is a rigid type variable whose bound is the receiver interface.
    // When the bound pins `Item` it collapses to that type; otherwise it stays symbolic.
    #[test]
    fn self_item_projection_resolves_and_collapses() {
        let db = compile("interface Iterator {\n  type Item\n}\n");
        let items = baml_compiler2_ppir::package_items(&db, PackageId::new(&db, Name::new("user")));
        let iterator = QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Iterator"));

        let lower_self_item = |bound_assoc: Vec<(Name, Ty)>| -> Ty {
            let mut bounds = FxHashMap::default();
            bounds.insert(
                Name::new("Self"),
                baml_type::Interface::new(iterator.clone(), vec![], bound_assoc),
            );
            let self_param = [Name::new("Self")];
            let ctx = ScopeCtx {
                db: &db,
                package_items: items,
                ns_context: &[],
                generic_params: &self_param,
                bounds: &bounds,
                self_ty: Some(Ty::TypeVar(Name::new("Self"), TyAttr::default())),
            };
            let mut diags = Vec::new();
            lower_type_expr(&path_segments(&["Self", "Item"]), &ctx, &mut diags)
        };

        // Rigid `Self`, `Item` unbound → a symbolic projection through `Iterator`.
        match lower_self_item(vec![]) {
            Ty::AssociatedTypeProjection {
                interface, member, ..
            } => {
                assert_eq!(member.as_str(), "Item");
                assert_eq!(
                    interface
                        .expect("interface determined")
                        .name
                        .name()
                        .as_str(),
                    "Iterator"
                );
            }
            other => panic!("expected a symbolic Self.Item projection, got {other:?}"),
        }

        // Pinned `Item = int` → collapses to `int`.
        let pinned = lower_self_item(vec![(
            Name::new("Item"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        )]);
        assert!(
            matches!(pinned, Ty::Int { .. }),
            "expected Self.Item to collapse to int, got {pinned:?}"
        );
    }

    // End-to-end: inside an interface's own default-method body, an associated type resolves to
    // a symbolic `Self.Item` projection (through the interface bound), never the `Ty::Error`
    // placeholder — for both `self.next()` (member resolution) and the block's value.
    #[test]
    fn interface_default_body_resolves_self_associated_type_symbolically() {
        let db = compile(concat!(
            "interface Iterator {\n",
            "  type Item\n",
            "  function next(self) -> Item throws never\n",
            "  function first(self) -> Item throws never {\n",
            "    self.next()\n",
            "  }\n",
            "}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Interface(iface_loc)) =
            items.lookup_type(&[], &Name::new("Iterator"))
        else {
            panic!("Iterator interface should resolve");
        };
        let file = iface_loc.file(&db);
        let tree = baml_compiler2_hir::file_item_tree(&db, file);
        let iface_data = tree
            .interfaces
            .get(&iface_loc.id(&db))
            .expect("Iterator item-tree data");
        let fn_id = iface_data
            .default_methods
            .iter()
            .copied()
            .find(|&id| tree[id].name.as_str() == "first")
            .expect("`first` default method");
        let func_data = &tree[fn_id];

        let index = baml_compiler2_ppir::file_semantic_index(&db, file);
        let scope_id = index
            .scope_ids
            .iter()
            .copied()
            .find(|scope_id| {
                let scope = &index.scopes[scope_id.file_scope_id(&db).index() as usize];
                scope.range == func_data.span && scope.name.as_ref() == Some(&func_data.name)
            })
            .expect("`first`'s body scope");

        let inference = crate::inference::infer_scope_types(&db, scope_id);

        let is_symbolic_item = |ty: &Ty| matches!(ty, Ty::AssociatedTypeProjection { member, .. } if member.as_str() == "Item");
        assert!(
            inference
                .iter_expressions()
                .any(|(_, ty)| is_symbolic_item(ty)),
            "expected `Item` to resolve to a symbolic projection in the body, got: {:?}",
            inference
                .iter_expressions()
                .map(|(_, ty)| ty)
                .collect::<Vec<_>>(),
        );
        assert!(
            !inference
                .iter_expressions()
                .any(|(_, ty)| matches!(ty, Ty::Error { .. })),
            "no expression in the body should lower to Ty::Error",
        );
    }

    #[test]
    fn substitute_paths_recurses_into_function_type() {
        let type_expr = TypeExpr::Function {
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
    fn substitute_paths_rewrites_substituted_root_member_path_to_projection() {
        let type_expr = path_segments(&["T", "Item"]);
        let replacement = path("Box");
        let mut subst = std::collections::HashMap::new();
        subst.insert(Name::new("T"), replacement.clone());

        let TypeExpr::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = substitute_paths_in(&type_expr, &subst)
        else {
            panic!("expected associated type projection");
        };

        assert_eq!(*base, replacement);
        assert!(interface.is_none());
        assert_eq!(member, Name::new("Item"));
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
