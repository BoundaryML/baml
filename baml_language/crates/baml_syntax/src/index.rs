//! Links a finished tree's fields do not give: a node's parent, the node a
//! token belongs to, the node covering a range. The compiler reads trees
//! downward and never needs them; the IDE does, so they are built on demand
//! in one non-recursive pass over the tree and kept beside it.

use text_size::TextRange;

use crate::{
    nodes::{Arenas, FileTree, NodeKind},
    token::{TokenAtOffset, TokenIdx, Tokens},
    tree::{AnyNodeId, Missing, TokenRange, Visitor},
};

/// "No node": the root's parent, a trivia token's owner. A kind no node type
/// has, so it can never collide with a real id.
const NONE: AnyNodeId = AnyNodeId::from_raw_parts(u16::MAX, u32::MAX);

/// One pass over the tree: records each node's parent and each token's
/// owner as it reaches them.
struct Build {
    current: AnyNodeId,
    parents: Vec<Box<[AnyNodeId]>>,
    token_owner: Vec<AnyNodeId>,
    pending: Vec<AnyNodeId>,
}
impl Visitor for Build {
    fn node(&mut self, _field: &'static str, id: AnyNodeId) {
        self.parents[usize::from(id.kind_raw())][id.raw() as usize] = self.current;
        self.pending.push(id);
    }
    fn token(&mut self, _field: &'static str, token: TokenIdx) {
        self.token_owner[token.index()] = self.current;
    }
    fn range(&mut self, _field: &'static str, range: TokenRange) {
        for token in range.iter() {
            self.token_owner[token.index()] = self.current;
        }
    }
    fn missing(&mut self, _field: &'static str, _missing: Missing) {}
}

/// One node's child nodes, for the pre-order walk.
struct Children<'c>(&'c mut Vec<AnyNodeId>);
impl Visitor for Children<'_> {
    fn node(&mut self, _field: &'static str, id: AnyNodeId) {
        self.0.push(id);
    }
    fn token(&mut self, _field: &'static str, _token: TokenIdx) {}
    fn range(&mut self, _field: &'static str, _range: TokenRange) {}
    fn missing(&mut self, _field: &'static str, _missing: Missing) {}
}

/// Parent and owner links over one [`FileTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeIndex {
    /// `parents[kind][index]`: each node's parent; [`NONE`] for the root.
    parents: Box<[Box<[AnyNodeId]>]>,
    /// Each token's owning node; [`NONE`] for trivia and for a token no node
    /// owns (possible only in a parse with errors).
    token_owner: Box<[AnyNodeId]>,
}

impl TreeIndex {
    /// Index `tree`: O(nodes + tokens), no recursion.
    pub fn build(tree: &FileTree) -> Self {
        let arenas = tree.arenas();
        let mut build = Build {
            current: tree.root_id(),
            parents: NodeKind::ALL
                .iter()
                .map(|&kind| vec![NONE; arenas.len_of(kind)].into_boxed_slice())
                .collect(),
            token_owner: vec![NONE; tree.tokens().len()],
            pending: vec![tree.root_id()],
        };
        while let Some(id) = build.pending.pop() {
            build.current = id;
            arenas.walk_node(id, &mut build);
        }
        Self {
            parents: build.parents.into_boxed_slice(),
            token_owner: build.token_owner.into_boxed_slice(),
        }
    }

    /// Bytes the index holds on the heap.
    pub fn heap_bytes(&self) -> usize {
        self.parents
            .iter()
            .map(|arena| size_of_val::<[AnyNodeId]>(arena))
            .sum::<usize>()
            + size_of_val::<[AnyNodeId]>(&self.token_owner)
    }

    /// Whether this index was built from a tree of `tree`'s shape — the guard
    /// against reading one tree's index against another.
    pub fn is_for(&self, tree: &FileTree) -> bool {
        self.token_owner.len() == tree.tokens().len()
            && NodeKind::ALL.iter().all(|&kind| {
                self.parents[usize::from(kind.to_raw())].len() == tree.arenas().len_of(kind)
            })
    }

    /// The node `id` is a field of; `None` for the root.
    pub fn parent(&self, id: AnyNodeId) -> Option<AnyNodeId> {
        let parent = self
            .parents
            .get(usize::from(id.kind_raw()))
            .and_then(|arena| arena.get(id.raw() as usize))
            .copied()
            .unwrap_or_else(|| unreachable!("`id` is a node of the indexed tree"));
        (parent != NONE).then_some(parent)
    }

    /// `id`, its parent, and so on up to the root.
    pub fn ancestors(&self, id: AnyNodeId) -> impl Iterator<Item = AnyNodeId> + '_ {
        std::iter::successors(Some(id), move |&node| self.parent(node))
    }

    /// The node owning `token`; `None` for trivia, and for a token no node
    /// owns in a parse with errors.
    pub fn token_owner(&self, token: TokenIdx) -> Option<AnyNodeId> {
        let owner = *self
            .token_owner
            .get(token.index())
            .unwrap_or_else(|| unreachable!("`token` is a token of the indexed tree"));
        (owner != NONE).then_some(owner)
    }

    /// The deepest node whose span covers `range`: the nearest common
    /// ancestor of the owners of the owned tokens at its two ends. A range
    /// lying in trivia (or empty) is covered by the node spanning the owned
    /// tokens on either side of it; with no owned token on either side, the
    /// root.
    pub fn covering_node(&self, tree: &FileTree, range: TextRange) -> AnyNodeId {
        debug_assert!(self.is_for(tree), "an index is read against its own tree");
        let tokens = tree.tokens();
        let first = self.owned_at_or_after(tokens, range.start());
        let last = self.owned_at_or_before(tokens, range.end());
        match (
            first.and_then(|t| self.token_owner(t)),
            last.and_then(|t| self.token_owner(t)),
        ) {
            (Some(a), Some(b)) => self.common_ancestor(a, b),
            (Some(node), None) | (None, Some(node)) => node,
            (None, None) => tree.root_id(),
        }
    }

    /// The nearest ancestor `a` and `b` share (either one, when it is an
    /// ancestor of the other).
    pub fn common_ancestor(&self, a: AnyNodeId, b: AnyNodeId) -> AnyNodeId {
        let above_a: Vec<AnyNodeId> = self.ancestors(a).collect();
        self.ancestors(b)
            .find(|node| above_a.contains(node))
            .unwrap_or_else(|| unreachable!("every node descends from the one root"))
    }

    /// The first owned token ending after `offset`.
    fn owned_at_or_after(&self, tokens: &Tokens, offset: text_size::TextSize) -> Option<TokenIdx> {
        let mut token = match tokens.token_at_offset(offset) {
            TokenAtOffset::None => return None,
            TokenAtOffset::Single(token) | TokenAtOffset::Between { right: token, .. } => token,
        };
        while self.token_owner(token).is_none() {
            token = tokens.next_significant(token)?;
        }
        Some(token)
    }

    /// The last owned token starting before `offset`.
    fn owned_at_or_before(&self, tokens: &Tokens, offset: text_size::TextSize) -> Option<TokenIdx> {
        let mut token = match tokens.token_at_offset(offset) {
            TokenAtOffset::None => TokenIdx(u32::try_from(tokens.len()).ok()?.checked_sub(1)?),
            TokenAtOffset::Between { left, .. } => left,
            TokenAtOffset::Single(token) => token,
        };
        // `Single` at the first token's start: that token lies after `offset`.
        if tokens.range(token).start() >= offset {
            token = tokens.prev_significant(token)?;
        }
        while self.token_owner(token).is_none() {
            token = tokens.prev_significant(token)?;
        }
        Some(token)
    }
}

/// `id` and every node under it, in source (pre-)order.
pub fn descendants(tree: &FileTree, id: AnyNodeId) -> Descendants<'_> {
    Descendants {
        arenas: tree.arenas(),
        stack: vec![id],
        children: Vec::new(),
    }
}

/// See [`descendants`].
pub struct Descendants<'t> {
    arenas: &'t Arenas,
    stack: Vec<AnyNodeId>,
    /// Scratch for one node's children, reused across steps.
    children: Vec<AnyNodeId>,
}

impl Iterator for Descendants<'_> {
    type Item = AnyNodeId;
    fn next(&mut self) -> Option<AnyNodeId> {
        let id = self.stack.pop()?;
        self.children.clear();
        self.arenas.walk_node(id, &mut Children(&mut self.children));
        self.stack.extend(self.children.drain(..).rev());
        Some(id)
    }
}
