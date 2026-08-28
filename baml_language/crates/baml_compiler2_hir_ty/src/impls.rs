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
//!   (`BLANKET_IMPL_BOUND_DEPTH`); a bound still carrying variables
//!   after substitution is vacuously satisfied (its discharge is the
//!   call site's obligation - I4 records it properly).
//! - Matching uses a deliberately FACT-POOR equality (`AliasOnlyFacts`,
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
//! here). Method frames are computed at `lookup_impl_member` from the
//! match's bindings, where an unbound impl param is unreachable, never a
//! stand-in type.

use baml_compiler2_hir::{loc::ImplLoc, package::PackageId};
use baml_type::{
    Name, ParamTy, TypeName,
    interned::{InterfaceRef, Ty, TyKind},
    normalize::{TypeContext, equivalent_interned},
};
use rustc_hash::FxHashMap;

/// Recursion budget for verifying blanket bounds: a bounded blanket can
/// itself be satisfied by another blanket, so bound-checking re-enters
/// the resolver.
const BLANKET_IMPL_BOUND_DEPTH: u32 = 16;

/// The plain-to-interned conversion at the `TypeContext` boundary.
pub fn interned_ty(ty: &baml_type::Ty) -> Ty {
    Ty::from_plain(ty)
}

/// [`interned_ty`], declining input the interned family cannot
/// represent. An oracle asked about such a type answers "undecidable",
/// never panics.
pub fn try_interned_ty(ty: &baml_type::Ty) -> Option<Ty> {
    fn representable(ty: &baml_type::Ty) -> bool {
        use baml_type::Ty as P;
        match ty {
            P::List(inner, _) => representable(inner),
            P::Map { key, value, .. } => representable(key) && representable(value),
            P::Future(value, error, _) => representable(value) && representable(error),
            P::Union(members, _) => members.iter().all(representable),
            P::Class(_, args, _) => args.iter().all(representable),
            P::Interface(_, args, pins, _) => {
                args.iter().all(representable) && pins.iter().all(|(_, ty)| representable(ty))
            }
            P::AssociatedTypeProjection {
                base, interface, ..
            } => representable(base) && interface.tys().all(representable),
            P::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params.iter().all(|param| representable(&param.ty))
                    && representable(ret)
                    && representable(throws)
            }
            _ => true,
        }
    }
    representable(ty).then(|| Ty::from_plain(ty))
}

/// One impl's resolution-relevant facts, normalized to the free shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplFacts<'db> {
    pub interface: InterfaceRef,
    pub for_ty_pattern: Ty,
    /// The impl's own generic params with their CONJUNCTIVE bounds (each
    /// bound an interface reference).
    pub generic_params: Vec<(ParamTy, Vec<InterfaceRef>)>,
    pub associated_types: Vec<(Name, Ty)>,
    pub methods: Vec<baml_compiler2_hir::loc::FunctionLoc<'db>>,
}

/// A PartialEq-driven whole-value `salsa::Update` for a `'db`-carrying
/// type. Salsa's own `update_fallback` has these exact semantics but is
/// `'static`-gated, and the field-wise derive requires every field type to
/// implement `Update` — `baml_type`'s types don't (it has no salsa
/// dependency) — so compare-and-overwrite of the whole value is the
/// correct impl, written once.
macro_rules! partial_eq_salsa_update {
    ($ty:ident) => {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned;
        // `PartialEq` decides whether consumers see a change.
        #[allow(unsafe_code)]
        unsafe impl salsa::Update for $ty<'_> {
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
    };
}

partial_eq_salsa_update!(ImplFacts);

/// One impl block's header resolution — THE single decision point for
/// header validity. The resolution substrate reads it through
/// [`Self::resolved`]; the E0135 diagnostic renders [`Self::Poisoned`]'s
/// very list — so the two can never drift.
#[derive(Debug, Clone, PartialEq)]
pub enum ImplHeaderResolution<'db> {
    /// The header does not resolve to an interface (S17's diagnostic).
    Unresolved,
    /// The header resolves, but declares generic params that neither the
    /// for-target nor the interface arguments determine — listed in frame
    /// order. The facts are deliberately NOT constructed: a poisoned impl
    /// is unresolvable everywhere by construction, since no consumer has
    /// anything to match on. Associated-type pins are OUTPUTS of a match,
    /// never inputs, so they do not determine a param (Rust's E0207 rule).
    /// An in-class block can never poison — its frame is the class's, and
    /// `class_self_ty` applies every param.
    Poisoned {
        unconstrained: Vec<Name>,
    },
    Resolved(ImplFacts<'db>),
}

impl<'db> ImplHeaderResolution<'db> {
    /// The facts, when the header resolved cleanly.
    pub fn resolved(&self) -> Option<&ImplFacts<'db>> {
        match self {
            Self::Resolved(facts) => Some(facts),
            Self::Unresolved | Self::Poisoned { .. } => None,
        }
    }
}

partial_eq_salsa_update!(ImplHeaderResolution);

