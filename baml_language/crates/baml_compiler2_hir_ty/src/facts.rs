//! The engine's fact oracle: a [`TypeContext`] implementation backed by
//! ppir's item data, consulted by every subtype/equivalence/canonicalization
//! query. Facts are FAIL-SAFE per the trait's contract: an unanswerable
//! question returns the conservative answer, never a guess.
//!
//! Alias definitions (lazy, cycle-guarded by the normalizer's mu-binders),
//! enum variant sets (complete-set collapse), and the interface facts
//! (`implements_interface`, `interface_requires`, bounds, projections) -
//! all LIVE since I1/I2/I5, backed by the impl registry and the scope's
//! param env.

use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{
    Interface, Name, ParamTy, QualifiedTypeName, Ty,
    interned::InterfaceRef,
    normalize::{ProjectionStep, TypeContext},
};

pub struct Facts<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    /// The current scope's param env (I2): each rigid variable's declared
    /// bound conjunction, as plain constraints (the trait's vocabulary).
    bounds: rustc_hash::FxHashMap<ParamTy, Vec<Interface>>,
    /// Canonicalization asks for the same recursive alias and enum facts many
    /// times inside one body. Cache the owned plain rows at the oracle boundary
    /// instead of repeatedly materializing them from interned compiler data.
    alias_defs: std::cell::RefCell<rustc_hash::FxHashMap<QualifiedTypeName, Option<Ty>>>,
    enum_variants: std::cell::RefCell<rustc_hash::FxHashMap<QualifiedTypeName, Option<Vec<Name>>>>,
}

impl<'db> Facts<'db> {
    pub fn new(db: &'db dyn baml_compiler2_ppir::Db) -> Facts<'db> {
        Facts {
            db,
            bounds: rustc_hash::FxHashMap::default(),
            alias_defs: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
            enum_variants: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
        }
    }

    pub fn with_bounds(
        db: &'db dyn baml_compiler2_ppir::Db,
        bounds: rustc_hash::FxHashMap<ParamTy, Vec<Interface>>,
    ) -> Facts<'db> {
        Facts {
            db,
            bounds,
            alias_defs: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
            enum_variants: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
        }
    }

    /// The scope's param env verbatim - the overlap oracle's `bounds` input
    /// shares this shape (`TypeVarBoundsMap`).
    pub fn bounds(&self) -> &rustc_hash::FxHashMap<ParamTy, Vec<Interface>> {
        &self.bounds
    }

    /// Resolves a qualified name back to its definition through the owning
    /// package's canonical (ppir) items.
    pub fn definition_of(&self, name: &QualifiedTypeName) -> Option<Definition<'db>> {
        definition_of(self.db, name)
    }
}

fn definition_of<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    name: &QualifiedTypeName,
) -> Option<Definition<'db>> {
    let package = PackageId::new(db, name.package().clone());
    baml_compiler2_ppir::package_items(db, package).lookup_type(name.namespace(), name.name())
}

/// Resolves an alias without retaining the result in a memo. One-shot
/// fact-poor contexts use this directly; repeated scans use a cached context.
pub(crate) fn uncached_alias_def(
    db: &dyn baml_compiler2_ppir::Db,
    name: &QualifiedTypeName,
) -> Option<Ty> {
    if let Some(Definition::TypeAlias(alias)) = definition_of(db, name) {
        return Some(crate::lower::type_alias_value(db, alias).to_plain());
    }
    match crate::package_interface::mounted_type_row(db, name) {
        Some(crate::package_interface::ExportedType::TypeAlias { resolved, .. }) => {
            Some(resolved.clone())
        }
        _ => None,
    }
}

/// Resolves enum variants without retaining the result in a memo. One-shot
/// fact-poor contexts use this directly; repeated scans use a cached context.
pub(crate) fn uncached_enum_variants(
    db: &dyn baml_compiler2_ppir::Db,
    name: &QualifiedTypeName,
) -> Option<Vec<Name>> {
    if let Some(Definition::Enum(enum_loc)) = definition_of(db, name) {
        return Some(
            baml_compiler2_ppir::item_data::enum_data(db, enum_loc)
                .variants
                .iter()
                .map(|variant| variant.name.clone())
                .collect(),
        );
    }
    match crate::package_interface::mounted_type_row(db, name) {
        Some(crate::package_interface::ExportedType::Enum { variants, .. }) => {
            Some(variants.clone())
        }
        _ => None,
    }
}

