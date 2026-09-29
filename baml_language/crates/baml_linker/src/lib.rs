//! The linker: binds [`CompilationUnit`]s to packages and lays them out into
//! one runnable [`Program`].
//!
//! This is the STATIC lane: every package a unit reaches is in the
//! [`LinkSet`], each dependency slot binds to one of them, and every import
//! resolves against the bound package's own export table. The runtime lane
//! has no linker — a `Package.compile` artifact is loaded by the graft, which
//! consumes the same unit format directly, resolves each import against the
//! live package objects it was compiled against, and relocates operands into
//! pointers with the same walk
//! ([`relink::visit_object_operands`](bex_vm_types::relink::visit_object_operands)).
//! The two share the format and its index arithmetic, not a driver: there is
//! nothing for a link to bind at run time except the graft itself.
//!
//! The link is one pipeline — bind dependency tables, lay out slots, resolve
//! global imports, lay out objects (interning generic values), resolve object
//! imports, assemble — with one relocation walk over an operand space and one
//! set of structural checks: an import of one kind never binds an export of
//! another, a package never exports one path twice, an unbindable dependency
//! or import is an error, and a direct dependency binds only to a package
//! whose interface payload carries the fingerprint the unit was compiled
//! against ([`LinkError::InterfaceMismatch`] — the edge locates, the
//! fingerprint binds).
//!
//! # Type tags
//!
//! The linker owns the image's type tags. A unit names every type by its
//! declaration's object operand (a head, a declaration key of a type switch,
//! a declaration's own tag field are all operands in the unit convention); the
//! link assigns each declaration `CLASS_BASE + its absolute object index`,
//! rewrites every head through the same relocation as every other operand,
//! and solves each type switch's hash table over the tags it assigned. Two
//! declarations cannot share a tag by construction.
//!
//! # Layout
//!
//! Package-major, in set order: each package's buckets, then its init part,
//! then its test part —
//!
//! ```text
//! objects:  [classes][enums][interfaces][aliases][code][init part][test part] per package
//! globals:  [functions + bodies][lets][init-part slots][test-part slots]      per package
//! ```
//!
//! — so a package's objects are one contiguous range and its position in the
//! set is its position in the image. Init parts EXECUTE in package
//! initialization order — a Kahn sort over the set's edges with alphabetical
//! ties — which `Program::init_order` records as package ordinals; placement
//! does not encode it. A generic-function VALUE (`foo<int>` as data) is
//! emitted once per unit and interned here by `(base function's absolute
//! slot, type args by declaration identity)` across code and tails: the first
//! copy in layout order is placed, every later copy is a shadow whose
//! references redirect to it.
//!
//! # No rendered views
//!
//! The image carries no name map. A consumer that legitimately starts from a
//! host-supplied name derives its view from the package tables at load
//! (`Program::rendered_callables`); nothing in the link renders a spelling.

use std::{
    collections::HashSet,
    ops::{Index, IndexMut},
};

use baml_base::Name;
use baml_linker_types::{CompilationUnit, InitTail, PackageRecord};
use bex_vm_types::Program;

mod assemble;
mod bind;
mod error;
mod layout;
mod order;
mod resolve;
mod space;
#[cfg(test)]
mod tests;

use assemble::Assembler;
pub use error::LinkError;
use order::{LayoutOrder, Objects, Slots};
use resolve::{Space, export_globals, export_objects};
use space::Resolved;

/// A package's position in a [`LinkSet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LinkPackageId(pub u32);

impl LinkPackageId {
    fn of(index: usize) -> Self {
        Self(u32::try_from(index).expect("link sets fit u32"))
    }
}

/// One package of a [`LinkSet`]: its unit, its records, and how it reaches
/// the other packages of the set.
#[derive(Clone, Debug)]
pub struct LinkPackage<'a> {
    /// The package's display spelling: `ProgramPackage::name`. Display data —
    /// a package's link identity is its [`LinkPackageId`], and its executable
    /// identity its position in
    /// [`Program::packages`](bex_vm_types::Program::packages). Names live on
    /// edges, so two packages may share a spelling.
    pub name: Name,
    /// The package's edge table: every direct dependency and prelude package
    /// by the name this package reaches it under. Each name reaches one
    /// package ([`LinkError::DuplicateEdge`]); the table is carried into the
    /// executable as the package's viewpoint.
    pub edges: Vec<LinkEdge>,
    pub unit: &'a CompilationUnit,
    pub record: &'a PackageRecord,
    pub tail: Option<&'a InitTail>,
}

