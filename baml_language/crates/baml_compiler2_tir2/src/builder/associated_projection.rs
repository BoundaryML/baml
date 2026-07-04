//! Lowering logic for associated type projections.
//!
//! An associated type projection has three parts:
//! 1. The base (concrete type or bounded type variable)
//! 2. The interface
//! 3. The associated type name
//!
//! This can either be `(C as I).A` or `C.A`.
//! If the interface is not explicitly specified,
//! it must be unambiguously inferrable at lowering time.
//!
//! This file contains the logic for lowering and doing this inference.
//! The canonical form for an associated type projection is this triple.
//! At runtime, it will be resolved to a fully realized type.
//! However, for projections with sufficient information available
//! at compile time, we may be able to pre-compute the result.
//! This is an opportunistic optimization and is not guaranteed to be applied.
//!
//! # Boundary
//!
//! Inference depends only on the surrounding scope — the salsa database, the
//! visible type-alias expansions, each in-scope type variable's interface
//! bound, and the scope's concrete-projection resolver (impl lookup for a
//! concrete base) — supplied through the [`TypeExprContext`] trait. Nothing
//! else about the builder or the wider inference state crosses into this
//! module: the determination is a pure function of the base type, the optional
//! explicit interface, the member name, and that context.
//!
//! [`TypeExprContext`]: crate::lower_type_expr::TypeExprContext

use baml_base::{Name, attr::TyAttr};
use baml_compiler2_hir::{contributions::Definition, loc::InterfaceLoc, package::PackageId};
use rustc_hash::FxHashMap;

use crate::{
    infer_context::{AssocContainer, TirTypeError},
    lower_type_expr::{ConcreteProjection, TypeExprContext},
    ty::{QualifiedTypeName, Ty},
    type_context::{GlobalTypeContext, TypeVarBounds},
};

/// The result of lowering a projection: the resolved [`Ty`] — the canonical
/// triple when the interface is determined, otherwise [`Ty::Error`] — plus any
/// diagnostics the caller should surface.
pub(crate) struct ProjectionLowering {
    pub ty: Ty,
    pub diagnostics: Vec<TirTypeError>,
}

/// Lower `base.member` or `(base as explicit_interface).member` to its canonical
/// [`Ty::AssociatedTypeProjection`], determining the declaring interface.
///
/// The interface is determined here so the triple is self-describing: an
/// explicit `as I` qualifier is validated to actually declare `member`; an
/// unqualified base has its interface inferred by the base's kind — a type
/// variable's bound conjunction, an interface existential's own
/// `requires`-closure, a concrete type's visible impls, or (for a chained
/// `T.A.B`) the inner associated type's declared bound. When no interface can
/// be determined the projection is ill-formed and lowers to [`Ty::Error`].
pub(crate) fn lower_projection(
    ctx: &dyn TypeExprContext<'_>,
    base: Ty,
    explicit_interface: Option<Ty>,
    member: Name,
) -> ProjectionLowering {
    let mut diagnostics = Vec::new();
    let ty = match determine_interface(ctx, &base, explicit_interface, &member) {
        Determination::Determined(interface) => {
            // Opportunistic precompute: if the determined interface already pins `member`
            // to a concrete type — `Self` carrying `Item→int`, or a bound
            // `T extends Iterator<Item = int>` — collapse to that type rather than emit a
            // symbolic projection triple. Otherwise the projection stays symbolic and is
            // realized at monomorphization.
            if let Some((_, pinned)) = interface
                .associated_types
                .iter()
                .find(|(name, _)| *name == member)
            {
                pinned.clone()
            } else {
                Ty::AssociatedTypeProjection {
                    base: Box::new(base),
                    interface: Some(Box::new(interface)),
                    member,
                    attr: TyAttr::default(),
                }
            }
        }
        Determination::Undeclared { container } => {
            diagnostics.push(TirTypeError::UnknownAssociatedType { member, container });
            error_ty()
        }
        Determination::Ambiguous(candidates) => {
            diagnostics.push(TirTypeError::AmbiguousAssociatedTypeProjection {
                member,
                candidates: candidates.into_iter().map(|iface| iface.name).collect(),
            });
            error_ty()
        }
        Determination::NonInterfaceQualifier => {
            diagnostics.push(TirTypeError::NonInterfaceProjectionQualifier);
            error_ty()
        }
        // Undeterminable here but not ill-formed: a base that cannot carry a projection,
        // or a base/qualifier that already errored (or resolves elsewhere). Lower to
        // `Ty::Error` without a fresh diagnostic.
        Determination::InvalidBase | Determination::Poisoned => error_ty(),
    };
    ProjectionLowering { ty, diagnostics }
}