/// The resolution-relevant facts of one impl block, behind the header's
/// validity decision ([`ImplHeaderResolution`]).
#[salsa::tracked(returns(ref))]
pub fn impl_facts<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: ImplLoc<'db>,
) -> ImplHeaderResolution<'db> {
    use baml_compiler2_ppir::item_data::ImplSubjectData;
    let data = baml_compiler2_ppir::item_data::impl_block_data(db, block);
    let file = block.file(db);

    // The generic frame and for-target, normalized to the free shape.
    let (params, param_bounds, for_ty_pattern): (Vec<ParamTy>, Vec<Vec<InterfaceRef>>, _) =
        match &data.subject {
            ImplSubjectData::InClass { class, .. } => {
                let frame = crate::lower::class_generic_frame(db, *class);
                let class_data = baml_compiler2_ppir::item_data::class_data(db, *class);
                let ctx = crate::lower::lower_ctx_for_file(db, file).with_frame(frame.clone());
                let bounds = class_data
                    .generic_params
                    .iter()
                    .map(|declared| {
                        declared
                            .bounds
                            .iter()
                            .filter_map(|&type_ref| {
                                InterfaceRef::of_ty(&ctx.lower_type_ref_at(
                                    &class_data.type_refs,
                                    type_ref,
                                    crate::lower::TypePosition::ConstraintHead,
                                ))
                            })
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
                                InterfaceRef::of_ty(&ctx.lower_type_ref_at(
                                    &data.type_refs,
                                    type_ref,
                                    crate::lower::TypePosition::ConstraintHead,
                                ))
                            })
                            .collect()
                    })
                    .collect();
                let for_ty = ctx.lower_type_ref(&data.type_refs, *for_target);
                (frame, bounds, for_ty)
            }
        };

    // The impl's own bounds ride along: `type Output = T.Item` must
    // find `Item`'s declaring interface through `T`'s declared bound.
    let bounds_map: FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>> = params
        .iter()
        .cloned()
        .zip(param_bounds.iter())
        .map(|(param, bounds)| (param, bounds.clone()))
        .collect();
    let ctx = crate::lower::lower_ctx_for_file(db, file)
        .with_frame(params.clone())
        .with_bounds(bounds_map);
    let Some(interface) = InterfaceRef::of_ty(&ctx.lower_type_ref_at(
        &data.type_refs,
        data.interface_target,
        crate::lower::TypePosition::ConstraintHead,
    )) else {
        return ImplHeaderResolution::Unresolved;
    };
    // A declared generic param that neither the for-target nor the
    // implemented interface's arguments determine leaves a frame slot no
    // match can ever bind: the header is POISONED (see
    // [`ImplHeaderResolution::Poisoned`]) and the impl resolves nowhere.
    // E0135 renders this very list (`validate_impl_signatures`).
    let unconstrained: Vec<Name> = params
        .iter()
        .filter(|param| {
            !crate::infer::ty_mentions_param(&for_ty_pattern, param)
                && !interface
                    .generics
                    .iter()
                    .any(|arg| crate::infer::ty_mentions_param(arg, param))
        })
        .map(|param| param.name().clone())
        .collect();
    if !unconstrained.is_empty() {
        return ImplHeaderResolution::Poisoned { unconstrained };
    }
    let associated_types = data
        .associated_type_bindings
        .iter()
        .filter_map(|binding| {
            binding.type_ref.map(|type_ref| {
                (
                    binding.name.clone(),
                    ctx.lower_type_ref(&data.type_refs, type_ref),
                )
            })
        })
        .collect();

    ImplHeaderResolution::Resolved(ImplFacts {
        interface,
        for_ty_pattern,
        generic_params: params.into_iter().zip(param_bounds).collect(),
        associated_types,
        methods: data.methods.clone(),
    })
}

/// An interface-existential lowering read back as a target reference.
/// Every impl block a package declares, in source order (coherence
/// guarantees at most one match; stable order keeps a coherence-violating
/// program from resolving arbitrarily).
#[salsa::tracked(returns(ref))]
pub fn package_impl_locs<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package: PackageId<'db>,
) -> Vec<ImplLoc<'db>> {
    let mut out = Vec::new();
    // Scan only the package's own files (`package_files`), so edits to
    // another root's file set never invalidate this query.
    for file in baml_compiler2_hir::package::package_files(db, package) {
        out.extend(
            baml_compiler2_ppir::item_data::file_impls(db, *file)
                .iter()
                .copied(),
        );
    }
    out
}

/// Every source impl block whose HEAD names `interface`, across all packages
/// contributing files — the definition-site inverse of [`impls_for_type`]'s
/// per-receiver scan, for `describe`/IDE surfaces ("who implements this?").
///
/// Matching is nominal on the head's qualified name (implements is nominal):
/// every generic instantiation of `Foo` names `Foo`, so instantiations are not
/// distinguished here. Order is deterministic — packages sorted by name (via
/// `all_packages`), blocks in source order within each. Mounted and
/// precompiled packages ship no source blocks, so their impls are not listed.
#[salsa::tracked(returns(ref))]
pub fn impls_naming_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Vec<ImplLoc<'db>> {
    let name = &baml_compiler2_ppir::item_data::interface_data(db, interface).name;
    let target = crate::lower::qualify_def(
        db,
        baml_compiler2_hir::contributions::Definition::Interface(interface),
        name,
    );
    let mut out = Vec::new();
    for &package in all_packages(db) {
        for &block in package_impl_locs(db, package) {
            let Some(facts) = impl_facts(db, block).resolved() else {
                continue;
            };
            if facts.interface.name == target {
                out.push(block);
            }
        }
    }
    out
}

/// The fact-poor equality context (TIR's `AliasEquivCtx`): aliases and
/// enum variants only. Everything else answers the conservative default,
/// which is both sufficient (matching is invariant equality) and the
/// termination argument (a fact-rich context would let the matcher
/// re-enter the resolver that called it).
pub(crate) struct AliasOnlyFacts<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    memoized: Option<crate::facts::Facts<'db>>,
}

