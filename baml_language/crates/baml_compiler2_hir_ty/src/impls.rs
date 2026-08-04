//! The impl registry (I1): `(interface, concrete type) -> the unique
//! implements block`, re-authored from TIR's `impl_rules.rs` in this
//! crate's interned vocabulary (reference, never a dependency).
//!
//! Design invariants carried over deliberately:
//! - Every impl - in-body or out-of-body - normalizes to the same FREE
//!   shape at fact-extraction time: an in-body `implements I {}` inside
//!   `class C<T>` resolves exactly as `implement<T> I for C<T>`. The
//!   syntactic origin never drives resolution.
//! - Interfaces are BOUNDS, not inheritance: the interface head must
//!   match exactly; an impl of a sub-interface never satisfies a
//!   requested super-interface (`requires` is consulted separately, by
//!   the `interface_requires` fact).
//! - Coherence guarantees at most one impl per realized pair, so
//!   resolution returns an `Option`, never a candidate set.
//! - Blanket bounds re-enter the resolver with a depth budget
//!   ([`BLANKET_IMPL_BOUND_DEPTH`]); a bound still carrying variables
//!   after substitution is vacuously satisfied (its discharge is the
//!   call site's obligation - I4 records it properly).
//! - Matching uses a deliberately FACT-POOR equality ([`AliasOnlyFacts`],
//!   TIR's `AliasEquivCtx`): aliases and enum variants only, no
//!   implements/projection/bounds facts. Termination depends on it - the
//!   matcher is a link in the `implements_interface -> resolver ->
//!   matcher` chain, and a fact-rich equality would close the loop.
//! - Inputs are gated on realizedness with a `None`/`false` return (TIR
//!   debug-asserts instead; this engine runs with live inference vars,
//!   so the gate is a contract, not an assertion).
//!
//! Deliberately NOT replicated from TIR (survey-recorded defects): the
//! single-bound conjunction asymmetry (bounds are `Vec` end to end
//! here), and `get_method`'s silent `BuiltinUnknown` fill for unbound
//! params (resolution here leaves methods to I3).

use baml_compiler2_hir::loc::ImplLoc;
use baml_compiler2_hir::package::PackageId;
use baml_type::{
    Name, ParamTy, TypeName,
    interned::{Ty, TyKind},
    normalize::{TypeContext, equivalent_interned},
};
use rustc_hash::FxHashMap;

/// Recursion budget for verifying blanket bounds: a bounded blanket can
/// itself be satisfied by another blanket, so bound-checking re-enters
/// the resolver.
const BLANKET_IMPL_BOUND_DEPTH: u32 = 16;

/// An interface reference in this crate's vocabulary: the requested or
/// implemented head, its generic args, and any associated-type pins.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceTarget {
    pub name: TypeName,
    pub args: Vec<Ty>,
    pub pins: Vec<(Name, Ty)>,
}

/// The plain-to-interned conversion at the `TypeContext` boundary.
pub(crate) fn interned_ty(ty: &baml_type::Ty) -> Ty {
    Ty::from_plain(ty)
}

impl InterfaceTarget {
    /// From the shared algebra's plain constraint (the `TypeContext`
    /// boundary).
    pub fn from_constraint(interface: &baml_type::Interface) -> InterfaceTarget {
        InterfaceTarget {
            name: interface.name.clone(),
            args: interface.generics.iter().map(Ty::from_plain).collect(),
            pins: interface
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                .collect(),
        }
    }
}

/// One impl's resolution-relevant facts, normalized to the free shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplFacts<'db> {
    pub interface: InterfaceTarget,
    pub for_ty_pattern: Ty,
    /// The impl's own generic params with their CONJUNCTIVE bounds (each
    /// bound an interface reference).
    pub generic_params: Vec<(ParamTy, Vec<InterfaceTarget>)>,
    pub associated_types: Vec<(Name, Ty)>,
    pub methods: Vec<baml_compiler2_hir::loc::FunctionLoc<'db>>,
}

// SAFETY: PartialEq-driven overwrite, the CallableThrows precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ImplFacts<'_> {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

