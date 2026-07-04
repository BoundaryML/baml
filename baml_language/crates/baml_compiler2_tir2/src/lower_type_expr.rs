//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::TypeExpr;
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
        TypeExpr::AssociatedTypeProjection {
            base: Box::new(path(base)),
            interface: None,
            member: Name::new(member),
            attrs: vec![],
        }
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
        let projection = TypeExpr::AssociatedTypeProjection {
            base: Box::new(TypeExpr::Int { attrs: vec![] }),
            interface: None,
            member: Name::new("Missing"),
            attrs: vec![],
        };
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
            items,
            &[],
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
            items,
            &[],
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
            items,
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
            items,
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

    // A sibling type-variable bound (`<T, U extends T>`) is a special form, not a
    // non-interface bound.
    #[test]
    fn sibling_type_var_bound_is_not_an_error() {
        let db = compile("function pick<T, U extends T>(a: T, b: U) -> T {\n  return a\n}\n");
        let errs = decl_scope_type_errors(&db, "pick");
        assert!(
            !errs
                .iter()
                .any(|e| matches!(e, TirTypeError::GenericBoundNotInterface { .. })),
            "sibling type-var bound should not error, got {errs:?}"
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
        let expr = TypeExpr::AssociatedTypeProjection {
            base: Box::new(path(tvar)),
            interface: None,
            member: Name::new(member),
            attrs: vec![],
        };
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
            TypeExpr::AssociatedTypeProjection {
                base: Box::new(base),
                interface: None,
                member: Name::new(member),
                attrs: vec![],
            }
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
            package: Name::new("test"),
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
}