impl<'db> AliasOnlyFacts<'db> {
    /// A one-shot alias/enum context. Use [`Self::memoized`] when the same
    /// context spans a candidate scan and will see repeated type heads.
    pub(crate) fn new(db: &'db dyn baml_compiler2_ppir::Db) -> AliasOnlyFacts<'db> {
        AliasOnlyFacts { db, memoized: None }
    }

    /// A scan-local alias/enum context. A memo must not outlive one query
    /// execution - it would serve rows from a stale revision and suppress the
    /// dependency reads a later execution needs.
    fn memoized(db: &'db dyn baml_compiler2_ppir::Db) -> AliasOnlyFacts<'db> {
        AliasOnlyFacts {
            db,
            memoized: Some(crate::facts::Facts::new(db)),
        }
    }
}

impl TypeContext for AliasOnlyFacts<'_> {
    /// A name-based context represents a declaration by its own name, so this
    /// is the identity — no resolution step, and never `None`.
    fn head_lookup(&self, qtn: &TypeName) -> Option<TypeName> {
        Some(qtn.clone())
    }
    fn alias_def(&self, name: &TypeName) -> Option<baml_type::Ty> {
        self.memoized.as_ref().map_or_else(
            || crate::facts::uncached_alias_def(self.db, name),
            |facts| facts.alias_def(name),
        )
    }
    fn enum_variants(&self, name: &TypeName) -> Option<Vec<Name>> {
        self.memoized.as_ref().map_or_else(
            || crate::facts::uncached_enum_variants(self.db, name),
            |facts| facts.enum_variants(name),
        )
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
    fn associated_type_bound(
        &self,
        _: &baml_type::Interface,
        _: Name,
    ) -> Vec<baml_type::Interface> {
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
pub(crate) fn is_concrete_receiver(ty: &Ty) -> bool {
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

/// Resolution-relevant impl facts in a location-free shape shared by source
/// blocks and mounted export rows.
#[derive(Clone, PartialEq)]
pub struct MountedImplFacts {
    pub interface: InterfaceRef,
    pub for_ty_pattern: Ty,
    pub generic_params: Vec<(ParamTy, Vec<InterfaceRef>)>,
    pub associated_types: Vec<(Name, Ty)>,
}

// SAFETY: mounted/precompiled facts are fully owned interned values and
// collections. PartialEq therefore completely determines whether Salsa may
// retain the old allocation.
#[allow(unsafe_code)]
unsafe impl salsa::Update for MountedImplFacts {
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

#[derive(Clone, PartialEq)]
pub enum ResolvedImplFacts<'db> {
    Source(&'db ImplFacts<'db>),
    Mounted(MountedImplFacts),
    /// Compiler-built source-less facts are re-hydrated from a tracked row and
    /// borrowed, keeping the memoized candidate entry fact-free like Source.
    Precompiled(&'db MountedImplFacts),
}

impl ResolvedImplFacts<'_> {
    pub fn interface(&self) -> &InterfaceRef {
        match self {
            Self::Source(facts) => &facts.interface,
            Self::Mounted(facts) => &facts.interface,
            Self::Precompiled(facts) => &facts.interface,
        }
    }

    pub fn for_ty_pattern(&self) -> &Ty {
        match self {
            Self::Source(facts) => &facts.for_ty_pattern,
            Self::Mounted(facts) => &facts.for_ty_pattern,
            Self::Precompiled(facts) => &facts.for_ty_pattern,
        }
    }

    pub fn generic_params(&self) -> &[(ParamTy, Vec<InterfaceRef>)] {
        match self {
            Self::Source(facts) => &facts.generic_params,
            Self::Mounted(facts) => &facts.generic_params,
            Self::Precompiled(facts) => &facts.generic_params,
        }
    }

    pub fn associated_types(&self) -> &[(Name, Ty)] {
        match self {
            Self::Source(facts) => &facts.associated_types,
            Self::Mounted(facts) => &facts.associated_types,
            Self::Precompiled(facts) => &facts.associated_types,
        }
    }
}

/// The dispatch identity retained after matching an impl.
#[derive(Clone, PartialEq)]
pub enum ResolvedImplOrigin<'db> {
    Source {
        block: ImplLoc<'db>,
        methods: &'db [baml_compiler2_hir::loc::FunctionLoc<'db>],
    },
    Mounted {
        methods: Vec<crate::package_interface::ExportedFunction>,
    },
    /// A compiler-built immutable interface row. Methods and facts borrow from
    /// tracked artifact queries; unlike a live mount, no owned fact payload is
    /// retained in each impl-cache entry.
    Precompiled {
        package: PackageId<'db>,
        row: u32,
        methods: &'db [crate::package_interface::ExportedFunction],
    },
}

/// One resolved impl plus the generic instantiation the match pinned.
#[derive(Clone, PartialEq)]
pub struct ResolvedImpl<'db> {
    pub origin: ResolvedImplOrigin<'db>,
    pub facts: ResolvedImplFacts<'db>,
    pub bindings: FxHashMap<ParamTy, Ty>,
}

/// A method an impl provides, by where its body lives: a source block's
/// function item, or a mounted/precompiled row's exported descriptor.
pub enum ProvidedMethod<'db, 'a> {
    Source {
        block: ImplLoc<'db>,
        func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    Mounted(&'a crate::package_interface::ExportedFunction),
}

#[derive(Clone, PartialEq)]
enum CachedResolvedImplOrigin<'db> {
    /// Deliberately fact-free: source facts are Salsa-derived and must be
    /// re-hydrated by `impls_for_type` so the caller records their live query
    /// dependency rather than retaining a stale borrowed result here.
    Source { block: ImplLoc<'db> },
    /// Mounted facts are owned imported data with no Salsa query from which to
    /// re-hydrate them, so this arm retains the facts alongside the methods.
    Mounted {
        methods: Vec<crate::package_interface::ExportedFunction>,
        facts: MountedImplFacts,
    },
    /// Fact-free identity for an immutable compiler-built interface row.
    Precompiled { package: PackageId<'db>, row: u32 },
}

#[derive(Clone, PartialEq)]
struct CachedResolvedImpl<'db> {
    origin: CachedResolvedImplOrigin<'db>,
    bindings: FxHashMap<ParamTy, Ty>,
}

// SAFETY: cached rows contain no `db` borrows: only owned collections and
// Copy/interned handles. PartialEq therefore completely determines whether the
// old allocation can be retained, matching Salsa's update contract.
#[allow(unsafe_code)]
unsafe impl salsa::Update for CachedResolvedImpl<'_> {
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

impl ResolvedImpl<'_> {
    /// The interface this impl provides, realized through the match's
    /// bindings. Associated members carry only what the HEADER wrote -
    /// block-level `type X = ...` bindings and defaults resolve
    /// per-member (`resolved_pin`); [`Self::implemented_view`] is the
    /// complete spelling.
    pub fn implemented(&self) -> InterfaceRef {
        realized(self.facts.interface(), &self.bindings)
    }

    /// The COMPLETE realized view of the implemented interface for
    /// subject `self_ty`: every associated member the interface declares,
    /// resolved through the same ladder projection uses (header pin,
    /// block-level binding, realized default) - the spelling runtime
    /// dispatch keys on.
    pub fn implemented_view(&self, db: &dyn baml_compiler2_ppir::Db, self_ty: &Ty) -> InterfaceRef {
        let header = self.implemented();
        let declared: Vec<baml_type::Name> = {
            let package =
                baml_compiler2_hir::package::PackageId::new(db, header.name.package().clone());
            match baml_compiler2_ppir::package_items(db, package)
                .lookup_type(header.name.namespace(), header.name.name())
            {
                Some(baml_compiler2_hir::contributions::Definition::Interface(loc)) => {
                    baml_compiler2_ppir::item_data::interface_data(db, loc)
                        .associated_types
                        .iter()
                        .map(|assoc| assoc.name.clone())
                        .collect()
                }
                _ => match crate::package_interface::mounted_type_row(db, &header.name) {
                    Some(crate::package_interface::ExportedType::Interface {
                        associated_types,
                        ..
                    }) => associated_types
                        .iter()
                        .map(|assoc| assoc.name.clone())
                        .collect(),
                    _ => return header,
                },
            }
        };
        let associated_types = declared
            .into_iter()
            .filter_map(|member| resolved_pin(db, self, self_ty, &member).map(|pin| (member, pin)))
            .collect();
        InterfaceRef::new(header.name.clone(), header.generics, associated_types)
    }
}

impl<'db> ResolvedImpl<'db> {
    /// What this impl PROVIDES for method `name` — the function that runs
    /// when the impl matches. `None` means the block leaves the method to
    /// the interface's default body (Rust-trait semantics: an impl provides
    /// a method or adopts the default; there is no override relation).
    pub fn provided_method(
        &self,
        db: &'db dyn baml_compiler2_ppir::Db,
        name: &Name,
    ) -> Option<ProvidedMethod<'db, '_>> {
        match &self.origin {
            ResolvedImplOrigin::Source { block, methods } => methods
                .iter()
                .copied()
                .find(|&method| {
                    baml_compiler2_ppir::item_data::function_data(db, method).name == *name
                })
                .map(|func| ProvidedMethod::Source {
                    block: *block,
                    func,
                }),
            ResolvedImplOrigin::Mounted { methods } => methods
                .iter()
                .find(|method| method.name == *name)
                .map(ProvidedMethod::Mounted),
            ResolvedImplOrigin::Precompiled { methods, .. } => methods
                .iter()
                .find(|method| method.name == *name)
                .map(ProvidedMethod::Mounted),
        }
    }

    pub fn source_block(&self) -> Option<ImplLoc<'db>> {
        match self.origin {
            ResolvedImplOrigin::Source { block, .. } => Some(block),
            ResolvedImplOrigin::Mounted { .. } | ResolvedImplOrigin::Precompiled { .. } => None,
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
        .associated_types()
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
    target: &InterfaceRef,
    self_ty: &Ty,
    member: &Name,
) -> Option<Ty> {
    if let Some(crate::package_interface::ExportedType::Interface {
        generic_params,
        associated_types,
        ..
    }) = crate::package_interface::mounted_type_row(db, &target.name)
    {
        let default = associated_types
            .iter()
            .find(|assoc| &assoc.name == member)?
            .default
            .as_ref()?;
        let instantiation = mounted_interface_instantiation(target, self_ty, generic_params)?;
        return Some(crate::lower::substitute_params(
            &Ty::from_plain(default),
            &instantiation,
        ));
    }
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
    target: &InterfaceRef,
    self_ty: &Ty,
    member: &Name,
) -> Option<Ty> {
    if let Some(crate::package_interface::ExportedType::Interface {
        generic_params,
        associated_types,
        ..
    }) = crate::package_interface::mounted_type_row(db, &target.name)
    {
        let bound = associated_types
            .iter()
            .find(|assoc| &assoc.name == member)?
            .bound
            .as_ref()?;
        let instantiation = mounted_interface_instantiation(target, self_ty, generic_params)?;
        let bound_ty = Ty::intern(TyKind::Interface(
            bound.name.clone(),
            bound.generics.iter().map(Ty::from_plain).collect(),
            bound
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                .collect(),
            baml_type::TyAttr::default(),
        ));
        return Some(crate::lower::substitute_params(&bound_ty, &instantiation));
    }
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
    target: &InterfaceRef,
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
    if data.generic_params.len() != target.generics.len() {
        return None;
    }
    Some((interface, data))
}

/// The mounted twin of `method_resolution::interface_instantiation`:
/// `[Self, args..]` - associated types are projection-only, reduced by the
/// oracle at use rather than substituted as slots.
pub(crate) fn mounted_interface_instantiation(
    target: &InterfaceRef,
    self_ty: &Ty,
    generic_params: &[ParamTy],
) -> Option<Vec<Ty>> {
    if generic_params.len() != target.generics.len() {
        return None;
    }
    let mut out = vec![self_ty.clone()];
    out.extend(target.generics.iter().cloned());
    Some(out)
}

/// Every impl an admissible `concrete` type matches, across every package
/// in the compilation (enumeration has no interface side to derive
/// search roots from, and coherence guarantees the set is
/// overlap-free; per-package visibility gating is I7/S17's). The
/// candidate source for concrete-receiver interface members - the
/// rust-analyzer trait-impl candidate tier of method resolution.
/// Candidates whose implemented interface still carries impl variables
/// after the for-target match (`implement<O> Add<O> for int` - `O` is
/// not determined by the receiver) are SKIPPED: surfacing them needs
/// call-site variable introduction (the probe machinery), and leaking
/// raw impl params into a body's inference would collide with its
/// frame. Fail-safe, not fail-wrong.
pub fn impls_for_type<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
) -> Vec<ResolvedImpl<'db>> {
    // The same admission as `resolve_impl`: RIGID vars (skolems) are
    // legal in goals - `self: RbeBox<T>` inside the impl's own body
    // matches the impl with the placeholder as an opaque constant.
    // Bounds the match places on rigid bindings are the CALLER's to
    // discharge against its param env (`lookup_impl_member` does).
    if concrete.has_infer() || concrete.has_error() {
        return Vec::new();
    }
    impls_for_type_cached(db, ImplTypeKey::new(db, concrete.clone()))
        .iter()
        .map(|cached| match &cached.origin {
            CachedResolvedImplOrigin::Source { block } => {
                let facts = impl_facts(db, *block)
                    .resolved()
                    .expect("cached source impl remains well formed");
                ResolvedImpl {
                    origin: ResolvedImplOrigin::Source {
                        block: *block,
                        methods: &facts.methods,
                    },
                    facts: ResolvedImplFacts::Source(facts),
                    bindings: cached.bindings.clone(),
                }
            }
            CachedResolvedImplOrigin::Mounted { methods, facts } => ResolvedImpl {
                origin: ResolvedImplOrigin::Mounted {
                    methods: methods.clone(),
                },
                facts: ResolvedImplFacts::Mounted(facts.clone()),
                bindings: cached.bindings.clone(),
            },
            CachedResolvedImplOrigin::Precompiled { package, row } => {
                let row_index = usize::try_from(*row).expect("precompiled impl row fits usize");
                let interface = crate::package_interface::mounted_interface(db, &package.name(db))
                    .expect("cached precompiled package remains installed");
                let exported = interface
                    .impls
                    .get(row_index)
                    .expect("cached precompiled impl row remains present");
                let facts = precompiled_impl_facts(db, *package, *row)
                    .as_ref()
                    .expect("cached precompiled impl facts remain present");
                ResolvedImpl {
                    origin: ResolvedImplOrigin::Precompiled {
                        package: *package,
                        row: *row,
                        methods: &exported.methods,
                    },
                    facts: ResolvedImplFacts::Precompiled(facts),
                    bindings: cached.bindings.clone(),
                }
            }
        })
        .filter(|resolved| {
            let implemented = resolved.implemented();
            // `AnyClass` is an explicit narrowing surface, not another
            // concrete-member provider. Keep its blanket witness out of the
            // concrete receiver lookup tier so established fields and methods
            // named `get`, `name`, `type`, and so on retain their resolution.
            // Explicit `reflect.AnyClass` receivers dispatch through
            // `resolve_impl`, where the witness remains available.
            provides_concrete_members(&implemented.name)
        })
        .collect()
}

/// Whether an implemented interface contributes members to an unqualified
/// concrete-receiver lookup. `AnyClass` is reachable only after explicit
/// narrowing, so its blanket default methods must stay out of both the ground
/// registry and the inference-variable method probe.
pub(crate) fn provides_concrete_members(interface: &TypeName) -> bool {
    !interface.is_reflect_root_type("AnyClass")
}

/// Compiler-derived interfaces may deliberately narrow a blanket stdlib impl.
/// Keep impl enumeration and direct goal resolution on the same membership
/// surface as normalization; otherwise their default methods leak onto values
/// the derived interface excludes (for example, `map.get` resolving to
/// `AnyClass.get`).
fn derived_impl_allows(
    db: &dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &TypeName,
) -> bool {
    if !interface.is_reflect_root_type("AnyClass") {
        return true;
    }
    let normalized = baml_type::normalize::normalize_interned(concrete, &AliasOnlyFacts::new(db));
    matches!(normalized.kind(), TyKind::Class(..))
}

#[salsa::interned]
struct ImplTypeKey<'db> {
    #[returns(ref)]
    concrete: Ty,
}

/// A recursive obligation can re-enter candidate assembly through
/// `bounds_hold`; inductive impl cycles contribute no candidates.
fn impls_for_type_cycle_result<'db>(
    _db: &'db dyn baml_compiler2_ppir::Db,
    _id: salsa::Id,
    _type_key: ImplTypeKey<'db>,
) -> Vec<CachedResolvedImpl<'db>> {
    Vec::new()
}

