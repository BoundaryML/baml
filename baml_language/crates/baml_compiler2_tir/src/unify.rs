//! Relocated to [`baml_type::unify`] (the shared plain-type algebra home) -
//! this module re-exports it and keeps only the db-backed constructors for
//! the oracle's inputs.

use baml_base::Name;
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::TypeName;

pub use baml_type::unify::*;

use crate::ty::Ty;

/// Type aliases visible to `pkg_id` — its own plus its dependency closure's exports —
/// with every body folded toward the union canonical form the overlap machinery assumes
/// (`type TF = true | false` → `bool`, `type OI = 1 | int` → `int`). An alias-obscured
/// union member must be compared by the same union laws (ACI + finite-base completeness)
/// as its spelled-out form: without the fold, `Bar<TF>` and `Bar<bool>` normalize to
/// structurally-different args and are wrongly judged disjoint — for coherence a
/// fails-open hole (admitting two impls of one interface for one type), for
/// [`crate::pattern_overlap`] a wrong `No` (a spurious compile error on a reachable
/// arm).
pub(crate) fn normalized_alias_map<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> std::collections::HashMap<TypeName, Ty> {
    let mut aliases =
        crate::inference::collect_type_aliases(db, baml_compiler2_ppir::package_items(db, pkg_id));
    for dep in baml_compiler2_hir::package::package_dependency_closure(db, pkg_id) {
        for (qtn, ty) in
            crate::inference::collect_type_aliases(db, baml_compiler2_ppir::package_items(db, *dep))
        {
            aliases.entry(qtn).or_insert(ty);
        }
    }
    let enum_variants = |qtn: &TypeName| enum_variant_names(db, qtn);
    for body in aliases.values_mut() {
        *body = nf(body, &enum_variants);
    }
    aliases
}

/// Resolve an enum's full set of variant names, or `None` if it can't be resolved. Used
/// by `nf` to fold a complete variant union (`Cmp.Less | Cmp.Equal | Cmp.More`) back to
/// its enum (`Cmp`).
pub(crate) fn enum_variant_names(db: &dyn crate::Db, enum_qtn: &TypeName) -> Option<Vec<Name>> {
    let package_id = PackageId::new(db, enum_qtn.package().clone());
    let items = baml_compiler2_hir::package::package_items(db, package_id);
    let Some(Definition::Enum(enum_loc)) = items.lookup_type(enum_qtn.namespace(), enum_qtn.name())
    else {
        return None;
    };
    let enum_data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
    Some(enum_data.variants.iter().map(|v| v.name.clone()).collect())
}