fn error_ty() -> Ty {
    Ty::Error {
        attr: TyAttr::default(),
    }
}

/// The outcome of resolving which interface a projection's `member` is declared on.
enum Determination {
    /// The declaring interface, at its realized instantiation — the canonical
    /// triple's interface component.
    Determined(baml_type::Interface),
    /// The subject cannot resolve `member`: an interface subject does not
    /// declare it directly (interfaces do not inherit associated types through
    /// `requires`), or no impl of a concrete subject provides it.
    Undeclared { container: AssocContainer },
    /// More than one distinct interface in scope declares `member`; the projection
    /// must be disambiguated with an explicit `(base as I).member`.
    Ambiguous(Vec<baml_type::Interface>),
    /// The explicit `(base as I)` qualifier resolved to something other than an
    /// interface — a class, enum, primitive, or type variable cannot qualify a
    /// projection.
    NonInterfaceQualifier,
    /// `base` is a kind that cannot carry an associated-type projection.
    InvalidBase,
    /// `base` or the explicit qualifier already errored upstream.
    Poisoned,
}

fn determine_interface(
    ctx: &dyn TypeExprContext<'_>,
    base: &Ty,
    explicit_interface: Option<Ty>,
    member: &Name,
) -> Determination {
    // Explicit `(base as I).member`: the interface is given. It must be an
    // interface and must declare `member` *directly* — `requires` is a bound,
    // not inheritance, so a required interface's member projects through that
    // interface, not its requirer. The base is not consulted on this path, so
    // its alias expansion is deferred to the unqualified path below.
    if let Some(explicit_interface) = explicit_interface {
        let explicit_interface = expand_aliases(ctx, explicit_interface);
        let Some(interface) = explicit_interface.as_interface() else {
            return match explicit_interface {
                // The qualifier itself failed to lower — already diagnosed there.
                Ty::Error { .. } | Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } => {
                    Determination::Poisoned
                }
                // A *resolved* non-interface (`(x as SomeClass).Item`) lowered
                // cleanly with no diagnostic, so silence here would swallow the
                // error entirely — it is its own ill-formedness.
                _ => Determination::NonInterfaceQualifier,
            };
        };
        return if interface_declares_member(ctx.db(), &interface.name, member) {
            Determination::Determined(interface)
        } else {
            Determination::Undeclared {
                container: AssocContainer::Interface(interface.name),
            }
        };
    }

    // Unqualified `base.member`: pick the interface root(s) to search by the base's
    // kind, then resolve through their `requires`-closures. Exhaustive — every kind
    // is classified, never silently dropped.
    let base = expand_aliases(ctx, base.clone());
    match &base {
        // An interface existential is its own (single) search root.
        Ty::Interface(qtn, args, assoc, _) => {
            let root = baml_type::Interface::new(qtn.clone(), args.clone(), assoc.clone());
            let container = AssocContainer::Interface(root.name.clone());
            resolve_through_roots(ctx.db(), vec![root], member, container)
        }
        // A type variable searches the closure of *every* interface in its bound
        // conjunction (`T extends A & B`). No bound at all means it cannot be proven
        // to implement any interface, so no interface can declare `member`.
        Ty::TypeVar(name, _) => match ctx.type_var_bounds(name) {
            // A `Ty::TypeVar` projection base is always an in-scope generic parameter — a
            // bare path lowers to a type variable only when it *is* one, and a rigid `Self`
            // receiver is registered in `generic_params` wherever its `self_ty` is set. So
            // this is unreachable; if a lowering site ever sets a type variable / `Self`
            // receiver without threading it into `generic_params`, that bug surfaces here.
            None => unreachable!(
                "type variable `{name}` projected but not in this scope's generic_params — \
                 a lowering site failed to thread it"
            ),
            // Declared but genuinely unbounded.
            Some(bounds) if bounds.is_empty() => Determination::Undeclared {
                container: AssocContainer::TypeVar(name.clone()),
            },
            Some(bounds) => {
                // Report against the first bound if none declares `member`.
                let container = AssocContainer::Interface(bounds[0].name.clone());
                resolve_through_roots(ctx.db(), bounds.into_vec(), member, container)
            }
        },
        // Concrete receivers resolve through their own impls: an associated type
        // lives on a *separate* `impl I for C` (interfaces are bounds, not
        // inheritance), found via the visible impl set rather than any closure.
        Ty::Class(..)
        | Ty::Enum(..)
        | Ty::List(..)
        | Ty::Map { .. }
        | Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::EnumVariant(..) => determine_concrete(ctx, &base, member),
        // A chained projection base (`T.A.B`): the inner `T.A` already resolved to a
        // symbolic projection through the interface that declares `A`; `B` resolves
        // through the interface bound declared on `A` (`type A extends J`).
        Ty::AssociatedTypeProjection {
            base: inner_base,
            interface: inner_interface,
            member: inner_member,
            ..
        } => {
            let inner_interface = inner_interface.as_ref().unwrap_or_else(|| {
                unreachable!("a symbolic projection base always carries its determined interface")
            });
            determine_chained(
                ctx.db(),
                &base,
                inner_base,
                inner_interface,
                inner_member,
                member,
            )
        }
        // Already-errored bases: propagate without a fresh diagnostic.
        Ty::Error { .. } | Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } => {
            Determination::Poisoned
        }
        // A surviving alias means the alias map was incomplete; degrade conservatively.
        Ty::TypeAlias(..) => Determination::Poisoned,
        // Kinds that cannot carry an associated-type projection.
        Ty::Union(..)
        | Ty::Future(..)
        | Ty::Function { .. }
        | Ty::RustType { .. }
        | Ty::WatchAccessor(..)
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Never { .. }
        | Ty::Void { .. }
        | Ty::Null { .. }
        | Ty::EvolvingList(..)
        | Ty::EvolvingMap(..) => Determination::InvalidBase,
    }
}

