//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::{TypeExpr, TypeExprKind};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};
use rustc_hash::FxHashSet;

use crate::{
    builder::associated_projection::resolve_concrete_projection,
    infer_context::TirTypeError,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, MediaKind, QualifiedTypeName, Ty, TyAttr},
};

/// The outcome of resolving a concrete base's associated-type projection
/// (`C.member`) against the impls visible in a scope — the return of
/// [`TypeExprContext::concrete_projection`].
pub enum ConcreteProjection {
    /// Exactly one impl's interface declares `member`, at its realized
    /// instantiation (associated types the impl pins are carried, so an
    /// unambiguous pin lets the projection collapse to a concrete type).
    Determined(baml_type::Interface),
    /// Two or more *distinct* interfaces among the base's impls declare `member`;
    /// the projection must be disambiguated with an explicit `(base as I).member`.
    /// (Two impls resolving to the *same* realized interface can only arise in a
    /// coherence-violating program, which coherence reports separately.)
    Ambiguous(Vec<baml_type::Interface>),
    /// No impl for the base provides an interface that declares `member`.
    Undeclared,
}

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

    /// Resolve a concrete base's associated-type projection (`C.member`) through
    /// the impls visible in this scope. Only consulted for concrete bases — an
    /// interface or type-variable base resolves through its `requires`-closure,
    /// not its impls.
    fn concrete_projection(&self, base: &Ty, member: &baml_base::Name) -> ConcreteProjection;

    /// A concrete base's realized view of `interface` — the `implements` block's realized
    /// interface (with the impl's associated-type pins) when `base` implements it, else
    /// `None`. Narrows an explicit `(base as I).member` qualifier to the written interface
    /// `I` (unlike [`concrete_projection`](Self::concrete_projection), which searches by
    /// member). Only meaningful for a fully-realized concrete base.
    fn concrete_realized_interface(
        &self,
        base: &Ty,
        interface: &baml_type::Interface,
    ) -> Option<baml_type::Interface>;
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

/// A scope's type-variable bounds: each in-scope variable name mapped to the
/// *conjunction* of interface constraints bounding it (`T extends A & B` yields
/// two entries in `T`'s `Vec`). The bounds helpers
/// ([`class_generic_param_bounds`] and friends) produce this shape natively.
pub type TypeVarBoundsMap = rustc_hash::FxHashMap<baml_base::Name, Vec<baml_type::Interface>>;