/// Memoized ground candidate assembly. Concrete primitive/container types recur
/// throughout one project (especially through operator and interface lookup),
/// while their impl set is a pure Salsa-dependent function of the type and
/// package inputs. Cache that scan once instead of re-walking every impl block
/// for every expression that mentions the same receiver type.
#[salsa::tracked(returns(ref), cycle_result = impls_for_type_cycle_result)]
fn impls_for_type_cached<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    type_key: ImplTypeKey<'db>,
) -> Vec<CachedResolvedImpl<'db>> {
    let concrete = type_key.concrete(db);
    let eq = AliasOnlyFacts::memoized(db);
    let mut out = Vec::new();
    for &package in all_packages(db) {
        // Do not short-circuit this iterator: `impl_facts` dependencies are
        // registered lazily as source rows are visited. This memoized query
        // must exhaust every package so later fact changes invalidate it.
        for (origin, facts) in package_impl_candidates(db, package) {
            let pattern = facts.for_ty_pattern();
            let pattern_has_typevar = pattern.has_typevar();
            if !pattern_has_typevar && !equivalent_interned(pattern, concrete, &eq) {
                continue;
            }
            let params: Vec<ParamTy> = facts
                .generic_params()
                .iter()
                .map(|(param, _)| param.clone())
                .collect();
            // Bare-blanket guard, as in `match_impl_head`.
            if let TyKind::TypeVar(param, _) = facts.for_ty_pattern().kind()
                && params.contains(param)
                && !is_concrete_receiver(concrete)
            {
                continue;
            }
            let mut bindings = FxHashMap::default();
            if pattern_has_typevar && !match_pattern(pattern, concrete, &params, &mut bindings, &eq)
            {
                continue;
            }
            if !bounds_hold(
                db,
                &facts,
                &bindings,
                BLANKET_IMPL_BOUND_DEPTH,
                &mut Vec::new(),
            ) {
                continue;
            }
            // The skip the doc comment describes: an implemented
            // interface still mentioning UNBOUND impl params is
            // undetermined by the receiver. Checked on the PATTERN side
            // (pre-substitution), so receiver-supplied rigid vars -
            // which are the body's own frame, legal in its inference -
            // never trip it even when their `ParamTy` identity shadows
            // an impl param's.
            let undetermined = |ty: &Ty| !pattern_fully_bound(ty, &params, &bindings);
            if facts.interface().generics.iter().any(&undetermined)
                || facts
                    .interface()
                    .associated_types
                    .iter()
                    .any(|(_, ty)| undetermined(ty))
            {
                continue;
            }
            // Every accepted candidate binds its FULL declared frame:
            // `impl_facts` poisons a phantom-param header at resolution
            // (E0135's impls never produce facts), and the guard above
            // skipped candidates whose params only the interface goal could
            // bind. Pinned because `realized_impl_frame` realizes the frame
            // with `unreachable!` on an absent binding.
            debug_assert!(
                params.iter().all(|param| bindings.contains_key(param)),
                "accepted impl candidate left a declared generic unbound"
            );
            let origin = match (origin, facts) {
                (ResolvedImplOrigin::Source { block, .. }, ResolvedImplFacts::Source(_)) => {
                    CachedResolvedImplOrigin::Source { block }
                }
                (ResolvedImplOrigin::Mounted { methods }, ResolvedImplFacts::Mounted(facts)) => {
                    CachedResolvedImplOrigin::Mounted { methods, facts }
                }
                (
                    ResolvedImplOrigin::Precompiled { package, row, .. },
                    ResolvedImplFacts::Precompiled(_),
                ) => CachedResolvedImplOrigin::Precompiled { package, row },
                _ => unreachable!("impl candidate origin and facts have the same provenance"),
            };
            out.push(CachedResolvedImpl { origin, bindings });
        }
    }
    out
}