impl TypeContext for Facts<'_> {
    /// A name-based context represents a declaration by its own name, so this
    /// is the identity — no resolution step, and never `None`.
    fn head_lookup(&self, qtn: &QualifiedTypeName) -> Option<QualifiedTypeName> {
        Some(qtn.clone())
    }

    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        if let Some(cached) = self.alias_defs.borrow().get(name) {
            return cached.clone();
        }
        let resolved = uncached_alias_def(self.db, name);
        self.alias_defs
            .borrow_mut()
            .insert(name.clone(), resolved.clone());
        resolved
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        if let Some(cached) = self.enum_variants.borrow().get(name) {
            return cached.clone();
        }
        let resolved = uncached_enum_variants(self.db, name);
        self.enum_variants
            .borrow_mut()
            .insert(name.clone(), resolved.clone());
        resolved
    }

    // -- Interface facts (I1: the impl registry answers; bounds and
    // projections stay conservative until I2/I5) ------------------------------

    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool {
        let concrete = crate::impls::interned_ty(concrete);
        let target = InterfaceRef::from_constraint(interface);
        crate::impls::implements_interface(self.db, &concrete, &target)
    }

    fn type_var_bound(&self, param: &ParamTy) -> Vec<Interface> {
        self.bounds.get(param).cloned().unwrap_or_default()
    }

    fn interface_requires(&self, sub: &Interface, sup: &Interface) -> bool {
        let sub = InterfaceRef::from_constraint(sub);
        // No better subject at this fact boundary: `Self` in `sub`'s
        // requires targets realizes as the existential itself (rustc's
        // `dyn A: B` elaboration).
        let subject = sub.existential();
        crate::impls::interface_requires(
            self.db,
            &sub,
            &InterfaceRef::from_constraint(sup),
            &subject,
            8,
        )
    }

    fn associated_type_bound(&self, interface: &Interface, assoc: Name) -> Vec<Interface> {
        // The declared `type assoc extends J`, realized at the qualifier's
        // args with `Self` left symbolic (the trait's contract: the oracle
        // is a function of the reference, not an implementor) - rustc's
        // `explicit_item_bounds` instantiated.
        let target = InterfaceRef::from_constraint(interface);
        let symbolic_self = baml_type::interned::Ty::intern(baml_type::interned::TyKind::TypeVar(
            ParamTy::new(0, Name::new("Self")),
            baml_type::TyAttr::default(),
        ));
        let symbolic_self_plain = symbolic_self.to_plain();
        crate::impls::realized_assoc_bound(self.db, &target, &symbolic_self, &assoc)
            .and_then(|bound| bound.to_plain().as_interface())
            .map(|bound| {
                // Reduce sibling-pin projections the substitution left behind
                // (`Producer<(Self as Parser).Item>` with `Item` pinned on the
                // qualifier -> the pin): TIR re-lowered the bound with the
                // realized pins in scope, so its callers always saw the
                // collapsed form; hir_ty realizes a once-lowered form by
                // substitution, so collapse here at the oracle boundary — the
                // qualifier's own pins first (the declared bound's projection
                // rides the pinless declaration ref, which `project` cannot
                // answer from a bare symbolic `Self`), then normalize.
                let collapse = |ty: &baml_type::Ty| {
                    let collapsed = crate::interfaces::collapse_self_assoc_projections(
                        ty,
                        &[&symbolic_self_plain],
                        Some(&interface.name),
                        &interface.associated_types,
                    );
                    baml_type::normalize::normalize(&collapsed, self)
                };
                Interface {
                    name: bound.name,
                    generics: bound.generics.iter().map(collapse).collect(),
                    associated_types: bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), collapse(ty)))
                        .collect(),
                }
            })
            .into_iter()
            .collect()
    }

    /// Single-step projection reduction, in rustc's `project.rs` candidate
    /// order: param-env candidates first (the qualifier's own pin, then a
    /// rigid var's carried bounds elaborated through the `requires`
    /// closure - several DISAGREEING candidates are an ambiguity and stay
    /// Opaque, never pick-first), then the base's own reference (an
    /// existential's pin or its interface's default), then impl candidates
    /// (the registry; binding-else-default inside `resolved_pin`). An
    /// unpinned member of a written reference realizes its declared
    /// DEFAULT - the spec's fill-at-reference rule, deliberately broader
    /// than rustc (documented at `realized_assoc_default`).
    fn project(
        &self,
        base: &Ty,
        interface: &Interface,
        member: &Name,
        // Single-step: the canonical `from_ty` walk decrements its own
        // fuel across the reduction chain (the TIR-side precedent).
        _fuel: u32,
    ) -> ProjectionStep {
        use baml_type::normalize::equivalent_interned;
        if let Some((_, pin)) = interface
            .associated_types
            .iter()
            .find(|(name, _)| name == member)
        {
            return ProjectionStep::Reduced(pin.clone());
        }
        let target = InterfaceRef::from_constraint(interface);
        let eq = crate::impls::AliasOnlyFacts::new(self.db);
        if let Ty::TypeVar(param, _) = base {
            let base_interned = crate::impls::interned_ty(base);
            let mut candidates: Vec<baml_type::interned::Ty> = Vec::new();
            for bound in self.type_var_bound(param) {
                let have = InterfaceRef::from_constraint(&bound);
                for head in crate::impls::requires_heads(self.db, &have, &base_interned, 8) {
                    if !crate::impls::head_matches(&head, &target, &eq) {
                        continue;
                    }
                    // Param-env pins ONLY (rustc's projection discipline
                    // for a param base): the declared default belongs to
                    // impl selection - an impl may override it - so an
                    // unpinned member stays a rigid projection, resolved
                    // per-receiver at runtime.
                    let value = head
                        .associated_types
                        .iter()
                        .find(|(name, _)| name == member)
                        .map(|(_, ty)| ty.clone());
                    if let Some(value) = value
                        && !candidates
                            .iter()
                            .any(|have| equivalent_interned(have, &value, &eq))
                    {
                        candidates.push(value);
                    }
                }
            }
            // A rigid var reaches no impl, so the param env decides alone.
            return match candidates.as_slice() {
                [only] => ProjectionStep::Reduced(only.to_plain()),
                _ => ProjectionStep::Opaque,
            };
        }
        if let Ty::Interface(name, args, pins, _) = base {
            if let Some((_, pin)) = pins.iter().find(|(pin_name, _)| pin_name == member) {
                return ProjectionStep::Reduced(pin.clone());
            }
            // An existential fixes an omitted defaulted member to the
            // default, with `Self` = the base itself (so a
            // Self-referencing default resolves against the base's pins).
            let base_target = InterfaceRef::new(
                name.clone(),
                args.iter()
                    .map(crate::impls::interned_ty)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                pins.iter()
                    .map(|(pin_name, ty)| (pin_name.clone(), crate::impls::interned_ty(ty)))
                    .collect(),
            );
            let Some(base_interned) = crate::impls::try_interned_ty(base) else {
                return ProjectionStep::Opaque;
            };
            // A pin inherited through the requires closure (`interface Child
            // requires Parent<Item = int>`; the projection asks for
            // `Parent.Item` on a `Child` existential): elaborate the base's
            // closure and read the matching head's pin — rustc's supertrait
            // elaboration, after the base's own reference and before its
            // declared default.
            for head in crate::impls::requires_heads(self.db, &base_target, &base_interned, 8) {
                if !crate::impls::head_matches(&head, &target, &eq) {
                    continue;
                }
                if let Some((_, ty)) = head
                    .associated_types
                    .iter()
                    .find(|(name, _)| name == member)
                {
                    return ProjectionStep::Reduced(ty.to_plain());
                }
            }
            if let Some(default) =
                crate::impls::realized_assoc_default(self.db, &base_target, &base_interned, member)
            {
                return ProjectionStep::Reduced(default.to_plain());
            }
            return ProjectionStep::Opaque;
        }
        let Some(base_interned) = crate::impls::try_interned_ty(base) else {
            return ProjectionStep::Opaque;
        };
        if let Some(resolved) = crate::impls::resolve_impl(self.db, &base_interned, &target)
            && let Some(pin) =
                crate::impls::resolved_pin(self.db, &resolved, &base_interned, member)
        {
            return ProjectionStep::Reduced(pin.to_plain());
        }
        ProjectionStep::Opaque
    }
}