/// The resolution-relevant facts of one impl block, or `None` when its
/// header does not resolve to an interface (the diagnostic is S17's).
#[salsa::tracked(returns(ref))]
pub fn impl_facts<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: ImplLoc<'db>,
) -> Option<ImplFacts<'db>> {
    use baml_compiler2_ppir::item_data::ImplSubjectData;
    let data = baml_compiler2_ppir::item_data::impl_block_data(db, block);
    let file = block.file(db);

    // The generic frame and for-target, normalized to the free shape.
    let (params, param_bounds, for_ty_pattern): (Vec<ParamTy>, Vec<Vec<InterfaceTarget>>, _) =
        match &data.subject {
            ImplSubjectData::InClass { class, .. } => {
                let frame = crate::lower::class_generic_frame(db, *class);
                let class_data = baml_compiler2_ppir::item_data::class_data(db, *class);
                let ctx = crate::lower::lower_ctx_for_file(db, file).with_frame(frame.clone());
                let bounds = class_data
                    .generic_param_bounds
                    .iter()
                    .map(|bound| {
                        bound
                            .and_then(|type_ref| {
                                interface_target_of(
                                    &ctx.lower_type_ref(&class_data.type_refs, type_ref),
                                )
                            })
                            .into_iter()
                            .collect()
                    })
                    .collect();
                (frame, bounds, crate::lower::class_self_ty(db, *class))
            }
            ImplSubjectData::Free {
                for_target,
                generics,
            } => {
                let frame: Vec<ParamTy> = generics
                    .iter()
                    .enumerate()
                    .map(|(index, param)| {
                        ParamTy::new(
                            u32::try_from(index).expect("impl generic index overflow"),
                            param.name.clone(),
                        )
                    })
                    .collect();
                let ctx = crate::lower::lower_ctx_for_file(db, file).with_frame(frame.clone());
                let bounds = generics
                    .iter()
                    .map(|param| {
                        param
                            .bounds
                            .iter()
                            .filter_map(|&type_ref| {
                                interface_target_of(&ctx.lower_type_ref(&data.type_refs, type_ref))
                            })
                            .collect()
                    })
                    .collect();
                let for_ty = ctx.lower_type_ref(&data.type_refs, *for_target);
                (frame, bounds, for_ty)
            }
        };

    let ctx = crate::lower::lower_ctx_for_file(db, file).with_frame(params.clone());
    let interface = interface_target_of(&ctx.lower_type_ref(&data.type_refs, data.interface_target))?;
    let associated_types = data
        .associated_type_bindings
        .iter()
        .filter_map(|binding| {
            binding
                .type_ref
                .map(|type_ref| (binding.name.clone(), ctx.lower_type_ref(&data.type_refs, type_ref)))
        })
        .collect();

    Some(ImplFacts {
        interface,
        for_ty_pattern,
        generic_params: params.into_iter().zip(param_bounds).collect(),
        associated_types,
        methods: data.methods.clone(),
    })
}

/// An interface-existential lowering read back as a target reference.
fn interface_target_of(ty: &Ty) -> Option<InterfaceTarget> {
    match ty.kind() {
        TyKind::Interface(name, args, pins, _) => Some(InterfaceTarget {
            name: name.clone(),
            args: args.to_vec(),
            pins: pins.to_vec(),
        }),
        _ => None,
    }
}

/// Every impl block a package declares, in source order (coherence
/// guarantees at most one match; stable order keeps a coherence-violating
/// program from resolving arbitrarily).
#[salsa::tracked(returns(ref))]
pub fn package_impl_locs<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package: PackageId<'db>,
) -> Vec<ImplLoc<'db>> {
    let mut out = Vec::new();
    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let file_package = baml_compiler2_hir::file_package::file_package(db, file);
        if PackageId::new(db, file_package.package) != package {
            continue;
        }
        out.extend(
            baml_compiler2_ppir::item_data::file_impls(db, file)
                .iter()
                .copied(),
        );
    }
    out
}

