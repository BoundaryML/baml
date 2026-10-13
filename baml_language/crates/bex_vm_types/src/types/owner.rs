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
    /// exports such a declaration through an edge whose target is the
    /// declaration itself ([`Edge`](super::Edge)), and the compile seam gives
    /// it a root of its own.
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

impl Owner {
    /// How the declaration this owns is named, given its path — see
    /// [`declaration_name`].
    #[must_use]
    pub fn name_of(&self, path: &super::LocalName) -> crate::DeclarationName {
        declaration_name(self.package(), path)
    }
}

/// How the declaration at `path` is named, given the package that owns it.
///
/// A declaration carries only its own coordinates — its path inside its
/// package. The package half of its name is not the declaration's to state:
/// it is its package's display label
/// ([`Package::name`](super::Package::name)), read here off the package at
/// the moment a name is wanted. So no declaration stores a qualified name.
///
/// The result is for display, and no more identifying than the label is:
/// two declarations in two packages labelled alike render alike. A
/// declaration's identity is its tag, and its object.
///
/// With no package (`None`: a declaration minted through `reflect`, or a
/// compiled one the loader has not yet placed), the name is the path alone.
#[must_use]
pub fn declaration_name(
    package: Option<HeapPtr>,
    path: &super::LocalName,
) -> crate::DeclarationName {
    let Some(package) = package else {
        return crate::DeclarationName::Anonymous(baml_base::Name::new(path.to_string()));
    };
    // SAFETY: a declaration's package is a GC edge of the declaration —
    // traced and forwarded with it — so it is live wherever the declaration
    // is, which the caller's heap permit (or the compile-time region) holds.
    // Read-only: a package's display name does not change after it is
    // allocated.
    #[expect(
        unsafe_code,
        reason = "a declaration's name reads its package's display name"
    )]
    let crate::Object::Package(package) = (unsafe { package.get() }) else {
        unreachable!("a declaration's owner edge points at a package")
    };
    crate::DeclarationName::Declared(path.under(&package.name))
}

impl Default for Owner {
    fn default() -> Self {
        Self::anonymous()
    }
}
