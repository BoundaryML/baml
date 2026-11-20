//! The layouts rules share: a declaration's docs, a braced body of members,
//! a delimited list, a separator. Every width decision in them is a group;
//! the rules add none of their own.

use baml_syntax::{
    Token,
    nodes::{AnyDoc, AnyId, AnyStmt, Block},
    token::{Token, TokenIdx, TokenSet, punct},
    tree::{AnyNodeId, Id, List, NodeUnion, SepItem},
};

use super::{
    Formatter,
    rules::control::{diverges, statement},
};
use crate::{
    comments::{CommentKind, SourceComment, gap_before},
    ir::{Doc, Docs, GroupId},
};

/// One item of a bracketed list, its document built by the caller.
#[derive(Debug, Clone, Copy)]
pub struct ListItem<'a> {
    pub doc: Doc<'a>,
    /// The separator written after it, if any, whose comments are kept.
    pub comma: Option<TokenIdx>,
    /// Whether the source left a blank line before it: kept, one at most,
    /// and the list then breaks — as between the statements of a block.
    pub blank_before: bool,
}

/// Whether a node ends in braces of its own that can break instead of the
/// construct around it: a block, an `if`, a `match`, a `catch`, a lambda or
/// a `spawn`. A right-hand side like that stays on the `=` line, and an
/// operator after one stays on its `}` line.
pub(super) fn braced(id: AnyNodeId) -> bool {
    matches!(
        id.typed(),
        AnyId::Block(_)
            | AnyId::IfExpr(_)
            | AnyId::IfLetExpr(_)
            | AnyId::MatchExpr(_)
            | AnyId::CatchExpr(_)
            | AnyId::LambdaExpr(_)
            | AnyId::SpawnExpr(_)
    )
}

/// An [`EXPAND_PARENT`](Doc::EXPAND_PARENT) when an item but the last
/// [spans lines](Doc::spans_lines): the item after it would land on its
/// closing line, so the list breaks one item per line instead. Nothing when
/// only the last does — `f(a, `…`)` hugs it — or none does.
pub(super) fn multi_line_before_last<'a>(docs: &[Doc<'a>]) -> Doc<'a> {
    match docs.split_last() {
        Some((_, init)) if init.iter().any(|doc| doc.spans_lines()) => Doc::EXPAND_PARENT,
        _ => Doc::NIL,
    }
}

/// `doc` within `max_width` columns when there is a limit (see
/// [`Docs::limited`]), as it is without one.
fn within<'a>(d: Docs<'a>, doc: Doc<'a>, max_width: Option<u32>) -> Doc<'a> {
    match max_width {
        Some(max_width) => d.limited(doc, max_width),
        None => doc,
    }
}

/// [`multi_line_before_last`] over list items.
fn multi_line_item_before_last<'a>(items: &[ListItem<'a>]) -> Doc<'a> {
    match items.split_last() {
        Some((_, init)) if init.iter().any(|item| item.doc.spans_lines()) => Doc::EXPAND_PARENT,
        _ => Doc::NIL,
    }
}

