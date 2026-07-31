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
}

impl<'db> Facts<'db> {
    pub fn new(db: &'db dyn baml_compiler2_ppir::Db) -> Facts<'db> {
        Facts { db }
    }

    /// Resolves a qualified name back to its definition through the owning
    /// package's canonical (ppir) items.
    fn definition_of(&self, name: &QualifiedTypeName) -> Option<Definition<'db>> {
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

    // -- Interface facts: conservative until the impl registry (I1+) ----------

    fn implements_interface(&self, _concrete: &Ty, _interface: &Interface) -> bool {
        false
    }

    fn type_var_bound(&self, _param: &ParamTy) -> Vec<Interface> {
        Vec::new()
    }

    fn interface_requires(&self, _sub: &Interface, _sup: &Interface) -> bool {
        false
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
