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
    type_context::GlobalTypeContext,
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
    // The explicit `(base as I)` qualifier is an interface *constraint* — a
    // [`baml_type::Interface`] that may pin only some associated types — never a
    // `Ty::Interface` existential (which would require all of them). Lower it to that
    // constraint here; a qualifier that isn't an interface is its own ill-formedness.
    let explicit = match explicit_interface {
        None => None,
        Some(iface_ty) => {
            let iface_ty = expand_aliases(ctx, iface_ty);
            match iface_ty.as_interface() {
                Some(interface) => Some(interface),
                // An already-errored qualifier (`(x as Nonexistent).Item`) was diagnosed
                // where it lowered — propagate without a fresh diagnostic.
                None if is_poisoned(&iface_ty) => {
                    return ProjectionLowering {
                        ty: error_ty(),
                        diagnostics,
                    };
                }
                // A resolved non-interface qualifier (`(x as SomeClass).Item`) is an error.
                None => {
                    diagnostics.push(TirTypeError::NonInterfaceProjectionQualifier);
                    return ProjectionLowering {
                        ty: error_ty(),
                        diagnostics,
                    };
                }
            }
        }
    };
    let ty = match determine_interface(ctx, &base, explicit, &member) {
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
                    interface: Box::new(interface),
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
            // BUG: dropping the generics here renders two distinct declarers of the
            // same interface (`Codec<TextFormat>` and `Codec<CodeFormat>`) as an
            // indistinguishable `(Codec, Codec)`. The full `Interface`s are in hand;
            // carrying them (not just the QTN) would let the diagnostic show the args.
            diagnostics.push(TirTypeError::AmbiguousAssociatedTypeProjection {
                member,
                candidates: candidates.into_iter().map(|iface| iface.name).collect(),
            });
            error_ty()
        }
        Determination::SubjectDoesNotImplementQualifier { subject, qualifier } => {
            diagnostics.push(TirTypeError::TypeDoesNotImplementInterface {
                value_type: subject,
                interface: qualifier.to_ty(),
            });
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

/// Whether `ty` already carries an upstream error — an error/unknown sentinel or an
/// unfilled inference hole — so a projection over it must not emit a fresh diagnostic.
fn is_poisoned(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Error { .. } | Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Infer { .. }
    )
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
    /// The explicit qualifier is an interface that declares `member`, but `base`
    /// does not implement / is not bounded by it, so `(base as I).member` is
    /// ill-formed (Rust's E0277). Carries the subject and the written qualifier.
    SubjectDoesNotImplementQualifier {
        subject: Ty,
        qualifier: baml_type::Interface,
    },
    /// `base` is a kind that cannot carry an associated-type projection.
    InvalidBase,
    /// `base` or the explicit qualifier already errored upstream.
    Poisoned,
}