impl<'a> Formatter<'a> {
    /// The line break before `node` when it starts a line of its own: a
    /// blank line if the source had one before it (its leading comments
    /// included), else just the break.
    pub fn line_before(&self, node: AnyNodeId) -> Doc<'a> {
        if self
            .comments
            .gap_before_node(self.tree, node)
            .has_blank_line()
        {
            Doc::EMPTY_LINE
        } else {
            Doc::HARD_LINE
        }
    }

    /// [`Self::line_before`] for a line that starts with the node's own
    /// `token`.
    pub fn line_before_token(&self, token: TokenIdx) -> Doc<'a> {
        if gap_before(self.tree.tokens(), token).has_blank_line() {
            Doc::EMPTY_LINE
        } else {
            Doc::HARD_LINE
        }
    }

    /// A declaration's `///` lines, directives and title, each on a line of
    /// its own before it, with the blank lines between them kept (one at
    /// most) and the one after the last kept likewise; `first` is the
    /// declaration's first token after them. Nothing for a declaration with
    /// no docs.
    pub fn docs(&self, docs: List<AnyDoc>, first: TokenIdx) -> Doc<'a> {
        let docs = self.tree.list(docs);
        let d = self.docs;
        d.concat(docs.iter().enumerate().map(|(i, &doc)| {
            let line_after = match docs.get(i + 1) {
                Some(next) => self.line_before(next.node_id()),
                None => self.line_before_token(first),
            };
            d.concat([self.alt(doc), line_after])
        }))
    }

    /// `{ … }` around `members`, one per line, indented, with the blank
    /// lines between them kept (one at most) and none at the edges; `{}`
    /// when there are no members and no comments inside.
    pub fn body(
        &self,
        node: AnyNodeId,
        l: Token!["{"],
        members: &[impl NodeUnion + Copy],
        r: Token!["}"],
    ) -> Doc<'a> {
        let items: Vec<AnyNodeId> = members.iter().map(NodeUnion::node_id).collect();
        self.braced(node, l, &items, r, false)
    }

    /// `{ … }` around `items`, each printed by [`Formatter::any`]: one per
    /// line, indented, with the blank lines between them kept (one at most)
    /// and none at the edges; `{}` when there are no items and no comments
    /// inside. With `flat`, a single item may stay on the braces' line when
    /// it fits (`{ x }`). The comments after `{` are printed inside the
    /// indentation, with the items. A comment that ended the line before `{`
    /// still does: the brace then starts the next line, as it did in the
    /// source.
    pub fn braced(
        &self,
        node: AnyNodeId,
        l: Token!["{"],
        items: &[AnyNodeId],
        r: Token!["}"],
        flat: bool,
    ) -> Doc<'a> {
        let items: Vec<(AnyNodeId, Doc<'a>)> = items.iter().map(|&id| (id, self.any(id))).collect();
        let doc = self.braced_items(node, l, &items, r, flat);
        if flat { self.docs.group(doc) } else { doc }
    }

    /// [`Self::braced`] over items already printed and without the group a
    /// flat layout gets, so that two layouts of one block (flat and broken)
    /// share them and a layout deciding several pairs of braces at once (an
    /// `if` chain breaks every block or none) adds one group of its own.
    /// Braces with nothing inside print as `{}`; with only comments, the
    /// comments go inside them on the braces' line when they fit there
    /// (`{ /* c */ }`), else the closing brace takes a line of its own.
    ///
    /// A broken body — items on lines of their own — is a group of its own,
    /// forced by those lines: a forced group prints in break mode even
    /// inside a layout that was chosen flat (a member chain's, a hugged
    /// lambda's), so that what follows an item on its line — an arm's body
    /// after its `=>`, a `let`'s initialiser — is measured up to its first
    /// possible line break, as anywhere else, rather than whole and flat.
    pub fn braced_items(
        &self,
        node: AnyNodeId,
        l: Token!["{"],
        items: &[(AnyNodeId, Doc<'a>)],
        r: Token!["}"],
        flat: bool,
    ) -> Doc<'a> {
        let d = self.docs;
        let open_comments = self.comments.dangling_after(node, l.idx);
        if items.is_empty() {
            if open_comments.is_empty() {
                return d.concat([Doc::LINE_SUFFIX_BOUNDARY, d.text("{"), self.token(r)]);
            }
            let comments = self.comments_after_opener(l.idx);
            let close = if comments.will_break() {
                Doc::HARD_LINE
            } else {
                Doc::SPACE
            };
            return d.concat([
                Doc::LINE_SUFFIX_BOUNDARY,
                d.text("{"),
                d.indent(comments),
                close,
                self.token(r),
            ]);
        }
        let flat = flat && items.len() == 1 && open_comments.is_empty();
        let edge = if flat {
            Doc::SOFT_LINE_OR_SPACE
        } else {
            Doc::HARD_LINE
        };
        let inside = items.iter().enumerate().map(|(i, &(id, doc))| {
            let line = if i == 0 { edge } else { self.line_before(id) };
            d.concat([line, d.scope(doc)])
        });
        let doc = d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("{"),
            d.indent(d.concat(std::iter::once(self.comments_after_opener(l.idx)).chain(inside))),
            edge,
            self.token(r),
        ]);
        if flat { doc } else { d.group(doc) }
    }

    /// A block in statement position — a body, a loop's, an `else`'s —
    /// always broken, its leading and trailing comments around it.
    pub fn expanded_block(&self, id: Id<Block>) -> Doc<'a> {
        self.with_comments(id, self.block(id, false))
    }

    /// A block in both of its layouts, its comments around each — on one
    /// line when it is only a tail and fits, and broken — built from one
    /// printing of its statements, for a lambda whose body is laid out both
    /// ways by the call it is an argument of.
    pub fn block_forms(&self, id: Id<Block>) -> (Doc<'a>, Doc<'a>) {
        let node = self.tree.get(id);
        let items = self.block_items(id);
        let any = AnyNodeId::of(id);
        let r = self.tree.present(node.r);
        let flat = self.braced_items(any, node.l, &items, r, self.tail_only(id));
        let flat = if self.tail_only(id) {
            self.docs.group(flat)
        } else {
            flat
        };
        (
            self.with_comments(id, flat),
            self.with_comments(id, self.braced_items(any, node.l, &items, r, false)),
        )
    }

    /// Whether a block is only a tail — `{ x }` — the shape that may stay on
    /// one line. A diverging tail (`{ throw x }`) is printed as a statement,
    /// so it is not one.
    pub fn tail_only(&self, id: Id<Block>) -> bool {
        let node = self.tree.get(id);
        self.tree.list(node.stmts).is_empty() && node.tail.is_some_and(|tail| !diverges(tail))
    }

    /// The braces and contents of a block: its statements and tail one per
    /// line (a stray `;` prints nothing, so its line is not there), the tail
    /// in statement position (an `if` there breaks, as in rustfmt, and a
    /// diverging tail is closed with `;`). With `flat`, a block that is only
    /// a tail may stay on one line.
    pub fn block(&self, id: Id<Block>, flat: bool) -> Doc<'a> {
        let doc = self.block_parts(id, flat);
        if flat { self.docs.group(doc) } else { doc }
    }

    /// [`Self::block`] without its group (see [`Self::braced_items`]).
    pub fn block_parts(&self, id: Id<Block>, flat: bool) -> Doc<'a> {
        let node = self.tree.get(id);
        self.braced_items(
            AnyNodeId::of(id),
            node.l,
            &self.block_items(id),
            self.tree.present(node.r),
            flat,
        )
    }

    /// A block's statements and tail, each printed: the tail in statement
    /// position, a diverging one closed with `;`. A stray `;` is no line of
    /// its own: its comments follow the statement before it (`let a = 1; ;
    /// // c` keeps `// c` on the `let`'s line), or start the block when
    /// none does.
    fn block_items(&self, id: Id<Block>) -> Vec<(AnyNodeId, Doc<'a>)> {
        let node = self.tree.get(id);
        let mut items: Vec<(AnyNodeId, Doc<'a>)> = Vec::new();
        for &stmt in self.tree.list(node.stmts) {
            let doc = self.alt(stmt);
            match (stmt, items.last_mut()) {
                (AnyStmt::Empty(_), Some(previous)) => {
                    previous.1 = self.docs.concat([previous.1, doc]);
                }
                (AnyStmt::Empty(_), None) if self.comments.of(stmt.node_id()).is_empty() => {}
                _ => items.push((stmt.node_id(), doc)),
            }
        }
        items.extend(node.tail.map(|tail| {
            let doc = if diverges(tail) {
                self.docs
                    .concat([statement(self, tail), self.docs.text(";")])
            } else {
                statement(self, tail)
            };
            (tail.node_id(), doc)
        }));
        items
    }

    /// The right-hand side after an assignment's `=` (or an arm's `=>`),
    /// whose tokens are `op`, as rustfmt's `choose_rhs` lays it out: on the
    /// same line when it fits there flat; else on a line of its own,
    /// indented, when it fits there whole; else on the same line, broken.
    /// One that is sure to break (a lambda with statements in its body), or
    /// ends in a block of its own that can break instead — an `if`,
    /// `match`, `catch`, lambda, `spawn` or block — stays on the same line
    /// as long as its opening line fits there (`let x = if (c) {` over its
    /// branches, never `let x =` over a one-line `if`), and moves to a line
    /// of its own only when that line would overflow. With a comment between
    /// the operator and it, a line comment there ends the line, and the
    /// right-hand side follows one level in.
    pub fn assigned(
        &self,
        op: impl IntoIterator<Item = TokenIdx>,
        rhs: impl NodeUnion + Copy,
    ) -> Doc<'a> {
        let id = rhs.node_id();
        self.assigned_doc(op, id, self.any(id))
    }

    /// [`Self::assigned`] over the right-hand side's document `doc`, already
    /// printed by [`Self::any`], for a caller that lays the same document
    /// out another way too.
    pub fn assigned_doc(
        &self,
        op: impl IntoIterator<Item = TokenIdx>,
        id: AnyNodeId,
        doc: Doc<'a>,
    ) -> Doc<'a> {
        let d = self.docs;
        let same_line = d.concat([Doc::SPACE, doc]);
        if self.comments_between(op, id) {
            return d.indent(same_line);
        }
        let next_line = d.indent(d.concat([Doc::HARD_LINE, doc]));
        if braced(id) || doc.will_break() {
            // The braces break inside themselves, so the opening line is
            // what must fit: `let x = Foo {`, the right-hand side laying
            // itself out; when even that line overflows, the right-hand
            // side moves to a line of its own.
            return d.layouts([same_line.broken(), next_line.into()]);
        }
        d.best_fitting([same_line.into(), next_line.all_lines(), same_line.into()])
    }

    /// `op x` for a node that keeps its operator's line — a type after `:`,
    /// `=`, `->` or `throws`, a parameter's default: a space and the node,
    /// one level in when comments sit between the two, so that they and the
    /// node after them hang under the operator instead of falling to its
    /// owner's column. An expression that may move to the next line goes
    /// through [`Self::assigned`] instead.
    pub fn after_operator(
        &self,
        op: impl IntoIterator<Item = TokenIdx>,
        rhs: impl NodeUnion + Copy,
    ) -> Doc<'a> {
        let d = self.docs;
        let id = rhs.node_id();
        let same_line = d.concat([Doc::SPACE, self.any(id)]);
        if self.comments_between(op, id) {
            d.indent(same_line)
        } else {
            same_line
        }
    }

    /// Whether a comment that ends a line sits between the operator tokens
    /// `op` and the node `id` after them — a line comment, a block comment
    /// that ended its line, or one on a line of its own, dangling after one
    /// of the tokens or leading the node — so that the node starts a new
    /// line. A block comment inside the line changes nothing about the
    /// layout: `let z = /* c */ match (x) {`.
    pub fn comments_between(&self, op: impl IntoIterator<Item = TokenIdx>, id: AnyNodeId) -> bool {
        let ends_line = |comment: &SourceComment| {
            comment.kind == CommentKind::Line
                || comment.before.is_line_break()
                || comment.after.is_line_break()
        };
        op.into_iter()
            .any(|token| self.dangling_after(token).iter().any(ends_line))
            || self.comments.leading(id).iter().any(ends_line)
    }

    /// The body after a header: its `flat` form, or its `broken` form once
    /// the header's group broke — a one-line body under a multi-line header
    /// reads as part of it. (What goes between a header and its body's `{`
    /// is a [`Doc::HANG_LINE`]: a space, unless the header's last line is a
    /// continuation line, when the `{` takes a line of its own.)
    pub fn body_after(
        &self,
        header: Option<GroupId<'a>>,
        flat: Doc<'a>,
        broken: Doc<'a>,
    ) -> Doc<'a> {
        header.map_or(flat, |group| self.docs.if_group_breaks(group, broken, flat))
    }

    /// `( … )` around a separated list: all on one line, or one item per
    /// line with a trailing comma. A written trailing comma is kept only
    /// when the list breaks; its comments stay either way. With a
    /// `max_width`, the items stay on one line only when they are no wider
    /// than that (see [`Heuristics`](super::Heuristics)).
    pub fn delimited<L: TokenSet, R: TokenSet, T: Copy>(
        &self,
        l: Token<L>,
        items: &[SepItem<T, punct::Comma>],
        r: Token<R>,
        max_width: Option<u32>,
        node: impl Fn(T) -> AnyNodeId,
        item: impl Fn(T) -> Doc<'a>,
    ) -> Doc<'a> {
        self.docs
            .group(self.delimited_parts(l, items, r, max_width, node, item))
    }

    /// `( … )` around a tuple's elements: a [`Self::delimited`] list within
    /// the width of a call's arguments, except that a one-element tuple
    /// keeps its comma when flat too — `(T,)` is what tells it from `(T)`.
    pub fn tuple<L: TokenSet, R: TokenSet, T: Copy>(
        &self,
        l: Token<L>,
        items: &[SepItem<T, punct::Comma>],
        r: Token<R>,
        node: impl Fn(T) -> AnyNodeId,
        item: impl Fn(T) -> Doc<'a>,
    ) -> Doc<'a> {
        let d = self.docs;
        let max_width = self.heuristics.call_args;
        if let [only] = items {
            return d.group(d.concat([
                d.text(self.tree.token_text(l.idx)),
                d.indent(d.limited(
                    d.concat([
                        self.comments_inside(l.idx),
                        self.line_after_opener(l.idx),
                        item(only.item),
                        self.separator(only.sep, ","),
                    ]),
                    max_width,
                )),
                Doc::SOFT_LINE,
                self.token(r),
            ]));
        }
        self.delimited(l, items, r, Some(max_width), node, item)
    }

    /// [`Self::delimited`] without its group, for a caller that groups the
    /// list together with what follows it so that the list breaks first: a
    /// signature's parameters break before its return type does. `node`
    /// names each item's node, for the blank line the source may have left
    /// before it.
    pub fn delimited_parts<L: TokenSet, R: TokenSet, T: Copy>(
        &self,
        l: Token<L>,
        items: &[SepItem<T, punct::Comma>],
        r: Token<R>,
        max_width: Option<u32>,
        node: impl Fn(T) -> AnyNodeId,
        item: impl Fn(T) -> Doc<'a>,
    ) -> Doc<'a> {
        let items: Vec<ListItem<'a>> = items
            .iter()
            .enumerate()
            .map(|(i, &SepItem { item: it, sep })| ListItem {
                doc: item(it),
                comma: sep.map(|comma| comma.idx),
                blank_before: i > 0
                    && self
                        .comments
                        .gap_before_node(self.tree, node(it))
                        .has_blank_line(),
            })
            .collect();
        self.delimited_items(l.idx, &items, r, max_width)
    }

    /// [`Self::delimited_parts`] over items already printed. Empty brackets
    /// print as written, unless a comment is inside, which goes inside them.
    pub fn delimited_items<R: TokenSet>(
        &self,
        l: TokenIdx,
        items: &[ListItem<'a>],
        r: Token<R>,
        max_width: Option<u32>,
    ) -> Doc<'a> {
        let d = self.docs;
        if items.is_empty() && !self.has_comments_after(l) {
            return d.concat([d.text(self.tree.token_text(l)), self.token(r)]);
        }
        let last = items.len().checked_sub(1);
        let inside = items.iter().enumerate().map(|(i, item)| {
            let separator = match (item.comma, Some(i) == last) {
                (Some(comma), false) => self.separator_at(Some(comma), ","),
                (None, false) => d.text(","),
                (Some(comma), true) => d.concat([
                    d.if_break(d.text(","), Doc::NIL),
                    self.comments_after(comma),
                ]),
                (None, true) => d.if_break(d.text(","), Doc::NIL),
            };
            let line = if i == 0 {
                self.line_after_opener(l)
            } else if item.blank_before {
                Doc::EMPTY_LINE
            } else {
                Doc::SOFT_LINE_OR_SPACE
            };
            d.concat([line, d.scope(d.concat([item.doc, separator]))])
        });
        let inside = d.concat(std::iter::once(self.comments_inside(l)).chain(inside));
        d.concat([
            multi_line_item_before_last(items),
            d.text(self.tree.token_text(l)),
            d.indent(within(d, inside, max_width)),
            Doc::SOFT_LINE,
            self.token(r),
        ])
    }

    /// `l … r` around items that each carry their own optional comma (an
    /// array's elements, a map's entries, an object's or destructure's
    /// fields): all on one line when they are no wider than `max_width`
    /// (see [`Heuristics`](super::Heuristics)), or one per line with a
    /// trailing comma; the commas are canonical. With `padded`, a space
    /// inside each bracket when flat (`{ a, b }`); without, none (`[a, b]`).
    /// Empty brackets print as `l` and `r` alone, unless a comment is
    /// inside.
    pub fn list_parts(
        &self,
        node: AnyNodeId,
        l: TokenIdx,
        items: &[ListItem<'a>],
        r: TokenIdx,
        padded: bool,
        max_width: u32,
    ) -> Doc<'a> {
        let d = self.docs;
        let open_comments = if padded {
            self.comments_after_opener(l)
        } else {
            self.comments_inside(l)
        };
        // A comment that ended the line before the bracket (a constructor's
        // own-line trailing comment) still does: the bracket starts the next
        // line, so the comment is read the same way again.
        let open = d.concat([Doc::LINE_SUFFIX_BOUNDARY, d.text(self.tree.token_text(l))]);
        let close = d.concat([d.text(self.tree.token_text(r)), self.comments_after(r)]);
        if items.is_empty() && self.comments.dangling_after(node, l).is_empty() {
            return d.concat([open, close]);
        }
        let edge = if padded {
            Doc::SOFT_LINE_OR_SPACE
        } else {
            Doc::SOFT_LINE
        };
        let last = items.len().checked_sub(1);
        // The edge before the first item is the padding, outside the width
        // the items are held to; the comments after the opener and that
        // edge go inside the indentation with the items, so that a comment
        // on a line of its own sits one level in, in an empty list too.
        let inside = items.iter().enumerate().map(|(i, item)| {
            let separator = match (item.comma, Some(i) == last) {
                (Some(comma), false) => d.concat([d.text(","), self.comments_after(comma)]),
                (None, false) => d.text(","),
                (Some(comma), true) => d.concat([
                    d.if_break(d.text(","), Doc::NIL),
                    self.comments_after(comma),
                ]),
                (None, true) => d.if_break(d.text(","), Doc::NIL),
            };
            let line = if i == 0 {
                Doc::NIL
            } else if item.blank_before {
                Doc::EMPTY_LINE
            } else {
                Doc::SOFT_LINE_OR_SPACE
            };
            d.concat([line, d.scope(d.concat([item.doc, separator]))])
        });
        let first_edge = if items.is_empty() {
            Doc::NIL
        } else if padded {
            edge
        } else {
            self.line_after_opener(l)
        };
        d.concat([
            multi_line_item_before_last(items),
            open,
            d.indent(d.concat([
                open_comments,
                first_edge,
                d.limited(d.concat(inside), max_width),
            ])),
            edge,
            close,
        ])
    }

    /// A [`ListItem`] for `node`, printed by [`Self::any`], with its blank
    /// line before it if the source left one.
    pub fn list_item(&self, node: AnyNodeId, comma: Option<TokenIdx>, first: bool) -> ListItem<'a> {
        ListItem {
            doc: self.any(node),
            comma,
            blank_before: !first
                && self
                    .comments
                    .gap_before_node(self.tree, node)
                    .has_blank_line(),
        }
    }

    /// Items after a keyword (`requires A, B`, `with a, b`): on the
    /// keyword's line when they fit, else each on a line of its own under
    /// it, indented with a trailing comma, the keyword ending its line — as
    /// rustfmt lays out a `where` clause. `tail` goes after the last item,
    /// inside that indentation: the clause's trailing comments, which sit
    /// with the last item whether or not it has a comma to dangle after.
    /// The group is named, so the caller can lay the body out differently
    /// when the items broke.
    pub fn separated<T: Copy>(
        &self,
        items: &[SepItem<T, punct::Comma>],
        tail: Doc<'a>,
        item: impl Fn(T) -> Doc<'a>,
    ) -> (Doc<'a>, GroupId<'a>) {
        let d = self.docs;
        let last = items.len().checked_sub(1);
        let docs: Vec<Doc<'a>> = items.iter().map(|it| item(it.item)).collect();
        let parts =
            items
                .iter()
                .zip(&docs)
                .enumerate()
                .flat_map(|(i, (&SepItem { sep, .. }, &doc))| {
                    let separator = match (sep, Some(i) == last) {
                        (Some(comma), false) => self.separator(Some(comma), ","),
                        (None, false) => d.text(","),
                        (Some(comma), true) => d.concat([
                            d.if_break(d.text(","), Doc::NIL),
                            self.comments_after(comma.idx),
                        ]),
                        (None, true) => d.if_break(d.text(","), Doc::NIL),
                    };
                    [Doc::SOFT_LINE_OR_SPACE, doc, separator]
                });
        d.group_with_id(d.concat([
            multi_line_before_last(&docs),
            d.indent(d.concat(parts.chain([tail]))),
        ]))
    }

    /// A member's separator: the written `,` or `;` is replaced by `text`
    /// (and its comments kept); an unwritten one is added. The separator is
    /// text, not a token: it tucks in before a line comment pending on its
    /// line (`a, // c`), as the comments it follows are attached as if it
    /// came first.
    pub fn separator<S: TokenSet>(&self, written: Option<Token<S>>, text: &'a str) -> Doc<'a> {
        self.separator_at(written.map(|token| token.idx), text)
    }

    /// [`Self::separator`] by token index.
    pub fn separator_at(&self, written: Option<TokenIdx>, text: &'a str) -> Doc<'a> {
        let d = self.docs;
        match written {
            Some(token) => d.concat([d.text(text), self.comments_after(token)]),
            None => d.text(text),
        }
    }

    /// Punctuation the source may leave out (`name: T` or `name T`): the
    /// token as written, else `text`.
    pub fn punct<S: TokenSet>(&self, written: Option<Token<S>>, text: &'a str) -> Doc<'a> {
        match written {
            Some(token) => self.token(token),
            None => self.docs.text(text),
        }
    }
}
