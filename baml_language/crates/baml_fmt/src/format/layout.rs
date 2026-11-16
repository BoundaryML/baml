//! The layouts rules share: a declaration's docs, a braced body of members,
//! a delimited list, a separator. Every width decision in them is a group;
//! the rules add none of their own.

use baml_syntax::{
    nodes::{AnyDoc, AnyId, Block},
    token::{Token, TokenIdx, TokenSet, punct},
    tree::{AnyNodeId, Id, List, NodeUnion, SepItem},
};

use super::{
    Formatter,
    rules::control::{diverges, statement},
};
use crate::{
    comments::gap_before,
    ir::{Doc, GroupId},
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
        l: Token<punct::LBrace>,
        members: &[impl NodeUnion + Copy],
        r: Token<punct::RBrace>,
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
    ///
    /// The braces are a group either way. A broken body's group is forced by
    /// the line breaks between its items, and a forced group prints in break
    /// mode even inside a layout that was chosen flat (a member chain's, a
    /// call's), so that what follows an item on its line — an arm's body
    /// after its `=>` — is measured up to its first possible line break, as
    /// anywhere else, rather than whole.
    pub fn braced(
        &self,
        node: AnyNodeId,
        l: Token<punct::LBrace>,
        items: &[AnyNodeId],
        r: Token<punct::RBrace>,
        flat: bool,
    ) -> Doc<'a> {
        self.docs
            .group(self.braced_parts(node, l, items, r, flat, |item| self.any(item)))
    }

    /// [`Self::braced`] without its group, for a layout that decides the
    /// braces' lines together with more than one pair: an `if` chain breaks
    /// every block or none. Each item is printed by `item`.
    pub fn braced_parts(
        &self,
        node: AnyNodeId,
        l: Token<punct::LBrace>,
        items: &[AnyNodeId],
        r: Token<punct::RBrace>,
        flat: bool,
        item: impl Fn(AnyNodeId) -> Doc<'a>,
    ) -> Doc<'a> {
        let items: Vec<(AnyNodeId, Doc<'a>)> = items.iter().map(|&id| (id, item(id))).collect();
        self.braced_items(node, l, &items, r, flat)
    }

    /// [`Self::braced_parts`] over items already printed, so that two
    /// layouts of one block (flat and broken) share them. Braces with
    /// nothing inside print as `{}`; with only comments, the comments go
    /// inside them on the braces' line when they fit there (`{ /* c */ }`),
    /// else the closing brace takes a line of its own.
    pub fn braced_items(
        &self,
        node: AnyNodeId,
        l: Token<punct::LBrace>,
        items: &[(AnyNodeId, Doc<'a>)],
        r: Token<punct::RBrace>,
        flat: bool,
    ) -> Doc<'a> {
        let d = self.docs;
        let open_comments = self.comments.dangling_after(node, l.idx);
        if items.is_empty() {
            if open_comments.is_empty() {
                return d.concat([Doc::LINE_SUFFIX_BOUNDARY, d.text("{"), self.token(r)]);
            }
            let comments = self.comments_after(l.idx);
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
            d.concat([line, doc])
        });
        d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("{"),
            d.indent(d.concat(std::iter::once(self.comments_after(l.idx)).chain(inside))),
            edge,
            self.token(r),
        ])
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

    /// [`Self::block`] without its group (see [`Self::braced_parts`]).
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
    /// position, a diverging one closed with `;`.
    fn block_items(&self, id: Id<Block>) -> Vec<(AnyNodeId, Doc<'a>)> {
        let node = self.tree.get(id);
        self.tree
            .list(node.stmts)
            .iter()
            .map(|stmt| (stmt.node_id(), self.alt(*stmt)))
            .chain(node.tail.map(|tail| {
                let doc = if diverges(tail) {
                    self.docs
                        .concat([statement(self, tail), self.docs.text(";")])
                } else {
                    statement(self, tail)
                };
                (tail.node_id(), doc)
            }))
            .collect()
    }

    /// The right-hand side after an assignment's `=` (or an arm's `=>`),
    /// whose tokens are `op`, as rustfmt's `choose_rhs` lays it out: on the
    /// same line when it fits there flat; else on a line of its own,
    /// indented, when it fits there whole; else on the same line, broken.
    /// It stays on the same line when it is sure to break (a lambda with
    /// statements in its body), when it ends in a block of its own that can
    /// break instead — an `if`, `match`, `catch`, lambda, `spawn` or block:
    /// `let x = if (c) {` over its branches, never `let x =` over a one-line
    /// `if` — and when a comment sits between the operator and it: a line
    /// comment there ends the line, and the right-hand side follows one
    /// level in.
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
        let commented = op.into_iter().any(|token| self.has_comments_after(token))
            || !self.comments.leading(id).is_empty();
        if commented {
            return d.indent(same_line);
        }
        if braced(id) || doc.will_break() {
            return same_line;
        }
        d.best_fitting([
            same_line.into(),
            d.indent(d.concat([Doc::HARD_LINE, doc])).all_lines(),
            same_line.into(),
        ])
    }

    /// What goes between a header and its body's `{`: a space, or — when
    /// `header` names a group that broke, a signature's `throws` clause or
    /// an `if let`'s scrutinee over several lines — a line break, so that
    /// the `{` takes a line of its own, as rustfmt puts it after a broken
    /// `where` clause. A space when there is no such group.
    pub fn brace_after(&self, header: Option<GroupId<'a>>) -> Doc<'a> {
        header.map_or(Doc::SPACE, |group| {
            self.docs.if_group_breaks(group, Doc::HARD_LINE, Doc::SPACE)
        })
    }

    /// The body after a header (see [`Self::brace_after`]): its `flat` form,
    /// or its `broken` form once the header's group broke and put the `{`
    /// on a line of its own — a one-line body under a multi-line header
    /// reads as part of it.
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
    /// when the list breaks; its comments stay either way.
    pub fn delimited<L: TokenSet, R: TokenSet, T: Copy>(
        &self,
        l: Token<L>,
        items: &[SepItem<T, punct::Comma>],
        r: Token<R>,
        item: impl Fn(T) -> Doc<'a>,
    ) -> Doc<'a> {
        self.docs.group(self.delimited_parts(l, items, r, item))
    }

    /// `( … )` around a tuple's elements: a [`Self::delimited`] list, except
    /// that a one-element tuple keeps its comma when flat too — `(T,)` is
    /// what tells it from `(T)`.
    pub fn tuple<L: TokenSet, R: TokenSet, T: Copy>(
        &self,
        l: Token<L>,
        items: &[SepItem<T, punct::Comma>],
        r: Token<R>,
        item: impl Fn(T) -> Doc<'a>,
    ) -> Doc<'a> {
        let d = self.docs;
        if let [only] = items {
            return d.group(d.concat([
                d.text(self.tree.token_text(l.idx)),
                d.indent(d.concat([
                    self.comments_inside(l.idx),
                    Doc::SOFT_LINE,
                    item(only.item),
                    self.separator(only.sep, ","),
                ])),
                Doc::SOFT_LINE,
                self.token(r),
            ]));
        }
        self.delimited(l, items, r, item)
    }

    /// [`Self::delimited`] without its group, for a caller that groups the
    /// list together with what follows it so that the list breaks first: a
    /// signature's parameters break before its return type does.
    pub fn delimited_parts<L: TokenSet, R: TokenSet, T: Copy>(
        &self,
        l: Token<L>,
        items: &[SepItem<T, punct::Comma>],
        r: Token<R>,
        item: impl Fn(T) -> Doc<'a>,
    ) -> Doc<'a> {
        let items: Vec<ListItem<'a>> = items
            .iter()
            .map(|&SepItem { item: it, sep }| ListItem {
                doc: item(it),
                comma: sep.map(|comma| comma.idx),
                blank_before: false,
            })
            .collect();
        self.delimited_items(l.idx, &items, r)
    }

    /// [`Self::delimited_parts`] over items already printed. Empty brackets
    /// print as written, unless a comment is inside, which goes inside them.
    pub fn delimited_items<R: TokenSet>(
        &self,
        l: TokenIdx,
        items: &[ListItem<'a>],
        r: Token<R>,
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
                Doc::SOFT_LINE
            } else if item.blank_before {
                Doc::EMPTY_LINE
            } else {
                Doc::SOFT_LINE_OR_SPACE
            };
            d.concat([line, item.doc, separator])
        });
        d.concat([
            multi_line_item_before_last(items),
            d.text(self.tree.token_text(l)),
            d.indent(d.concat(std::iter::once(self.comments_inside(l)).chain(inside))),
            Doc::SOFT_LINE,
            self.token(r),
        ])
    }

    /// `l … r` around items that each carry their own optional comma (an
    /// array's elements, a map's entries, an object's or destructure's
    /// fields): all on one line, or one per line with a trailing comma; the
    /// commas are canonical. With `padded`, a space inside each bracket when
    /// flat (`{ a, b }`); without, none (`[a, b]`). Empty brackets print as
    /// `l` and `r` alone, unless a comment is inside.
    pub fn list_parts(
        &self,
        node: AnyNodeId,
        l: TokenIdx,
        items: &[ListItem<'a>],
        r: TokenIdx,
        padded: bool,
    ) -> Doc<'a> {
        let d = self.docs;
        let open_comments = if padded {
            self.comments_after(l)
        } else {
            self.comments_inside(l)
        };
        // A comment that ended the line before the bracket (a constructor's
        // own-line trailing comment) still does: the bracket starts the next
        // line, so the comment is read the same way again.
        let open = d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text(self.tree.token_text(l)),
            open_comments,
        ]);
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
                edge
            } else if item.blank_before {
                Doc::EMPTY_LINE
            } else {
                Doc::SOFT_LINE_OR_SPACE
            };
            d.concat([line, item.doc, separator])
        });
        d.concat([
            multi_line_item_before_last(items),
            open,
            d.indent(d.concat(inside)),
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
    /// it, indented, the keyword ending its line — as rustfmt lays out a
    /// `where` clause. The group is named, so the caller can put a `{` on a
    /// line of its own when the items broke.
    pub fn separated<T: Copy>(
        &self,
        items: &[SepItem<T, punct::Comma>],
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
                        (Some(comma), true) => self.comments_after(comma.idx),
                        (None, true) => Doc::NIL,
                    };
                    [Doc::SOFT_LINE_OR_SPACE, doc, separator]
                });
        d.group_with_id(d.concat([multi_line_before_last(&docs), d.indent(d.concat(parts))]))
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