fn determine_interface(
    ctx: &dyn TypeExprContext<'_>,
    base: &Ty,
    explicit: Option<baml_type::Interface>,
    member: &Name,
) -> Determination {
    // An explicit `(base as I).member` qualifier must declare `member` *directly*:
    // `requires` is a bound, not inheritance, so a required interface's member projects
    // through *that* interface, not its requirer. Base-independent, so checked before
    // dispatching on the base kind.
    if let Some(qualifier) = &explicit
        && !interface_declares_member(ctx.db(), &qualifier.name, member)
    {
        return Determination::Undeclared {
            container: AssocContainer::Interface(qualifier.name.clone()),
        };
    }

    // Dispatch on the base's *kind* — that fixes the resolution *mechanism* (a type
    // variable searches its bounds' closures, an existential its own, a concrete type its
    // impls). An explicit qualifier then narrows that search to its interface (by QTN),
    // realizing the base's pins; an unqualified projection searches every candidate by
    // `member`. Exhaustive — every kind is classified, never silently dropped.
    let base = expand_aliases(ctx, base.clone());
    match &base {
        // An interface existential is its own (single) search root.
        Ty::Interface(qtn, args, assoc, _) => {
            let root = baml_type::Interface::new(qtn.clone(), args.clone(), assoc.clone());
            let undeclared = AssocContainer::Interface(root.name.clone());
            resolve_via_roots(ctx, vec![root], explicit, member, undeclared, &base)
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
            // Declared but genuinely unbounded: it implements nothing, so an explicit
            // qualifier cannot hold and an unqualified member is undeclared.
            Some(bounds) if bounds.is_empty() => match explicit {
                Some(qualifier) => Determination::SubjectDoesNotImplementQualifier {
                    subject: base.clone(),
                    qualifier,
                },
                None => Determination::Undeclared {
                    container: AssocContainer::TypeVar(name.clone()),
                },
            },
            Some(bounds) => {
                // Report against the first bound if none declares `member`.
                let undeclared = AssocContainer::Interface(bounds[0].name.clone());
                resolve_via_roots(ctx, bounds.into_vec(), explicit, member, undeclared, &base)
            }
        },
        // Concrete receivers resolve through their own impls: an associated type
        // lives on a *separate* `impl I for C` (interfaces are bounds, not
        // inheritance), found via the visible impl set rather than any closure. An
        // explicit qualifier narrows to the impl of *that* interface (E0277 if none).
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
        | Ty::EnumVariant(..) => match explicit {
            None => determine_concrete(ctx, &base, member),
            Some(qualifier) => match ctx.concrete_realized_interface(&base, &qualifier) {
                Some(realized) => Determination::Determined(realized),
                None => Determination::SubjectDoesNotImplementQualifier {
                    subject: base.clone(),
                    qualifier,
                },
            },
        },
        // A chained projection base (`T.A.B`): the inner `T.A` already resolved to a
        // symbolic projection through the interface that declares `A`; `B` resolves
        // through the interface bound declared on `A` (`type A extends J`).
        Ty::AssociatedTypeProjection {
            base: inner_base,
            interface: inner_interface,
            member: inner_member,
            ..
        } => determine_chained(
            ctx,
            &base,
            inner_base,
            inner_interface,
            inner_member,
            explicit,
            member,
        ),
        // Already-errored bases — and the unfilled `_` inference hole, which the
        // fill machinery resolves or diagnoses — propagate without a fresh diagnostic.
        Ty::Error { .. } | Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Infer { .. } => {
            Determination::Poisoned
        }
        // A surviving alias means the alias map was incomplete; degrade conservatively.
        Ty::TypeAlias(..) => Determination::Poisoned,
        // Kinds that cannot carry an associated-type projection — with an explicit
        // qualifier that is "the subject does not implement it"; unqualified, an invalid base.
        Ty::Union(..)
        | Ty::Future(..)
        | Ty::Function { .. }
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Never { .. }
        | Ty::Void { .. }
        | Ty::Null { .. }
        | Ty::EvolvingList(..)
        | Ty::EvolvingMap(..) => match explicit {
            Some(qualifier) => Determination::SubjectDoesNotImplementQualifier {
                subject: base.clone(),
                qualifier,
            },
            None => Determination::InvalidBase,
        },
    }
}

/// Resolve `member` through `roots` — the interfaces `subject` provides (an existential
/// itself, a type variable's bounds, or a chained projection's declared bound). Unqualified,
/// search every root's `requires`-closure for the declaring interface. With an explicit
/// qualifier, narrow to that interface's QTN and realize it at the base (carrying the base's
/// pins); the subject must actually provide it, else it does not implement it (E0277).
fn resolve_via_roots(
    ctx: &dyn TypeExprContext<'_>,
    roots: Vec<baml_type::Interface>,
    explicit: Option<baml_type::Interface>,
    member: &Name,
    undeclared: AssocContainer,
    subject: &Ty,
) -> Determination {
    match explicit {
        None => resolve_through_roots(ctx.db(), roots, member, undeclared),
        Some(qualifier) => match realize_qualifier_through_roots(ctx, &roots, &qualifier) {
            Some(realized) => Determination::Determined(realized),
            None => Determination::SubjectDoesNotImplementQualifier {
                subject: subject.clone(),
                qualifier,
            },
        },
    }
}

