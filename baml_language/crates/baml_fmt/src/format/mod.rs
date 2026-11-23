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
    FormatOptions,
    comments::{
        CommentKind, Comments, SourceComment,
        docs::{self, After},
    },
    ir::{Doc, Docs, VerbatimPiece, display_width},
};

mod layout;
mod rules;

pub use layout::ListItem;
pub(crate) use rules::{control::diverges, exprs::ends_optional_chain};

/// What a rule builds its document with.
#[derive(Debug, Clone, Copy)]
pub struct Formatter<'a> {
    pub docs: Docs<'a>,
    pub tree: &'a CompleteTree<'a>,
    pub index: &'a TreeIndex,
    pub comments: &'a Comments,
    /// One level of indentation, for a rule that lays out a literal's lines
    /// itself.
    pub indent_width: usize,
    /// The widths past which a construct breaks though it would fit.
    pub heuristics: Heuristics,
}

/// The widths past which a construct that would fit on its line breaks
/// anyway: rustfmt's small heuristics (`use_small_heuristics = Default`),
/// each the share of the line width that rustfmt takes of its `max_width`,
/// and measured as rustfmt measures them — the items between the brackets,
/// the chain, the whole `if … else`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heuristics {
    /// `fn_call_width`: a call's, or a tuple's, arguments.
    pub call_args: u32,
    /// `attr_fn_like_width`: an attribute's arguments.
    pub attr_args: u32,
    /// `struct_lit_width`: an object literal's, or a class pattern's,
    /// fields.
    pub object_fields: u32,
    /// `array_width`: an array's elements — and a map's entries, a literal
    /// rustfmt has no counterpart of.
    pub array_elements: u32,
    /// `chain_width`: a member chain of two or more calls.
    pub chain: u32,
    /// `single_line_if_else_max_width`: an `if … else` kept on one line.
    pub if_else: u32,
}

impl Heuristics {
    pub fn of(options: FormatOptions) -> Self {
        let share = |percent: u64| {
            let width = u64::try_from(options.line_width).unwrap_or(u64::MAX);
            u32::try_from(width.saturating_mul(percent) / 100).unwrap_or(u32::MAX)
        };
        Self {
            call_args: share(60),
            attr_args: share(70),
            object_fields: share(18),
            array_elements: share(60),
            chain: share(60),
            if_else: share(50),
        }
    }
}

