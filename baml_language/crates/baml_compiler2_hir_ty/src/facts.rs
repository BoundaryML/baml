//! The engine's fact oracle: a [`TypeContext`] implementation backed by
//! ppir's item data, consulted by every subtype/equivalence/canonicalization
//! query. Facts are FAIL-SAFE per the trait's contract: an unanswerable
//! question returns the conservative answer, never a guess.
//!
//! S7 scope: alias definitions (lazy, cycle-guarded by the normalizer's
//! mu-binders) and enum variant sets (complete-set collapse). The interface
//! facts (`implements_interface`, `interface_requires`, bounds, projections)
//! come alive with the impl registry in I1/I2/I5; until then they answer
//! "unknown" conservatively.

use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{
    Interface, Name, ParamTy, QualifiedTypeName, Ty,
    normalize::{ProjectionStep, TypeContext},
};

pub struct Facts<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    /// The current scope's param env (I2): each rigid variable's declared
    /// bound conjunction, as plain constraints (the trait's vocabulary).
    bounds: rustc_hash::FxHashMap<ParamTy, Vec<Interface>>,
}

impl<'db> Facts<'db> {
    pub fn new(db: &'db dyn baml_compiler2_ppir::Db) -> Facts<'db> {
        Facts {
            db,
            bounds: rustc_hash::FxHashMap::default(),
        }
    }

    pub fn with_bounds(
        db: &'db dyn baml_compiler2_ppir::Db,
        bounds: rustc_hash::FxHashMap<ParamTy, Vec<Interface>>,
    ) -> Facts<'db> {
        Facts { db, bounds }
    }

    /// Resolves a qualified name back to its definition through the owning
    /// package's canonical (ppir) items.
    pub fn definition_of(&self, name: &QualifiedTypeName) -> Option<Definition<'db>> {
        let package = PackageId::new(self.db, name.package().clone());
        baml_compiler2_ppir::package_items(self.db, package)
            .lookup_type(name.namespace(), name.name())
    }
}

impl TypeContext for Facts<'_> {
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        let Some(Definition::TypeAlias(alias)) = self.definition_of(name) else {
            return None;
        };
        Some(crate::lower::type_alias_value(self.db, alias).to_plain())
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        let Some(Definition::Enum(enum_loc)) = self.definition_of(name) else {
            return None;
        };
        Some(
            baml_compiler2_ppir::item_data::enum_data(self.db, enum_loc)
                .variants
                .iter()
                .map(|variant| variant.name.clone())
                .collect(),
        )
    }

    // -- Interface facts (I1: the impl registry answers; bounds and
    // projections stay conservative until I2/I5) ------------------------------

    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool {
        let concrete = crate::impls::interned_ty(concrete);
        let target = crate::impls::InterfaceTarget::from_constraint(interface);
        crate::impls::implements_interface(self.db, &concrete, &target)
    }

    fn type_var_bound(&self, param: &ParamTy) -> Vec<Interface> {
        self.bounds.get(param).cloned().unwrap_or_default()
    }

    fn interface_requires(&self, sub: &Interface, sup: &Interface) -> bool {
        crate::impls::interface_requires(
            self.db,
            &crate::impls::InterfaceTarget::from_constraint(sub),
            &crate::impls::InterfaceTarget::from_constraint(sup),
            8,
        )
    }

    fn associated_type_bound(&self, interface: &Interface, assoc: Name) -> Vec<Interface> {
        // The declared `type assoc extends J`, realized at the qualifier's
        // args with `Self` left symbolic (the trait's contract: the oracle
        // is a function of the reference, not an implementor) - rustc's
        // `explicit_item_bounds` instantiated.
        let target = crate::impls::InterfaceTarget::from_constraint(interface);
        let symbolic_self = baml_type::interned::Ty::intern(baml_type::interned::TyKind::TypeVar(
            ParamTy::new(0, Name::new("Self")),
            baml_type::TyAttr::default(),
        ));
        crate::impls::realized_assoc_bound(self.db, &target, &symbolic_self, &assoc)
            .and_then(|bound| bound.to_plain().as_interface())
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
        let target = crate::impls::InterfaceTarget::from_constraint(interface);
        let eq = crate::impls::AliasOnlyFacts::new(self.db);
        if let Ty::TypeVar(param, _) = base {
            let base_interned = crate::impls::interned_ty(base);
            let mut candidates: Vec<baml_type::interned::Ty> = Vec::new();
            for bound in self.type_var_bound(param) {
                let have = crate::impls::InterfaceTarget::from_constraint(&bound);
                let mut heads = vec![have.clone()];
                heads.extend(crate::impls::direct_requires_closure(self.db, &have, 8));
                for head in heads {
                    let head_matches = head.name == target.name
                        && head.args.len() == target.args.len()
                        && head
                            .args
                            .iter()
                            .zip(&target.args)
                            .all(|(a, b)| equivalent_interned(a, b, &eq));
                    if !head_matches {
                        continue;
                    }
                    let value = head
                        .pins
                        .iter()
                        .find(|(name, _)| name == member)
                        .map(|(_, ty)| ty.clone())
                        .or_else(|| {
                            crate::impls::realized_assoc_default(
                                self.db,
                                &head,
                                &base_interned,
                                member,
                            )
                        });
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
            let base_target = crate::impls::InterfaceTarget {
                name: name.clone(),
                args: args.iter().map(crate::impls::interned_ty).collect(),
                pins: pins
                    .iter()
                    .map(|(pin_name, ty)| (pin_name.clone(), crate::impls::interned_ty(ty)))
                    .collect(),
            };
            let base_interned = crate::impls::interned_ty(base);
            if let Some(default) = crate::impls::realized_assoc_default(
                self.db,
                &base_target,
                &base_interned,
                member,
            ) {
                return ProjectionStep::Reduced(default.to_plain());
            }
            return ProjectionStep::Opaque;
        }
        let base_interned = crate::impls::interned_ty(base);
        if let Some(resolved) = crate::impls::resolve_impl(self.db, &base_interned, &target)
            && let Some(pin) =
                crate::impls::resolved_pin(self.db, &resolved, &base_interned, member)
        {
            return ProjectionStep::Reduced(pin.to_plain());
        }
        ProjectionStep::Opaque
    }
}