/// The fact-poor equality context (TIR's `AliasEquivCtx`): aliases and
/// enum variants only. Everything else answers the conservative default,
/// which is both sufficient (matching is invariant equality) and the
/// termination argument (a fact-rich context would let the matcher
/// re-enter the resolver that called it).
pub(crate) struct AliasOnlyFacts<'db> {
    facts: crate::facts::Facts<'db>,
}

impl<'db> AliasOnlyFacts<'db> {
    pub(crate) fn new(db: &'db dyn baml_compiler2_ppir::Db) -> AliasOnlyFacts<'db> {
        AliasOnlyFacts {
            facts: crate::facts::Facts::new(db),
        }
    }
}

impl TypeContext for AliasOnlyFacts<'_> {
    fn alias_def(&self, name: &TypeName) -> Option<baml_type::Ty> {
        self.facts.alias_def(name)
    }
    fn enum_variants(&self, name: &TypeName) -> Option<Vec<Name>> {
        self.facts.enum_variants(name)
    }
    fn implements_interface(&self, _: &baml_type::Ty, _: &baml_type::Interface) -> bool {
        false
    }
    fn type_var_bound(&self, _: &ParamTy) -> Vec<baml_type::Interface> {
        Vec::new()
    }
    fn interface_requires(&self, _: &baml_type::Interface, _: &baml_type::Interface) -> bool {
        false
    }
    fn associated_type_bound(&self, _: &baml_type::Interface, _: Name) -> Vec<baml_type::Interface> {
        Vec::new()
    }
    fn project(
        &self,
        _: &baml_type::Ty,
        _: &baml_type::Interface,
        _: &Name,
        _: u32,
    ) -> baml_type::normalize::ProjectionStep {
        baml_type::normalize::ProjectionStep::Opaque
    }
}

/// A concrete receiver: a value whose runtime type pins a single static
/// impl. A bare blanket `implement<T> I for T` applies ONLY to these.
fn is_concrete_receiver(ty: &Ty) -> bool {
    matches!(
        ty.kind(),
        TyKind::Class(..)
            | TyKind::Enum(..)
            | TyKind::Int { .. }
            | TyKind::Bigint { .. }
            | TyKind::Float { .. }
            | TyKind::String { .. }
            | TyKind::Bool { .. }
            | TyKind::Null { .. }
            | TyKind::Uint8Array { .. }
            | TyKind::Media(..)
            | TyKind::List(..)
            | TyKind::Map { .. }
            | TyKind::Future(..)
            | TyKind::Type { .. }
            | TyKind::Resource { .. }
            | TyKind::PromptAst { .. }
    )
}

/// One resolved impl: the block plus the generic instantiation the match
/// pinned.
pub struct ResolvedImpl<'db> {
    pub block: ImplLoc<'db>,
    pub facts: &'db ImplFacts<'db>,
    pub bindings: FxHashMap<ParamTy, Ty>,
}

impl ResolvedImpl<'_> {
    /// The interface this impl provides, realized through the match's
    /// bindings.
    pub fn implemented(&self) -> InterfaceTarget {
        InterfaceTarget {
            name: self.facts.interface.name.clone(),
            args: self
                .facts
                .interface
                .args
                .iter()
                .map(|arg| substitute_bindings(arg, &self.bindings))
                .collect(),
            pins: self
                .facts
                .interface
                .pins
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_bindings(ty, &self.bindings)))
                .collect(),
        }
    }
}

/// The realized value of associated `member` under a resolved impl: the
/// impl's own binding substituted through the match, else the interface's
/// default realized at the receiver - rustc's `leaf_def` walk (the
/// default fills only when the impl omits the binding).
pub(crate) fn resolved_pin(
    db: &dyn baml_compiler2_ppir::Db,
    resolved: &ResolvedImpl<'_>,
    self_ty: &Ty,
    member: &Name,
) -> Option<Ty> {
    if let Some((_, declared)) = resolved
        .facts
        .associated_types
        .iter()
        .find(|(name, _)| name == member)
    {
        return Some(substitute_bindings(declared, &resolved.bindings));
    }
    realized_assoc_default(db, &resolved.implemented(), self_ty, member)
}