/// Every package contributing files to the compilation, deduplicated.
///
/// Reads the source-root table (every root carries exactly one package) plus
/// the external (mounted/precompiled) package names — never the files
/// themselves, so adding or removing a file cannot invalidate the package set.
#[salsa::tracked(returns(ref))]
fn all_packages(db: &dyn baml_compiler2_ppir::Db) -> Vec<PackageId<'_>> {
    let mut names: Vec<Name> = db
        .source_roots()
        .roots(db)
        .iter()
        .map(|root| root.package(db))
        .collect();
    names.extend(baml_compiler2_hir::package::external_package_names(db));
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| PackageId::new(db, name))
        .collect()
}

fn package_impl_candidates<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package: PackageId<'db>,
) -> impl Iterator<Item = (ResolvedImplOrigin<'db>, ResolvedImplFacts<'db>)> + 'db {
    let source = package_impl_locs(db, package)
        .iter()
        .filter_map(move |&block| {
            let facts = impl_facts(db, block).resolved()?;
            Some((
                ResolvedImplOrigin::Source {
                    block,
                    methods: &facts.methods,
                },
                ResolvedImplFacts::Source(facts),
            ))
        });
    let precompiled = baml_compiler2_hir::package::is_precompiled_package(db, &package.name(db));
    let immutable = precompiled
        .then(|| crate::package_interface::mounted_interface(db, &package.name(db)))
        .into_iter()
        .flatten()
        .flat_map(move |interface| {
            interface
                .impls
                .iter()
                .enumerate()
                .filter_map(move |(index, row)| {
                    let row_index = u32::try_from(index).ok()?;
                    let facts = precompiled_impl_facts(db, package, row_index).as_ref()?;
                    Some((
                        ResolvedImplOrigin::Precompiled {
                            package,
                            row: row_index,
                            methods: &row.methods,
                        },
                        ResolvedImplFacts::Precompiled(facts),
                    ))
                })
        });
    let mounted = (!precompiled)
        .then(|| crate::package_interface::mounted_interface(db, &package.name(db)))
        .into_iter()
        .flatten()
        .flat_map(move |interface| {
            interface.impls.iter().map(|row| {
                (
                    ResolvedImplOrigin::Mounted {
                        methods: row.methods.clone(),
                    },
                    ResolvedImplFacts::Mounted(exported_impl_facts(row)),
                )
            })
        });
    source.chain(immutable).chain(mounted)
}

