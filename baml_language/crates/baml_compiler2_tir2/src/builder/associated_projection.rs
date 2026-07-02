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
//! visible type-alias expansions, and each in-scope type variable's interface
//! bound — supplied through the [`TypeExprContext`] trait. Nothing else about
//! the builder or the wider inference state crosses into this module: the
//! determination is a pure function of the base type, the optional explicit
//! interface, the member name, and that context.
//!
//! [`TypeExprContext`]: crate::lower_type_expr::TypeExprContext

use baml_base::{Name, attr::TyAttr};
use baml_compiler2_hir::{contributions::Definition, loc::InterfaceLoc, package::PackageId};

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
/// unqualified base has its interface inferred (its bound for a type variable,
/// its own `requires`-closure for an interface existential). When no interface
/// can be determined the projection is ill-formed and lowers to [`Ty::Error`].
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
        // Undeterminable here but not ill-formed: a chained base awaiting its inner
        // resolution, a base that cannot carry a projection, or a base/qualifier (or
        // out-of-scope type variable) that resolves elsewhere. Lower to `Ty::Error`
        // without a fresh diagnostic.
        Determination::Deferred | Determination::InvalidBase | Determination::Poisoned => {
            error_ty()
        }
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
    /// The interface cannot be determined yet, but the projection is not ill-formed:
    /// a chained base (`T.A.B`) whose inner projection must resolve first.
    #[deprecated = "placeholder scaffolding: a chained projection must resolve through the inner \
        associated type's declared bound (or be an error). Remove once the chained path lands."]
    Deferred,
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
    let base = expand_aliases(ctx, base.clone());

    // Explicit `(base as I).member`: the interface is given. It must be an
    // interface and must declare `member` *directly* — `requires` is a bound,
    // not inheritance, so a required interface's member projects through that
    // interface, not its requirer.
    if let Some(explicit_interface) = explicit_interface {
        let explicit_interface = expand_aliases(ctx, explicit_interface);
        let Some(interface) = explicit_interface.as_interface() else {
            // Qualifier resolved to a non-interface; already lowered/diagnosed there.
            return Determination::Poisoned;
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
        // A chained projection base (`T.A.B`) needs its inner projection resolved first.
        Ty::AssociatedTypeProjection { .. } => Determination::Deferred,
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
/// without walking its `requires`-closure (`requires` is a bound, not inheritance;
/// this also avoids re-entering the closure while it lowers a
/// `requires I<Assoc = Self.Assoc>` binding). Otherwise the root's closure is
/// searched. Declarers across all roots are deduplicated by realized identity: one
/// declarer is [`Determination::Determined`], several distinct ones (e.g. both
/// arms of a `T: A & B` bound declaring `member`) are [`Determination::Ambiguous`],
/// none is [`Determination::Undeclared`] against `undeclared`.
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
    let pkg_id = PackageId::new(db, root.name.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
        db,
        root_loc,
        &root.generics,
        &root.associated_types,
        pkg_items,
        root.name.namespace(),
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