/// The realized view of `qualifier` reachable from `roots` — walk each root's
/// `requires`-closure for the qualifier's QTN and return it at its realized generic
/// arguments and associated pins, provided the *written* qualifier constraints are
/// consistent with that realization. `None` if no root provides a compatible
/// realization. This is the narrowed counterpart of [`resolve_through_roots`]: the QTN
/// is known, so it selects that one interface instead of searching every declarer of a
/// member.
fn realize_qualifier_through_roots(
    ctx: &dyn TypeExprContext<'_>,
    roots: &[baml_type::Interface],
    qualifier: &baml_type::Interface,
) -> Option<baml_type::Interface> {
    let db = ctx.db();
    for root in roots {
        let Some(root_loc) = resolve_interface_loc(db, &root.name) else {
            continue;
        };
        for (loc, args, assoc) in crate::interfaces::interface_closure_locs_with_args_and_assoc(
            db,
            root_loc,
            &root.generics,
            &root.associated_types,
            true,
        ) {
            if let Some(qtn) = crate::interfaces::interface_loc_qtn(db, loc)
                && qtn == qualifier.name
            {
                let realized = baml_type::Interface::new(qtn, args, assoc);
                if qualifier_compatible_with_realization(ctx, qualifier, &realized) {
                    return Some(realized);
                }
                // The right interface at an incompatible realization — keep
                // scanning; another root may prove a compatible one.
            }
        }
    }
    None
}

/// Whether the *written* qualifier constraints are consistent with a realization the
/// base's bounds prove: every written generic argument and associated pin must be
/// equivalent to the realization's. A pin the realization leaves unproven rejects —
/// `T extends Entity` (whose closure realizes `HasKey<Key = string>`) does not prove
/// `(T as HasKey<Key = int>)`, exactly as Rust's `T: Entity` does not prove
/// `T: HasKey<Key = int>`. A symbolic (type-variable-carrying) position resolves only
/// at instantiation, so it fails open rather than rejecting a valid generic spelling.
fn qualifier_compatible_with_realization(
    ctx: &dyn TypeExprContext<'_>,
    qualifier: &baml_type::Interface,
    realized: &baml_type::Interface,
) -> bool {
    // A bare qualifier (`(T as Codec).Out` with no written args) accepts whatever
    // realization the bound proves; written args must correspond positionally.
    if !qualifier.generics.is_empty() {
        if qualifier.generics.len() != realized.generics.len() {
            return false;
        }
        for (written, real) in qualifier.generics.iter().zip(realized.generics.iter()) {
            if crate::generics::contains_typevar(written) || crate::generics::contains_typevar(real)
            {
                continue;
            }
            if !ctx.types_equivalent(written, real) {
                return false;
            }
        }
    }
    for (name, written) in &qualifier.associated_types {
        let Some((_, real)) = realized
            .associated_types
            .iter()
            .find(|(real_name, _)| real_name == name)
        else {
            return false;
        };
        if crate::generics::contains_typevar(written) || crate::generics::contains_typevar(real) {
            continue;
        }
        if !ctx.types_equivalent(written, real) {
            return false;
        }
    }
    true
}

