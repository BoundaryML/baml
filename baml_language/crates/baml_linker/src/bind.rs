//! Dependency binding: each slot of a unit's or tail's dependency table
//! bound to a package of the set, through the edge tables — the edge LOCATES
//! the package, its recorded interface fingerprint BINDS it (rustc's
//! `(name, crate hash)`): a unit compiled against one interface never links
//! against another.

use baml_linker_types::{DependencyEntry, PackageRecord};
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
    /// to `via`, and the entry's fingerprint must be the digest of that
    /// package's interface payload. The fingerprint is present exactly where
    /// there is a payload to check: a direct dependency carries one; a
    /// prelude package (no blob by design — the toolchain pairing is the
    /// build fingerprint's) and a transitive root (an interface the owner
    /// never read) carry none.
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
            let Some(edge) = self
                .set
                .package(parent)
                .edges
                .iter()
                .find(|edge| edge.name == entry.edge)
            else {
                return Err(LinkError::UnknownDependency {
                    package: owner_name.clone(),
                    edge: entry.edge.clone(),
                });
            };
            Self::verify_fingerprint(
                owner_name,
                entry,
                edge.kind,
                self.set.package(edge.target).record,
            )?;
            bound.push(edge.target);
        }
        Ok(bound)
    }

    fn verify_fingerprint(
        owner_name: &baml_base::Name,
        entry: &DependencyEntry,
        kind: EdgeKind,
        record: &PackageRecord,
    ) -> Result<(), LinkError> {
        let expected = match (kind, entry.fingerprint) {
            (EdgeKind::Prelude, None) => return Ok(()),
            (EdgeKind::Prelude, Some(_)) => {
                return Err(LinkError::invalid(format!(
                    "package `{owner_name}` records an interface fingerprint for the prelude \
                     package `{}`",
                    entry.edge
                )));
            }
            (EdgeKind::Declared, None) if entry.via.is_self() => {
                return Err(LinkError::invalid(format!(
                    "package `{owner_name}` records no interface fingerprint for its dependency \
                     `{}`",
                    entry.edge
                )));
            }
            (EdgeKind::Declared, None) => return Ok(()),
            (EdgeKind::Declared, Some(expected)) => expected,
            // A static link set holds packages reached under declared and
            // prelude names only; a view's re-export edge and an anonymous
            // declaration's edge exist on runtime package objects.
            (EdgeKind::ReExported | EdgeKind::Anonymous, _) => {
                return Err(LinkError::invalid(format!(
                    "package `{owner_name}` reaches `{}` through a runtime-only edge",
                    entry.edge
                )));
            }
        };
        let found = baml_artifact::payload_digest(
            baml_artifact::ArtifactKind::PackageInterface,
            &record.interface_blob,
        )
        .map_err(|_| LinkError::MissingInterface {
            package: owner_name.clone(),
            edge: entry.edge.clone(),
        })?;
        if found != expected {
            return Err(LinkError::InterfaceMismatch {
                package: owner_name.clone(),
                edge: entry.edge.clone(),
                expected,
                found,
            });
        }
        Ok(())
    }
}
