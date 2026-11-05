//! Lowering from the typed syntax tree (`baml_syntax`) to the AST arenas:
//! the same `ExprBody`/`TypeExpr` the rowan lowering (`lower_expr_body`,
//! `lower_type_expr`) produces, read off typed fields instead of searched for
//! by node kind. Developed construct by construct beside the rowan lowering
//! and held equal to it by the per-body oracle in `baml_tests::ast_dump`; the
//! rowan lowering is deleted once every construct is ported.
//!
//! Spans are the node's own: a node's span never includes trivia, a postfix
//! modifier spans the type it modifies plus itself. (The rowan lowering gave a
//! modifier chain one span; `TypeExpr` equality ignores spans, so the oracle
//! does not see the difference, and the diagnostics tiers will show it as a
//! reviewable change at the switch.)

use baml_base::Name;
use baml_syntax::{
    nodes::{self, FileTree},
    token::TokenIdx,
    tree::{AnyNode, AnyNodeId, Id, Missing, NodeKindOf},
};
use text_size::TextRange;

pub mod body;
pub mod templates;
pub mod types;

pub use body::lower_body;

/// A read-only view of the tree being lowered: the lookups every lowering
/// function needs, named for what they return.
#[derive(Clone, Copy)]
pub struct View<'t> {
    pub tree: &'t FileTree,
}

impl<'t> View<'t> {
    pub fn new(tree: &'t FileTree) -> Self {
        Self { tree }
    }

    pub(crate) fn node<T: NodeKindOf>(self, id: Id<T>) -> &'t T
    where
        nodes::Arenas: baml_syntax::tree::HasArena<T>,
    {
        self.tree.get(id)
    }

    /// The text of a node: its tokens from first to last, trivia inside
    /// included, none outside.
    pub(crate) fn range(self, id: AnyNodeId) -> TextRange {
        self.tree.tokens().text_range(self.tree.arenas().span(id))
    }

    pub(crate) fn range_of<T: NodeKindOf>(self, id: Id<T>) -> TextRange {
        self.range(AnyNodeId::of(id))
    }

    /// The range of a union value; a union's every alternative is a node.
    pub(crate) fn range_any(self, node: &impl AnyNode) -> TextRange {
        self.range(
            node.any_node_id()
                .unwrap_or_else(|| unreachable!("every alternative of the union is a node")),
        )
    }

    pub(crate) fn token_range(self, token: TokenIdx) -> TextRange {
        self.tree.tokens().range(token)
    }

    pub(crate) fn text(self, token: TokenIdx) -> &'t str {
        self.tree.token_text(token)
    }

    /// The source text a node covers.
    pub(crate) fn text_of(self, id: AnyNodeId) -> &'t str {
        &self.tree.text()[self.range(id)]
    }

    pub(crate) fn name(self, id: Id<nodes::Name>) -> Name {
        Name::new(self.text(self.node(id).token.idx))
    }

    pub(crate) fn name_range(self, id: Id<nodes::Name>) -> TextRange {
        self.token_range(self.node(id).token.idx)
    }

    /// Where something that is not there would have been.
    pub(crate) fn missing_range(missing: Missing) -> TextRange {
        TextRange::empty(missing.at())
    }
}