fn exported_impl_facts(row: &crate::package_interface::ExportedImpl) -> MountedImplFacts {
    let generic_params = row
        .generic_params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let bounds = row
                .param_bounds
                .get(index)
                .into_iter()
                .flatten()
                .map(InterfaceRef::from_constraint)
                .collect();
            (param.clone(), bounds)
        })
        .collect();
    MountedImplFacts {
        interface: InterfaceRef::from_constraint(&row.interface),
        for_ty_pattern: Ty::from_plain(&row.for_ty_pattern),
        generic_params,
        associated_types: row
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
            .collect(),
    }
}

/// Rehydrate an immutable compiler-built impl row through a tracked query.
/// Cache entries retain only `(package, row)`; all callers borrow this shared
/// fact value and record the live package-interface dependency.
#[salsa::tracked(returns(ref))]
fn precompiled_impl_facts<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package: PackageId<'db>,
    row: u32,
) -> Option<MountedImplFacts> {
    if !baml_compiler2_hir::package::is_precompiled_package(db, &package.name(db)) {
        return None;
    }
    let interface = crate::package_interface::mounted_interface(db, &package.name(db))?;
    let row = interface.impls.get(usize::try_from(row).ok()?)?;
    Some(exported_impl_facts(row))
}

/// Every impl block implementing `interface_name`, drawn from the
/// packages the goal's qualified names point into plus the interface's
/// own package - SELECTION's candidate set (rustc's candidate assembly
/// for a goal that may still carry inference variables). The orphan
/// rule makes these roots complete, exactly as in `search_roots`.
pub(crate) fn impl_candidates<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    goal: &Ty,
    interface_name: &TypeName,
) -> Vec<&'db ImplFacts<'db>> {
    let mut names: Vec<Name> = vec![interface_name.package().clone()];
    collect_packages(goal, &mut names);
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for name in names {
        let package = PackageId::new(db, name);
        for &block in package_impl_locs(db, package) {
            if let Some(facts) = impl_facts(db, block).resolved()
                && facts.interface.name == *interface_name
            {
                out.push(facts);
            }
        }
    }
    out
}

/// Every impl block in the project, for the method PROBE's candidate
/// assembly: the receiver's interface is unknown there, so no name
/// filter applies - all packages, the same walk the ground registry
/// (`impls_for_type`) does.
pub(crate) fn all_impl_facts(db: &dyn baml_compiler2_ppir::Db) -> Vec<&ImplFacts<'_>> {
    let mut out = Vec::new();
    for &package in all_packages(db) {
        for &block in package_impl_locs(db, package) {
            if let Some(facts) = impl_facts(db, block).resolved() {
                out.push(facts);
            }
        }
    }
    out
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
    interface: &InterfaceRef,
) -> bool {
    resolve_impl(db, concrete, interface).is_some()
}

/// The unique impl by which realized `concrete` implements realized
/// `interface`, with bindings.
pub fn resolve_impl<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceRef,
) -> Option<ResolvedImpl<'db>> {
    // RIGID vars (skolems) are legal in goals: rustc proves
    // `Arr<T>: Iter<T>` inside a generic body by matching impls with
    // the placeholder as an opaque constant, and `match_pattern` binds
    // only impl-frame params - a rigid var unifies with itself or an
    // impl var, never with ground structure. Only unresolved inference
    // vars and error sentinels stay out.
    let admissible = |ty: &Ty| !ty.has_infer() && !ty.has_error();
    if !admissible(concrete) || !interface.generics.iter().all(admissible) {
        return None;
    }
    // A literal-typed value implements what its base primitive does
    // (the receiver-class rule applied to impl goals): `1` proves
    // `GrptChild<int>` through `implements GrptChild<int> for int`.
    if let TyKind::Literal(literal, _, attr) = concrete.kind() {
        let widened = Ty::intern(crate::infer::literal_base(literal, attr.clone()));
        return resolve_impl(db, &widened, interface);
    }
    resolve_within_depth(
        db,
        concrete,
        interface,
        BLANKET_IMPL_BOUND_DEPTH,
        &mut Vec::new(),
    )
}

fn is_realized(ty: &Ty) -> bool {
    !ty.has_infer() && !ty.has_typevar() && !ty.has_error()
}

fn resolve_within_depth<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceRef,
    depth: u32,
    in_progress: &mut Vec<(Ty, InterfaceRef)>,
) -> Option<ResolvedImpl<'db>> {
    // CYCLE DETECTION (rustc's obligation-stack shape): a goal already
    // being resolved on this path has no finite proof - inductive
    // failure, immediately. The depth budget stays as the backstop for
    // GROWING chains (`Wrap<Wrap<...>>` bounds that never repeat a
    // goal), exactly the split rustc makes between cycle detection and
    // `recursion_limit`.
    if in_progress
        .iter()
        .any(|(ty, target)| ty == concrete && target == interface)
    {
        return None;
    }
    // Every selection path, including nested blanket-bound discharge, must
    // apply compiler-derived membership before consulting the blanket rule.
    if !derived_impl_allows(db, concrete, &interface.name) {
        return None;
    }
    in_progress.push((concrete.clone(), interface.clone()));
    let eq = AliasOnlyFacts::memoized(db);
    let mut resolved = None;
    'search: for package in search_roots(db, concrete, interface) {
        for (origin, facts) in package_impl_candidates(db, package) {
            let Some(bindings) = match_impl_head(db, &facts, concrete, interface, &eq) else {
                continue;
            };
            if !bounds_hold(db, &facts, &bindings, depth, in_progress) {
                continue;
            }
            resolved = Some(ResolvedImpl {
                origin,
                facts,
                bindings,
            });
            break 'search;
        }
    }
    in_progress.pop();
    resolved
}