/// Invariant equality under a lowering scope's package + bounds — the
/// [`TypeExprContext::types_equivalent`] implementation for [`ScopeCtx`], kept here
/// beside the other scope-driven algebra entries ([`resolve_concrete_projection`]).
///
/// [`ScopeCtx`]: crate::lower_type_expr::ScopeCtx
pub(crate) fn scope_types_equivalent(
    db: &dyn crate::Db,
    pkg: &Name,
    bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    a: &Ty,
    b: &Ty,
) -> bool {
    let pkg_id = PackageId::new(db, pkg.clone());
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let aliases = crate::inference::package_resolved_aliases(db, pkg_id);
    let gctx = GlobalTypeContext {
        db,
        res_ctx,
        aliases,
        bounds,
    };
    baml_type::normalize::equivalent(a, b, &gctx)
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
    ctx: &dyn TypeExprContext<'_>,
    projection: &Ty,
    inner_base: &Ty,
    inner_interface: &baml_type::Interface,
    inner_member: &Name,
    explicit: Option<baml_type::Interface>,
    member: &Name,
) -> Determination {
    let db = ctx.db();
    let Some(iface_loc) = resolve_interface_loc(db, &inner_interface.name) else {
        // The inner interface does not resolve — already errored upstream.
        return Determination::Poisoned;
    };
    let Some(root) =
        associated_type_bound_interface(db, iface_loc, inner_interface, inner_base, inner_member)
    else {
        // The inner projection's member has no declared interface bound, so it provides no
        // roots to resolve `member` through: unqualified it is undeclared; qualified the
        // subject cannot implement the written interface.
        return match explicit {
            Some(qualifier) => Determination::SubjectDoesNotImplementQualifier {
                subject: projection.clone(),
                qualifier,
            },
            None => Determination::Undeclared {
                container: AssocContainer::Ty(projection.clone()),
            },
        };
    };
    let container = AssocContainer::Interface(root.name.clone());
    resolve_via_roots(ctx, vec![root], explicit, member, container, projection)
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
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let assoc = iface.associated_types.iter().find(|a| &a.name == member)?;
    let bound_ref = assoc.bound?;

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
                interface: Box::new(realized.clone()),
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
        &crate::lower_type_expr::lower_type_ref(&iface.type_refs, bound_ref, &scope, &mut diags),
        &bindings,
    );
    lowered.as_interface()
}

