//! Owner-defined bridge projection. Importing another package's implementation
//! must not silently change an existing class's record/live representation.

use std::collections::BTreeSet;

use baml_compiler2_ast::FunctionOrigin;
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc},
    package::PackageId,
};
use baml_compiler2_ppir::item_data::{class_data, function_data, interface_data};
use baml_type::{ClassProjection, QualifiedTypeName, interned::TyKind};

/// Shared source predicate, also serialized with exported method metadata.
pub(crate) fn is_authored_instance_method(
    db: &dyn baml_compiler2_ppir::Db,
    method: FunctionLoc<'_>,
) -> bool {
    let method = function_data(db, method);
    method.metadata.origin == FunctionOrigin::UserDefined
        && !method.metadata.is_language_internal
        && method
            .params
            .first()
            .is_some_and(|p| p.name.as_str() == "self")
}

fn interface_has_behavior(
    db: &dyn baml_compiler2_ppir::Db,
    name: &QualifiedTypeName,
    visited: &mut BTreeSet<QualifiedTypeName>,
) -> bool {
    if !visited.insert(name.clone()) {
        return false;
    }
    if let Some(crate::package_interface::ExportedType::Interface {
        required_methods,
        default_methods,
        requires,
        ..
    }) = crate::package_interface::mounted_type_row(db, name)
    {
        required_methods
            .iter()
            .chain(default_methods)
            .any(|m| m.authored_instance)
            || requires
                .iter()
                .any(|required| interface_has_behavior(db, &required.name, visited))
    } else if let Some(Definition::Interface(loc)) =
        crate::facts::Facts::new(db).definition_of(name)
    {
        let data = interface_data(db, loc);
        if data
            .methods
            .iter()
            .any(|&method| is_authored_instance_method(db, method))
        {
            return true;
        }
        let ctx = crate::lower::lower_ctx_for_file(db, loc.file(db))
            .with_frame(crate::lower::interface_frame(db, loc))
            .with_bounds(crate::lower::interface_scope_bounds(db, loc));
        data.requires.iter().any(|&required| {
            let required = ctx.lower_type_ref_at(
                &data.type_refs,
                required,
                crate::lower::TypePosition::ConstraintHead,
            );
            baml_type::interned::InterfaceRef::of_ty(&required)
                .is_some_and(|required| interface_has_behavior(db, &required.name, visited))
        })
    } else {
        false // unresolved declarations already have compiler diagnostics
    }
}

/// Class-headed implementations declared by the class's own package. A
/// blanket rule or downstream extension supplies an interface view; it cannot
/// change the default codec of arbitrary declarations. The choice is stable
/// across class type arguments, including conditional implementation rules.
#[salsa::tracked(returns(ref))]
fn package_behavior_classes<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package: PackageId<'db>,
) -> BTreeSet<QualifiedTypeName> {
    let mut classes = BTreeSet::new();
    let facts = crate::facts::Facts::new(db);
    for &block in crate::impls::package_impl_locs(db, package) {
        let Some(rule) = crate::impls::impl_facts(db, block).resolved() else {
            continue;
        };
        let receiver = baml_type::normalize::normalize_interned(&rule.for_ty_pattern, &facts);
        let TyKind::Class(name, _, _) = receiver.kind() else {
            continue;
        };
        if *name.package() != package.name(db) {
            continue;
        }
        // Generated implementation bodies are serialization plumbing. Empty
        // authored blocks can still acquire real behavior through defaults.
        if !rule.methods.is_empty()
            && rule.methods.iter().all(|&method| {
                let metadata = function_data(db, method).metadata;
                metadata.origin == FunctionOrigin::AutoDerive || metadata.is_language_internal
            })
        {
            continue;
        }
        if rule
            .methods
            .iter()
            .any(|&method| is_authored_instance_method(db, method))
            || interface_has_behavior(db, &rule.interface.name, &mut BTreeSet::new())
        {
            classes.insert(name.clone());
        }
    }
    classes
}

pub fn class_projection(db: &dyn baml_compiler2_ppir::Db, class: ClassLoc<'_>) -> ClassProjection {
    let name = crate::lower::class_qualified_name(db, class);
    // Runtime mounts can also have field-only source stubs for linking. Those
    // stubs do not replace the original declaration's behavior/codec metadata.
    if let Some(crate::package_interface::ExportedType::Class {
        boundary_projection,
        ..
    }) = crate::package_interface::mounted_type_row(db, &name)
    {
        return *boundary_projection;
    }
    // Canonical carrier metadata is shared with compiler alias lowering.
    if baml_type::compiler_aliases::by_definition(&name).is_some()
        || (name.package().as_str() == "ai"
            && name.namespace().is_empty()
            && name.name().as_str() == "Prompt")
    {
        return ClassProjection::Builtin;
    }
    if class_data(db, class)
        .methods
        .iter()
        .any(|&method| is_authored_instance_method(db, method))
        || package_behavior_classes(db, PackageId::new(db, name.package().clone())).contains(&name)
    {
        ClassProjection::Live
    } else {
        ClassProjection::Record
    }
}

/// Source-less consumers use the owner's serialized decision. No host
/// generator reconstructs this from a list of known implementations.
pub fn for_name(
    db: &dyn baml_compiler2_ppir::Db,
    name: &QualifiedTypeName,
) -> Option<ClassProjection> {
    if let Some(Definition::Class(class)) = crate::facts::Facts::new(db).definition_of(name) {
        return Some(class_projection(db, class));
    }
    match crate::package_interface::mounted_type_row(db, name)? {
        crate::package_interface::ExportedType::Class {
            boundary_projection,
            ..
        } => Some(*boundary_projection),
        _ => None,
    }
}
