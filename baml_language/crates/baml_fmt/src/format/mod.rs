//! The rule layer: how each node of the tree becomes a document.
//!
//! Every node type implements [`FormatNode`], which builds the document for
//! the node's own tokens and children; the [`Formatter`] supplies what a rule
//! needs — the arena, the tree, the comments — and the helpers every rule
//! prints through. Those helpers are where the comments come from, so a rule
//! never handles them itself: [`Formatter::node`] puts a node's leading and
//! trailing comments around it, and [`Formatter::token`] prints the comments
//! that dangle after a token right after it. A rule that leaves a token out
//! (a trailing comma, a redundant parenthesis) hands that token's comments
//! over with [`Formatter::comments_after`].
//!
//! The dispatch over every node type is generated from the tree's own list,
//! so a node type without a rule does not compile. A rule that is not written
//! yet prints its node as written ([`Formatter::verbatim`]), comments inside
//! it included.
//!
//! The formatter prints only error-free trees: its tree is a
//! [`CompleteTree`], so a required part is read with `f.tree.present(…)` and
//! a `Bogus` arm is `f.tree.absent(bogus)`.

use baml_syntax::{
    complete::CompleteTree,
    index::TreeIndex,
    nodes::{self, AnyId},
    token::{Token, TokenIdx, TokenSet},
    tree::{AnyNodeId, Id, NodeKindOf, NodeUnion},
};

use crate::{
    comments::{Comments, docs},
    ir::{Doc, Docs},
};

mod rules;

/// What a rule builds its document with.
#[derive(Debug, Clone, Copy)]
pub struct Formatter<'a> {
    pub docs: Docs<'a>,
    pub tree: &'a CompleteTree<'a>,
    pub index: &'a TreeIndex,
    pub comments: &'a Comments,
}

/// The layout of one node type.
pub trait FormatNode: NodeKindOf + Sized {
    /// The document for the node's own tokens and children. Its leading and
    /// trailing comments are not its business: [`Formatter::node`] puts them
    /// around whatever this returns.
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a>;
}

impl<'a> Formatter<'a> {
    pub fn new(
        docs: Docs<'a>,
        tree: &'a CompleteTree<'a>,
        index: &'a TreeIndex,
        comments: &'a Comments,
    ) -> Self {
        Self {
            docs,
            tree,
            index,
            comments,
        }
    }

    /// The document for the whole file.
    pub fn file(&self) -> Doc<'a> {
        self.node(self.tree.root())
    }

    /// `id`'s layout, with its leading and trailing comments around it.
    pub fn node<T: FormatNode>(&self, id: Id<T>) -> Doc<'a> {
        let any = AnyNodeId::of(id);
        self.docs.concat([
            docs::leading(self.docs, self.tree, self.comments.leading(any)),
            T::fmt(id, self),
            docs::trailing(self.docs, self.tree, self.comments.trailing(any)),
        ])
    }

    /// [`Self::node`] for an alternative of a union whose alternatives are
    /// all nodes.
    pub fn alt(&self, alternative: impl NodeUnion + Copy) -> Doc<'a> {
        self.any(alternative.node_id())
    }

    /// A token as written, followed by the comments that dangle after it.
    pub fn token<S: TokenSet>(&self, token: Token<S>) -> Doc<'a> {
        self.docs.concat([
            self.docs.text(self.tree.token_text(token.idx)),
            self.comments_after(token.idx),
        ])
    }

    /// The comments dangling after `token`, for a rule that does not print
    /// the token itself to put where it would have been.
    pub fn comments_after(&self, token: TokenIdx) -> Doc<'a> {
        let comments = self
            .index
            .token_owner(token)
            .map_or(&[][..], |owner| self.comments.dangling_after(owner, token));
        docs::dangling(self.docs, self.tree, comments)
    }

    /// The node's source text as one piece, the comments inside it included.
    pub fn verbatim<T: NodeKindOf>(&self, id: Id<T>) -> Doc<'a> {
        let span = self.tree.arenas().span(AnyNodeId::of(id));
        let range = self.tree.tokens().text_range(span);
        self.docs.text(&self.tree.text()[range])
    }
}

/// `Formatter::any`: the dispatch over every node type, generated from the
/// tree's list of them so that a node type without a rule does not compile.
macro_rules! dispatch {
    ($($node:ident),* $(,)?) => {
        impl<'a> Formatter<'a> {
            /// [`Self::node`] for a node of any type.
            pub fn any(&self, id: AnyNodeId) -> Doc<'a> {
                match id.typed() {
                    $(AnyId::$node(id) => self.node::<nodes::$node>(id),)*
                }
            }
        }
    };
}
baml_syntax::for_each_node_type!(dispatch);