/// Every package a qualified name on either side points into - the
/// orphan rule guarantees the impl lives in one of them.
fn search_roots<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &InterfaceRef,
) -> Vec<PackageId<'db>> {
    let mut names: Vec<Name> = vec![interface.name.package().clone()];
    collect_packages(concrete, &mut names);
    for arg in &interface.generics {
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
    facts: &ResolvedImplFacts<'_>,
    concrete: &Ty,
    interface: &InterfaceRef,
    eq: &AliasOnlyFacts<'_>,
) -> Option<FxHashMap<ParamTy, Ty>> {
    if facts.interface().name != interface.name
        || facts.interface().generics.len() != interface.generics.len()
    {
        return None;
    }
    // Bare-blanket guard: `implement<T> I for T` applies only to
    // concrete receivers - never existentials, unions, or vars.
    if let TyKind::TypeVar(param, _) = facts.for_ty_pattern().kind()
        && facts.generic_params().iter().any(|(p, _)| p == param)
        && !is_concrete_receiver(concrete)
    {
        return None;
    }
    let params: Vec<ParamTy> = facts
        .generic_params()
        .iter()
        .map(|(param, _)| param.clone())
        .collect();
    let mut bindings = FxHashMap::default();
    if !match_pattern(facts.for_ty_pattern(), concrete, &params, &mut bindings, eq) {
        return None;
    }
    for (pattern, target) in facts.interface().generics.iter().zip(&interface.generics) {
        if !match_pattern(pattern, target, &params, &mut bindings, eq) {
            return None;
        }
    }
    // Every pin the REQUEST carries must equal what this impl realizes
    // for that member: the impl's binding substituted through the match,
    // else the interface DEFAULT realized at this impl (rustc's
    // `leaf_def`). A member the interface neither binds nor defaults
    // fails closed - the request pins something this impl cannot supply.
    for (name, requested) in &interface.associated_types {
        let supplied = match facts
            .associated_types()
            .iter()
            .find(|(declared_name, _)| declared_name == name)
        {
            Some((_, declared)) => Some(substitute_bindings(declared, &bindings)),
            None => {
                let implemented = realized(facts.interface(), &bindings);
                realized_assoc_default(db, &implemented, concrete, name)
            }
        };
        match supplied {
            Some(supplied) => {
                // Normalize-then-compare (the confirmation road's
                // discipline): the impl's binding may be a projection
                // over the now-bound params (`type Output = T.Item`)
                // that only meets `Output = string` by VALUE.
                let supplied = reduce_ground_projections(db, &supplied, 8);
                let requested = reduce_ground_projections(db, requested, 8);
                if !equivalent_interned(&supplied, &requested, eq) {
                    return None;
                }
            }
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
/// Fuel-bounded projection reduction over GROUND types for the registry
/// matcher - rustc's associated-type normalization during winnowing,
/// recursion-limited: a requested pin `Output = string` must meet an
/// impl's `type Output = T.Item` by VALUE once the match binds `T`, and
/// the alias-only equivalence cannot project. Var-carrying input returns
/// unchanged (the oracle's plain conversion erases inference vars).
pub(crate) fn reduce_ground_projections(
    db: &dyn baml_compiler2_ppir::Db,
    ty: &Ty,
    fuel: u32,
) -> Ty {
    if fuel == 0 || !ty.has_projection() || ty.has_infer() {
        return ty.clone();
    }
    let rebuilt = Ty::intern(
        ty.kind()
            .map_children(|child| reduce_ground_projections(db, child, fuel)),
    );
    if let TyKind::AssociatedTypeProjection {
        base,
        interface,
        member,
        ..
    } = rebuilt.kind()
    {
        let facts = crate::facts::Facts::new(db);
        let plain_base = base.to_plain();
        let plain_interface = baml_type::Interface::new(
            interface.name.clone(),
            interface.generics.iter().map(Ty::to_plain).collect(),
            interface
                .associated_types
                .iter()
                .map(|(name, pin)| (name.clone(), pin.to_plain()))
                .collect(),
        );
        if let baml_type::normalize::ProjectionStep::Reduced(step) =
            baml_type::normalize::TypeContext::project(
                &facts,
                &plain_base,
                &plain_interface,
                member,
                fuel,
            )
        {
            return reduce_ground_projections(db, &Ty::from_plain(&step), fuel - 1);
        }
    }
    rebuilt
}

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
                && pp.iter().zip(tp.iter()).all(|(p, t)| {
                    p.mode == t.mode && match_pattern(&p.ty, &t.ty, params, bindings, eq)
                })
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

/// An interface reference REALIZED through impl-param bindings: name
/// kept, generics and pins substituted - the one spelling of the
/// five hand-copied blocks this replaces.
pub(crate) fn realized(
    reference: &InterfaceRef,
    bindings: &FxHashMap<ParamTy, Ty>,
) -> InterfaceRef {
    InterfaceRef::new(
        reference.name.clone(),
        reference
            .generics
            .iter()
            .map(|arg| substitute_bindings(arg, bindings))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        reference
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), substitute_bindings(ty, bindings)))
            .collect(),
    )
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
    Ty::intern(
        ty.kind()
            .map_children(|child| substitute_bindings(child, bindings)),
    )
}

/// Verifies a matched impl's declared bounds at the realized bindings.
/// Unbound or still-symbolic bounds are vacuous (the caller's
/// obligation); realized ones recurse with the budget.
fn bounds_hold(
    db: &dyn baml_compiler2_ppir::Db,
    facts: &ResolvedImplFacts<'_>,
    bindings: &FxHashMap<ParamTy, Ty>,
    depth: u32,
    in_progress: &mut Vec<(Ty, InterfaceRef)>,
) -> bool {
    for (param, bounds) in facts.generic_params() {
        let Some(actual) = bindings.get(param) else {
            continue;
        };
        for bound in bounds {
            let bound = realized(bound, bindings);
            if !is_realized(actual) || !bound.generics.iter().all(is_realized) {
                continue;
            }
            if depth == 0
                || resolve_within_depth(db, actual, &bound, depth - 1, in_progress).is_none()
            {
                return false;
            }
        }
    }
    true
}