/// Resolve `member` through a set of interface roots — a type variable's bound
/// conjunction, or a single interface existential.
///
/// Each root that declares `member` *directly* is itself a declarer, resolved
/// without walking its `requires`-closure. NOTE: this deliberately deviates from
/// Rust's E0221 — in Rust a root and a required interface both declaring the
/// same name is *ambiguous* (they are distinct associated types; `requires` is a
/// bound, not inheritance). Here the root shadows: the stdlib's
/// `Iterator requires Iterable<Item = Self.Item>` pinning idiom relies on
/// `Self.Item` resolving to the root's own declaration while that very clause is
/// being lowered (the short-circuit is also what terminates that recursion).
/// Otherwise the root's closure is searched. Declarers across all roots are
/// deduplicated by realized identity: one declarer is
/// [`Determination::Determined`], several distinct ones (e.g. both arms of a
/// `T: A & B` bound declaring `member`) are [`Determination::Ambiguous`], none
/// is [`Determination::Undeclared`] against `undeclared`.
fn resolve_through_roots(
    db: &dyn crate::Db,
    roots: Vec<baml_type::Interface>,
    member: &Name,
    undeclared: AssocContainer,
) -> Determination {
    let mut declarers: Vec<baml_type::Interface> = Vec::new();
    let mut push = |interface: baml_type::Interface| {
        if !declarers.contains(&interface) {
            declarers.push(interface);
        }
    };
    for root in roots {
        if interface_declares_member(db, &root.name, member) {
            push(root);
        } else {
            for declarer in closure_declarers(db, &root, member) {
                push(declarer);
            }
        }
    }
    match declarers.len() {
        0 => Determination::Undeclared {
            container: undeclared,
        },
        1 => Determination::Determined(
            declarers
                .into_iter()
                .next()
                .unwrap_or_else(|| unreachable!("length checked == 1")),
        ),
        _ => Determination::Ambiguous(declarers),
    }
}

