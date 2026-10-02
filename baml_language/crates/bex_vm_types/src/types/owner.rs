//! What a class or enum declaration belongs to.

use crate::HeapPtr;

/// What a class or enum declaration belongs to. Interfaces and type aliases
/// always belong to a package (nothing mints them anonymously), so they
/// carry the package pointer directly.
#[derive(Clone, Debug)]
pub enum Owner {
    /// The package that declared it: a static package (assigned at load), or
    /// the runtime package that compiled it. A GC edge: reaching the
    /// declaration keeps its package — and so its globals and dependencies
    /// — alive.
    Package(HeapPtr),
    /// No package: a declaration minted through `reflect`, reachable only by
    /// reference (its name is display). It holds what a package would hold
    /// for it — the `Object::ImplRule`s witnessing the interfaces it
    /// implements, alive exactly as long as it is. A `with_types` view
    /// exports such a declaration under an [`EdgeKind::Anonymous`](
    /// super::EdgeKind::Anonymous) edge, and the compile seam gives it a
    /// root of its own.
    Anonymous { witnesses: Vec<HeapPtr> },
}

impl Owner {
    /// A declaration no package has claimed: every minted declaration's
    /// state, and a decoded compiled declaration's until the loader assigns
    /// its package.
    #[must_use]
    pub const fn anonymous() -> Self {
        Self::Anonymous {
            witnesses: Vec::new(),
        }
    }

    /// The package, if the declaration belongs to one.
    #[must_use]
    pub fn package(&self) -> Option<HeapPtr> {
        match self {
            Self::Package(package) => Some(*package),
            Self::Anonymous { .. } => None,
        }
    }

    /// The rules witnessing interfaces for an anonymous declaration. Empty
    /// for a package-owned one: its rules are its package's.
    #[must_use]
    pub fn witnesses(&self) -> &[HeapPtr] {
        match self {
            Self::Package(_) => &[],
            Self::Anonymous { witnesses } => witnesses,
        }
    }
}

impl Default for Owner {
    fn default() -> Self {
        Self::anonymous()
    }
}
