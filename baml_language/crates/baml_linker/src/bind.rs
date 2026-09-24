//! Dependency binding: each slot of a unit's or tail's dependency table
//! bound to a package of the set, through the edge tables.

use baml_linker_types::DependencyEntry;

use super::{LinkError, LinkPackageId, Linker, PerPackage};

/// A package's bound dependency tables: its unit's and its tail's, each
/// indexed by [`DepSlot`](baml_linker_types::DepSlot) (slot 0 is the package
/// itself).
pub(super) struct Tables {
    pub(super) unit: Vec<LinkPackageId>,
    pub(super) tail: Vec<LinkPackageId>,
}

impl Linker<'_, '_> {
    pub(super) fn bind_all(&self) -> Result<PerPackage<Tables>, LinkError> {
        PerPackage::try_from_set(self.set, |id, package| {
            Ok(Tables {
                unit: self.bind(id, &package.unit.dependencies)?,
                tail: match package.tail {
                    Some(tail) => self.bind(id, &tail.dependencies)?,
                    None => Vec::new(),
                },
            })
        })
    }

    /// Bind one dependency table (a unit's or a tail's): slot `k + 1` is the
    /// package entry `k`'s edge names in the edge table of the package bound
    /// to `via`.
    fn bind(
        &self,
        owner: LinkPackageId,
        dependencies: &[DependencyEntry],
    ) -> Result<Vec<LinkPackageId>, LinkError> {
        let owner_name = self.set.name(owner);
        let mut bound = vec![owner];
        for (index, entry) in dependencies.iter().enumerate() {
            let Some(&parent) = bound.get(entry.via.0 as usize) else {
                return Err(LinkError::invalid(format!(
                    "package `{owner_name}` dependency slot {} is reached via slot {}, which is \
                     not earlier",
                    index + 1,
                    entry.via.0
                )));
            };
            let Some(&(_, id)) = self
                .set
                .package(parent)
                .edges
                .iter()
                .find(|(name, _)| *name == entry.edge)
            else {
                return Err(LinkError::UnknownDependency {
                    package: owner_name.clone(),
                    edge: entry.edge.clone(),
                });
            };
            bound.push(id);
        }
        Ok(bound)
    }
}
