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
    token::{Token, TokenIdx, TokenKind, TokenSet},
    tree::{AnyNodeId, Id, NodeKindOf, NodeUnion},
};
use text_size::TextSize;

use crate::{
    comments::{Comments, SourceComment, docs},
    ir::{Doc, Docs, display_width},
};

mod layout;
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
        self.with_comments(id, T::fmt(id, self))
    }

    /// `doc`, a layout of node `id` built by its parent's rule, with the
    /// node's leading and trailing comments around it.
    pub fn with_comments<T: NodeKindOf>(&self, id: Id<T>, doc: Doc<'a>) -> Doc<'a> {
        let any = AnyNodeId::of(id);
        self.docs.concat([
            docs::leading(self.docs, self.tree, self.comments.leading(any)),
            doc,
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
        docs::dangling(self.docs, self.tree, self.dangling_after(token), true)
    }

    /// [`Self::comments_after`] for an opening bracket nothing is spaced
    /// from — `(`, `[`, `<` — so a block comment inside it is not either:
    /// `f(/* c */)`.
    pub fn comments_inside(&self, token: TokenIdx) -> Doc<'a> {
        docs::dangling(self.docs, self.tree, self.dangling_after(token), false)
    }

    /// The leading comments of node `id`, for a rule that prints the node
    /// bare ([`Self::bare`]) and places its comments itself.
    pub fn leading_of(&self, id: AnyNodeId) -> Doc<'a> {
        docs::leading(self.docs, self.tree, self.comments.leading(id))
    }

    /// The trailing comments of node `id` (see [`Self::leading_of`]).
    pub fn trailing_of(&self, id: AnyNodeId) -> Doc<'a> {
        docs::trailing(self.docs, self.tree, self.comments.trailing(id))
    }

    /// Whether any comment dangles after `token`.
    pub fn has_comments_after(&self, token: TokenIdx) -> bool {
        !self.dangling_after(token).is_empty()
    }

    fn dangling_after(&self, token: TokenIdx) -> &'a [SourceComment] {
        self.index
            .token_owner(token)
            .map_or(&[][..], |owner| self.comments.dangling_after(owner, token))
    }

    /// The node's source text as written, the comments inside it included.
    /// Its later lines move with the node — they keep their indentation
    /// relative to the node's shallowest line — except inside a string
    /// literal, a template or a block comment, whose lines are content and
    /// stay exactly as they are.
    ///
    /// The base is the shallowest line, the one the node starts on included,
    /// rather than the start line alone: a node may start on a continuation
    /// line of its parent's layout (a body's `{` after a broken `throws`
    /// union), indented deeper than its own lines, and measuring from there
    /// would pull those lines left on the next pass.
    pub fn verbatim<T: NodeKindOf>(&self, id: Id<T>) -> Doc<'a> {
        let tree = self.tree;
        let tokens = tree.tokens();
        let text = tree.text();
        let span = tree.arenas().span(AnyNodeId::of(id));
        if span.is_empty() {
            return Doc::NIL;
        }
        let start = tokens.range(span.start()).start();
        let mut pieces = Vec::new();
        let mut piece_start = start;
        let mut token = span.start();
        while token < span.end() {
            let exact_end = match tokens.kind(token) {
                TokenKind::BacktickStart => Some(self.backtick_end(token)),
                TokenKind::String
                | TokenKind::RawString
                | TokenKind::ByteString
                | TokenKind::LegacyHashString
                | TokenKind::BlockComment
                    if tree.token_text(token).contains('\n') =>
                {
                    Some(TokenIdx(token.0 + 1))
                }
                _ => None,
            };
            let Some(end) = exact_end else {
                token = TokenIdx(token.0 + 1);
                continue;
            };
            let exact = tokens.text_range(baml_syntax::tree::TokenRange::new(token, end));
            pieces.push(Piece::Moved(
                &text[piece_start.into()..exact.start().into()],
            ));
            pieces.push(Piece::Exact(&text[exact]));
            piece_start = exact.end();
            token = end;
        }
        let end = tokens.text_range(span).end();
        pieces.push(Piece::Moved(&text[piece_start.into()..end.into()]));
        let source_indent = pieces
            .iter()
            .filter_map(|piece| match *piece {
                Piece::Moved(moved) => Some(moved),
                Piece::Exact(_) => None,
            })
            .flat_map(|moved| moved.split('\n').skip(1))
            .filter(|line| !line.trim_start_matches([' ', '\t']).is_empty())
            .map(indent_of)
            .fold(self.line_indent(start), u32::min);
        self.docs
            .concat(pieces.into_iter().map(|piece| match piece {
                Piece::Moved(moved) => self.docs.verbatim(moved, source_indent),
                Piece::Exact(exact) => self.docs.text(exact),
            }))
    }

    /// The indentation of the line `offset` is on: the width of its leading
    /// blanks.
    fn line_indent(&self, offset: TextSize) -> u32 {
        let text = self.tree.text();
        let line_start = text[..offset.into()].rfind('\n').map_or(0, |i| i + 1);
        indent_of(&text[line_start..])
    }

    /// One past the `BacktickEnd` that closes the `BacktickStart` at
    /// `start`: the end of the template that owns it, whose span covers the
    /// templates nested in its interpolations.
    fn backtick_end(&self, start: TokenIdx) -> TokenIdx {
        let template = self
            .index
            .token_owner(start)
            .unwrap_or_else(|| unreachable!("a template owns its opening backticks"));
        self.tree.arenas().span(template).end()
    }
}

/// A piece of a node's source text: one whose later lines move with the
/// node, or one that stays exactly as written.
#[derive(Debug, Clone, Copy)]
enum Piece<'a> {
    Moved(&'a str),
    Exact(&'a str),
}

/// The width of `line`'s leading blanks.
fn indent_of(line: &str) -> u32 {
    display_width(&line[..line.len() - line.trim_start_matches([' ', '\t']).len()])
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

            /// [`Self::any`] without the node's leading and trailing
            /// comments, for a rule that places them itself — inside a group
            /// the node starts, say ([`Self::leading_of`],
            /// [`Self::trailing_of`]).
            pub fn bare(&self, id: AnyNodeId) -> Doc<'a> {
                match id.typed() {
                    $(AnyId::$node(id) => <nodes::$node as FormatNode>::fmt(id, self),)*
                }
            }
        }
    };
}
baml_syntax::for_each_node_type!(dispatch);
