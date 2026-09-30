//! Dependency binding: each slot of a unit's or tail's dependency table
//! bound to a package of the set, through the edge tables — the edge LOCATES
//! the package, its recorded interface fingerprint BINDS it (rustc's
//! `(name, crate hash)`): a unit compiled against one interface never links
//! against another.

use baml_linker_types::{Locator, PackageRecord};
use bex_vm_types::types::EdgeKind;

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
    /// to its `via`, and the entry's kind must be the edge's — a direct entry
    /// binds a declared edge whose package's interface payload has the
    /// recorded digest, a prelude entry a prelude edge, a transitive entry
    /// either (its parent's digest covers it). A static link set holds
    /// packages reached under declared and prelude names only; a view's
    /// re-export edge and an anonymous declaration's edge exist on runtime
    /// package objects.
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
            let Some(edge) = self
                .set
                .package(parent)
                .edges
                .iter()
                .find(|edge| edge.name == *locator.edge())
            else {
                // The edge is read in the PARENT's table: the owner's own for
                // a direct entry, an earlier slot's package for a transitive
                // one — so the package named is the one whose table lacks it.
                return Err(LinkError::UnknownDependency {
                    package: self.set.name(parent).clone(),
                    edge: locator.edge().clone(),
                });
            };
            let runtime_only = |what: &str| {
                LinkError::invalid(format!(
                    "package `{owner_name}` reaches `{}` through a runtime-only {what} edge",
                    locator.edge()
                ))
            };
            match (locator, edge.kind) {
                (Locator::Direct { edge: name, digest }, EdgeKind::Declared) => {
                    Self::verify_digest(
                        owner_name,
                        name,
                        *digest,
                        self.set.package(edge.target).record,
                    )?;
                }
                (Locator::Direct { edge: name, .. }, EdgeKind::Prelude) => {
                    return Err(LinkError::invalid(format!(
                        "package `{owner_name}` records an interface fingerprint for the prelude \
                         package `{name}`"
                    )));
                }
                (Locator::Prelude { .. }, EdgeKind::Prelude) => {}
                (Locator::Prelude { edge: name }, EdgeKind::Declared) => {
                    return Err(LinkError::invalid(format!(
                        "package `{owner_name}` records no interface fingerprint for its dependency \
                         `{name}`"
                    )));
                }
                (Locator::Transitive { .. }, EdgeKind::Declared | EdgeKind::Prelude) => {}
                (
                    Locator::Direct { .. } | Locator::Prelude { .. } | Locator::Transitive { .. },
                    EdgeKind::ReExported,
                ) => return Err(runtime_only("re-export")),
                (
                    Locator::Direct { .. } | Locator::Prelude { .. } | Locator::Transitive { .. },
                    EdgeKind::Anonymous,
                ) => return Err(runtime_only("anonymous-declaration")),
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
