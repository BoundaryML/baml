//! The wire's dependency table: how an artifact — a package interface blob,
//! a compilation unit — locates the packages its rows and imports name.
//!
//! Packages have no global identity: a package is named only by the edges
//! that reach it, and two consumers may reach one package under different
//! names. So an artifact never names a foreign declaration by a spelling a
//! reader would have to look up; it names the declaration's package by a SLOT
//! in its own [`Locator`] table, and each locator is a path over edge tables
//! from the artifact's own package — a direct dependency by the edge the
//! artifact's package reaches it under, a transitive one by an edge of an
//! earlier slot. A reader binds the slots by walking its own edges, never by a
//! name lookup across its closure, and the digests bind what the compile
//! assumed (rustc's `crate_deps` table: the name locates, the hash binds).

use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};

/// SHA-256 over a package interface's payload: the surface a compile assumed
/// of a dependency, checked wherever the artifact is bound to a package.
pub type Digest = [u8; 32];

/// A slot in an artifact's dependency table. Slot `0` is the artifact's own
/// package and is never stored; slot `k >= 1` is `dependencies[k - 1]`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub struct DepSlot(pub u32);

impl DepSlot {
    /// The artifact's own package. As an IMPORT's slot it is meaningful only
    /// for a session submission, whose package already holds the declarations
    /// of earlier submissions; any other unit's own declarations are locals.
    pub const SELF: Self = Self(0);

    #[must_use]
    pub fn is_self(self) -> bool {
        self == Self::SELF
    }

    /// The slot for entry `index` of a dependency table.
    #[must_use]
    pub fn of_dependency_index(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("dependency tables fit u32"))
    }

    /// The dependency-table entry this slot names, or `None` for
    /// [`Self::SELF`].
    #[must_use]
    pub fn dependency_index(self) -> Option<usize> {
        self.0.checked_sub(1).map(|k| k as usize)
    }
}

/// One entry of an artifact's dependency table: how the artifact's own
/// package reaches the package bound to the slot, and what binds it there.
///
/// One entry per package ROOT — a package reached under two names is one
/// entry, located by the first edge found. The table is topologically
/// ordered: a transitive entry's `via` always names an earlier slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum Locator {
    /// A direct dependency: `edge` in the artifact's own edge table, bound
    /// only to a package whose interface payload has `digest` — the surface
    /// this compile read of it. The digest is Merkle over the dependency's
    /// own table, so it covers everything the dependency's interface names.
    Direct { edge: Name, digest: Digest },
    /// A language package under its fixed name in the artifact's own edge
    /// table. It has no payload by design: the toolchain pairing is the build
    /// fingerprint's.
    Prelude { edge: Name },
    /// A transitive root: `edge` in the edge table of the package bound to
    /// `via`. It carries no digest of its own — the parent's covers it.
    Transitive { via: DepSlot, edge: Name },
}

impl Locator {
    /// The edge name that locates the package, in the edge table of
    /// [`Self::via`].
    #[must_use]
    pub fn edge(&self) -> &Name {
        match self {
            Self::Direct { edge, .. } | Self::Prelude { edge } | Self::Transitive { edge, .. } => {
                edge
            }
        }
    }

    /// Whose edge table [`Self::edge`] is read from: the artifact's own for a
    /// direct or prelude entry, an earlier slot's for a transitive one.
    #[must_use]
    pub fn via(&self) -> DepSlot {
        match self {
            Self::Direct { .. } | Self::Prelude { .. } => DepSlot::SELF,
            Self::Transitive { via, .. } => *via,
        }
    }
}