/// `comments`, a run between two tokens, split into the ones on the first
/// token's line and the ones on lines of their own after it, before the
/// second. A run of comments on one line goes together, so the split is
/// before the first comment with a line break before it — not a partition
/// by that gap, which the second comment of a run on a line of its own
/// lacks.
pub(crate) fn split_at_line_break(
    comments: &[SourceComment],
) -> (&[SourceComment], &[SourceComment]) {
    let own_line = comments
        .iter()
        .position(|comment| comment.before.is_line_break())
        .unwrap_or(comments.len());
    comments.split_at(own_line)
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
        options: FormatOptions,
    ) -> Self {
        Self {
            docs,
            tree,
            index,
            comments,
            indent_width: options.indent_width,
            heuristics: Heuristics::of(options),
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
    /// It starts a new line when a line comment is pending on the current
    /// one: nothing a rule prints this way goes in front of a comment it
    /// followed in the source. Punctuation the formatter owns — a separator
    /// it makes canonical, a bracket that cannot start a line — is printed
    /// as text instead, and tucks in before such a comment.
    pub fn token<S: TokenSet>(&self, token: Token<S>) -> Doc<'a> {
        self.token_at(token.idx)
    }

    /// [`Self::token`] by index, for a token a rule reaches without its
    /// marker type.
    pub fn token_at(&self, token: TokenIdx) -> Doc<'a> {
        self.spelled(self.tree.token_text(token), token)
    }

    /// [`Self::token`] with a canonical spelling in place of the token's
    /// own: `->` for a `=>` arrow, `implements` for `implement`, an
    /// operator's spelling for its tokens.
    pub fn spelled(&self, text: &'a str, token: TokenIdx) -> Doc<'a> {
        self.docs.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            self.docs.text(text),
            self.comments_after(token),
        ])
    }

    /// The comments dangling after `token`, for a rule that does not print
    /// the token itself to put where it would have been.
    pub fn comments_after(&self, token: TokenIdx) -> Doc<'a> {
        docs::dangling(
            self.docs,
            self.tree,
            self.dangling_after(token),
            After::Token,
        )
    }

    /// [`Self::comments_after`] for an opening bracket nothing is spaced
    /// from — `(`, `[`, `<` — so a block comment inside it is not either:
    /// `f(/* c */)`.
    pub fn comments_inside(&self, token: TokenIdx) -> Doc<'a> {
        docs::dangling(
            self.docs,
            self.tree,
            self.dangling_after(token),
            After::Opener { spaced: false },
        )
    }

    /// `comments`, in source order, carried past the construct they were
    /// written in, as one run after it (see [`docs::closing`]): a type
    /// suffix's, which cannot start a line, or a prefix operator's, which
    /// must not end one. One run, so that each comment is carried past the
    /// whole construct on the first pass.
    pub fn comments_past(&self, comments: impl IntoIterator<Item = SourceComment>) -> Doc<'a> {
        let comments: Vec<SourceComment> = comments.into_iter().collect();
        docs::closing(self.docs, self.tree, &comments)
    }

    /// Whether the comments dangling after an opening bracket end with a
    /// block comment that ended its line in the source — printed in place
    /// after the bracket, it needs separating from what follows, which the
    /// bracket's layout supplies (see [`Self::line_after_opener`]).
    pub fn opener_comment_ended_line(&self, token: TokenIdx) -> bool {
        self.dangling_after(token).last().is_some_and(|comment| {
            comment.kind == CommentKind::Block && comment.after.is_line_break()
        })
    }

    /// The soft line between an unspaced opening bracket's comments and the
    /// first thing inside the bracket: a space when flat if a block comment
    /// there ended its line in the source (`g(/* c */ 1)`), else nothing.
    pub fn line_after_opener(&self, token: TokenIdx) -> Doc<'a> {
        if self.opener_comment_ended_line(token) {
            Doc::SOFT_LINE_OR_SPACE
        } else {
            Doc::SOFT_LINE
        }
    }

    /// [`Self::line_after_opener`] where the bracket's contents never break
    /// from it: a space or nothing.
    pub fn space_after_opener(&self, token: TokenIdx) -> Doc<'a> {
        if self.opener_comment_ended_line(token) {
            Doc::SPACE
        } else {
            Doc::NIL
        }
    }

    /// [`Self::comments_after`] for an opening brace, whose contents are
    /// spaced from it: `{ /* c */ }`.
    pub fn comments_after_opener(&self, token: TokenIdx) -> Doc<'a> {
        docs::dangling(
            self.docs,
            self.tree,
            self.dangling_after(token),
            After::Opener { spaced: true },
        )
    }

    /// [`Self::token`] for a keyword directly before a brace or a
    /// parenthesised head of its own — `else`, `defer`, `catch`, a bare
    /// `spawn`: after a line comment on the keyword's line what follows
    /// starts at the keyword's indent, so the body's `{` lines up with its
    /// `}`.
    pub fn keyword<S: TokenSet>(&self, token: Token<S>) -> Doc<'a> {
        self.docs.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            self.docs.text(self.tree.token_text(token.idx)),
            docs::dangling(
                self.docs,
                self.tree,
                self.dangling_after(token.idx),
                After::Keyword,
            ),
        ])
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

    /// The trailing comments of node `id` in two documents: those on its
    /// line, and those on lines of their own after it. A rule that prints
    /// the node somewhere other than where it was written keeps the first
    /// with it and leaves the second where the node was.
    pub fn trailing_split(&self, id: AnyNodeId) -> (Doc<'a>, Doc<'a>) {
        let (on_line, own_line) = split_at_line_break(self.comments.trailing(id));
        (
            docs::trailing(self.docs, self.tree, on_line),
            docs::trailing(self.docs, self.tree, own_line),
        )
    }

    /// `doc`, which follows node `before` in its construct, one level in
    /// when `before`'s trailing comments end its line: a `requires` clause
    /// or a `-> R` tail that cannot follow a line comment on its line then
    /// starts the next one under the construct it continues, and what it
    /// hangs under itself goes one level further.
    pub fn continuation(&self, before: AnyNodeId, doc: Doc<'a>) -> Doc<'a> {
        if self.trailing_of(before).will_break() {
            self.docs.indent(doc)
        } else {
            doc
        }
    }

    /// Whether a comment lies anywhere inside node `id`.
    pub fn has_comments_inside(&self, id: AnyNodeId) -> bool {
        let tokens = self.tree.tokens();
        self.tree
            .arenas()
            .span(id)
            .iter()
            .any(|token| crate::comments::is_comment(tokens.kind(token)))
    }

    /// The source text of node `id`, its comments included.
    pub fn node_text(&self, id: AnyNodeId) -> &'a str {
        let span = self.tree.arenas().span(id);
        &self.tree.text()[self.tree.tokens().text_range(span)]
    }

    /// Whether any comment dangles after `token`.
    pub fn has_comments_after(&self, token: TokenIdx) -> bool {
        !self.dangling_after(token).is_empty()
    }

    /// The comments dangling after `token`, for a rule that prints them
    /// itself (see [`Self::comments_after`]).
    pub fn dangling_after(&self, token: TokenIdx) -> &'a [SourceComment] {
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
            pieces.push(VerbatimPiece::Moved(
                &text[piece_start.into()..exact.start().into()],
            ));
            pieces.push(VerbatimPiece::Exact(&text[exact]));
            piece_start = exact.end();
            token = end;
        }
        let end = tokens.text_range(span).end();
        pieces.push(VerbatimPiece::Moved(&text[piece_start.into()..end.into()]));
        let source_indent = pieces
            .iter()
            .filter_map(|piece| match *piece {
                VerbatimPiece::Moved(moved) => Some(moved),
                VerbatimPiece::Exact(_) => None,
            })
            .flat_map(|moved| moved.split('\n').skip(1))
            .filter(|line| !line.trim_start_matches([' ', '\t']).is_empty())
            .map(indent_of)
            .fold(self.line_indent(start), u32::min);
        self.docs.verbatim_pieces(pieces, source_indent)
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
            /// comments, for a rule that places them itself — inside the
            /// indentation of a chain the node starts, say
            /// ([`Self::leading_of`], [`Self::trailing_of`]).
            pub fn bare(&self, id: AnyNodeId) -> Doc<'a> {
                match id.typed() {
                    $(AnyId::$node(id) => <nodes::$node as FormatNode>::fmt(id, self),)*
                }
            }
        }
    };
}
baml_syntax::for_each_node_type!(dispatch);