/// The interface's declared DEFAULT for `member`, realized at a use site:
/// `Self` = `self_ty`, generic and associated slots via the shared
/// positional instantiation (a Self-referencing default like `type Items
/// = Self.Item[]` becomes a projection on `self_ty` that the canonical
/// walk re-reduces, fuel-bounded). This implements the spec's
/// fill-at-reference rule ("associated types with defaults may be omitted
/// and will use said defaults") - deliberately BROADER than rustc, where
/// a rigid projection never reduces to a trait-definition default; in
/// BAML the written reference itself fixes omitted defaulted members.
pub(crate) fn realized_assoc_default(
    db: &dyn baml_compiler2_ppir::Db,
    target: &InterfaceTarget,
    self_ty: &Ty,
    member: &Name,
) -> Option<Ty> {
    let (interface, data) = assoc_realization_env(db, target)?;
    let lowered = crate::lower::interface_assoc_default(db, interface, member.clone())
        .0
        .as_ref()?;
    let instantiation = crate::method_resolution::interface_instantiation(self_ty, target, data);
    Some(crate::lower::substitute_params(lowered, &instantiation))
}

/// The declared BOUND of `member` (`type member extends J`), realized at
/// the reference - rustc's `explicit_item_bounds` instantiated: what a
/// still-symbolic projection is provable against.
pub(crate) fn realized_assoc_bound(
    db: &dyn baml_compiler2_ppir::Db,
    target: &InterfaceTarget,
    self_ty: &Ty,
    member: &Name,
) -> Option<Ty> {
    let (interface, data) = assoc_realization_env(db, target)?;
    let lowered = crate::lower::interface_assoc_bound(db, interface, member.clone())
        .0
        .as_ref()?;
    let instantiation = crate::method_resolution::interface_instantiation(self_ty, target, data);
    Some(crate::lower::substitute_params(lowered, &instantiation))
}

/// The interface definition and its data for a realization, arity-gated
/// (a bare or mis-applied reference realizes nothing - fail-safe, the
/// diagnostic is S17's).
fn assoc_realization_env<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    target: &InterfaceTarget,
) -> Option<(
    baml_compiler2_hir::loc::InterfaceLoc<'db>,
    &'db baml_compiler2_ppir::item_data::InterfaceData<'db>,
)> {
    let facts = crate::facts::Facts::new(db);
    let Some(baml_compiler2_hir::contributions::Definition::Interface(interface)) =
        facts.definition_of(&target.name)
    else {
        return None;
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    if data.generic_params.len() != target.args.len() {
        return None;
    }
    Some((interface, data))
}

/// Whether realized `concrete` implements realized `interface`. The
/// public fact: searches the root packages derivable from every
/// qualified name on both sides (a single guessed root misses
/// orphan-legal placements like `implement dep.I for LocalEnum`).
/// Unrealized inputs answer `false` - the conservative direction; the
/// symbolic resolvers join with I4.
pub fn implements_interface(
    db: &dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceTarget,
) -> bool {
    resolve_impl(db, concrete, interface).is_some()
}

/// The unique impl by which realized `concrete` implements realized
/// `interface`, with bindings.
pub fn resolve_impl<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceTarget,
) -> Option<ResolvedImpl<'db>> {
    if !is_realized(concrete) || !interface.args.iter().all(is_realized) {
        return None;
    }
    resolve_within_depth(db, concrete, interface, BLANKET_IMPL_BOUND_DEPTH)
}

fn is_realized(ty: &Ty) -> bool {
    !ty.has_infer() && !ty.has_typevar() && !ty.has_error()
}

fn resolve_within_depth<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceTarget,
    depth: u32,
) -> Option<ResolvedImpl<'db>> {
    let eq = AliasOnlyFacts::new(db);
    for package in search_roots(db, concrete, interface) {
        for &block in package_impl_locs(db, package) {
            let Some(facts) = impl_facts(db, block) else {
                continue;
            };
            let Some(bindings) = match_impl_head(db, facts, concrete, interface, &eq) else {
                continue;
            };
            if !bounds_hold(db, facts, &bindings, depth) {
                continue;
            }
            return Some(ResolvedImpl {
                block,
                facts,
                bindings,
            });
        }
    }
    None
}

