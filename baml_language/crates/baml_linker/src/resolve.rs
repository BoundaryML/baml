//! Exports by path, and every import resolved against them.

use std::collections::HashMap;

use baml_linker_types::{CompilationUnit, ImportEntry, InitTail};
use bex_vm_types::{BodyKey, DeclPath};

use super::{
    LinkError, LinkPackageId, LinkSet, Linker, PerPackage,
    bind::Tables,
    order::{Objects, Slots},
    space::Resolved,
};

pub(super) type PathKey = (LinkPackageId, DeclPath);

/// Every package's global exports by path, to image slot.
pub(super) fn export_globals(
    set: &LinkSet<'_>,
    slots: &Slots,
) -> Result<HashMap<PathKey, usize>, LinkError> {
    let mut map = HashMap::new();
    for id in set.ids() {
        let package = set.package(id);
        for (path, flat) in &package.unit.exports.globals {
            let slot = slots.units[id]
                .local(*flat as usize)
                .unwrap_or_else(|| unreachable!("the partition was validated"));
            if map.insert((id, path.clone()), slot).is_some() {
                return Err(LinkError::DuplicateExport {
                    package: package.name.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    Ok(map)
}

/// Every package's object exports by path, to image index.
pub(super) fn export_objects(
    set: &LinkSet<'_>,
    objects: &Objects,
) -> Result<HashMap<PathKey, usize>, LinkError> {
    let mut map = HashMap::new();
    for id in set.ids() {
        let package = set.package(id);
        for (path, local) in &package.unit.exports.objects {
            if !local.holds(path) {
                return Err(LinkError::invalid(format!(
                    "package `{}` exports {path} from the {local:?} bucket",
                    package.name
                )));
            }
            let Some(abs) = objects.units[id].export(package.unit, *local) else {
                return Err(LinkError::invalid(format!(
                    "package `{}` export {path} points outside its {local:?} bucket",
                    package.name
                )));
            };
            if map.insert((id, path.clone()), abs).is_some() {
                return Err(LinkError::DuplicateExport {
                    package: package.name.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    Ok(map)
}

/// Which index space an import table addresses.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Space {
    Object,
    Global,
}

impl Space {
    fn entries(self, unit: &CompilationUnit) -> &[ImportEntry] {
        match self {
            Self::Object => &unit.object_imports,
            Self::Global => &unit.global_imports,
        }
    }

    fn tail_entries(self, tail: &InitTail) -> &[ImportEntry] {
        match self {
            Self::Object => &tail.object_imports,
            Self::Global => &tail.global_imports,
        }
    }

    /// The checks every import passes before its slot is even read.
    fn check_shape(self, entry: &ImportEntry) -> Result<(), LinkError> {
        let path = &entry.key.path;
        let owns = match self {
            Self::Object => path.owns_object(),
            Self::Global => path.owns_global_slot(),
        };
        if !owns {
            return Err(LinkError::invalid(format!(
                "import of {path} in an index space it owns nothing in"
            )));
        }
        if matches!(path, DeclPath::InterfaceBody(BodyKey::ImplMethod(_)))
            && !entry.key.dep.is_self()
        {
            return Err(LinkError::invalid(format!(
                "{path} is imported from dependency slot {}; an impl-provided body is reachable \
                 only within its own package",
                entry.key.dep.0
            )));
        }
        Ok(())
    }
}

/// Whose import table is being resolved. A unit's own declarations are its
/// locals, so a unit importing its own package is malformed; a tail is a
/// separate table and reaches its package's declarations exactly as it
/// reaches a dependency's.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Importer {
    Unit,
    Tail,
}

impl Linker<'_, '_> {
    /// Resolve every unit's and every tail's import table in `space`.
    pub(super) fn resolve(
        &self,
        tables: &PerPackage<Tables>,
        exports: &HashMap<PathKey, usize>,
        space: Space,
    ) -> Result<Resolved, LinkError> {
        let units = PerPackage::try_from_set(self.set, |id, package| {
            space
                .entries(package.unit)
                .iter()
                .map(|entry| {
                    self.resolve_import(Importer::Unit, &tables[id].unit, exports, space, entry)
                })
                .collect()
        })?;
        let tails = PerPackage::try_from_set(self.set, |id, package| {
            package.tail.map_or(Ok(Vec::new()), |tail| {
                space
                    .tail_entries(tail)
                    .iter()
                    .map(|entry| {
                        self.resolve_import(Importer::Tail, &tables[id].tail, exports, space, entry)
                    })
                    .collect()
            })
        })?;
        Ok(Resolved { units, tails })
    }

    /// Resolve one import through a bound dependency table against `exports`.
    fn resolve_import(
        &self,
        importer: Importer,
        table: &[LinkPackageId],
        exports: &HashMap<PathKey, usize>,
        space: Space,
        entry: &ImportEntry,
    ) -> Result<usize, LinkError> {
        space.check_shape(entry)?;
        let path = &entry.key.path;
        let Some(&id) = table.get(entry.key.dep.0 as usize) else {
            return Err(LinkError::invalid(format!(
                "import of {path} names dependency slot {} of {}",
                entry.key.dep.0,
                table.len()
            )));
        };
        if importer == Importer::Unit && entry.key.dep.is_self() {
            return Err(LinkError::invalid(format!(
                "package `{}` imports its own {path}; its own declarations are locals",
                self.set.name(id)
            )));
        }
        exports
            .get(&(id, path.clone()))
            .copied()
            .ok_or_else(|| LinkError::UnresolvedImport {
                package: self.set.name(id).clone(),
                path: path.clone(),
            })
    }
}
