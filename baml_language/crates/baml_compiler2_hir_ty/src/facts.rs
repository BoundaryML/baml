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

    fn associated_type_bound(&self, _interface: &Interface, _assoc: Name) -> Vec<Interface> {
        Vec::new()
    }

    fn project(
        &self,
        _base: &Ty,
        _interface: &Interface,
        _member: &Name,
        _fuel: u32,
    ) -> ProjectionStep {
        ProjectionStep::Opaque
    }
}