/// Every package a qualified name on either side points into - the
/// orphan rule guarantees the impl lives in one of them.
fn search_roots<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceTarget,
) -> Vec<PackageId<'db>> {
    let mut names: Vec<Name> = vec![interface.name.package().clone()];
    collect_packages(concrete, &mut names);
    for arg in &interface.args {
        collect_packages(arg, &mut names);
    }
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| PackageId::new(db, name))
        .collect()
}

fn collect_packages(ty: &Ty, out: &mut Vec<Name>) {
    match ty.kind() {
        TyKind::Class(qtn, ..)
        | TyKind::Interface(qtn, ..)
        | TyKind::Enum(qtn, _)
        | TyKind::EnumVariant(qtn, ..)
        | TyKind::TypeAlias(qtn, _) => out.push(qtn.package().clone()),
        _ => {}
    }
    // Primitives and structural types live in the stdlib package.
    if matches!(
        ty.kind(),
        TyKind::Int { .. }
            | TyKind::Bigint { .. }
            | TyKind::Float { .. }
            | TyKind::String { .. }
            | TyKind::Bool { .. }
            | TyKind::Null { .. }
            | TyKind::Uint8Array { .. }
            | TyKind::Media(..)
            | TyKind::List(..)
            | TyKind::Map { .. }
            | TyKind::Future(..)
            | TyKind::Literal(..)
    ) {
        out.push(Name::new("baml"));
    }
    let mut children = Vec::new();
    baml_type::interned::for_each_child(ty.kind(), |child| children.push(child.clone()));
    for child in children {
        collect_packages(&child, out);
    }
}

/// Structural head match: exact interface, joint unification of the
/// for-target and every interface arg against ONE shared binding set,
/// then the associated-pin gate. Declared bounds are NOT checked here.
fn match_impl_head(
    db: &dyn baml_compiler2_ppir::Db,
    facts: &ImplFacts<'_>,
    concrete: &Ty,
    interface: &InterfaceTarget,
    eq: &AliasOnlyFacts<'_>,
) -> Option<FxHashMap<ParamTy, Ty>> {
    if facts.interface.name != interface.name
        || facts.interface.args.len() != interface.args.len()
    {
        return None;
    }
    // Bare-blanket guard: `implement<T> I for T` applies only to
    // concrete receivers - never existentials, unions, or vars.
    if let TyKind::TypeVar(param, _) = facts.for_ty_pattern.kind()
        && facts.generic_params.iter().any(|(p, _)| p == param)
        && !is_concrete_receiver(concrete)
    {
        return None;
    }
    let params: Vec<ParamTy> = facts
        .generic_params
        .iter()
        .map(|(param, _)| param.clone())
        .collect();
    let mut bindings = FxHashMap::default();
    if !match_pattern(&facts.for_ty_pattern, concrete, &params, &mut bindings, eq) {
        return None;
    }
    for (pattern, target) in facts.interface.args.iter().zip(&interface.args) {
        if !match_pattern(pattern, target, &params, &mut bindings, eq) {
            return None;
        }
    }
    // Every pin the REQUEST carries must equal what this impl realizes
    // for that member: the impl's binding substituted through the match,
    // else the interface DEFAULT realized at this impl (rustc's
    // `leaf_def`). A member the interface neither binds nor defaults
    // fails closed - the request pins something this impl cannot supply.
    for (name, requested) in &interface.pins {
        let supplied = match facts
            .associated_types
            .iter()
            .find(|(declared_name, _)| declared_name == name)
        {
            Some((_, declared)) => Some(substitute_bindings(declared, &bindings)),
            None => {
                let implemented = InterfaceTarget {
                    name: facts.interface.name.clone(),
                    args: facts
                        .interface
                        .args
                        .iter()
                        .map(|arg| substitute_bindings(arg, &bindings))
                        .collect(),
                    pins: facts
                        .interface
                        .pins
                        .iter()
                        .map(|(pin_name, ty)| {
                            (pin_name.clone(), substitute_bindings(ty, &bindings))
                        })
                        .collect(),
                };
                realized_assoc_default(db, &implemented, concrete, name)
            }
        };
        match supplied {
            Some(supplied) if equivalent_interned(&supplied, requested, eq) => {}
            _ => return None,
        }
    }
    Some(bindings)
}

