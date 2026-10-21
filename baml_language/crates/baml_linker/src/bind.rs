//! Dependency binding: each slot of a unit's or tail's dependency table
//! bound to a package of the set, through the edge tables — the edge LOCATES
//! the package, its recorded interface fingerprint BINDS it (rustc's
//! `(name, crate hash)`): a unit compiled against one interface never links
//! against another.

use baml_linker_types::{Locator, PackageRecord};
use bex_vm_types::types::{EdgeKind, PackageKind};

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
    /// to its `via`, and the entry's kind must match the package reached — a
    /// direct entry binds an ordinary package whose interface payload has the
    /// recorded digest, a transitive entry anything (its parent's digest
    /// covers it). A prelude entry binds the set's language package of that
    /// identity, through no edge at all. A static link set reaches its
    /// packages under plain dependency edges only; a view's re-export edge
    /// exists on runtime package objects.
    fn bind(
        &self,
        owner: LinkPackageId,
        dependencies: &[Locator],
    ) -> Result<Vec<LinkPackageId>, LinkError> {
        let owner_name = self.set.name(owner);
        let mut bound = vec![owner];
        for (index, locator) in dependencies.iter().enumerate() {
            let via = locator.via();
            let Some(&parent) = bound.get(via.0 as usize) else {
                return Err(LinkError::invalid(format!(
                    "package `{owner_name}` dependency slot {} is reached via slot {}, which is \
                     not earlier",
                    index + 1,
                    via.0
                )));
            };
            // A prelude entry binds by identity, through no edge; the other
            // two locate an edge in the parent's table.
            let edge_name = match locator {
                Locator::Prelude(language) => {
                    let Some(target) = self.set.language_package(*language) else {
                        return Err(LinkError::MissingLanguagePackage {
                            package: owner_name.clone(),
                            language: *language,
                        });
                    };
                    bound.push(target);
                    continue;
                }
                Locator::Direct { edge, .. } | Locator::Transitive { edge, .. } => edge,
            };
            let Some(edge) = self
                .set
                .package(parent)
                .edges
                .iter()
                .find(|edge| edge.name == *edge_name)
            else {
                // The edge is read in the PARENT's table: the owner's own for
                // a direct entry, an earlier slot's package for a transitive
                // one — so the package named is the one whose table lacks it.
                return Err(LinkError::UnknownDependency {
                    package: self.set.name(parent).clone(),
                    edge: edge_name.clone(),
                });
            };
            if edge.kind == EdgeKind::ReExported {
                return Err(LinkError::invalid(format!(
                    "package `{owner_name}` reaches `{edge_name}` through a runtime-only re-export \
                     edge"
                )));
            }
            let target = self.set.package(edge.target);
            match (locator, target.kind) {
                (Locator::Direct { edge: name, digest }, PackageKind::Ordinary) => {
                    Self::verify_digest(owner_name, name, *digest, target.record)?;
                }
                (Locator::Direct { edge: name, .. }, PackageKind::Language(_)) => {
                    return Err(LinkError::invalid(format!(
                        "package `{owner_name}` records an interface fingerprint for the prelude \
                         package `{name}`"
                    )));
                }
                (Locator::Prelude(_), PackageKind::Language(_) | PackageKind::Ordinary) => {
                    unreachable!("a prelude entry binds by identity above")
                }
                (Locator::Transitive { .. }, PackageKind::Language(_) | PackageKind::Ordinary) => {}
            }
            bound.push(edge.target);
        }
        Ok(bound)
    }

    /// The fingerprint law for a direct dependency: the bound package's
    /// interface payload must have the digest the unit was compiled against
    /// (rustc's `(name, crate hash)`) — the edge locates, the digest binds.
    fn verify_digest(
        owner_name: &baml_base::Name,
        edge: &baml_base::Name,
        expected: baml_linker_types::Digest,
        record: &PackageRecord,
    ) -> Result<(), LinkError> {
        let found = baml_artifact::payload_digest(
            baml_artifact::ArtifactKind::PackageInterface,
            &record.interface_blob,
        )
        .map_err(|_| LinkError::MissingInterface {
            package: owner_name.clone(),
            edge: edge.clone(),
        })?;
        if found != expected {
            return Err(LinkError::InterfaceMismatch {
                package: owner_name.clone(),
                edge: edge.clone(),
                expected,
                found,
            });
        }
        Ok(())
    }
}