/// The general lowering scope: a package's items, a namespace, the in-scope type-variable
/// names, and their bounds. Constructed directly at each lowering site and passed to
/// [`lower_type_expr`].
pub(crate) struct ScopeCtx<'a, 'db> {
    pub db: &'db dyn crate::Db,
    pub package_items: &'a PackageItems<'db>,
    pub ns_context: &'a [baml_base::Name],
    pub generic_params: &'a [baml_base::Name],
    /// The in-scope type variables' interface bounds, as *constraints*
    /// ([`baml_type::Interface`], which may pin only some associated types) — never
    /// [`Ty::Interface`] existentials, which would have to specify them all.
    pub bounds: &'a TypeVarBoundsMap,
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
            .then(|| self.bounds.get(name).cloned().unwrap_or_default().into())
    }

    fn concrete_projection(&self, base: &Ty, member: &baml_base::Name) -> ConcreteProjection {
        resolve_concrete_projection(
            self.db,
            &self.package_items.package,
            self.bounds,
            base,
            member,
        )
    }

    fn concrete_realized_interface(
        &self,
        base: &Ty,
        interface: &baml_type::Interface,
    ) -> Option<baml_type::Interface> {
        crate::builder::associated_projection::resolve_concrete_realized_interface(
            self.db,
            &self.package_items.package,
            base,
            interface,
        )
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
            let span = base.span;
            TypeExprKind::AssociatedTypeProjection {
                base: Box::new(base),
                interface: None,
                member: member.clone(),
                attrs: if idx + 1 == members.len() {
                    attrs.to_vec()
                } else {
                    Vec::new()
                },
            }
            .at(span)
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
    match &ty.kind {
        TypeExprKind::Path {
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
            TypeExprKind::Path {
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
            .at(ty.span)
        }
        TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            attrs,
        } => TypeExprKind::AssociatedTypeProjection {
            base: Box::new(substitute_paths_walk(base, subst)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute_paths_walk(interface, subst))),
            member: member.clone(),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::List { inner, attrs } => TypeExprKind::List {
            inner: Box::new(substitute_paths_walk(inner, subst)),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Optional { inner, attrs } => TypeExprKind::Optional {
            inner: Box::new(substitute_paths_walk(inner, subst)),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Map { key, value, attrs } => TypeExprKind::Map {
            key: Box::new(substitute_paths_walk(key, subst)),
            value: Box::new(substitute_paths_walk(value, subst)),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Union { variants, attrs } => TypeExprKind::Union {
            variants: variants
                .iter()
                .map(|v| substitute_paths_walk(v, subst))
                .collect(),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Function {
            params,
            ret,
            throws,
            attrs,
        } => TypeExprKind::Function {
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
        }
        .at(ty.span),
        _ => ty.clone(),
    }
}

/// Build a `TypeExprKind::Path` referring to a single named type — used to desugar the bare
/// `self` receiver to a `Self` path so it flows through normal parameter lowering.
pub fn type_expr_for_name(name: baml_base::Name) -> TypeExpr {
    TypeExprKind::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
    .at(text_size::TextRange::default())
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
    match &type_expr.kind {
        TypeExprKind::Path {
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
                        let base_expr = TypeExprKind::Path {
                            segments: segments[..segments.len() - 1].to_vec(),
                            generic_args: Vec::new(),
                            associated_type_bindings: Vec::new(),
                            attrs: Vec::new(),
                        }
                        .at(type_expr.span);
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
        TypeExprKind::Int { .. } => Ty::Int {
            attr: TyAttr::default(),
        },
        TypeExprKind::Bigint { .. } => Ty::Bigint {
            attr: TyAttr::default(),
        },
        TypeExprKind::Float { .. } => Ty::Float {
            attr: TyAttr::default(),
        },
        TypeExprKind::String { .. } => Ty::String {
            attr: TyAttr::default(),
        },
        TypeExprKind::Bool { .. } => Ty::Bool {
            attr: TyAttr::default(),
        },
        TypeExprKind::Null { .. } => Ty::Null {
            attr: TyAttr::default(),
        },
        TypeExprKind::Never { .. } => Ty::Never {
            attr: TyAttr::default(),
        },
        TypeExprKind::Void { .. } => Ty::Void {
            attr: TyAttr::default(),
        },
        TypeExprKind::Uint8Array { .. } => Ty::Uint8Array {
            attr: TyAttr::default(),
        },
        TypeExprKind::Media { kind, .. } => match kind {
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
        TypeExprKind::Optional { inner, .. } => {
            Ty::optional(lower_type_expr(inner, ctx, diagnostics))
        }
        TypeExprKind::List { inner, .. } => Ty::List(
            Box::new(lower_type_expr(inner, ctx, diagnostics)),
            TyAttr::default(),
        ),
        TypeExprKind::Map { key, value, .. } => Ty::Map {
            key: Box::new(lower_type_expr(key, ctx, diagnostics)),
            value: Box::new(lower_type_expr(value, ctx, diagnostics)),
            attr: TyAttr::default(),
        },
        TypeExprKind::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| lower_type_expr(m, ctx, diagnostics))
                .collect(),
            TyAttr::default(),
        ),
        TypeExprKind::Function {
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
        TypeExprKind::AssociatedTypeProjection {
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
        TypeExprKind::Literal { value: lit, .. } => {
            Ty::Literal(lit.clone(), Freshness::Regular, TyAttr::default())
        }
        TypeExprKind::BuiltinUnknown { .. } => Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        },
        TypeExprKind::Error { .. } | TypeExprKind::Unknown { .. } => Ty::Unknown {
            attr: TyAttr::default(),
        },
        // Dedicated Ty::Type variant — see ty.rs doc comment for design rationale.
        TypeExprKind::Type { .. } => Ty::Type {
            attr: TyAttr::default(),
        },
        // `$rust_type` — opaque Rust-managed state field type.
        TypeExprKind::Rust { .. } => Ty::RustType {
            attr: TyAttr::default(),
        },
        // The wildcard `_` (a type-inference placeholder) cannot be inferred — inference for
        // `_` is unimplemented. Reject it with a diagnostic and lower to `Ty::Error` — never
        // `Ty::Infer`, which the canonical normalizer treats as `unreachable!`. The user must
        // write the type explicitly.
        TypeExprKind::Infer { .. } => {
            diagnostics.push(TirTypeError::CannotInferType);
            Ty::Error {
                attr: TyAttr::default(),
            }
        }
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

/// Lower a declaration's generic-parameter interface bounds to a [`TypeVarBoundsMap`],
/// with unbounded parameters omitted. Delegates the per-bound lowering to
/// [`crate::builder::lower_generic_param_bounds`] (every sibling parameter in scope, so a
/// bound naming another parameter resolves); bound-lowering diagnostics are discarded — the
/// declaration's bounds are checked where the declaration is, not here.
///
/// Each bound is kept as a [`baml_type::Interface`] *constraint* (a bare `T extends Iterator`
/// pins no associated types), never a [`Ty::Interface`] existential; a bound that does not
/// lower to an interface is dropped.
pub(crate) fn lower_decl_generic_param_bounds(
    db: &dyn crate::Db,
    package_items: &PackageItems<'_>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    generic_param_bounds: &[Option<TypeExpr>],
) -> TypeVarBoundsMap {
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
            .map(|constraint| (name.clone(), vec![constraint]))
    })
    .collect()
}

/// A class's generic-parameter interface bounds, keyed by parameter name — lets a
/// projection `T.member` on a class type variable find `T`'s bound interface.
#[salsa::tracked(returns(ref))]
pub fn class_generic_param_bounds<'db>(
    db: &'db dyn crate::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
) -> TypeVarBoundsMap {
    let file = class_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let Some(class) = item_tree.classes.get(&class_loc.id(db)) else {
        return TypeVarBoundsMap::default();
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
) -> TypeVarBoundsMap {
    let file = interface_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let Some(interface) = item_tree.interfaces.get(&interface_loc.id(db)) else {
        return TypeVarBoundsMap::default();
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
/// enclosing class or interface's parameters (when the function is a method), plus the
/// function's own parameters. Keyed by parameter name; own parameters are inserted last so
/// they replace an enclosing parameter of the same name (which is itself a diagnosed
/// shadowing error).
#[salsa::tracked(returns(ref))]
pub fn function_in_scope_generic_param_bounds<'db>(
    db: &'db dyn crate::Db,
    function_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> TypeVarBoundsMap {
    let file = function_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let function_id = function_loc.id(db);
    let mut bounds = TypeVarBoundsMap::default();
    for (class_id, class) in &item_tree.classes {
        if class.methods.contains(&function_id) {
            bounds.extend(
                class_generic_param_bounds(
                    db,
                    baml_compiler2_hir::loc::ClassLoc::new(db, file, *class_id),
                )
                .iter()
                .map(|(name, conjunction)| (name.clone(), conjunction.clone())),
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
                .map(|(name, conjunction)| (name.clone(), conjunction.clone())),
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
    use baml_compiler2_ast::{FunctionTypeParam, TypeExpr, TypeExprKind};
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
        TypeExprKind::Path {
            segments: names.iter().map(|name| Name::new(*name)).collect(),
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(text_size::TextRange::default())
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
            let bounds: TypeVarBoundsMap = std::iter::once((
                Name::new("Self"),
                vec![baml_type::Interface::new(
                    iterator.clone(),
                    vec![],
                    bound_assoc,
                )],
            ))
            .collect();
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

    /// An unqualified associated-type projection on a concrete base (`Base.member`),
    /// built directly so the test drives `lower_projection` without depending on how
    /// the parser splits a dotted path.
    fn concrete_projection(base: &str, member: &str) -> TypeExpr {
        TypeExprKind::AssociatedTypeProjection {
            base: Box::new(path(base)),
            interface: None,
            member: Name::new(member),
            attrs: vec![],
        }
        .at(text_size::TextRange::default())
    }

    /// Lower `expr` in a root-namespace scope over `db`'s `user` package — no generics,
    /// bounds, or `Self` (the scope an ordinary type alias / free signature lowers in).
    fn lower_in_user_scope(db: &TestDb, expr: &TypeExpr) -> (Ty, Vec<TirTypeError>) {
        let items = baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("user")));
        let ctx = ScopeCtx {
            db,
            package_items: items,
            ns_context: &[],
            generic_params: &[],
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        };
        let mut diags = Vec::new();
        let ty = lower_type_expr(expr, &ctx, &mut diags);
        (ty, diags)
    }

    // A concrete class's associated-type projection resolves through the class's own
    // `implements` block (not any closure), and a pinned binding collapses to the type.
    #[test]
    fn concrete_class_projection_resolves_and_collapses() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "class Numbers {\n",
            "  n: int\n",
            "  implements HasItem {\n    type Item = int\n  }\n",
            "}\n",
        ));
        let (ty, diags) = lower_in_user_scope(&db, &concrete_projection("Numbers", "Item"));
        assert!(
            matches!(ty, Ty::Int { .. }),
            "expected Numbers.Item to collapse to int, got {ty:?}"
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

    // Wiring the `associated_type_bound` oracle: `type Assoc extends Bar` makes the
    // oracle report `[Bar]` for `(_ as Foo).Assoc` — the input that lights up the
    // canonical `(base as I).member <: J` subtype rule for a still-symbolic projection.
    #[test]
    fn associated_type_declared_bound_returns_the_extends_clause() {
        let db = compile("interface Bar {}\ninterface Foo {\n  type Assoc extends Bar\n}\n");
        let foo = baml_type::Interface::new(
            QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Foo")),
            vec![],
            vec![],
        );
        let bound = crate::builder::associated_projection::associated_type_declared_bound(
            &db,
            &foo,
            &Name::new("Assoc"),
        );
        assert_eq!(
            bound
                .iter()
                .map(|i| i.name.name().to_string())
                .collect::<Vec<_>>(),
            vec!["Bar".to_string()],
            "expected `type Assoc extends Bar` to yield the `Bar` bound, got {bound:?}"
        );
    }

    // End-to-end: the oracle feeds the canonical subtype rule, so a still-symbolic
    // `(Self as Foo).Assoc` is a subtype of its declared bound `Bar` (the projection
    // analogue of a bounded type variable). It was opaque — a subtype of nothing but
    // itself — before the wiring.
    #[test]
    fn symbolic_projection_is_subtype_of_its_declared_bound() {
        let db = compile("interface Bar {}\ninterface Foo {\n  type Assoc extends Bar\n}\n");
        let user = PackageId::new(&db, Name::new("user"));
        let res_ctx = crate::package_interface::package_resolution_context(&db, user);
        let aliases = crate::inference::package_alias_map(&db, res_ctx);
        let bounds = TypeVarBoundsMap::default();
        let gctx = crate::type_context::GlobalTypeContext {
            db: &db,
            res_ctx,
            aliases: &aliases,
            bounds: &bounds,
        };
        let foo = baml_type::Interface::new(
            QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Foo")),
            vec![],
            vec![],
        );
        let projection = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::TypeVar(Name::new("Self"), TyAttr::default())),
            interface: Some(Box::new(foo)),
            member: Name::new("Assoc"),
            attr: TyAttr::default(),
        };
        let bar = Ty::Interface(
            QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Bar")),
            vec![],
            vec![],
            TyAttr::default(),
        );
        assert!(
            baml_type::normalize::is_subtype(&projection, &bar, &gctx),
            "expected (Self as Foo).Assoc <: Bar via the declared `extends Bar` bound",
        );
    }

    // A projection over a *concrete* base is a pure type-level operator —
    // `(int as Foo).Assoc` with `impl Foo for int { type Assoc = string }` *is* `string`.
    // It normalizes to (compares equal to) its realization, not a dead symbolic leaf.
    #[test]
    fn concrete_projection_reduces_to_its_realization() {
        let db = compile(concat!(
            "interface Foo {\n  type Assoc\n}\n",
            "implements Foo for int {\n  type Assoc = string\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let res_ctx = crate::package_interface::package_resolution_context(&db, user);
        let aliases = crate::inference::package_alias_map(&db, res_ctx);
        let bounds = TypeVarBoundsMap::default();
        let gctx = crate::type_context::GlobalTypeContext {
            db: &db,
            res_ctx,
            aliases: &aliases,
            bounds: &bounds,
        };
        let foo = baml_type::Interface::new(
            QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Foo")),
            vec![],
            vec![],
        );
        let projection = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            interface: Some(Box::new(foo)),
            member: Name::new("Assoc"),
            attr: TyAttr::default(),
        };
        let string_ty = Ty::String {
            attr: TyAttr::default(),
        };
        assert!(
            baml_type::normalize::equivalent(&projection, &string_ty, &gctx),
            "(int as Foo).Assoc should reduce to (be equivalent to) string",
        );
    }

    // Member-access half: a value of a projection type (`Self.Assoc` where
    // `type Assoc extends Bar`) dispatches members through the declared bound, like a
    // bounded type variable — so `self.get().bark()` resolves `bark` on `Bar`. It was
    // an `UnresolvedMember` error before the `AssociatedTypeProjection` receiver arm.
    #[test]
    fn projection_value_resolves_members_through_its_declared_bound() {
        let db = compile(concat!(
            "interface Bar {\n  function bark(self) -> string throws never\n}\n",
            "interface Foo {\n",
            "  type Assoc extends Bar\n",
            "  function get(self) -> Self.Assoc throws never\n",
            "  function run(self) -> string throws never {\n    self.get().bark()\n  }\n",
            "}\n",
        ));
        let errs = scope_type_errors_at(&db, "Foo", "run");
        assert!(
            errs.is_empty(),
            "`.bark()` on a `Self.Assoc` value should resolve through the `extends Bar` \
             bound with no diagnostics, got {errs:?}"
        );
    }

    // An *unbounded* associated type confers no members (like an unbounded type
    // variable): `.bark()` on its value is unresolved. Confirms the declared bound —
    // not the projection alone — is what makes members available.
    #[test]
    fn unbounded_projection_value_has_no_members() {
        let db = compile(concat!(
            "interface Bar {\n  function bark(self) -> string throws never\n}\n",
            "interface Foo {\n",
            "  type Assoc\n",
            "  function get(self) -> Self.Assoc throws never\n",
            "  function run(self) -> string throws never {\n    self.get().bark()\n  }\n",
            "}\n",
        ));
        let errs = scope_type_errors_at(&db, "Foo", "run");
        assert!(
            errs.iter()
                .any(|e| matches!(e, TirTypeError::UnresolvedMember { .. })),
            "an unbounded `Self.Assoc` value must expose no members, got {errs:?}"
        );
    }

    // Tiered member resolution through a bound (`resolve_interface_member`): two
    // *incomparable* required interfaces both declaring `m` (neither requires the other)
    // make `x.m()` ambiguous — qualify with `x.as<A>.m()`. Was silently first-won.
    #[test]
    fn member_declared_by_two_incomparable_required_interfaces_is_ambiguous() {
        let db = compile(concat!(
            "interface A {\n  function m(self) -> string throws never\n}\n",
            "interface B {\n  function m(self) -> string throws never\n}\n",
            "interface D requires A, B {}\n",
            "interface Use {\n",
            "  function run(self, x: D) -> string throws never {\n    x.m()\n  }\n",
            "}\n",
        ));
        let errs = scope_type_errors_at(&db, "Use", "run");
        assert!(
            errs.iter()
                .any(|e| matches!(e, TirTypeError::AmbiguousInterfaceMethod { .. })),
            "`x.m()` where D requires A and B (both declare `m`) must be ambiguous, got {errs:?}"
        );
    }

    // Root-wins: a directly-named interface's member shadows the same-named member of one
    // it transitively `requires` — `x.m()` on `x: D` (D requires B, both declare `m`)
    // resolves to D's `m` with no ambiguity, consistent with the associated-type ruling.
    #[test]
    fn directly_declared_member_shadows_required_interface() {
        let db = compile(concat!(
            "interface B {\n  function m(self) -> string throws never\n}\n",
            "interface D requires B {\n  function m(self) -> string throws never\n}\n",
            "interface Use {\n",
            "  function run(self, x: D) -> string throws never {\n    x.m()\n  }\n",
            "}\n",
        ));
        let errs = scope_type_errors_at(&db, "Use", "run");
        assert!(
            errs.is_empty(),
            "D's own `m` should shadow B's (root-wins), no ambiguity, got {errs:?}"
        );
    }

    // Two distinct interfaces declaring the same associated type make an unqualified
    // concrete projection ambiguous — it must be disambiguated with `(Base as I).member`.
    #[test]
    fn concrete_projection_ambiguous_across_interfaces() {
        let db = compile(concat!(
            "interface A {\n  type Item\n}\n",
            "interface B {\n  type Item\n}\n",
            "class C {\n",
            "  n: int\n",
            "  implements A {\n    type Item = int\n  }\n",
            "  implements B {\n    type Item = string\n  }\n",
            "}\n",
        ));
        let (ty, diags) = lower_in_user_scope(&db, &concrete_projection("C", "Item"));
        assert!(
            matches!(ty, Ty::Error { .. }),
            "an ambiguous projection lowers to Error, got {ty:?}"
        );
        assert!(
            diags.iter().any(|d| matches!(
                d,
                TirTypeError::AmbiguousAssociatedTypeProjection { member, .. } if member.as_str() == "Item"
            )),
            "expected an ambiguity diagnostic, got {diags:?}"
        );
    }

    // A member no `implements` block on the class declares is an unknown associated type,
    // reported against the class (concrete, so `container_is_interface` is false).
    #[test]
    fn concrete_projection_unknown_member_reports_against_class() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "class Numbers {\n",
            "  n: int\n",
            "  implements HasItem {\n    type Item = int\n  }\n",
            "}\n",
        ));
        let (ty, diags) = lower_in_user_scope(&db, &concrete_projection("Numbers", "Missing"));
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        assert!(
            diags.iter().any(|d| matches!(
                d,
                TirTypeError::UnknownAssociatedType {
                    member,
                    container: crate::infer_context::AssocContainer::Class(_),
                } if member.as_str() == "Missing"
            )),
            "expected an unknown-associated-type diagnostic against the class, got {diags:?}"
        );
    }

    // A non-nominal concrete base (a primitive) with no impl declaring the member
    // is an unknown associated type naming the rendered type — not a silent error.
    #[test]
    fn concrete_projection_unknown_member_on_primitive_names_the_type() {
        let db = compile("interface HasItem {\n  type Item\n}\n");
        let projection = TypeExprKind::AssociatedTypeProjection {
            base: Box::new(TypeExprKind::Int { attrs: vec![] }.at(text_size::TextRange::default())),
            interface: None,
            member: Name::new("Missing"),
            attrs: vec![],
        }
        .at(text_size::TextRange::default());
        let (ty, diags) = lower_in_user_scope(&db, &projection);
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        let diag = diags
            .iter()
            .find(|d| matches!(d, TirTypeError::UnknownAssociatedType { .. }))
            .unwrap_or_else(|| {
                panic!("expected an unknown-associated-type diagnostic, got {diags:?}")
            });
        assert_eq!(
            diag.to_string(),
            "unknown associated type `Missing` for type `int`"
        );
    }

    // An enum base renders as "enum", not "class", in the unknown-member diagnostic.
    #[test]
    fn concrete_projection_unknown_member_on_enum_renders_enum() {
        let db = compile("enum Color {\n  Red\n  Blue\n}\n");
        let (ty, diags) = lower_in_user_scope(&db, &concrete_projection("Color", "Missing"));
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        let diag = diags
            .iter()
            .find(|d| matches!(d, TirTypeError::UnknownAssociatedType { .. }))
            .unwrap_or_else(|| {
                panic!("expected an unknown-associated-type diagnostic, got {diags:?}")
            });
        assert_eq!(
            diag.to_string(),
            "unknown associated type `Missing` for enum `Color`"
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

    // A `requires I<Assoc = Self.Assoc>` clause must realize the required interface's associated
    // binding to the enclosing interface's own associated type: `Iterator<Item = int> requires
    // Iterable<Item = Self.Item>` puts `Iterable<Item = int>` in the closure. Characterizes the
    // `Self.member` handling that the `_with_generics` collapse (S4) must preserve.
    #[test]
    fn requires_clause_self_associated_binding_realizes_through_closure() {
        let db = compile(concat!(
            "interface Iterable {\n  type Item\n}\n",
            "interface Iterator requires Iterable<Item = Self.Item> {\n  type Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let iface_loc = |name: &str| match items.lookup_type(&[], &Name::new(name)) {
            Some(baml_compiler2_hir::contributions::Definition::Interface(loc)) => loc,
            _ => panic!("{name} interface should resolve"),
        };
        let iterator = iface_loc("Iterator");
        let iterable = iface_loc("Iterable");

        let int_ty = Ty::Int {
            attr: TyAttr::default(),
        };
        let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            &db,
            iterator,
            &[],
            &[(Name::new("Item"), int_ty.clone())],
        );
        let iterable_entry = closure
            .iter()
            .find(|entry| entry.0 == iterable)
            .expect("Iterable in the requires closure");
        assert_eq!(
            iterable_entry.2,
            vec![(Name::new("Item"), int_ty)],
            "Iterable's `Item` must realize to `int` through `Self.Item`",
        );
    }

    // A `Self.Assoc` nested inside a larger type (`Item = Self.Item[]`) must realize too — this
    // is the case handled by `_with_generics`' structural recursion, which the S4 collapse
    // replaces with `lower_type_expr`.
    #[test]
    fn requires_clause_nested_self_associated_binding_realizes_through_closure() {
        let db = compile(concat!(
            "interface Iterable {\n  type Item\n}\n",
            "interface Iterator requires Iterable<Item = Self.Item[]> {\n  type Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let iface_loc = |name: &str| match items.lookup_type(&[], &Name::new(name)) {
            Some(baml_compiler2_hir::contributions::Definition::Interface(loc)) => loc,
            _ => panic!("{name} interface should resolve"),
        };
        let iterator = iface_loc("Iterator");
        let iterable = iface_loc("Iterable");

        let int_ty = Ty::Int {
            attr: TyAttr::default(),
        };
        let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            &db,
            iterator,
            &[],
            &[(Name::new("Item"), int_ty.clone())],
        );
        let iterable_entry = closure
            .iter()
            .find(|entry| entry.0 == iterable)
            .expect("Iterable in the requires closure");
        assert_eq!(
            iterable_entry.2,
            vec![(
                Name::new("Item"),
                Ty::List(Box::new(int_ty), TyAttr::default()),
            )],
            "Iterable's `Item` must realize to `int[]` through `Self.Item[]`",
        );
    }

    // An associated-type default projecting through a *bounded generic parameter*
    // (`type Elem = T.Item` with `T extends HasItem<Item = int>`) resolves through the
    // declared bound — the bound's pin collapses the projection at lowering.
    #[test]
    fn interface_assoc_default_collapses_through_pinned_param_bound() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "interface Wrapper<T extends HasItem<Item = int>> {\n  type Elem = T.Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Interface(wrapper)) =
            items.lookup_type(&[], &Name::new("Wrapper"))
        else {
            panic!("Wrapper interface should resolve");
        };

        let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            &db,
            wrapper,
            &[Ty::String {
                attr: TyAttr::default(),
            }],
            &[],
        );
        let wrapper_entry = closure
            .iter()
            .find(|entry| entry.0 == wrapper)
            .expect("Wrapper itself heads its closure");
        assert_eq!(
            wrapper_entry.2,
            vec![(
                Name::new("Elem"),
                Ty::Int {
                    attr: TyAttr::default(),
                },
            )],
            "`T.Item` must collapse to the bound's `Item = int` pin",
        );
    }

    // With an unpinned bound (`T extends HasItem`), the same default stays a *symbolic*
    // projection through the bound's interface, with the realized argument as its base.
    #[test]
    fn interface_assoc_default_projects_symbolically_through_param_bound() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "interface Wrapper<T extends HasItem> {\n  type Elem = T.Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Interface(wrapper)) =
            items.lookup_type(&[], &Name::new("Wrapper"))
        else {
            panic!("Wrapper interface should resolve");
        };

        let string_ty = Ty::String {
            attr: TyAttr::default(),
        };
        let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            &db,
            wrapper,
            std::slice::from_ref(&string_ty),
            &[],
        );
        let wrapper_entry = closure
            .iter()
            .find(|entry| entry.0 == wrapper)
            .expect("Wrapper itself heads its closure");
        let [(name, ty)] = wrapper_entry.2.as_slice() else {
            panic!(
                "expected exactly one associated binding, got {:?}",
                wrapper_entry.2
            );
        };
        assert_eq!(name.as_str(), "Elem");
        match ty {
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                assert_eq!(**base, string_ty, "base must be the realized argument");
                assert_eq!(
                    interface
                        .as_ref()
                        .expect("interface determined")
                        .name
                        .name()
                        .as_str(),
                    "HasItem"
                );
                assert_eq!(member.as_str(), "Item");
            }
            other => panic!("expected a symbolic T.Item projection, got {other:?}"),
        }
    }

    // An in-package class field projecting through a bounded class parameter
    // (`x: T.Item` with `T extends HasItem<Item = int>`) resolves through the
    // declared bound — `resolve_class_fields` threads the class's bounds.
    #[test]
    fn class_field_projection_resolves_through_param_bound() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "class Holder<T extends HasItem<Item = int>> {\n  x: T.Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Class(holder)) =
            items.lookup_type(&[], &Name::new("Holder"))
        else {
            panic!("Holder class should resolve");
        };

        let resolved = crate::inference::resolve_class_fields(&db, holder);
        let (_, ty, _) = resolved
            .fields
            .iter()
            .find(|(name, _, _)| name.as_str() == "x")
            .expect("field x resolved");
        assert!(
            matches!(ty, Ty::Int { .. }),
            "expected T.Item to collapse to the bound's pin, got {ty:?}"
        );
        assert!(
            resolved.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            resolved.diagnostics
        );
    }

    // The explicit qualifier is first-class: `(T as HasItem).Item` intersects T's bound
    // pins exactly as the unqualified `T.Item` does, so under `T extends HasItem<Item = int>`
    // it collapses to `int` too — the two spellings agree (identity across spellings).
    #[test]
    fn explicit_qualifier_projection_carries_the_base_bound_pin() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "class Holder<T extends HasItem<Item = int>> {\n  x: (T as HasItem).Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Class(holder)) =
            items.lookup_type(&[], &Name::new("Holder"))
        else {
            panic!("Holder class should resolve");
        };
        let resolved = crate::inference::resolve_class_fields(&db, holder);
        let (_, ty, _) = resolved
            .fields
            .iter()
            .find(|(name, _, _)| name.as_str() == "x")
            .expect("field x resolved");
        assert!(
            matches!(ty, Ty::Int { .. }),
            "expected (T as HasItem).Item to collapse to the bound's pin like T.Item, got {ty:?}"
        );
        assert!(
            resolved.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            resolved.diagnostics
        );
    }

    // The explicit qualifier is verified against the base: `(string as HasItem).Item`
    // where `string` does not implement `HasItem` is an error (Rust's E0277), not a
    // silently-symbolic projection.
    #[test]
    fn explicit_qualifier_projection_requires_base_to_implement_it() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "class Holder {\n  x: (string as HasItem).Item\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Class(holder)) =
            items.lookup_type(&[], &Name::new("Holder"))
        else {
            panic!("Holder class should resolve");
        };
        let resolved = crate::inference::resolve_class_fields(&db, holder);
        assert!(
            resolved
                .diagnostics
                .iter()
                .any(|(e, _)| matches!(e, TirTypeError::TypeDoesNotImplementInterface { .. })),
            "`string` does not implement `HasItem`, so `(string as HasItem).Item` must error, got {:?}",
            resolved.diagnostics
        );
    }

    // An `implements` block that omits a defaulted associated type falls back to the
    // interface's `type Asdf = default` — so `Ipsum.Asdf` (and the explicit
    // `(Ipsum as Lorem).Asdf`) collapse to the default `int`, not a symbolic projection.
    #[test]
    fn concrete_projection_applies_interface_default_for_omitted_binding() {
        let db = compile(concat!(
            "interface Lorem {\n  type Asdf = int\n}\n",
            "class Ipsum {\n  n: int\n  implements Lorem {}\n}\n",
            "class Holder {\n  x: Ipsum.Asdf\n  y: (Ipsum as Lorem).Asdf\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let items = baml_compiler2_ppir::package_items(&db, user);
        let Some(baml_compiler2_hir::contributions::Definition::Class(holder)) =
            items.lookup_type(&[], &Name::new("Holder"))
        else {
            panic!("Holder class should resolve");
        };
        let resolved = crate::inference::resolve_class_fields(&db, holder);
        for field in ["x", "y"] {
            let (_, ty, _) = resolved
                .fields
                .iter()
                .find(|(name, _, _)| name.as_str() == field)
                .unwrap_or_else(|| panic!("field {field} resolved"));
            assert!(
                matches!(ty, Ty::Int { .. }),
                "expected `{field}` (Ipsum.Asdf) to collapse to the default `int`, got {ty:?}"
            );
        }
        assert!(
            resolved.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            resolved.diagnostics
        );
    }

    /// Drive `infer_scope_types` on the scope named `scope_name` in the file that
    /// declares `anchor` (any package-level type or value), returning the type
    /// errors it raised. `anchor` locates the file; `scope_name` may be a nested
    /// scope such as an impl-block method.
    fn scope_type_errors_at(db: &TestDb, anchor: &str, scope_name: &str) -> Vec<TirTypeError> {
        let items = baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("user")));
        let def = items
            .lookup_type(&[], &Name::new(anchor))
            .or_else(|| items.lookup_value(&[], &Name::new(anchor)))
            .unwrap_or_else(|| panic!("declaration `{anchor}` should resolve"));
        let file = def.file(db);
        let index = baml_compiler2_ppir::file_semantic_index(db, file);
        let scope_id = index
            .scope_ids
            .iter()
            .copied()
            .find(|scope_id| {
                let scope = &index.scopes[scope_id.file_scope_id(db).index() as usize];
                scope
                    .name
                    .as_ref()
                    .is_some_and(|n| n.as_str() == scope_name)
            })
            .unwrap_or_else(|| panic!("scope named `{scope_name}`"));
        crate::inference::infer_scope_types(db, scope_id)
            .diagnostics()
            .diagnostics
            .iter()
            .map(|d| d.error.clone())
            .collect()
    }

    /// Drive `infer_scope_types` on the declaration scope named `name` (a class,
    /// interface, or function) and return the type errors it raised.
    fn decl_scope_type_errors(db: &TestDb, name: &str) -> Vec<TirTypeError> {
        scope_type_errors_at(db, name, name)
    }

    fn has_bare_bound_arity_error(errs: &[TirTypeError], iface: &str, expected: usize) -> bool {
        errs.iter().any(|e| {
            matches!(
                e,
                TirTypeError::WrongNumberOfTypeArgs { type_name, expected: exp, got: 0 }
                    if type_name.as_str() == iface && *exp == expected
            )
        })
    }

    // A generic interface used as a bare bound (`T extends Box` where `interface Box<E>`)
    // under-instantiates it: a bound cannot infer the missing argument, so it is an arity
    // error — on a class declaration's generic parameter here.
    #[test]
    fn bare_generic_interface_bound_on_class_is_arity_error() {
        let db = compile(concat!(
            "interface Box<E> {\n  value: E\n}\n",
            "class Holder<T extends Box> {\n  item: T\n}\n",
        ));
        let errs = decl_scope_type_errors(&db, "Holder");
        assert!(
            has_bare_bound_arity_error(&errs, "Box", 1),
            "expected a bare-bound arity error against `Box`, got {errs:?}"
        );
    }

    // The same arity error on a function declaration's generic parameter.
    #[test]
    fn bare_generic_interface_bound_on_function_is_arity_error() {
        let db = compile(concat!(
            "interface Box<E> {\n  value: E\n}\n",
            "function unwrap<T extends Box>(b: T) -> int {\n  return 0\n}\n",
        ));
        let errs = decl_scope_type_errors(&db, "unwrap");
        assert!(
            has_bare_bound_arity_error(&errs, "Box", 1),
            "expected a bare-bound arity error against `Box`, got {errs:?}"
        );
    }

    // The same arity error on an associated type's `extends` bound.
    #[test]
    fn bare_generic_interface_bound_on_associated_type_is_arity_error() {
        let db = compile(concat!(
            "interface Box<E> {\n  value: E\n}\n",
            "interface Outer {\n  type A extends Box\n}\n",
        ));
        let errs = decl_scope_type_errors(&db, "Outer");
        assert!(
            has_bare_bound_arity_error(&errs, "Box", 1),
            "expected a bare-bound arity error against `Box`, got {errs:?}"
        );
    }

    // A declaration's bound diagnostics are reported exactly once, by the owning
    // declaration's scope — a method env inherits its class's bounds for
    // enforcement but must not re-report their errors.
    #[test]
    fn inherited_bound_errors_are_not_re_reported_by_method_scopes() {
        let db = compile(concat!(
            "interface Box<E> {\n  value: E\n}\n",
            "class Holder<T extends Box> {\n",
            "  item: T\n",
            "  function get(self) -> int {\n    return 0\n  }\n",
            "}\n",
        ));
        let class_errs = decl_scope_type_errors(&db, "Holder");
        assert_eq!(
            class_errs
                .iter()
                .filter(|e| matches!(e, TirTypeError::WrongNumberOfTypeArgs { .. }))
                .count(),
            1,
            "the class scope owns the error exactly once, got {class_errs:?}"
        );
        let method_errs = scope_type_errors_at(&db, "Holder", "get");
        assert!(
            !method_errs
                .iter()
                .any(|e| matches!(e, TirTypeError::WrongNumberOfTypeArgs { .. })),
            "the method scope must not re-report its class's bound error, got {method_errs:?}"
        );
    }

    // Impl-block generic bounds get the same bare-generic arity error as decl bounds.
    #[test]
    fn bare_generic_interface_bound_on_free_impl_is_arity_error() {
        let db = compile(concat!(
            "interface Box<E> {\n  value: E\n}\n",
            "interface Marker {}\n",
            "implements<T extends Box> Marker for T[] {}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let impls = crate::interfaces::package_impl_locs(&db, user);
        assert!(
            impls.iter().any(|&loc| {
                crate::interfaces::impl_data(&db, loc)
                    .as_ref()
                    .is_ok_and(|data| {
                        data.diagnostics.iter().any(|(e, _)| matches!(
                            e,
                            TirTypeError::WrongNumberOfTypeArgs { type_name, expected: 1, got: 0 }
                                if type_name.as_str() == "Box"
                        ))
                    })
            }),
            "expected the impl block to report the bare-bound arity error",
        );
    }

    // A fully-instantiated generic bound and a bound on a non-generic interface are
    // both well-formed: no arity error.
    #[test]
    fn instantiated_and_non_generic_interface_bounds_are_not_arity_errors() {
        let db = compile(concat!(
            "interface Box<E> {\n  value: E\n}\n",
            "interface Named {\n  name: string\n}\n",
            "class Full<T extends Box<int>> {\n  item: T\n}\n",
            "class Plain<T extends Named> {\n  item: T\n}\n",
        ));
        for decl in ["Full", "Plain"] {
            let errs = decl_scope_type_errors(&db, decl);
            assert!(
                !errs
                    .iter()
                    .any(|e| matches!(e, TirTypeError::WrongNumberOfTypeArgs { .. })),
                "`{decl}` should not raise an arity error, got {errs:?}"
            );
        }
    }

    /// Lower `(base as qualifier).member` with both sides given as bare paths.
    fn lower_qualified_projection(
        db: &TestDb,
        base: &str,
        qualifier: &str,
        member: &str,
    ) -> (Ty, Vec<TirTypeError>) {
        let items = baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("user")));
        let ctx = ScopeCtx {
            db,
            package_items: items,
            ns_context: &[],
            generic_params: &[],
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        };
        let expr = TypeExprKind::AssociatedTypeProjection {
            base: Box::new(path(base)),
            interface: Some(Box::new(path(qualifier))),
            member: Name::new(member),
            attrs: vec![],
        }
        .at(text_size::TextRange::default());
        let mut diags = Vec::new();
        let ty = lower_type_expr(&expr, &ctx, &mut diags);
        (ty, diags)
    }

    // An explicit qualifier that resolves to a non-interface (`(x as SomeClass).Item`)
    // is its own error — the qualifier lowered cleanly, so nothing upstream reported it.
    #[test]
    fn non_interface_explicit_qualifier_is_error() {
        let db = compile(concat!(
            "class Plain {\n  x: int\n}\n",
            "class Data {\n  y: int\n}\n",
        ));
        let (ty, diags) = lower_qualified_projection(&db, "Data", "Plain", "Item");
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        assert!(
            diags
                .iter()
                .any(|d| matches!(d, TirTypeError::NonInterfaceProjectionQualifier)),
            "expected a non-interface-qualifier diagnostic, got {diags:?}"
        );
    }

    // An *unresolvable* qualifier is already diagnosed by its own lowering — the
    // projection must stay silent rather than pile on a second error.
    #[test]
    fn unresolved_explicit_qualifier_stays_poisoned() {
        let db = compile("class Data {\n  y: int\n}\n");
        let (ty, diags) = lower_qualified_projection(&db, "Data", "Nonexistent", "Item");
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        assert!(
            !diags
                .iter()
                .any(|d| matches!(d, TirTypeError::NonInterfaceProjectionQualifier)),
            "an unresolved qualifier must not double-report, got {diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| matches!(d, TirTypeError::UnresolvedType { .. })),
            "the qualifier's own lowering reports it, got {diags:?}"
        );
    }

    // A bound must be an interface: a class (or other concrete non-interface type)
    // in bound position is an error on class, function, and associated-type
    // declarations alike (impls already had this check).
    #[test]
    fn non_interface_bound_is_error_on_decls() {
        let db = compile(concat!(
            "class Plain {\n  x: int\n}\n",
            "class Holder<T extends Plain> {\n  item: T\n}\n",
            "function get<T extends Plain>(x: T) -> int {\n  return 0\n}\n",
            "interface Outer {\n  type A extends Plain\n}\n",
        ));
        for decl in ["Holder", "get", "Outer"] {
            let errs = decl_scope_type_errors(&db, decl);
            assert!(
                errs.iter().any(|e| matches!(
                    e,
                    TirTypeError::GenericBoundNotInterface { bound: Ty::Class(qtn, ..) }
                        if qtn.name().as_str() == "Plain"
                )),
                "`{decl}` should report a non-interface bound, got {errs:?}"
            );
        }
    }

    // Bounds constrain by interface contract only: a sibling type variable
    // (`<T, U extends T>`) or an associated-type projection (`U extends T.Item`)
    // is not an interface, so both are errors.
    #[test]
    fn type_var_and_projection_bounds_are_errors() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "function pick<T, U extends T>(a: T, b: U) -> T {\n  return a\n}\n",
            "function proj<T extends HasItem, U extends T.Item>(a: T, b: U) -> T {\n  return a\n}\n",
        ));
        let pick_errs = decl_scope_type_errors(&db, "pick");
        assert!(
            pick_errs.iter().any(|e| matches!(
                e,
                TirTypeError::GenericBoundNotInterface { bound: Ty::TypeVar(name, _) }
                    if name.as_str() == "T"
            )),
            "`U extends T` should report a non-interface bound, got {pick_errs:?}"
        );
        // A projection bound errors through its own resolution (projections inside
        // bound expressions resolve with no bounds threaded — that would be
        // circular); if one ever lowers symbolically it is caught as a
        // non-interface bound instead. Either way `U extends T.Item` is an error.
        let proj_errs = decl_scope_type_errors(&db, "proj");
        assert!(
            proj_errs.iter().any(|e| matches!(
                e,
                TirTypeError::GenericBoundNotInterface {
                    bound: Ty::AssociatedTypeProjection { .. }
                } | TirTypeError::UnknownAssociatedType { .. }
            )),
            "`U extends T.Item` should report an error, got {proj_errs:?}"
        );
    }

    // `<T, T>` in one declaration list is a duplicate — on classes, functions, and
    // interfaces alike.
    #[test]
    fn duplicate_generic_param_is_error_on_class_function_and_interface() {
        let db = compile(concat!(
            "class Pair<T, T> {\n  a: T\n}\n",
            "function twice<T, T>(x: T) -> T {\n  return x\n}\n",
            "interface Both<T, T> {\n  value: T\n}\n",
        ));
        for decl in ["Pair", "twice", "Both"] {
            let errs = decl_scope_type_errors(&db, decl);
            assert!(
                errs.iter().any(|e| matches!(
                    e,
                    TirTypeError::DuplicateGenericParam { name } if name.as_str() == "T"
                )),
                "`{decl}` should report duplicate parameter `T`, got {errs:?}"
            );
        }
    }

    // An associated type may not share its name with the interface's generic
    // parameter: both occupy the interface's type-level namespace.
    #[test]
    fn associated_type_conflicting_with_generic_param_is_error() {
        let db = compile("interface Conflict<A> {\n  type A\n}\n");
        let errs = decl_scope_type_errors(&db, "Conflict");
        assert!(
            errs.iter().any(|e| matches!(
                e,
                TirTypeError::AssociatedTypeConflictsWithGenericParam { name } if name.as_str() == "A"
            )),
            "expected an associated-type/parameter conflict for `A`, got {errs:?}"
        );
    }

    // The same associated type declared twice is a duplicate.
    #[test]
    fn duplicate_associated_type_is_error() {
        let db = compile("interface Twice {\n  type A\n  type A\n}\n");
        let errs = decl_scope_type_errors(&db, "Twice");
        assert!(
            errs.iter().any(|e| matches!(
                e,
                TirTypeError::DuplicateAssociatedType { name } if name.as_str() == "A"
            )),
            "expected a duplicate associated type `A`, got {errs:?}"
        );
    }

    // A required (bodyless) interface method's generic may shadow neither the
    // interface's generic parameter nor its associated types — required methods
    // have no body scope, so this fires from the interface declaration's own
    // validation.
    #[test]
    fn required_method_generic_shadowing_interface_type_level_names_is_error() {
        let db = compile(concat!(
            "interface ShadowParam<T> {\n  function get<T>(self) -> T\n}\n",
            "interface ShadowAssoc {\n  type Item\n  function get<Item>(self) -> int\n}\n",
        ));
        for (decl, param) in [("ShadowParam", "T"), ("ShadowAssoc", "Item")] {
            let errs = decl_scope_type_errors(&db, decl);
            assert!(
                errs.iter().any(|e| matches!(
                    e,
                    TirTypeError::TypeParamShadowed { param_name, class_name }
                        if param_name.as_str() == param && class_name.as_str() == decl
                )),
                "`{decl}` should report method generic `{param}` shadowing, got {errs:?}"
            );
        }
    }

    // A method inside an `implements` block may not re-declare the block's own
    // generic parameter (the gap the class/interface method path already covers).
    #[test]
    fn impl_method_generic_shadowing_impl_param_is_error() {
        let db = compile(concat!(
            "interface Wrap2 {\n  function noop(self) -> int\n}\n",
            "implements<T> Wrap2 for T[] {\n",
            "  function noop<T>(self) -> int {\n    return 0\n  }\n",
            "}\n",
        ));
        let errs = scope_type_errors_at(&db, "Wrap2", "noop");
        assert!(
            errs.iter().any(|e| matches!(
                e,
                TirTypeError::TypeParamShadowedImplParam { param_name } if param_name.as_str() == "T"
            )),
            "expected impl-method generic `T` shadowing error, got {errs:?}"
        );
    }

    // `implements<T, T> …` — duplicate generics on the impl block itself surface in
    // `impl_data`'s own diagnostics (the block has no inference scope).
    #[test]
    fn duplicate_generic_param_on_free_impl_is_error() {
        let db = compile(concat!(
            "interface Marker {}\n",
            "implements<T, T> Marker for T[] {}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let impls = crate::interfaces::package_impl_locs(&db, user);
        assert!(
            impls.iter().any(|&loc| {
                crate::interfaces::impl_data(&db, loc)
                    .as_ref()
                    .is_ok_and(|data| {
                        data.diagnostics.iter().any(|(e, _)| {
                            matches!(
                                e,
                                TirTypeError::DuplicateGenericParam { name } if name.as_str() == "T"
                            )
                        })
                    })
            }),
            "expected the impl block to report duplicate parameter `T`",
        );
    }

    // An impl's binding values resolve names in the impl's own scope — its
    // generics and already-resolved sibling associated types — never the
    // interface's parameter names (those are the interface author's internal
    // names; an impl references its instantiation through its own generics).
    #[test]
    fn impl_binding_value_cannot_name_interface_params() {
        let db = compile(concat!(
            "interface Wrapper<P> {\n  type Item\n}\n",
            "class Thing {\n  x: int\n  implements Wrapper<int> {\n    type Item = P\n  }\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let impls = crate::interfaces::package_impl_locs(&db, user);
        assert!(
            impls.iter().any(|&loc| {
                crate::interfaces::impl_data(&db, loc)
                    .as_ref()
                    .is_ok_and(|data| {
                        data.diagnostics.iter().any(|(e, _)| {
                            matches!(
                                e,
                                TirTypeError::UnresolvedType { name, .. } if name.as_str() == "P"
                            )
                        })
                    })
            }),
            "the interface's `P` must be unresolvable inside the impl's binding value",
        );
    }

    // The impl's own generics and earlier siblings are in scope for binding values.
    #[test]
    fn impl_binding_value_resolves_impl_generics_and_siblings() {
        let db = compile(concat!(
            "interface TwoAssoc {\n  type A\n  type B\n}\n",
            "implements<T> TwoAssoc for T[] {\n  type A = T\n  type B = A[]\n}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let impls = crate::interfaces::package_impl_locs(&db, user);
        let resolved = impls
            .iter()
            .find_map(|&loc| crate::interfaces::impl_data(&db, loc).as_ref().ok())
            .expect("the impl should resolve");
        let assoc = |name: &str| {
            resolved
                .associated_types
                .iter()
                .find(|(n, _)| n.as_str() == name)
                .map(|(_, ty)| ty)
                .unwrap_or_else(|| panic!("binding for `{name}`"))
        };
        assert!(
            matches!(assoc("A"), Ty::TypeVar(n, _) if n.as_str() == "T"),
            "`type A = T` binds the impl generic, got {:?}",
            assoc("A")
        );
        assert!(
            matches!(assoc("B"), Ty::List(inner, _) if matches!(&**inner, Ty::TypeVar(n, _) if n.as_str() == "T")),
            "`type B = A[]` resolves the sibling, got {:?}",
            assoc("B")
        );
    }

    // Distinct names everywhere: none of the duplicate/shadow/conflict checks fire.
    #[test]
    fn distinct_generic_and_associated_names_raise_no_hygiene_errors() {
        let db = compile(concat!(
            "interface Clean<T> {\n  type Item\n  function get<U>(self) -> U\n}\n",
            "class Fine<A, B> {\n  a: A\n  b: B\n}\n",
        ));
        for decl in ["Clean", "Fine"] {
            let errs = decl_scope_type_errors(&db, decl);
            assert!(
                !errs.iter().any(|e| matches!(
                    e,
                    TirTypeError::DuplicateGenericParam { .. }
                        | TirTypeError::DuplicateAssociatedType { .. }
                        | TirTypeError::AssociatedTypeConflictsWithGenericParam { .. }
                        | TirTypeError::TypeParamShadowed { .. }
                        | TirTypeError::TypeParamShadowedImplParam { .. }
                )),
                "`{decl}` should raise no hygiene errors, got {errs:?}"
            );
        }
    }

    // A concrete associated-type projection in an impl header re-enters `impl_data`
    // through `impls_for_type` — a salsa cycle. `impl_data`'s `cycle_result` converges
    // it to `CyclicHeader` (illegal) rather than panicking.
    #[test]
    fn concrete_projection_in_impl_header_is_cyclic_not_panic() {
        let db = compile(concat!(
            "interface HasItem {\n  type Item\n}\n",
            "class Numbers {\n  n: int\n  implements HasItem {\n    type Item = int\n  }\n}\n",
            "interface Marker {}\n",
            "implement Marker for Numbers.Item {}\n",
        ));
        let user = PackageId::new(&db, Name::new("user"));
        let impls = crate::interfaces::package_impl_locs(&db, user);
        // The projection-header impl converges to CyclicHeader; every other impl still
        // resolves (the in-body `implements HasItem` is a leaf, not in the cycle).
        assert!(
            impls.iter().any(|&loc| matches!(
                crate::interfaces::impl_data(&db, loc),
                Err(crate::interfaces::ImplDataError::CyclicHeader)
            )),
            "a concrete projection in an impl header must resolve to CyclicHeader, not panic",
        );
        assert!(
            impls
                .iter()
                .any(|&loc| crate::interfaces::impl_data(&db, loc).is_ok()),
            "the non-cyclic in-body impl must still resolve",
        );
    }

    /// Lower `T.member` where `T` is an in-scope type variable bounded by the named
    /// interfaces (`&[]` = declared but unbounded), each a bare constraint.
    fn lower_tvar_projection(
        db: &TestDb,
        tvar: &str,
        bound_names: &[&str],
        member: &str,
    ) -> (Ty, Vec<TirTypeError>) {
        let items = baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("user")));
        let generic_params = [Name::new(tvar)];
        let conjunction: Vec<baml_type::Interface> = bound_names
            .iter()
            .map(|name| {
                baml_type::Interface::new(
                    QualifiedTypeName::new(Name::new("user"), vec![], Name::new(*name)),
                    vec![],
                    vec![],
                )
            })
            .collect();
        let mut bounds = TypeVarBoundsMap::default();
        if !conjunction.is_empty() {
            bounds.insert(Name::new(tvar), conjunction);
        }
        let ctx = ScopeCtx {
            db,
            package_items: items,
            ns_context: &[],
            generic_params: &generic_params,
            bounds: &bounds,
            self_ty: None,
        };
        let expr = TypeExprKind::AssociatedTypeProjection {
            base: Box::new(path(tvar)),
            interface: None,
            member: Name::new(member),
            attrs: vec![],
        }
        .at(text_size::TextRange::default());
        let mut diags = Vec::new();
        let ty = lower_type_expr(&expr, &ctx, &mut diags);
        (ty, diags)
    }

    // A declared-but-unbounded type variable cannot be proven to implement any
    // interface, so its projection is unknown — reported against the variable itself.
    #[test]
    fn unbounded_type_var_projection_is_unknown_naming_the_var() {
        let db = compile("interface HasItem {\n  type Item\n}\n");
        let (ty, diags) = lower_tvar_projection(&db, "T", &[], "Item");
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        let diag = diags
            .iter()
            .find(|d| matches!(d, TirTypeError::UnknownAssociatedType { .. }))
            .unwrap_or_else(|| panic!("expected an unknown-member diagnostic, got {diags:?}"));
        assert_eq!(
            diag.to_string(),
            "unknown associated type `Item` for type variable `T` (no interface bound)"
        );
    }

    // A bounded type variable whose bound does not declare the member is unknown,
    // reported against the bound (the interface to fix).
    #[test]
    fn bounded_type_var_projection_without_member_is_unknown_against_bound() {
        let db = compile("interface HasItem {\n  type Item\n}\n");
        let (ty, diags) = lower_tvar_projection(&db, "T", &["HasItem"], "Missing");
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        let diag = diags
            .iter()
            .find(|d| matches!(d, TirTypeError::UnknownAssociatedType { .. }))
            .unwrap_or_else(|| panic!("expected an unknown-member diagnostic, got {diags:?}"));
        assert_eq!(
            diag.to_string(),
            "unknown associated type `Missing` for interface `HasItem`"
        );
    }

    // Both arms of an intersection bound (`T: A & B`) declaring the same member make
    // the projection ambiguous — it must be qualified `(T as A).member`.
    #[test]
    fn type_var_projection_ambiguous_across_conjunction_bounds() {
        let db = compile(concat!(
            "interface A {\n  type Item\n}\n",
            "interface B {\n  type Item\n}\n",
        ));
        let (ty, diags) = lower_tvar_projection(&db, "T", &["A", "B"], "Item");
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        assert!(
            diags.iter().any(|d| matches!(
                d,
                TirTypeError::AmbiguousAssociatedTypeProjection { member, .. }
                    if member.as_str() == "Item"
            )),
            "expected an ambiguity diagnostic, got {diags:?}"
        );
    }

    // The same member reached through two `requires` paths to the *same* interface
    // is one associated type, not an ambiguity (dedup by realized identity).
    #[test]
    fn type_var_projection_through_diamond_requires_is_not_ambiguous() {
        let db = compile(concat!(
            "interface Base {\n  type X\n}\n",
            "interface A requires Base {}\n",
            "interface B requires Base {}\n",
            "interface Sub requires A, B {}\n",
        ));
        let (ty, diags) = lower_tvar_projection(&db, "T", &["Sub"], "X");
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        match ty {
            Ty::AssociatedTypeProjection {
                interface, member, ..
            } => {
                assert_eq!(member.as_str(), "X");
                assert_eq!(
                    interface
                        .expect("interface determined")
                        .name
                        .name()
                        .as_str(),
                    "Base"
                );
            }
            other => panic!("expected a symbolic projection through Base, got {other:?}"),
        }
    }

    // A bounded type variable whose bound declares the member (unpinned) resolves to
    // a symbolic projection through that bound — not an error.
    #[test]
    fn bounded_type_var_projection_resolves_symbolically() {
        let db = compile("interface HasItem {\n  type Item\n}\n");
        let (ty, diags) = lower_tvar_projection(&db, "T", &["HasItem"], "Item");
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        match ty {
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                assert!(matches!(*base, Ty::TypeVar(ref n, _) if n.as_str() == "T"));
                assert_eq!(
                    interface
                        .expect("interface determined")
                        .name
                        .name()
                        .as_str(),
                    "HasItem"
                );
                assert_eq!(member.as_str(), "Item");
            }
            other => panic!("expected a symbolic T.Item projection, got {other:?}"),
        }
    }

    /// Lower a chained projection `T.members[0].members[1]…` where `T` is bounded by
    /// the single named interface.
    fn lower_chain(
        db: &TestDb,
        tvar: &str,
        bound: &str,
        members: &[&str],
    ) -> (Ty, Vec<TirTypeError>) {
        let items = baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("user")));
        let generic_params = [Name::new(tvar)];
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(
            Name::new(tvar),
            vec![baml_type::Interface::new(
                QualifiedTypeName::new(Name::new("user"), vec![], Name::new(bound)),
                vec![],
                vec![],
            )],
        );
        let ctx = ScopeCtx {
            db,
            package_items: items,
            ns_context: &[],
            generic_params: &generic_params,
            bounds: &bounds,
            self_ty: None,
        };
        let expr = members.iter().fold(path(tvar), |base, member| {
            TypeExprKind::AssociatedTypeProjection {
                base: Box::new(base),
                interface: None,
                member: Name::new(member),
                attrs: vec![],
            }
            .at(text_size::TextRange::default())
        });
        let mut diags = Vec::new();
        let ty = lower_type_expr(&expr, &ctx, &mut diags);
        (ty, diags)
    }

    // `T.A.B` where `T: Outer`, `type A extends Inner`, and `Inner` declares `B`:
    // the outer `.B` resolves nominally through `A`'s declared bound.
    #[test]
    fn chained_projection_resolves_through_associated_type_bound() {
        let db = compile(concat!(
            "interface Inner {\n  type B\n}\n",
            "interface Outer {\n  type A extends Inner\n}\n",
        ));
        let (ty, diags) = lower_chain(&db, "T", "Outer", &["A", "B"]);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        match ty {
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                assert!(
                    matches!(*base, Ty::AssociatedTypeProjection { ref member, .. } if member.as_str() == "A"),
                    "outer projection's base is the inner `T.A` projection",
                );
                assert_eq!(
                    interface
                        .expect("interface determined")
                        .name
                        .name()
                        .as_str(),
                    "Inner"
                );
                assert_eq!(member.as_str(), "B");
            }
            other => panic!("expected a symbolic T.A.B projection, got {other:?}"),
        }
    }

    // When `A`'s bound pins the member (`type A extends Inner<B = int>`), the chain
    // collapses to the pinned type.
    #[test]
    fn chained_projection_collapses_through_pinning_bound() {
        let db = compile(concat!(
            "interface Inner {\n  type B\n}\n",
            "interface Outer {\n  type A extends Inner<B = int>\n}\n",
        ));
        let (ty, diags) = lower_chain(&db, "T", "Outer", &["A", "B"]);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert!(
            matches!(ty, Ty::Int { .. }),
            "expected T.A.B to collapse to the bound's pin, got {ty:?}"
        );
    }

    // An associated type with no declared bound cannot be proven to implement any
    // interface, so projecting off it (`T.A.B`) is unknown.
    #[test]
    fn chained_projection_without_bound_is_unknown() {
        let db = compile("interface Outer {\n  type A\n}\n");
        let (ty, diags) = lower_chain(&db, "T", "Outer", &["A", "B"]);
        assert!(matches!(ty, Ty::Error { .. }), "got {ty:?}");
        assert!(
            diags.iter().any(|d| matches!(
                d,
                TirTypeError::UnknownAssociatedType { member, .. } if member.as_str() == "B"
            )),
            "expected an unknown-member diagnostic for `B`, got {diags:?}"
        );
    }

    // A three-deep chain `T.A.B.C` resolves through each associated type's bound.
    #[test]
    fn chained_projection_three_deep_resolves() {
        let db = compile(concat!(
            "interface L3 {\n  type C\n}\n",
            "interface L2 {\n  type B extends L3\n}\n",
            "interface L1 {\n  type A extends L2\n}\n",
        ));
        let (ty, diags) = lower_chain(&db, "T", "L1", &["A", "B", "C"]);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        match ty {
            Ty::AssociatedTypeProjection {
                interface, member, ..
            } => {
                assert_eq!(
                    interface
                        .expect("interface determined")
                        .name
                        .name()
                        .as_str(),
                    "L3"
                );
                assert_eq!(member.as_str(), "C");
            }
            other => panic!("expected a symbolic T.A.B.C projection, got {other:?}"),
        }
    }

    // `type A extends Inner<C>` — the bound references a *sibling* associated type `C`.
    // When `A`'s inner interface leaves `C` unpinned, the realized bound must carry `C` as
    // its own symbolic `T.C` projection; a bare `Ty::TypeVar("C")` (an interface-internal
    // name) must never escape into the caller's `T.A.B`.
    #[test]
    fn chained_projection_bound_referencing_sibling_assoc_does_not_leak_typevar() {
        let db = compile(concat!(
            "interface Inner<E> {\n  type B\n}\n",
            "interface Outer {\n  type C\n  type A extends Inner<C>\n}\n",
        ));
        let (ty, diags) = lower_chain(&db, "T", "Outer", &["A", "B"]);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        let Ty::AssociatedTypeProjection {
            interface, member, ..
        } = &ty
        else {
            panic!("expected a symbolic T.A.B projection, got {ty:?}");
        };
        assert_eq!(member.as_str(), "B");
        let interface = interface.as_ref().expect("interface determined");
        assert_eq!(interface.name.name().as_str(), "Inner");
        // `Inner`'s sole argument must be the symbolic `T.C` projection, not `TypeVar("C")`.
        match interface.generics.as_slice() {
            [
                Ty::AssociatedTypeProjection {
                    member: sibling, ..
                },
            ] => {
                assert_eq!(sibling.as_str(), "C");
            }
            other => panic!("expected `Inner`'s arg to be the `T.C` projection, got {other:?}"),
        }
    }

    // Mutually-recursive sibling bounds (`type A extends J<B>`, `type B extends K<A>`) must
    // terminate: each sibling reference realizes to an inert symbolic projection, never
    // recursively expanding the other's bound (and `substitute_ty` is single-pass, so the
    // inserted projection is not re-expanded).
    #[test]
    fn chained_projection_mutually_recursive_sibling_bounds_terminate() {
        let db = compile(concat!(
            "interface J<E> {\n  type X\n}\n",
            "interface K<E> {\n  type Y\n}\n",
            "interface Outer {\n  type A extends J<B>\n  type B extends K<A>\n}\n",
        ));
        let (ty, diags) = lower_chain(&db, "T", "Outer", &["A", "X"]);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        let Ty::AssociatedTypeProjection {
            interface, member, ..
        } = &ty
        else {
            panic!("expected a symbolic T.A.X projection, got {ty:?}");
        };
        assert_eq!(member.as_str(), "X");
        let interface = interface.as_ref().expect("interface determined");
        assert_eq!(interface.name.name().as_str(), "J");
        // `J`'s arg is the finite symbolic `T.B` projection; B's own `K<A>` bound is not opened.
        match interface.generics.as_slice() {
            [
                Ty::AssociatedTypeProjection {
                    base,
                    member: sibling,
                    ..
                },
            ] => {
                assert_eq!(sibling.as_str(), "B");
                assert!(matches!(**base, Ty::TypeVar(ref n, _) if n.as_str() == "T"));
            }
            other => panic!("expected `J`'s arg to be the `T.B` projection, got {other:?}"),
        }
    }

    #[test]
    fn substitute_paths_recurses_into_function_type() {
        let type_expr = TypeExprKind::Function {
            params: vec![FunctionTypeParam {
                name: Some(Name::new("value")),
                optional: false,
                ty: path("T"),
            }],
            ret: Box::new(path("T")),
            throws: Some(Box::new(path("E"))),
            attrs: vec![],
        }
        .at(text_size::TextRange::default());
        let mut subst = std::collections::HashMap::new();
        subst.insert(
            Name::new("T"),
            TypeExprKind::String { attrs: vec![] }.at(text_size::TextRange::default()),
        );
        subst.insert(
            Name::new("E"),
            TypeExprKind::Bool { attrs: vec![] }.at(text_size::TextRange::default()),
        );

        let TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } = substitute_paths_in(&type_expr, &subst).kind
        else {
            panic!("expected function type");
        };

        assert!(matches!(params[0].ty.kind, TypeExprKind::String { .. }));
        assert!(matches!(ret.kind, TypeExprKind::String { .. }));
        assert!(matches!(
            throws.as_deref().map(|t| &t.kind),
            Some(TypeExprKind::Bool { .. })
        ));
    }

    #[test]
    fn substitute_paths_rewrites_substituted_root_member_path_to_projection() {
        let type_expr = path_segments(&["T", "Item"]);
        let replacement = path("Box");
        let mut subst = std::collections::HashMap::new();
        subst.insert(Name::new("T"), replacement.clone());

        let TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = substitute_paths_in(&type_expr, &subst).kind
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
            package: Name::new("test"),
            namespaces: FxHashMap::default(),
            extra: None,
        };
        let type_expr = TypeExprKind::Function {
            params: vec![
                FunctionTypeParam {
                    name: Some(Name::new("query")),
                    optional: false,
                    ty: TypeExprKind::String { attrs: vec![] }.at(text_size::TextRange::default()),
                },
                FunctionTypeParam {
                    name: Some(Name::new("limit")),
                    optional: true,
                    ty: TypeExprKind::Int { attrs: vec![] }.at(text_size::TextRange::default()),
                },
            ],
            ret: Box::new(TypeExprKind::Bool { attrs: vec![] }.at(text_size::TextRange::default())),
            throws: None,
            attrs: vec![],
        }
        .at(text_size::TextRange::default());
        let mut diagnostics = Vec::new();

        let ty = crate::lower_type_expr::lower_type_expr(
            &type_expr,
            &crate::lower_type_expr::ScopeCtx {
                db: &db,
                package_items: &package_items,
                ns_context: &[],
                generic_params: &[],
                bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                self_ty: None,
            },
            &mut diagnostics,
        );

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

    // ── value-checking: the interface-bounded type-argument concreteness gate ──
    //
    // An interface-bounded type parameter (`<T extends Marker>`) admits only a
    // concrete type argument, so virtual dispatch on `T` sees a single runtime
    // type. These drive the real value-checker end-to-end through the same entry
    // the LSP check uses.

    /// Compile `source` and collect every value-checking diagnostic across all
    /// scopes (the entry `check.rs` drives per file).
    fn all_type_errors(source: &str) -> Vec<TirTypeError> {
        let db = compile(source);
        let file = baml_compiler2_hir::compiler2_all_files(&db)
            .into_iter()
            .next()
            .expect("one compiled user file");
        crate::inference::collect_file_diagnostics(&db, file)
            .diagnostics
            .into_iter()
            .map(|d| d.error)
            .collect()
    }

    /// Decl-level `impl_data` diagnostics for every `implements` block in the (single-file,
    /// single-package) source — mirroring how check.rs surfaces `impl_data(loc).diagnostics`
    /// at cutover. This is the path conformance diagnostics travel, since `collect_file_diagnostics`
    /// (the expression/scope path) does not carry them.
    fn impl_diagnostics(source: &str) -> Vec<TirTypeError> {
        let db = compile(source);
        let file = baml_compiler2_hir::compiler2_all_files(&db)
            .into_iter()
            .next()
            .expect("one compiled user file");
        let pkg = baml_compiler2_hir::file_package::file_package(&db, file).package;
        let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, pkg);
        let mut out = Vec::new();
        for impl_loc in crate::interfaces::package_impl_locs(&db, pkg_id) {
            match crate::interfaces::impl_data(&db, impl_loc) {
                Ok(data) => out.extend(data.diagnostics.iter().map(|(e, _)| e.clone())),
                Err(crate::interfaces::ImplDataError::InterfaceUnresolved { diagnostics }) => {
                    out.extend(diagnostics.iter().map(|(e, _)| e.clone()));
                }
                Err(_) => {}
            }
            // Phase-5 signature/type conformance (E0116/E0120), surfaced alongside impl_data.
            out.extend(
                crate::interfaces::validate_impl_signatures(&db, impl_loc)
                    .iter()
                    .map(|(e, _)| e.clone()),
            );
        }
        out
    }

    fn has_not_concrete(errors: &[TirTypeError]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, TirTypeError::BoundedTypeArgNotConcrete { .. }))
    }

    /// An interface `Marker`, three concrete implementors, and a `Marker`-bounded
    /// generic `needs`. Each test appends a caller that binds `needs`'s `T`.
    const MARKER_PROGRAM: &str = "\
interface Marker {
  function mark(self) -> int throws never
}
class Widget {
  implements Marker {
    function mark(self) -> int throws never { 0 }
  }
}
class A {
  implements Marker {
    function mark(self) -> int throws never { 1 }
  }
}
class B {
  implements Marker {
    function mark(self) -> int throws never { 2 }
  }
}
function needs<T extends Marker>(x: T) -> int throws never {
  x.mark()
}
";

    #[test]
    fn existential_bounded_type_arg_is_rejected_as_not_concrete() {
        // `needs(v)` infers `T = Marker` (an interface-existential): a subtype of the
        // bound, but not concrete — the doc's `bar<dyn AdditiveIdentity>` BAD example.
        let src = format!(
            "{MARKER_PROGRAM}function bad(v: Marker) -> int throws never {{\n  needs(v)\n}}\n"
        );
        let errors = all_type_errors(&src);
        assert!(
            has_not_concrete(&errors),
            "existential type argument should trip the concreteness gate, got {errors:?}"
        );
    }

    #[test]
    fn union_bounded_type_arg_is_rejected_as_not_concrete() {
        // `needs(v)` with `v: A | B` infers `T = A | B`: both members implement
        // `Marker` (so it satisfies the bound), but a union has no single runtime type.
        let src = format!(
            "{MARKER_PROGRAM}function bad(v: A | B) -> int throws never {{\n  needs(v)\n}}\n"
        );
        let errors = all_type_errors(&src);
        assert!(
            has_not_concrete(&errors),
            "union type argument should trip the concreteness gate, got {errors:?}"
        );
    }

    #[test]
    fn concrete_bounded_type_arg_is_accepted() {
        // `needs(w)` with `w: Widget` — concrete AND implements `Marker` — satisfies
        // the bound cleanly. Asserting no diagnostics also confirms the fixture itself
        // type-checks, so the rejection tests above are not passing on a broken program.
        let src = format!(
            "{MARKER_PROGRAM}function good(w: Widget) -> int throws never {{\n  needs(w)\n}}\n"
        );
        let errors = all_type_errors(&src);
        assert!(
            errors.is_empty(),
            "a concrete argument that implements the bound should compile clean, got {errors:?}"
        );
    }

    #[test]
    fn class_type_arg_bounded_by_interface_must_be_concrete() {
        // A generic class's bound is checked where the type is written — a different
        // path than a function call — so a non-concrete `Wrap<Marker>` argument is
        // rejected there too.
        let src = format!(
            "{MARKER_PROGRAM}class Wrap<T extends Marker> {{\n  val: T\n}}\n\
             function uses(w: Wrap<Marker>) -> int throws never {{\n  0\n}}\n"
        );
        let errors = all_type_errors(&src);
        assert!(
            has_not_concrete(&errors),
            "a non-concrete class type argument should trip the concreteness gate, got {errors:?}"
        );
    }

    #[test]
    fn bounded_typevar_arg_satisfies_a_matching_bound() {
        // A properly-bounded type variable is a valid argument for a matching bound: it
        // stands for the concrete type that will fill it, which implements `Marker`. This is
        // the "typevar as a concrete member" capability — the bound is an *implements*
        // relation, so `U extends Marker` satisfies `T extends Marker` (rather than being
        // treated like an interface-existential and rejected as not concrete).
        let src = format!(
            "{MARKER_PROGRAM}function forwards<U extends Marker>(y: U) -> int throws never {{\n  needs(y)\n}}\n"
        );
        let errors = all_type_errors(&src);
        assert!(
            errors.is_empty(),
            "a bounded typevar argument should satisfy a matching bound cleanly, got {errors:?}"
        );
    }

    // ── existential-pins-required (E0191-analog): an interface used as a value type must
    //    pin every non-defaulted associated type; interface bounds need not ──

    fn has_missing_assoc(errors: &[TirTypeError]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, TirTypeError::MissingAssociatedTypeBindings { .. }))
    }

    #[test]
    fn existential_interface_must_pin_associated_types() {
        // `it: Iter` — an interface-existential value type — leaves `Item` unpinned.
        let errors = all_type_errors(
            "interface Iter {\n  type Item\n}\nfunction f(it: Iter) -> int throws never {\n  0\n}\n",
        );
        assert!(
            has_missing_assoc(&errors),
            "an unpinned existential should require its associated types, got {errors:?}"
        );
    }

    #[test]
    fn existential_interface_with_all_pins_is_ok() {
        let errors = all_type_errors(
            "interface Iter {\n  type Item\n}\n\
             function f(it: Iter<Item = int>) -> int throws never {\n  0\n}\n",
        );
        assert!(
            !has_missing_assoc(&errors),
            "a fully-pinned existential should not be flagged, got {errors:?}"
        );
    }

    #[test]
    fn existential_interface_may_omit_defaulted_associated_type() {
        // A defaulted associated type may be omitted (the default applies) — not flagged.
        let errors = all_type_errors(
            "interface Iter {\n  type Item = int\n}\nfunction f(it: Iter) -> int throws never {\n  0\n}\n",
        );
        assert!(
            !has_missing_assoc(&errors),
            "a defaulted associated type may be omitted, got {errors:?}"
        );
    }

    #[test]
    fn interface_bound_does_not_require_associated_types() {
        // `<T extends Iter>` is a bound, not an existential — Rust-parity `T: Iterator`
        // does not pin `Item`, so no MissingAssociatedTypeBindings here.
        let errors = all_type_errors(
            "interface Iter {\n  type Item\n}\n\
             function f<T extends Iter>(it: T) -> int throws never {\n  0\n}\n",
        );
        assert!(
            !has_missing_assoc(&errors),
            "an interface bound must not require its associated types be pinned, got {errors:?}"
        );
    }

    // ── object safety: a method whose return nests `Self` in an invariant container is
    //    not callable through an interface-existential receiver (bare `-> Self` still is) ──

    fn has_invalid_self_call(errors: &[TirTypeError]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, TirTypeError::InvalidSelfCallThroughInterface { .. }))
    }

    #[test]
    fn nested_self_return_through_existential_is_rejected() {
        // `-> Self[]`: an impl returns `Concrete[]`, which is NOT a subtype of the
        // existential-tagged `dyn Dup[]` (containers are invariant) — unsound to dispatch.
        let errors = all_type_errors(
            "interface Dup {\n  function dup(self) -> Self[] throws never\n}\n\
             function use_it(d: Dup) -> int throws never {\n  d.dup();\n  0\n}\n",
        );
        assert!(
            has_invalid_self_call(&errors),
            "`-> Self[]` through an existential should be rejected, got {errors:?}"
        );
    }

    #[test]
    fn self_in_generic_arg_return_through_existential_is_rejected() {
        // `-> Box<Self>`: `Self` in an invariant generic argument.
        let errors = all_type_errors(
            "class Box<T> {\n  val: T\n}\ninterface Wrap {\n  function wrap(self) -> Box<Self> throws never\n}\n\
             function use_it(w: Wrap) -> int throws never {\n  w.wrap();\n  0\n}\n",
        );
        assert!(
            has_invalid_self_call(&errors),
            "`-> Box<Self>` through an existential should be rejected, got {errors:?}"
        );
    }

    #[test]
    fn bare_self_return_through_existential_is_allowed() {
        // Bare `-> Self` collapses covariantly to the receiver (`dyn Dup`); the impl's
        // concrete return subtypes it nominally, so it stays object-safe.
        let errors = all_type_errors(
            "interface Dup {\n  function dup(self) -> Self throws never\n}\n\
             function use_it(d: Dup) -> int throws never {\n  d.dup();\n  0\n}\n",
        );
        assert!(
            !has_invalid_self_call(&errors),
            "bare `-> Self` through an existential should stay allowed, got {errors:?}"
        );
    }

    #[test]
    fn optional_self_return_through_existential_is_allowed() {
        // `-> Self?` is a covariant union (`Self | null`) — no invariant nesting.
        let errors = all_type_errors(
            "interface Dup {\n  function dup(self) -> Self? throws never\n}\n\
             function use_it(d: Dup) -> int throws never {\n  d.dup();\n  0\n}\n",
        );
        assert!(
            !has_invalid_self_call(&errors),
            "`-> Self?` through an existential should stay allowed, got {errors:?}"
        );
    }

    // ── uninferable type args = error (not silent erasure) ──

    fn has_cannot_infer(errors: &[TirTypeError]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, TirTypeError::CannotInferTypeParameter { .. }))
    }

    #[test]
    fn phantom_type_param_that_cannot_be_inferred_is_an_error() {
        // `T` occurs nowhere in the signature, so a call cannot infer it — reported now
        // (previously only params occurring in the return type were checked).
        let errors = all_type_errors(
            "function phantom<T>() -> int throws never {\n  0\n}\n\
             function caller() -> int throws never {\n  phantom();\n  0\n}\n",
        );
        assert!(
            has_cannot_infer(&errors),
            "a phantom uninferable type parameter should error, got {errors:?}"
        );
    }

    #[test]
    fn uninferable_return_type_param_with_no_expected_is_an_error() {
        // `let y = opt()` for `opt<T>() -> T?` with no annotation: `T` is uninferable and
        // reported (previously suppressed because the expected type was `unknown`).
        let errors = all_type_errors(
            "function opt<T>() -> T? throws never {\n  null\n}\n\
             function caller() -> int throws never {\n  let y = opt();\n  0\n}\n",
        );
        assert!(
            has_cannot_infer(&errors),
            "an uninferable return-type parameter with no expected type should error, got {errors:?}"
        );
    }

    #[test]
    fn inferable_type_param_from_argument_is_ok() {
        let errors = all_type_errors(
            "function id<T>(x: T) -> T throws never {\n  x\n}\n\
             function caller() -> int throws never {\n  let y = id(5);\n  y.to_json();\n  0\n}\n",
        );
        assert!(
            !has_cannot_infer(&errors),
            "a type parameter inferable from an argument should not error, got {errors:?}"
        );
    }

    fn has_unspecialized(errors: &[TirTypeError]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, TirTypeError::GenericFunctionValueNotSpecialized { .. }))
    }

    #[test]
    fn unspecialized_generic_function_bound_to_unknown_is_an_error() {
        // `let f: unknown = identity` — a non-constraining annotation doesn't specialize the
        // generic function value; it must be specialized (`identity<int>`) before use.
        let errors = all_type_errors(
            "function identity<T>(x: T) -> T throws never {\n  x\n}\n\
             function caller() -> int throws never {\n  let f: unknown = identity;\n  0\n}\n",
        );
        assert!(
            has_unspecialized(&errors),
            "an unspecialized generic function value should error, got {errors:?}"
        );
    }

    #[test]
    fn bare_unspecialized_generic_function_value_is_an_error() {
        // The pre-existing bare-`let` case still fires.
        let errors = all_type_errors(
            "function identity<T>(x: T) -> T throws never {\n  x\n}\n\
             function caller() -> int throws never {\n  let f = identity;\n  0\n}\n",
        );
        assert!(
            has_unspecialized(&errors),
            "a bare unspecialized generic function value should error, got {errors:?}"
        );
    }

    #[test]
    fn value_almost_implementing_a_blanket_names_the_unsatisfied_bound() {
        // `User` is not `Named`, so the blanket `implements<T extends Named> Printable for T`
        // does not apply — constructing a `User` in a `Printable` slot fails. Because the
        // receiver shape matches the blanket but its `Named` bound is unsatisfied, the diagnostic
        // names the bound (BlanketBoundNotSatisfied) rather than a bare type mismatch —
        // exercising the impl-data-backed `first_failing_impl_bound`.
        let errors = all_type_errors(
            "interface Named {\n  name: string\n}\n\
             interface Printable {\n  function display(self) -> string throws never\n}\n\
             class User {\n  name: string\n}\n\
             implements<T extends Named> Printable for T {\n  \
             function display(self) -> string throws never { \"named\" }\n}\n\
             function caller() -> int throws never {\n  \
             let p: Printable = User { name: \"hello\" };\n  0\n}\n",
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TirTypeError::BlanketBoundNotSatisfied { .. })),
            "expected BlanketBoundNotSatisfied naming the `Named` bound, got {errors:?}"
        );
    }

    // ── impl conformance (computed alongside `impl_data` lowering) ──

    #[test]
    fn missing_required_interface_method_is_reported() {
        // `Rude` implements `Greeter` but provides no body for the required `greet` (E0113).
        let diags = impl_diagnostics(
            "interface Greeter {\n  function greet(self) -> string throws never\n}\n\
             class Rude {\n  implements Greeter {}\n}\n",
        );
        assert!(
            diags.iter().any(|e| matches!(
                e,
                TirTypeError::MissingInterfaceMethod { method, .. } if method.as_str() == "greet"
            )),
            "expected MissingInterfaceMethod for `greet`, got {diags:?}"
        );
    }

    #[test]
    fn provided_interface_method_is_not_reported_missing() {
        // `Polite` provides `greet` — no missing-method diagnostic (signature conformance is a
        // separate slice, so a name match suffices here).
        let diags = impl_diagnostics(
            "interface Greeter {\n  function greet(self) -> string throws never\n}\n\
             class Polite {\n  implements Greeter {\n    \
             function greet(self) -> string throws never { \"hi\" }\n  }\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::MissingInterfaceMethod { .. })),
            "a provided method should not be reported missing, got {diags:?}"
        );
    }

    #[test]
    fn out_of_body_impl_of_field_interface_is_reported() {
        // A field-bearing interface can only be implemented in the class body (E0126). A simple
        // `implement HasField for Holder` is merged onto `Holder` for resolution but is written
        // out-of-body, so its origin is `OutOfBody` and the rule fires.
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  x: int\n}\n\
             implements HasField for Holder {}\n",
        );
        assert!(
            diags
                .iter()
                .any(|e| matches!(e, TirTypeError::OutOfBodyImplementsFieldInterface { .. })),
            "expected OutOfBodyImplementsFieldInterface, got {diags:?}"
        );
    }

    #[test]
    fn in_body_impl_of_field_interface_is_allowed() {
        // The same field interface implemented *in-body* is fine — the class provides the fields.
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  x: int\n  implements HasField {}\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::OutOfBodyImplementsFieldInterface { .. })),
            "an in-body impl of a field interface should be allowed, got {diags:?}"
        );
    }

    #[test]
    fn impl_method_not_on_interface_is_reported() {
        // `extra` is neither required nor a default of `Greeter`, so it overrides nothing (E0115).
        let diags = impl_diagnostics(
            "interface Greeter {\n  function greet(self) -> string throws never\n}\n\
             class Polite {\n  implements Greeter {\n    \
             function greet(self) -> string throws never { \"hi\" }\n    \
             function extra(self) -> int throws never { 0 }\n  }\n}\n",
        );
        assert!(
            diags.iter().any(|e| matches!(
                e,
                TirTypeError::UnknownInterfaceMember { member, .. } if member.as_str() == "extra"
            )),
            "expected UnknownInterfaceMember for `extra`, got {diags:?}"
        );
    }

    #[test]
    fn overriding_a_default_method_is_allowed() {
        // Overriding an interface *default* method is legal — not an unknown member.
        let diags = impl_diagnostics(
            "interface Greeter {\n  function greet(self) -> string throws never { \"default\" }\n}\n\
             class Polite {\n  implements Greeter {\n    \
             function greet(self) -> string throws never { \"hi\" }\n  }\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::UnknownInterfaceMember { .. })),
            "overriding a default should not be UnknownInterfaceMember, got {diags:?}"
        );
    }

    #[test]
    fn missing_interface_field_is_reported() {
        // `Holder` has no field for `HasField.x` (no same-name field, no link) → E0124.
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  y: int\n  implements HasField {}\n}\n",
        );
        assert!(
            diags.iter().any(|e| matches!(
                e,
                TirTypeError::MissingInterfaceField { field, .. } if field.as_str() == "x"
            )),
            "expected MissingInterfaceField for `x`, got {diags:?}"
        );
    }

    #[test]
    fn same_named_field_satisfies_interface_field() {
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  x: int\n  implements HasField {}\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::MissingInterfaceField { .. })),
            "a same-named class field should satisfy the interface field, got {diags:?}"
        );
    }

    #[test]
    fn linked_field_satisfies_interface_field() {
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  y: int\n  implements HasField {\n    x as y\n  }\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::MissingInterfaceField { .. })),
            "an explicit field link should satisfy the interface field, got {diags:?}"
        );
    }

    #[test]
    fn mismatched_interface_field_type_is_reported() {
        // `Holder.x: string` but `HasField.x: int` — field types are invariant (E0116).
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  x: string\n  implements HasField {}\n}\n",
        );
        assert!(
            diags.iter().any(|e| matches!(
                e,
                TirTypeError::InterfaceFieldTypeMismatch { field, .. } if field.as_str() == "x"
            )),
            "expected InterfaceFieldTypeMismatch for `x`, got {diags:?}"
        );
    }

    #[test]
    fn matching_interface_field_type_is_accepted() {
        let diags = impl_diagnostics(
            "interface HasField {\n  x: int\n}\n\
             class Holder {\n  x: int\n  implements HasField {}\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::InterfaceFieldTypeMismatch { .. })),
            "a matching field type should be accepted, got {diags:?}"
        );
    }

    #[test]
    fn method_signature_mismatch_is_reported() {
        // Override returns `int` but the interface declares `string` — return is covariant, and
        // `int </: string`, so the signature does not conform (E0120).
        let diags = impl_diagnostics(
            "interface Greeter {\n  function greet(self) -> string throws never\n}\n\
             class Rude {\n  implements Greeter {\n    \
             function greet(self) -> int throws never { 0 }\n  }\n}\n",
        );
        assert!(
            diags.iter().any(|e| matches!(
                e,
                TirTypeError::InterfaceMethodSignatureMismatch { method, .. } if method.as_str() == "greet"
            )),
            "expected InterfaceMethodSignatureMismatch for `greet`, got {diags:?}"
        );
    }

    #[test]
    fn widened_override_param_is_accepted() {
        // Args are contravariant: an override may accept a *supertype* (`int | string` ⊇ `int`).
        let diags = impl_diagnostics(
            "interface Handler {\n  function handle(self, x: int) -> int throws never\n}\n\
             class Wide {\n  implements Handler {\n    \
             function handle(self, x: int | string) -> int throws never { 0 }\n  }\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::InterfaceMethodSignatureMismatch { .. })),
            "a contravariantly-widened param should conform, got {diags:?}"
        );
    }

    #[test]
    fn missing_required_interface_is_reported() {
        // `Greeter requires Named`, but `Rude` implements only `Greeter` (E0125).
        let diags = impl_diagnostics(
            "interface Named {\n  function name(self) -> string throws never\n}\n\
             interface Greeter requires Named {\n  function greet(self) -> string throws never\n}\n\
             class Rude {\n  implements Greeter {\n    \
             function greet(self) -> string throws never { \"hi\" }\n  }\n}\n",
        );
        assert!(
            diags
                .iter()
                .any(|e| matches!(e, TirTypeError::MissingRequiredInterface { .. })),
            "expected MissingRequiredInterface, got {diags:?}"
        );
    }

    #[test]
    fn implemented_required_interface_is_accepted() {
        let diags = impl_diagnostics(
            "interface Named {\n  function name(self) -> string throws never\n}\n\
             interface Greeter requires Named {\n  function greet(self) -> string throws never\n}\n\
             class Polite {\n  \
             implements Named {\n    function name(self) -> string throws never { \"p\" }\n  }\n  \
             implements Greeter {\n    function greet(self) -> string throws never { \"hi\" }\n  }\n}\n",
        );
        assert!(
            !diags
                .iter()
                .any(|e| matches!(e, TirTypeError::MissingRequiredInterface { .. })),
            "implementing the required interface should conform, got {diags:?}"
        );
    }

    // ── `_` type-inference placeholder: rejected (inference variables unimplemented) ──

    #[test]
    fn type_inference_placeholder_is_rejected() {
        // A `_` type placeholder (inference variable) is not supported — it must be a hard
        // error, and must never lower to `Ty::Infer` (which the normalizer treats as
        // `unreachable!`), so checking cannot panic.
        let errors = all_type_errors(
            "class Box<T> {\n  val: T\n}\n\
             function make() -> Box<int> throws never {\n  Box<int> { val: 5 }\n}\n\
             function caller() -> int throws never {\n  let b: Box<_> = make();\n  0\n}\n",
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TirTypeError::CannotInferType)),
            "a `_` type placeholder should be rejected with CannotInferType, got {errors:?}"
        );
    }

    // The two tests below describe the deferred `_` inference-variable feature (fill a
    // partial annotation's holes from the initializer). They are ignored while `_` is a hard
    // error; un-ignore and rewrite them against the real fill diagnostic when it lands.

    fn has_unfilled_infer_hole(errors: &[TirTypeError]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, TirTypeError::UnresolvedType { name, .. } if name.as_str() == "_"))
    }

    #[test]
    #[ignore = "`_` inference variables are a deferred feature; `_` is currently a hard error"]
    fn infer_hole_in_annotation_is_filled_from_initializer() {
        // `let b: Box<_> = <Box<int>>` fills `_` to `int` — no un-inferable-`_` error, and
        // (crucially) no panic from a `Ty::Infer` reaching the normalizer.
        let errors = all_type_errors(
            "class Box<T> {\n  val: T\n}\n\
             function make() -> Box<int> throws never {\n  Box<int> { val: 5 }\n}\n\
             function caller() -> int throws never {\n  let b: Box<_> = make();\n  0\n}\n",
        );
        assert!(
            !has_unfilled_infer_hole(&errors),
            "a fillable `_` hole should be filled, not reported, got {errors:?}"
        );
    }

    #[test]
    #[ignore = "`_` inference variables are a deferred feature; `_` is currently a hard error"]
    fn uninferable_infer_hole_is_an_error() {
        // `Box<_>` filled from a non-`Box` initializer (`5`): the `_` cannot align to a
        // position of the initializer's type, so it is an un-inferable `_` — reported (not a
        // panic). (There is also a type mismatch; the test only asserts the `_` diagnostic.)
        let errors = all_type_errors(
            "class Box<T> {\n  val: T\n}\n\
             function caller() -> int throws never {\n  let b: Box<_> = 5;\n  0\n}\n",
        );
        assert!(
            has_unfilled_infer_hole(&errors),
            "an un-inferable `_` should be reported, got {errors:?}"
        );
    }
}