/// The one-directional pattern matcher (TIR's `match_ty_patterns`,
/// re-authored interned): impl params bind, ground positions compare via
/// the fact-poor equality, structural heads decompose. Includes the two
/// widenings real stdlib impls rely on: a literal target matches its
/// base-primitive pattern, and an enum-variant target matches its enum's
/// pattern.
fn match_pattern(
    pattern: &Ty,
    target: &Ty,
    params: &[ParamTy],
    bindings: &mut FxHashMap<ParamTy, Ty>,
    eq: &AliasOnlyFacts<'_>,
) -> bool {
    if let TyKind::TypeVar(param, _) = pattern.kind()
        && params.contains(param)
    {
        return match bindings.get(param) {
            Some(bound) => equivalent_interned(bound, target, eq),
            None => {
                bindings.insert(param.clone(), target.clone());
                true
            }
        };
    }
    if !pattern.has_typevar() {
        return equivalent_interned(pattern, target, eq);
    }
    // Substitute-and-compare escape: a pattern whose vars are all bound
    // already can be compared semantically (union normalization no
    // structural descent sees).
    if pattern_fully_bound(pattern, params, bindings) {
        let substituted = substitute_bindings(pattern, bindings);
        if equivalent_interned(&substituted, target, eq) {
            return true;
        }
    }
    match (pattern.kind(), target.kind()) {
        (TyKind::Class(a, a_args, _), TyKind::Class(b, b_args, _)) => {
            a == b
                && a_args.len() == b_args.len()
                && a_args
                    .iter()
                    .zip(b_args.iter())
                    .all(|(p, t)| match_pattern(p, t, params, bindings, eq))
        }
        (TyKind::Interface(a, a_args, a_pins, _), TyKind::Interface(b, b_args, b_pins, _)) => {
            a == b
                && a_args.len() == b_args.len()
                && a_args
                    .iter()
                    .zip(b_args.iter())
                    .all(|(p, t)| match_pattern(p, t, params, bindings, eq))
                // Every concrete target pin must find a pattern
                // counterpart.
                && b_pins.iter().all(|(name, target_pin)| {
                    a_pins.iter().any(|(pattern_name, pattern_pin)| {
                        pattern_name == name
                            && match_pattern(pattern_pin, target_pin, params, bindings, eq)
                    })
                })
        }
        (TyKind::List(p, _), TyKind::List(t, _)) => match_pattern(p, t, params, bindings, eq),
        (
            TyKind::Map {
                key: pk, value: pv, ..
            },
            TyKind::Map {
                key: tk, value: tv, ..
            },
        ) => {
            match_pattern(pk, tk, params, bindings, eq)
                && match_pattern(pv, tv, params, bindings, eq)
        }
        (TyKind::Future(pv, pe, _), TyKind::Future(tv, te, _)) => {
            match_pattern(pv, tv, params, bindings, eq)
                && match_pattern(pe, te, params, bindings, eq)
        }
        (TyKind::Union(p_members, _), TyKind::Union(t_members, _)) => {
            match_union_members(p_members, t_members, params, bindings, eq)
        }
        (
            TyKind::Function {
                params: pp,
                ret: pr,
                throws: pt,
                ..
            },
            TyKind::Function {
                params: tp,
                ret: tr,
                throws: tt,
                ..
            },
        ) => {
            pp.len() == tp.len()
                && pp
                    .iter()
                    .zip(tp.iter())
                    .all(|(p, t)| p.mode == t.mode && match_pattern(&p.ty, &t.ty, params, bindings, eq))
                && match_pattern(pr, tr, params, bindings, eq)
                && match_pattern(pt, tt, params, bindings, eq)
        }
        // Widenings: a literal target matches its base primitive; an
        // enum-variant target matches its enum.
        (TyKind::Int { .. }, TyKind::Literal(baml_type::Literal::Int(_), ..))
        | (TyKind::Bigint { .. }, TyKind::Literal(baml_type::Literal::Bigint(_), ..))
        | (TyKind::Float { .. }, TyKind::Literal(baml_type::Literal::Float(_), ..))
        | (TyKind::String { .. }, TyKind::Literal(baml_type::Literal::String(_), ..))
        | (TyKind::Bool { .. }, TyKind::Literal(baml_type::Literal::Bool(_), ..)) => true,
        (TyKind::Enum(p, _), TyKind::EnumVariant(t, ..)) => p == t,
        _ => false,
    }
}