/// The declared interface bound(s) of associated type `member` on `interface`
/// (`type member extends J`), realized through `interface`'s generic arguments
/// and sibling pins, with `Self` left symbolic. This is the tir2 implementation
/// of [`baml_type::normalize::TypeContext::associated_type_bound`]: it lights up
/// the canonical `(base as I).member <: J` subtype rule for a still-symbolic
/// projection — the projection is a subtype of its declared bound's supertypes,
/// the projection analogue of a bounded type variable.
///
/// `Self` stays symbolic (a `Ty::TypeVar("Self")` that substitutes to itself)
/// because the oracle is a function of `(interface, member)` only — the concrete
/// implementor a `Self`-referential bound would need is not available here, per
/// the trait's contract. Empty (fail-safe → opaque, never over-claims) when
/// `member` is undeclared or unbounded, its bound is not an interface, or
/// `interface`'s name resolves to no interface / with the wrong generic arity (an
/// under-instantiated qualifier, already reported as `WrongNumberOfTypeArgs`).
pub(crate) fn associated_type_declared_bound(
    db: &dyn crate::Db,
    interface: &baml_type::Interface,
    member: &Name,
) -> Vec<baml_type::Interface> {
    let Some(iface_loc) = resolve_interface_loc(db, &interface.name) else {
        return Vec::new();
    };
    // Guard the `associated_type_bound_interface` arity `debug_assert`: a bare or
    // over-applied qualifier (`(base as I).member` where `interface I<X>`) is
    // malformed — leave the projection opaque rather than realize a bound against
    // mismatched generics.
    let arity_ok = baml_compiler2_ppir::item_data::interface_data(db, iface_loc)
        .generic_params
        .len()
        == interface.generics.len();
    if !arity_ok {
        return Vec::new();
    }
    let symbolic_self = Ty::TypeVar(Name::new("Self"), TyAttr::default());
    associated_type_bound_interface(db, iface_loc, interface, &symbolic_self, member)
        .into_iter()
        .collect()
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
        true,
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

/// A concrete-headed `base`'s realized view of `interface` — the `implements` block's
/// realized interface (carrying the impl's associated-type pins) when `base` implements
/// it, else `None`. Narrows a projection search to the *written* qualifier (unlike
/// [`resolve_concrete_projection`], which searches by member).
///
/// The base/qualifier may carry *rigid* type variables (`(Map<T, R> as Iterator).Item`
/// inside a generic scope): the impl pattern is matched structurally and its generic
/// bounds are discharged against `bounds` (the scope's constraints on those rigid vars),
/// so the projection reduces to the impl's pin (`R`) exactly as at a realized
/// instantiation. A base with no matching impl — including a bare type variable, whose
/// members come from its bounds' closures, not impls — resolves to `None`.
pub(crate) fn resolve_concrete_realized_interface(
    db: &dyn crate::Db,
    pkg: &Name,
    bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    base: &Ty,
    interface: &baml_type::Interface,
) -> Option<baml_type::Interface> {
    let pkg_id = PackageId::new(db, pkg.clone());
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let aliases = crate::inference::package_resolved_aliases(db, pkg_id);
    let realized = !crate::generics::contains_typevar(base)
        && !interface
            .generics
            .iter()
            .any(crate::generics::contains_typevar)
        && !interface
            .associated_types
            .iter()
            .any(|(_, ty)| crate::generics::contains_typevar(ty));
    let resolved = if realized {
        // Realized fast path: unique by coherence, bounds discharged by bounded re-entry.
        crate::interfaces::get_implements_block(db, pkg_id, base, interface, aliases)
    } else {
        let gctx = GlobalTypeContext {
            db,
            res_ctx,
            aliases,
            bounds,
        };
        crate::interfaces::get_implements_block_symbolic(
            db,
            pkg_id,
            base,
            interface,
            aliases,
            |a, b| baml_type::normalize::is_subtype(a, b, &gctx),
        )
    };
    resolved.map(|resolved| resolved.implemented_interface(db))
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
    let aliases = crate::inference::package_resolved_aliases(db, pkg_id);
    let gctx = GlobalTypeContext {
        db,
        res_ctx,
        aliases,
        bounds,
    };

    let mut declarers: Vec<baml_type::Interface> = Vec::new();
    for resolved in crate::interfaces::impls_for_type(db, pkg_id, base, aliases, |a, b| {
        baml_type::normalize::is_subtype(a, b, &gctx)
    }) {
        let interface = resolved.implemented_interface(db);
        if interface_declares_member(db, &interface.name, member) && !declarers.contains(&interface)
        {
            declarers.push(interface);
        }
    }

    // Requires-aware root-wins: when the base implements interfaces that form a
    // `requires` chain (e.g. `Iterator requires Iterable`, both declaring `Item`),
    // the most-derived one wins — drop any declarer that another declarer
    // transitively requires. This mirrors the symbolic `resolve_through_roots`
    // root-wins rule so a concrete base and a type variable agree; two genuinely
    // *incomparable* declarers still remain and report ambiguous.
    if declarers.len() > 1 {
        // The `requires` relation is between interface *heads* (name + generic
        // args); associated types are outputs, not part of it — so compare with
        // the realized assoc pins stripped, or a bare `requires Base` (which pins
        // nothing) would never match a declarer realized as `Base<Assoc = …>`.
        let head = |i: &baml_type::Interface| {
            baml_type::Interface::new(i.name.clone(), i.generics.clone(), Vec::new())
        };
        declarers = declarers
            .iter()
            .filter(|d| {
                !declarers.iter().any(|other| {
                    other.name != d.name
                        && crate::interfaces::interface_requires(
                            db,
                            res_ctx,
                            &head(other),
                            &head(d),
                            |a, b| baml_type::normalize::equivalent(a, b, &gctx),
                        )
                })
            })
            .cloned()
            .collect();
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
    baml_compiler2_ppir::item_data::interface_data(db, loc)
        .associated_types
        .iter()
        .any(|assoc| &assoc.name == member)
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
