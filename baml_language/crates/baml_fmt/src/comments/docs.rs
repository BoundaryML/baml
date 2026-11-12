//! The documents that print a node's comments, for the rule printing the
//! node to put around and inside it.
//!
//! A line comment is deferred to the end of the line being printed (a
//! `line_suffix`), behind a boundary so that two of them never end up on one
//! line, and breaks the enclosing group so that nothing that would have
//! followed it on a flat line slips in before it. A block comment on a line
//! with code is printed in place, whether or not it was the last thing on
//! its line in the source: only its position in the layout decides, so the
//! layout is the same on every pass. A comment on a line of its own starts
//! one. Blank lines are kept, one at most: after a leading comment, before
//! a trailing or dangling one.

use baml_syntax::nodes::FileTree;

use super::{CommentKind, Gap, SourceComment};
use crate::ir::{Doc, Docs, SuffixWidth};

/// `comments`, to go before the node they lead: each on its own line when
/// written so, followed by the line break or space that separated it from
/// what came next.
pub fn leading<'a>(d: Docs<'a>, tree: &'a FileTree, comments: &[SourceComment]) -> Doc<'a> {
    d.concat(comments.iter().map(|&comment| {
        let line_before = if comment.before.is_line_break() {
            Doc::HARD_LINE
        } else {
            Doc::NIL
        };
        d.concat([
            line_before,
            d.text(comment.text(tree)),
            after(comment.after),
        ])
    }))
}

/// `comments`, to go after the node they trail: on its line when written
/// there, else on lines of their own after it, past whatever the enclosing
/// nodes still print on its line.
pub fn trailing<'a>(d: Docs<'a>, tree: &'a FileTree, comments: &[SourceComment]) -> Doc<'a> {
    d.concat(comments.iter().map(|&comment| {
        let text = comment.text(tree);
        match (comment.kind, comment.before.is_line_break()) {
            (CommentKind::Line | CommentKind::Block, true) => {
                d.trailing_line(text, comment.before.has_blank_line())
            }
            (CommentKind::Block, false) => d.concat([Doc::SPACE, d.text(text)]),
            (CommentKind::Line, false) => end_of_line(d, text),
        }
    }))
}

/// `comments`, to go directly after the token of the node they dangle
/// after: on that token's line when written there — a block comment in
/// place (`spaced` from the token, or not: `{ /* c */ }` but `(/* c */)`),
/// a line comment ending the line as it did, so that what follows the token
/// starts a new line, as in the source — else each on a line of its own. A
/// line comment always ends its line, wherever it is: what the rule prints
/// next starts a new one. After a block comment on a line of its own the
/// rule decides, with a space or a line break.
pub fn dangling<'a>(
    d: Docs<'a>,
    tree: &'a FileTree,
    comments: &[SourceComment],
    spaced: bool,
) -> Doc<'a> {
    d.concat(comments.iter().enumerate().map(|(i, &comment)| {
        let text = comment.text(tree);
        match (comment.kind, comment.before.is_line_break()) {
            (kind @ (CommentKind::Line | CommentKind::Block), true) => {
                // A blank line directly after the token is not kept.
                let line = if i > 0 && comment.before.has_blank_line() {
                    Doc::EMPTY_LINE
                } else {
                    Doc::HARD_LINE
                };
                let end = match kind {
                    CommentKind::Line => Doc::HARD_LINE,
                    CommentKind::Block => Doc::NIL,
                };
                d.concat([line, d.text(text), end])
            }
            (CommentKind::Block, false) if i == 0 && !spaced => d.text(text),
            (CommentKind::Block, false) => d.concat([Doc::SPACE, d.text(text)]),
            (CommentKind::Line, false) => d.concat([end_of_line(d, text), Doc::HARD_LINE]),
        }
    }))
}

/// A comment at the end of the line being printed: after any suffix already
/// pending there ends that line, so two comments never merge into one.
fn end_of_line<'a>(d: Docs<'a>, text: &'a str) -> Doc<'a> {
    d.concat([
        Doc::LINE_SUFFIX_BOUNDARY,
        d.line_suffix(" ", SuffixWidth::Ignored),
        d.line_suffix(text, SuffixWidth::Ignored),
        Doc::EXPAND_PARENT,
    ])
}

/// What separates a leading comment from what follows it.
fn after(gap: Gap) -> Doc<'static> {
    match gap {
        Gap::Inline => Doc::SPACE,
        gap if gap.has_blank_line() => Doc::EMPTY_LINE,
        Gap::Edge | Gap::Lines(_) => Doc::HARD_LINE,
    }
}
