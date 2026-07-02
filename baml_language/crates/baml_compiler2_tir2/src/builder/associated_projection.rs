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
        // Undeterminable here but not ill-formed: a type variable whose bound is
        // out of scope, a concrete or chained base awaiting impl-based resolution,
        // or a base/qualifier that already errored upstream. Lower to `Ty::Error`
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
    /// The interface cannot be determined in this scope, but the projection is not
    /// ill-formed: a type variable with no bound in scope, or a concrete/chained
    /// base whose resolution is impl-based.
    #[deprecated = "placeholder scaffolding: a bounded type variable must resolve through its \
        bound's closure or be an error (projections are nominal, not structural). Remove once \
        every typevar-projection lowering site threads its bounds and the concrete/chained \
        impl-based path lands."]
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

    // Unqualified `base.member`: pick the interface(s) to search by the base's
    // kind. Exhaustive — every kind is classified, never silently dropped.
    let root = match &base {
        // An interface existential is its own search root.
        Ty::Interface(qtn, args, assoc, _) => {
            baml_type::Interface::new(qtn.clone(), args.clone(), assoc.clone())
        }
        // A bounded type variable searches its bound interface's closure. No bound
        // in scope ⇒ unbounded *as far as this scope knows* ⇒ deferred.
        Ty::TypeVar(name, _) => {
            match ctx
                .type_var_bounds(name)
                .and_then(|bounds| bounds.first().cloned())
            {
                Some(interface) => interface,
                None => return Determination::Deferred,
            }
        }
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
        | Ty::EnumVariant(..) => return determine_concrete(ctx, &base, member),
        // A chained projection base (`T.A.B`) needs its inner projection resolved first.
        Ty::AssociatedTypeProjection { .. } => return Determination::Deferred,
        // Already-errored bases: propagate without a fresh diagnostic.
        Ty::Error { .. } | Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } => {
            return Determination::Poisoned;
        }
        // A surviving alias means the alias map was incomplete; degrade conservatively.
        Ty::TypeAlias(..) => return Determination::Poisoned,
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
        | Ty::EvolvingMap(..) => return Determination::InvalidBase,
    };

    // An associated type lives on the interface that declares it directly — `requires` is a
    // bound, not inheritance. So if `root` declares `member` itself, that *is* the projection's
    // interface; resolve to it without walking the `requires` closure. This avoids re-entering
    // that closure when it is itself lowering a `requires I<Assoc = Self.Assoc>` binding, and
    // avoids spuriously flagging the requirer and a required interface that re-declares the
    // same-named associated type as ambiguous.
    if interface_declares_member(ctx.db(), &root.name, member) {
        return Determination::Determined(root);
    }

    let declarers = closure_declarers(ctx.db(), &root, member);
    match declarers.len() {
        0 => Determination::Undeclared {
            container: AssocContainer::Interface(root.name),
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
    bounds: &FxHashMap<Name, baml_type::Interface>,
    base: &Ty,
    member: &Name,
) -> ConcreteProjection {
    let pkg_id = PackageId::new(db, pkg.clone());
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let aliases = crate::inference::package_alias_map(db, res_ctx);
    // The scope's type-variable bounds as one-element conjunctions, so the
    // subtype oracle can discharge an impl bound on a bounded type variable in
    // `base`. Single-bound today (intersection bounds not yet surfaced).
    let bound_conjunctions: FxHashMap<Name, Vec<baml_type::Interface>> = bounds
        .iter()
        .map(|(name, bound)| (name.clone(), vec![bound.clone()]))
        .collect();
    let gctx = GlobalTypeContext {
        db,
        res_ctx,
        aliases: &aliases,
        bounds: TypeVarBounds::Interfaces(&bound_conjunctions),
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
