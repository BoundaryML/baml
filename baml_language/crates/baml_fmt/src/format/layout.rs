//! The layouts rules share: a declaration's docs, a braced body of members,
//! a delimited list, a separator. Every width decision in them is a group;
//! the rules add none of their own.

use baml_syntax::{
    nodes::AnyDoc,
    token::{Token, TokenIdx, TokenSet, punct},
    tree::{AnyNodeId, List, NodeUnion, SepItem},
};

use super::Formatter;
use crate::{
    comments::gap_before,
    ir::{Doc, GroupId},
};

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
    /// when there are no members and no comments inside, and with only
    /// comments inside, those on the braces' line when they fit there
    /// (`{ /* c */ }`), else the closing brace on a line of its own. The
    /// comments after `{` are printed inside the indentation, with the
    /// members. A comment that ended the line before `{` still does: the
    /// brace then starts the next line, as it did in the source.
    pub fn body(
        &self,
        node: AnyNodeId,
        l: Token<punct::LBrace>,
        members: &[impl NodeUnion + Copy],
        r: Token<punct::RBrace>,
    ) -> Doc<'a> {
        let d = self.docs;
        if members.is_empty() {
            if self.comments.dangling_after(node, l.idx).is_empty() {
                return d.concat([Doc::LINE_SUFFIX_BOUNDARY, d.text("{"), self.token(r)]);
            }
            let comments = self.comments_after(l.idx);
            let close = if comments.forced() {
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
        let inside = members.iter().enumerate().map(|(i, &member)| {
            let line = if i == 0 {
                Doc::HARD_LINE
            } else {
                self.line_before(member.node_id())
            };
            d.concat([line, self.alt(member)])
        });
        d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            d.text("{"),
            d.indent(d.concat(std::iter::once(self.comments_after(l.idx)).chain(inside))),
            Doc::HARD_LINE,
            self.token(r),
        ])
    }

    /// What goes between a header and its body's `{`: a space, or — when
    /// `header` names a group that broke, a signature's `throws` clause or
    /// a `requires` list over several lines — a line break, so that the `{`
    /// takes a line of its own, as rustfmt puts it after a broken `where`
    /// clause. A space when there is no such group.
    pub fn brace_after(&self, header: Option<GroupId<'a>>) -> Doc<'a> {
        header.map_or(Doc::SPACE, |group| {
            self.docs.if_group_breaks(group, Doc::HARD_LINE, Doc::SPACE)
        })
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
        let d = self.docs;
        if items.is_empty() && !self.has_comments_after(l.idx) {
            return d.concat([d.text(self.tree.token_text(l.idx)), self.token(r)]);
        }
        let last = items.len().checked_sub(1);
        let inside = items
            .iter()
            .enumerate()
            .map(|(i, &SepItem { item: it, sep })| {
                let separator = match (sep, Some(i) == last) {
                    (Some(comma), false) => self.token(comma),
                    (None, false) => d.text(","),
                    (Some(comma), true) => d.concat([
                        d.if_break(d.text(","), Doc::NIL),
                        self.comments_after(comma.idx),
                    ]),
                    (None, true) => d.if_break(d.text(","), Doc::NIL),
                };
                let line = if i == 0 {
                    Doc::SOFT_LINE
                } else {
                    Doc::SOFT_LINE_OR_SPACE
                };
                d.concat([line, item(it), separator])
            });
        d.concat([
            d.text(self.tree.token_text(l.idx)),
            d.indent(d.concat(std::iter::once(self.comments_inside(l.idx)).chain(inside))),
            Doc::SOFT_LINE,
            self.token(r),
        ])
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
        d.group_with_id(d.indent(d.concat(items.iter().enumerate().flat_map(
            |(i, &SepItem { item: it, sep })| {
                let separator = match (sep, Some(i) == last) {
                    (Some(comma), false) => self.token(comma),
                    (None, false) => d.text(","),
                    (Some(comma), true) => self.comments_after(comma.idx),
                    (None, true) => Doc::NIL,
                };
                [Doc::SOFT_LINE_OR_SPACE, item(it), separator]
            },
        ))))
    }

    /// A member's separator: the written `,` or `;` is replaced by `text`
    /// (and its comments kept); an unwritten one is added.
    pub fn separator<S: TokenSet>(&self, written: Option<Token<S>>, text: &'a str) -> Doc<'a> {
        let d = self.docs;
        match written {
            Some(token) => d.concat([d.text(text), self.comments_after(token.idx)]),
            None => d.text(text),
        }
    }
}