/// The root PLUS its transitive `requires` closure - the candidate
/// head set every consumer walks. The closure itself excludes the
/// root (the language's requires-cycle-by-name rule), so each caller
/// re-prepended it; spelled once here.
pub(crate) fn requires_heads(
    db: &dyn baml_compiler2_ppir::Db,
    root: &InterfaceRef,
    self_ty: &Ty,
    fuel: u32,
) -> Vec<InterfaceRef> {
    let mut heads = vec![root.clone()];
    heads.extend(direct_requires_closure(db, root, self_ty, fuel));
    heads
}

/// Whether a carried or closure HEAD answers for `want`: same
/// interface, same arity, args equivalent under the fact-poor alias
/// oracle. Pins are outputs, not part of the relation
/// (`interface_requires`' rule) - callers with pin obligations layer
/// them separately. The ONE head-match relation; the drift between
/// `==`, `equivalent_interned`, and unification per site is what this
/// replaces (unification stays only in obligation CONFIRMATION, which
/// commits variables).
pub(crate) fn head_matches(
    have: &InterfaceRef,
    want: &InterfaceRef,
    eq: &AliasOnlyFacts<'_>,
) -> bool {
    use baml_type::normalize::equivalent_interned;
    have.name == want.name
        && have.generics.len() == want.generics.len()
        && have
            .generics
            .iter()
            .zip(&want.generics)
            .all(|(a, b)| equivalent_interned(a, b, eq))
}

/// The realized DIRECT-plus-transitive `requires` closure of an
/// interface reference (excluding itself), fuel-bounded. `self_ty` is
/// the SUBJECT the closure is elaborated for (the rigid var, the
/// existential, the concrete receiver) - rustc's elaboration
/// instantiates super-predicates with the subject's `Self`, which is
/// what realizes a `Self`-mentioning target (`requires Iterable<Item =
/// Self.Item>` becomes `Iterable<Item = (subject as I).Item>`, and the
/// projection reduces through the oracle wherever the facts determine
/// it). One subject threads the whole closure: whoever implements the
/// root implements every required interface (the requires contract).
pub fn direct_requires_closure(
    db: &dyn baml_compiler2_ppir::Db,
    root: &InterfaceRef,
    self_ty: &Ty,
    fuel: u32,
) -> Vec<InterfaceRef> {
    let mut out = Vec::new();
    let mut queue = vec![root.clone()];
    let mut budget = fuel;
    while let Some(current) = queue.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        for required in direct_requires(db, &current, self_ty) {
            if required.name != root.name && !out.contains(&required) {
                out.push(required.clone());
                queue.push(required);
            }
        }
    }
    out
}

/// The realized direct `requires` targets of `of`, for subject
/// `self_ty`: lowered in the full interface frame (so `Self` and
/// sibling associated names resolve), realized by the shared positional
/// instantiation.
fn direct_requires(
    db: &dyn baml_compiler2_ppir::Db,
    of: &InterfaceRef,
    self_ty: &Ty,
) -> Vec<InterfaceRef> {
    if let Some(crate::package_interface::ExportedType::Interface {
        generic_params,
        requires,
        ..
    }) = crate::package_interface::mounted_type_row(db, &of.name)
    {
        let Some(instantiation) = mounted_interface_instantiation(of, self_ty, generic_params)
        else {
            return Vec::new();
        };
        return requires
            .iter()
            .map(InterfaceRef::from_constraint)
            .map(|required| {
                InterfaceRef::new(
                    required.name.clone(),
                    required
                        .generics
                        .iter()
                        .map(|arg| crate::lower::substitute_params(arg, &instantiation))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    required
                        .associated_types
                        .iter()
                        .map(|(name, ty)| {
                            (
                                name.clone(),
                                crate::lower::substitute_params(ty, &instantiation),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
    }
    let Some((interface, data)) = assoc_realization_env(db, of) else {
        return Vec::new();
    };
    let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db))
        .with_frame(crate::lower::interface_frame(db, interface))
        .with_bounds(crate::lower::interface_scope_bounds(db, interface));
    let instantiation = crate::method_resolution::interface_instantiation(self_ty, of, data);
    data.requires
        .iter()
        .filter_map(|&required| {
            let target = InterfaceRef::of_ty(&ctx.lower_type_ref_at(
                &data.type_refs,
                required,
                crate::lower::TypePosition::ConstraintHead,
            ))?;
            Some(InterfaceRef::new(
                target.name.clone(),
                target
                    .generics
                    .iter()
                    .map(|arg| crate::lower::substitute_params(arg, &instantiation))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                target
                    .associated_types
                    .iter()
                    .map(|(name, ty)| {
                        (
                            name.clone(),
                            crate::lower::substitute_params(ty, &instantiation),
                        )
                    })
                    .collect(),
            ))
        })
        .collect()
}

/// The transitive `requires` closure: whether interface `sub` (with its
/// realized args) requires `sup`, for subject `self_ty` (heads compare
/// by name + args; pins are outputs, not part of the relation).
pub fn interface_requires(
    db: &dyn baml_compiler2_ppir::Db,
    sub: &InterfaceRef,
    sup: &InterfaceRef,
    self_ty: &Ty,
    fuel: u32,
) -> bool {
    interface_requires_inner(db, sub, sup, self_ty, fuel, &mut Vec::new())
}

fn interface_requires_inner(
    db: &dyn baml_compiler2_ppir::Db,
    sub: &InterfaceRef,
    sup: &InterfaceRef,
    self_ty: &Ty,
    fuel: u32,
    visited: &mut Vec<InterfaceRef>,
) -> bool {
    if fuel == 0 {
        return false;
    }
    // Cycle detection: a `requires` loop (rejected upstream as
    // InterfaceRequiresCycle, guarded here as defense-in-depth) revisits
    // a realized head - no new facts lie that way.
    if visited.contains(sub) {
        return false;
    }
    visited.push(sub.clone());
    let eq = AliasOnlyFacts::new(db);
    let mut holds = false;
    for required in direct_requires(db, sub, self_ty) {
        if head_matches(&required, sup, &eq) {
            holds = true;
            break;
        }
        if interface_requires_inner(db, &required, sup, self_ty, fuel - 1, visited) {
            holds = true;
            break;
        }
    }
    visited.pop();
    holds
}
