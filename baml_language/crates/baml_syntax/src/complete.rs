//! A tree with nothing missing: what an error-free parse promises, as a type.

use std::ops::Deref;

use crate::{
    nodes::{Arenas, FileTree, NodeKind},
    token::TokenIdx,
    tree::{AnyNodeId, Bogus, Id, Missing, TokenRange, Visitor},
};

/// A [`FileTree`] holding no `Missing` and no `Bogus`: every required
/// position is filled and every union alternative is a real node. The parser
/// promises this of an error-free parse; this type carries the promise to a
/// reader that handles only such trees, so that a required field is not a
/// check at every site but one walk at the boundary (the [`TryFrom`]) and two
/// places that spend it, [`present`](Self::present) and
/// [`absent`](Self::absent), whose impossible arms are the single home of the
/// argument. Reads go through to the tree.
#[derive(Debug, Clone, Copy)]
pub struct CompleteTree<'t>(&'t FileTree);

/// The tree holds a `Missing` or a `Bogus`: `node` is the first node found
/// with one in a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incomplete {
    pub node: AnyNodeId,
}

impl<'t> TryFrom<&'t FileTree> for CompleteTree<'t> {
    type Error = Incomplete;

    fn try_from(tree: &'t FileTree) -> Result<Self, Incomplete> {
        check(tree.arenas(), tree.root_id()).map(|()| Self(tree))
    }
}

impl Deref for CompleteTree<'_> {
    type Target = FileTree;

    fn deref(&self) -> &FileTree {
        self.0
    }
}

impl CompleteTree<'_> {
    /// A required part, which the tree has.
    pub fn present<T>(&self, part: Result<T, Missing>) -> T {
        part.unwrap_or_else(|_| unreachable!("a `CompleteTree` holds no `Missing`"))
    }

    /// A `Bogus` alternative, which the tree has none of: the arm for it is
    /// `tree.absent(bogus)`, and diverges.
    pub fn absent(&self, _bogus: Id<Bogus>) -> ! {
        unreachable!("a `CompleteTree` holds no `Bogus`")
    }
}

/// Whether the tree under `root` (itself included) holds no `Missing` and no
/// `Bogus`.
pub fn check(arenas: &Arenas, root: AnyNodeId) -> Result<(), Incomplete> {
    struct Completeness {
        current: AnyNodeId,
        found: Option<Incomplete>,
        pending: Vec<AnyNodeId>,
    }
    impl Visitor for Completeness {
        fn node(&mut self, _field: &'static str, id: AnyNodeId) {
            if id.kind() == NodeKind::Bogus {
                self.found = Some(Incomplete { node: self.current });
            } else {
                self.pending.push(id);
            }
        }
        fn token(&mut self, _field: &'static str, _token: TokenIdx) {}
        fn range(&mut self, _field: &'static str, _range: TokenRange) {}
        fn missing(&mut self, _field: &'static str, _missing: Missing) {
            self.found = Some(Incomplete { node: self.current });
        }
    }
    if root.kind() == NodeKind::Bogus {
        return Err(Incomplete { node: root });
    }
    let mut walk = Completeness {
        current: root,
        found: None,
        pending: vec![root],
    };
    while let Some(id) = walk.pending.pop() {
        walk.current = id;
        arenas.walk_node(id, &mut walk);
        if let Some(incomplete) = walk.found {
            return Err(incomplete);
        }
    }
    Ok(())
}