/// Resolve `member` on a chained base `inner_base.inner_member` (`= T.A`, whose
/// realized interface is `inner_interface`) — i.e. the outer `.member` of `T.A.B`.
///
/// `T.A`'s type is whatever an implementor pins it to, so it is searched
/// *nominally* through the interface bound declared on `A` (`type A extends J`),
/// realized at this projection: `Self → T`, the interface's generics → the inner
/// interface's, and each sibling associated type → its inner pin. `member` then
/// resolves through that bound (which may itself pin it, collapsing the chain).
/// An associated type with no declared bound cannot be proven to implement any
/// interface, so `member` is unknown on it.
fn determine_chained(
    db: &dyn crate::Db,
    projection: &Ty,
    inner_base: &Ty,
    inner_interface: &baml_type::Interface,
    inner_member: &Name,
    member: &Name,
) -> Determination {
    let Some(iface_loc) = resolve_interface_loc(db, &inner_interface.name) else {
        // The inner interface does not resolve — already errored upstream.
        return Determination::Poisoned;
    };
    let Some(root) =
        associated_type_bound_interface(db, iface_loc, inner_interface, inner_base, inner_member)
    else {
        return Determination::Undeclared {
            container: AssocContainer::Ty(projection.clone()),
        };
    };
    let container = AssocContainer::Interface(root.name.clone());
    resolve_through_roots(db, vec![root], member, container)
}