/// Everything one link consumes: the packages of the world, in world order.
/// One entry of a [`LinkPackage`]'s edge table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkEdge {
    pub name: Name,
    pub target: LinkPackageId,
    pub kind: bex_vm_types::types::EdgeKind,
}

#[derive(Clone, Debug)]
pub struct LinkSet<'a> {
    pub packages: Vec<LinkPackage<'a>>,
    /// The package the program is compiled for — the world's root: the
    /// viewpoint a host's names resolve from.
    pub root: LinkPackageId,
}

impl<'a> LinkSet<'a> {
    fn ids(&self) -> impl Iterator<Item = LinkPackageId> + '_ {
        (0..self.packages.len()).map(LinkPackageId::of)
    }

    fn package(&self, id: LinkPackageId) -> &LinkPackage<'a> {
        &self.packages[id.0 as usize]
    }

    fn name(&self, id: LinkPackageId) -> &Name {
        &self.package(id).name
    }
}

/// Link a set of packages into a runnable program.
///
/// # Errors
///
/// Every [`LinkError`]. A unit that imports its own package is
/// [`LinkError::InvalidUnit`]: its own declarations are locals.
pub fn link(set: &LinkSet<'_>) -> Result<Program, LinkError> {
    Linker { set }.link()
}

/// One entry per package of the set, addressed by [`LinkPackageId`].
struct PerPackage<T>(Vec<T>);

impl<T> PerPackage<T> {
    /// Build one entry per package, in set order, stopping at the first error.
    fn try_from_set<'a>(
        set: &LinkSet<'a>,
        mut build: impl FnMut(LinkPackageId, &LinkPackage<'a>) -> Result<T, LinkError>,
    ) -> Result<Self, LinkError> {
        set.ids()
            .map(|id| build(id, set.package(id)))
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}

impl<T> Index<LinkPackageId> for PerPackage<T> {
    type Output = T;

    fn index(&self, id: LinkPackageId) -> &T {
        &self.0[id.0 as usize]
    }
}

impl<T> IndexMut<LinkPackageId> for PerPackage<T> {
    fn index_mut(&mut self, id: LinkPackageId) -> &mut T {
        &mut self.0[id.0 as usize]
    }
}

/// One link: the set it binds, driven phase by phase.
struct Linker<'s, 'a> {
    set: &'s LinkSet<'a>,
}

impl Linker<'_, '_> {
    fn link(self) -> Result<Program, LinkError> {
        self.validate()?;
        let set = self.set;
        let order = LayoutOrder::new(set)?;
        let tables = self.bind_all()?;

        let slots = Slots::new(set, &order)?;
        let global_exports = export_globals(set, &slots)?;
        let globals = self.resolve(&tables, &global_exports, Space::Global)?;

        let objects = Objects::new(set, &order, &slots, &globals, &tables)?;
        let object_exports = export_objects(set, &objects)?;
        let resolved_objects = self.resolve(&tables, &object_exports, Space::Object)?;
        let (units, tails) = Resolved::zip(resolved_objects, globals);

        let assembler = Assembler {
            set,
            order: &order,
            slots: &slots,
            objects: &objects,
            exports: object_exports,
            units,
            tails,
        };
        assembler.assemble()
    }

    fn validate(&self) -> Result<(), LinkError> {
        if self.set.root.0 as usize >= self.set.packages.len() {
            return Err(LinkError::invalid(format!(
                "the root package {} is not in the set of {}",
                self.set.root.0,
                self.set.packages.len()
            )));
        }
        for package in &self.set.packages {
            let mut edges = HashSet::new();
            for edge in &package.edges {
                if !edges.insert(&edge.name) {
                    return Err(LinkError::DuplicateEdge {
                        package: package.name.clone(),
                        edge: edge.name.clone(),
                    });
                }
                if edge.target.0 as usize >= self.set.packages.len() {
                    return Err(LinkError::invalid(format!(
                        "package `{}` edge `{}` names package {} of {}",
                        package.name,
                        edge.name,
                        edge.target.0,
                        self.set.packages.len()
                    )));
                }
            }
        }
        Ok(())
    }
}