/// Order-insensitive union cover: every pattern member must claim a
/// distinct target member (backtracking search; unions are small).
fn match_union_members(
    pattern_members: &[Ty],
    target_members: &[Ty],
    params: &[ParamTy],
    bindings: &mut FxHashMap<ParamTy, Ty>,
    eq: &AliasOnlyFacts<'_>,
) -> bool {
    fn search(
        patterns: &[Ty],
        targets: &[Ty],
        used: &mut Vec<bool>,
        params: &[ParamTy],
        bindings: &mut FxHashMap<ParamTy, Ty>,
        eq: &AliasOnlyFacts<'_>,
    ) -> bool {
        let Some(pattern) = patterns.first() else {
            return true;
        };
        for (index, target) in targets.iter().enumerate() {
            if used[index] {
                continue;
            }
            let saved = bindings.clone();
            if match_pattern(pattern, target, params, bindings, eq) {
                used[index] = true;
                if search(&patterns[1..], targets, used, params, bindings, eq) {
                    return true;
                }
                used[index] = false;
            }
            *bindings = saved;
        }
        false
    }
    if pattern_members.len() != target_members.len() {
        return false;
    }
    let mut used = vec![false; target_members.len()];
    search(
        pattern_members,
        target_members,
        &mut used,
        params,
        bindings,
        eq,
    )
}

fn pattern_fully_bound(
    pattern: &Ty,
    params: &[ParamTy],
    bindings: &FxHashMap<ParamTy, Ty>,
) -> bool {
    fn walk(ty: &Ty, params: &[ParamTy], bindings: &FxHashMap<ParamTy, Ty>, out: &mut bool) {
        if let TyKind::TypeVar(param, _) = ty.kind()
            && params.contains(param)
            && !bindings.contains_key(param)
        {
            *out = false;
        }
        let mut children = Vec::new();
        baml_type::interned::for_each_child(ty.kind(), |child| children.push(child.clone()));
        for child in children {
            walk(&child, params, bindings, out);
        }
    }
    let mut all_bound = true;
    walk(pattern, params, bindings, &mut all_bound);
    all_bound
}

/// Substitutes impl-param bindings into a type by PARAM IDENTITY (the
/// registry's frame is not positional at use sites, unlike signature
/// instantiation).
pub fn substitute_bindings(ty: &Ty, bindings: &FxHashMap<ParamTy, Ty>) -> Ty {
    if !ty.has_typevar() {
        return ty.clone();
    }
    if let TyKind::TypeVar(param, _) = ty.kind()
        && let Some(bound) = bindings.get(param)
    {
        return bound.clone();
    }
    Ty::intern(ty.kind().map_children(|child| substitute_bindings(child, bindings)))
}

/// Verifies a matched impl's declared bounds at the realized bindings.
/// Unbound or still-symbolic bounds are vacuous (the caller's
/// obligation); realized ones recurse with the budget.
fn bounds_hold(
    db: &dyn baml_compiler2_ppir::Db,
    facts: &ImplFacts<'_>,
    bindings: &FxHashMap<ParamTy, Ty>,
    depth: u32,
) -> bool {
    for (param, bounds) in &facts.generic_params {
        let Some(actual) = bindings.get(param) else {
            continue;
        };
        for bound in bounds {
            let bound = InterfaceTarget {
                name: bound.name.clone(),
                args: bound
                    .args
                    .iter()
                    .map(|arg| substitute_bindings(arg, bindings))
                    .collect(),
                pins: bound
                    .pins
                    .iter()
                    .map(|(name, ty)| (name.clone(), substitute_bindings(ty, bindings)))
                    .collect(),
            };
            if !is_realized(actual) || !bound.args.iter().all(is_realized) {
                continue;
            }
            if depth == 0 || resolve_within_depth(db, actual, &bound, depth - 1).is_none() {
                return false;
            }
        }
    }
    true
}