/// The interface bound declared on associated type `member` of the interface at
/// `iface_loc` (`type member extends J`), realized at `realized` (the inner
/// interface as reached) with `self_ty` as the projection's own base. `None` when
/// `member` is not declared there, has no `extends` bound, or the bound does not
/// lower to an interface.
fn associated_type_bound_interface(
    db: &dyn crate::Db,
    iface_loc: InterfaceLoc<'_>,
    realized: &baml_type::Interface,
    self_ty: &Ty,
    member: &Name,
) -> Option<baml_type::Interface> {
    let tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
    let iface = tree.interfaces.get(&iface_loc.id(db))?;
    let assoc = iface.associated_types.iter().find(|a| &a.name == member)?;
    let bound_te = assoc.bound.as_ref()?;

    let pkg = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
    let pkg_items = baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg.package.clone()));

    // The bound is realized at the projection: `Self` is the projection's base, the
    // interface's generics are the realized ones, and each sibling associated type is
    // its inner pin.
    let mut bindings: FxHashMap<Name, Ty> = FxHashMap::default();
    bindings.insert(Name::new("Self"), self_ty.clone());
    // `realized` should carry exactly one argument per declared generic parameter; a mismatch
    // is a malformed bound (an under-instantiated generic interface, reported as
    // `WrongNumberOfTypeArgs` at its declaration). Bind any un-provided parameter to
    // `Ty::Error` rather than leaving it a bare `Ty::TypeVar` that would escape the result.
    debug_assert_eq!(
        iface.generic_params.len(),
        realized.generics.len(),
        "bound `{}` realized with {} generic args for {} declared parameters",
        realized.name.name(),
        realized.generics.len(),
        iface.generic_params.len(),
    );
    for (i, param) in iface.generic_params.iter().enumerate() {
        let arg = realized.generics.get(i).cloned().unwrap_or_else(error_ty);
        bindings.insert(param.clone(), arg);
    }
    // Every associated type is an in-scope name in the bound expression (`type A extends
    // Inner<C>` references sibling `C`), so each must be bound. Use its realized pin, or —
    // when the inner interface leaves it unpinned — its own symbolic `<base>.Assoc`
    // projection. Binding only the pins would leave an unpinned sibling reference as a bare
    // `Ty::TypeVar`, escaping unsubstituted into the result (an interface-internal name
    // leaking into the caller's type).
    for assoc in &iface.associated_types {
        let value = realized
            .associated_types
            .iter()
            .find(|(name, _)| name == &assoc.name)
            .map(|(_, pin)| pin.clone())
            .unwrap_or_else(|| Ty::AssociatedTypeProjection {
                base: Box::new(self_ty.clone()),
                interface: Some(Box::new(realized.clone())),
                member: assoc.name.clone(),
                attr: TyAttr::default(),
            });
        bindings.insert(assoc.name.clone(), value);
    }

    // The bound is written in the interface's own scope: `Self` is bounded by the
    // interface itself (so `Self.member` in the bound resolves), plus the interface's
    // declared parameter bounds; its generics and associated names are in scope.
    let mut bounds = crate::lower_type_expr::interface_generic_param_bounds(db, iface_loc).clone();
    bounds.insert(Name::new("Self"), vec![realized.clone()]);
    let generic_params: Vec<Name> = iface
        .generic_params
        .iter()
        .cloned()
        .chain(iface.associated_types.iter().map(|a| a.name.clone()))
        .chain(std::iter::once(Name::new("Self")))
        .collect();

    let scope = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &pkg.namespace_path,
        generic_params: &generic_params,
        bounds: &bounds,
        self_ty: Some(Ty::TypeVar(Name::new("Self"), TyAttr::default())),
    };
    // The bound is checked at the interface's declaration; diagnostics are discarded here.
    let mut diags = Vec::new();
    let lowered = crate::generics::substitute_ty(
        &crate::lower_type_expr::lower_type_expr(&bound_te.expr, &scope, &mut diags),
        &bindings,
    );
    lowered.as_interface()
}

/// Resolve a concrete base's projection through the impls visible in `ctx`'s
/// scope, mapping the impl-set result to a [`Determination`].
fn determine_concrete(ctx: &dyn TypeExprContext<'_>, base: &Ty, member: &Name) -> Determination {
    match ctx.concrete_projection(base, member) {
        ConcreteProjection::Determined(interface) => Determination::Determined(interface),
        ConcreteProjection::Ambiguous(candidates) => Determination::Ambiguous(candidates),
        ConcreteProjection::Undeclared => Determination::Undeclared {
            container: match base {
                Ty::Class(qtn, ..) => AssocContainer::Class(qtn.clone()),
                Ty::Enum(qtn, ..) => AssocContainer::Enum(qtn.clone()),
                _ => AssocContainer::Ty(base.clone()),
            },
        },
    }
}

/// Expand a top-level type alias to its target, following alias chains. Stops at
/// the first non-alias (or an unknown alias, left as-is for conservative
/// handling). Bounded to avoid spinning on a cyclic alias.
fn expand_aliases(ctx: &dyn TypeExprContext<'_>, mut ty: Ty) -> Ty {
    for _ in 0..64 {
        let Ty::TypeAlias(qtn, _) = &ty else {
            return ty;
        };
        match crate::inference::alias_def(ctx.db(), qtn) {
            Some(expanded) => ty = expanded,
            None => return ty,
        }
    }
    ty
}

