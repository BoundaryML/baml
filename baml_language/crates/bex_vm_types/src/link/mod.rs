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
//! ([`relink::visit_object_operands`](crate::relink::visit_object_operands)).
//! The two share the format and its index arithmetic, not a driver: there is
//! nothing for a link to bind at run time except the graft itself.
//!
//! The link is one pipeline — bind dependency tables, lay out slots, resolve
//! global imports, lay out objects (interning generic values), resolve object
//! imports, assemble — with one relocation walk over an operand space and one
//! set of structural checks: an import of one kind never binds an export of
//! another, a package never exports one path twice, an unbindable dependency
//! or import is an error, and a linked image never carries two declarations
//! with one type tag.
//!
//! # Layout
//!
//! Group-major (every [`LinkGroup::Stdlib`] package, then every
//! [`LinkGroup::User`] package), pass-major within a group across its
//! packages in set order, then the group's package tails:
//!
//! ```text
//! objects:  [classes][enums][interfaces][aliases][code][init parts][test parts]
//! globals:  [functions + bodies][lets][init-part slots][test-part slots]
//! ```
//!
//! Init parts are ordered by package initialization order — a Kahn sort over
//! the group's edges with alphabetical ties — and test parts by package name.
//! A generic-function VALUE (`foo<int>` as data) is emitted once per unit and
//! interned here by `(base function's absolute slot, type args)` across code
//! and tails: the first copy in layout order is placed, every later copy is a
//! shadow whose references redirect to it.
//!
//! # The rendered views
//!
//! The image's flat name maps (`function_indices`, `function_global_indices`,
//! `let_global_indices`, `package_init_order`) are filled from the export
//! tables as RENDERED VIEWS for the consumers that legitimately start from a
//! user-supplied name; nothing in the link resolves through them.

use std::{
    collections::HashSet,
    ops::{Index, IndexMut},
};

use baml_base::Name;

use crate::{
    Program,
    unit::{CompilationUnit, InitTail, PackageRecord},
};

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
pub use error::{LinkError, TagCollision};
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

/// Which layout group a package belongs to — a property of its root kind,
/// never of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[deprecated(
    note = "the flat emitter's layout group, kept only for byte identity with that emitter; a package-major layout in set order replaces it when the flat emitter is deleted"
)]
pub enum LinkGroup {
    /// A standard-library package.
    Stdlib,
    /// A workspace or dependency package.
    User,
}

/// One package of a [`LinkSet`]: its unit, its records, and how it reaches
/// the other packages of the set.
#[derive(Clone, Debug)]
pub struct LinkPackage<'a> {
    /// The package's display spelling: `ProgramPackage::name`, the package
    /// half of the rendered name views, and the name the prelude is bound by
    /// at load. Display and boundary data — a package's link identity is its
    /// [`LinkPackageId`], and its executable identity its position in
    /// [`Program::packages`]. Two packages may not share a spelling
    /// ([`LinkError::DuplicateLinkName`]), or the rendered views would collide.
    pub name: Name,
    pub group: LinkGroup,
    /// The package's edge table: every direct dependency and prelude package
    /// by the name this package reaches it under.
    pub edges: Vec<(Name, LinkPackageId)>,
    pub unit: &'a CompilationUnit,
    pub record: &'a PackageRecord,
    pub tail: Option<&'a InitTail>,
}

/// Everything one link consumes: the packages of the world, in world order.
#[derive(Clone, Debug, Default)]
pub struct LinkSet<'a> {
    pub packages: Vec<LinkPackage<'a>>,
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

        let objects = Objects::new(set, &order, &slots, &globals)?;
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
        let mut spellings = HashSet::new();
        for package in &self.set.packages {
            if !spellings.insert(&package.name) {
                return Err(LinkError::DuplicateLinkName(package.name.to_string()));
            }
            if let Some((edge, id)) = package
                .edges
                .iter()
                .find(|(_, id)| id.0 as usize >= self.set.packages.len())
            {
                return Err(LinkError::invalid(format!(
                    "package `{}` edge `{edge}` names package {} of {}",
                    package.name,
                    id.0,
                    self.set.packages.len()
                )));
            }
        }
        Ok(())
    }
}