/// The realized DIRECT-plus-transitive `requires` closure of an
/// interface reference (excluding itself), fuel-bounded. A `requires`
/// target mentioning `Self` (`requires Iterable<Item = Self.Item>`)
/// lowers its pins to Error here - the params-only frame has no `Self`
/// slot; realizing those pins against the sub-interface's implementor
/// joins with I6's `Self` work.
pub fn direct_requires_closure(
    db: &dyn baml_compiler2_ppir::Db,
    root: &InterfaceTarget,
    fuel: u32,
) -> Vec<InterfaceTarget> {
    let mut out = Vec::new();
    let mut queue = vec![root.clone()];
    let mut budget = fuel;
    while let Some(current) = queue.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        for required in direct_requires(db, &current) {
            if required.name != root.name && !out.contains(&required) {
                out.push(required.clone());
                queue.push(required);
            }
        }
    }
    out
}

fn direct_requires(
    db: &dyn baml_compiler2_ppir::Db,
    of: &InterfaceTarget,
) -> Vec<InterfaceTarget> {
    let facts = crate::facts::Facts::new(db);
    let Some(baml_compiler2_hir::contributions::Definition::Interface(interface)) =
        facts.definition_of(&of.name)
    else {
        return Vec::new();
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let frame = crate::lower::interface_generic_frame_params(&data.generic_params);
    let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db)).with_frame(frame.clone());
    let bindings: FxHashMap<ParamTy, Ty> =
        frame.into_iter().zip(of.args.iter().cloned()).collect();
    data.requires
        .iter()
        .filter_map(|&required| {
            let target = interface_target_of(&ctx.lower_type_ref(&data.type_refs, required))?;
            Some(InterfaceTarget {
                name: target.name.clone(),
                args: target
                    .args
                    .iter()
                    .map(|arg| substitute_bindings(arg, &bindings))
                    .collect(),
                pins: target
                    .pins
                    .iter()
                    .map(|(name, ty)| (name.clone(), substitute_bindings(ty, &bindings)))
                    .collect(),
            })
        })
        .collect()
}

/// The transitive `requires` closure: whether interface `sub` (with its
/// realized args) requires `sup`. `Self`-mentioning requires targets
/// stay conservative until I6 (see `direct_requires_closure`).
pub fn interface_requires(
    db: &dyn baml_compiler2_ppir::Db,
    sub: &InterfaceTarget,
    sup: &InterfaceTarget,
    fuel: u32,
) -> bool {
    if fuel == 0 {
        return false;
    }
    let facts = crate::facts::Facts::new(db);
    let Some(baml_compiler2_hir::contributions::Definition::Interface(interface)) =
        facts.definition_of(&sub.name)
    else {
        return false;
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let frame = crate::lower::interface_generic_frame_params(&data.generic_params);
    let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db)).with_frame(frame.clone());
    let eq = AliasOnlyFacts::new(db);
    let bindings: FxHashMap<ParamTy, Ty> = frame.into_iter().zip(sub.args.iter().cloned()).collect();
    for &required in &data.requires {
        let Some(required) = interface_target_of(&ctx.lower_type_ref(&data.type_refs, required))
        else {
            continue;
        };
        let realized = InterfaceTarget {
            name: required.name.clone(),
            args: required
                .args
                .iter()
                .map(|arg| substitute_bindings(arg, &bindings))
                .collect(),
            pins: Vec::new(),
        };
        if realized.name == sup.name
            && realized.args.len() == sup.args.len()
            && realized
                .args
                .iter()
                .zip(&sup.args)
                .all(|(a, b)| equivalent_interned(a, b, &eq))
        {
            return true;
        }
        if interface_requires(db, &realized, sup, fuel - 1) {
            return true;
        }
    }
    false
}