/// Every interface in `root`'s `requires`-closure (including `root` itself) that
/// declares `member` directly, at its realized instantiation. Deduplicated by
/// realized identity, so a member reachable through two paths to the *same*
/// interface counts once; distinct declaring interfaces each contribute.
fn closure_declarers(
    db: &dyn crate::Db,
    root: &baml_type::Interface,
    member: &Name,
) -> Vec<baml_type::Interface> {
    let Some(root_loc) = resolve_interface_loc(db, &root.name) else {
        return Vec::new();
    };
    let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
        db,
        root_loc,
        &root.generics,
        &root.associated_types,
    );

    let mut declarers: Vec<baml_type::Interface> = Vec::new();
    for (loc, args, assoc) in closure {
        if !interface_declares_member_at(db, loc, member) {
            continue;
        }
        let Some(qtn) = crate::interfaces::interface_loc_qtn(db, loc) else {
            continue;
        };
        let interface = baml_type::Interface::new(qtn, args, assoc);
        if !declarers.contains(&interface) {
            declarers.push(interface);
        }
    }
    declarers
}

/// Resolve `base.member` for a concrete `base` through the impls visible in
/// `pkg` (its own package plus its dependency closure).
///
/// Each impl for `base` contributes its realized interface
/// ([`ResolvedImpl::implemented_interface`]); those that declare `member`
/// *directly* are the candidates, deduplicated by realized identity. The impl
/// bounds are discharged through the canonical algebra driven by a
/// [`GlobalTypeContext`], so a bound on a type variable inside `base`
/// (`List<U>.Item` with `U: Ord`) resolves against the scope's bounds.
///
/// [`ResolvedImpl::implemented_interface`]: crate::interfaces::ResolvedImpl::implemented_interface
pub(crate) fn resolve_concrete_projection(
    db: &dyn crate::Db,
    pkg: &Name,
    bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    base: &Ty,
    member: &Name,
) -> ConcreteProjection {
    let pkg_id = PackageId::new(db, pkg.clone());
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let aliases = crate::inference::package_alias_map(db, res_ctx);
    let gctx = GlobalTypeContext {
        db,
        res_ctx,
        aliases: &aliases,
        bounds: TypeVarBounds::Interfaces(bounds),
    };

    let mut declarers: Vec<baml_type::Interface> = Vec::new();
    for resolved in crate::interfaces::impls_for_type(db, pkg_id, base, &aliases, |a, b| {
        baml_type::normalize::is_subtype(a, b, &gctx)
    }) {
        let interface = resolved.implemented_interface(db);
        if interface_declares_member(db, &interface.name, member) && !declarers.contains(&interface)
        {
            declarers.push(interface);
        }
    }

    match declarers.len() {
        0 => ConcreteProjection::Undeclared,
        1 => ConcreteProjection::Determined(
            declarers
                .into_iter()
                .next()
                .unwrap_or_else(|| unreachable!("length checked == 1")),
        ),
        _ => ConcreteProjection::Ambiguous(declarers),
    }
}

/// Whether interface `qtn` declares associated type `member` directly.
fn interface_declares_member(db: &dyn crate::Db, qtn: &QualifiedTypeName, member: &Name) -> bool {
    resolve_interface_loc(db, qtn).is_some_and(|loc| interface_declares_member_at(db, loc, member))
}

/// Whether the interface at `loc` declares associated type `member` directly.
fn interface_declares_member_at(db: &dyn crate::Db, loc: InterfaceLoc<'_>, member: &Name) -> bool {
    let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
    tree.interfaces.get(&loc.id(db)).is_some_and(|iface| {
        iface
            .associated_types
            .iter()
            .any(|assoc| &assoc.name == member)
    })
}

/// Resolve an interface's qualified name to its declaration location.
fn resolve_interface_loc<'db>(
    db: &'db dyn crate::Db,
    qtn: &QualifiedTypeName,
) -> Option<InterfaceLoc<'db>> {
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    match pkg_items.lookup_type(qtn.namespace(), qtn.name())? {
        Definition::Interface(loc) => Some(loc),
        _ => None,
    }
}
