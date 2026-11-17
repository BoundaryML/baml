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
use crate::ir::{Doc, Docs};

/// What the token that comments [dangle](dangling) after is to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    /// An opening bracket: the comments are the first thing inside it, so a
    /// blank line before the first is not kept (none at a body's edges), and
    /// a block comment on the bracket's line is `spaced` from it or not —
    /// `{ /* c */ }` but `(/* c */)`.
    Opener { spaced: bool },
    /// Any other token — a separator, an operator, a keyword with more of
    /// its construct after it: a line comment on its line puts what follows
    /// one level in, as the right-hand side after `let x = // c` is.
    Token,
    /// A keyword directly before a brace or a parenthesised head of its own
    /// (`else`, `defer`, `catch`, a bare `spawn`): what follows a line
    /// comment on its line starts at the keyword's own indent, so the body's
    /// `{` lines up with its `}`.
    Keyword,
}

/// `comments`, to go before the node they lead: each on its own line when
/// written so, followed by the line break or space that separated it from
/// what came next. A boundary before each keeps it off a line with a line
/// comment pending there, so two comments never merge into one.
pub fn leading<'a>(d: Docs<'a>, tree: &'a FileTree, comments: &[SourceComment]) -> Doc<'a> {
    d.concat(comments.iter().map(|&comment| {
        let line_before = if comment.before.is_line_break() {
            Doc::HARD_LINE
        } else {
            Doc::NIL
        };
        d.concat([
            Doc::LINE_SUFFIX_BOUNDARY,
            line_before,
            d.text(comment.text(tree)),
            after(comment.after),
        ])
    }))
}

/// `comments`, to go after the node they trail: on its line when written
/// there, else on lines of their own after it, past whatever the enclosing
/// nodes still print on its line — a run of comments on one such line
/// stays one line, as written.
pub fn trailing<'a>(d: Docs<'a>, tree: &'a FileTree, comments: &[SourceComment]) -> Doc<'a> {
    let mut docs = Vec::new();
    let mut rest = comments;
    while let Some((&first, _)) = rest.split_first() {
        if !first.before.is_line_break() {
            docs.push(match first.kind {
                CommentKind::Block => d.concat([
                    Doc::LINE_SUFFIX_BOUNDARY,
                    Doc::SPACE,
                    d.text(first.text(tree)),
                ]),
                CommentKind::Line => end_of_line(d, first.text(tree)),
            });
            rest = &rest[1..];
            continue;
        }
        // The run on `first`'s line: block comments, and a line comment
        // closing it; everything after is on a line of its own again.
        let run = rest
            .windows(2)
            .take_while(|pair| {
                pair[0].kind == CommentKind::Block && !pair[1].before.is_line_break()
            })
            .count()
            + 1;
        let last = rest[run - 1];
        docs.push(d.trailing_line(run_text(tree, first, last), first.before.has_blank_line()));
        rest = &rest[run..];
    }
    d.concat(docs)
}

/// The source from the start of `first` to the end of `last`, two comments
/// on one line: the run as written, blanks between included.
fn run_text(tree: &FileTree, first: SourceComment, last: SourceComment) -> &str {
    let tokens = tree.tokens();
    let start = tokens.range(first.token).start();
    let end = tokens.range(last.token).end();
    tree.text()[usize::from(start)..usize::from(end)].trim_end()
}

/// `comments`, to go directly after the token of the node they dangle
/// after (what the token is to them: `after`). One on that token's line
/// stays there: a block comment in place — inside an opening bracket
/// followed by a space or a line break when it ended its line in the
/// source, so that what comes next never touches it; after any other token
/// it ends its line as it did in the source, what follows the token then
/// starting a new line, as after a line comment — printed on one line with
/// what follows, it would lead that instead on the next pass. A comment
/// written on a line of its own, or following a line comment of the run,
/// is on a line of its own, at the indent the rule prints the comments at,
/// with the blank line before it kept except directly inside an opening
/// bracket; a line comment there ends its line too, and after a block
/// comment the rule decides, with a space or a line break. A boundary
/// before each comment on the token's line keeps it off a line with a line
/// comment pending there, in the order the two were written.
pub fn dangling<'a>(
    d: Docs<'a>,
    tree: &'a FileTree,
    comments: &[SourceComment],
    after: After,
) -> Doc<'a> {
    let mut after_line_comment = false;
    let docs: Vec<Doc<'a>> = comments
        .iter()
        .enumerate()
        .map(|(i, &comment)| {
            let text = comment.text(tree);
            let own_line = comment.before.is_line_break() || after_line_comment;
            after_line_comment = comment.kind == CommentKind::Line;
            match (comment.kind, own_line) {
                (kind, true) => {
                    let at_edge = i == 0 && matches!(after, After::Opener { .. });
                    let line = if !at_edge && comment.before.has_blank_line() {
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
                // What follows the comment is the construct's business: an
                // opening bracket's layout separates it from the first item
                // (see [`Formatter::line_after_opener`]), a keyword's or a
                // token's own spacing follows it.
                (CommentKind::Block, false) => {
                    let space = match after {
                        After::Opener { spaced: false } if i == 0 => Doc::NIL,
                        After::Opener { .. } | After::Token | After::Keyword => Doc::SPACE,
                    };
                    // The last of the run that ended its line in the
                    // source still does; one before another of the run (a
                    // comment moved past the token's terminator, with the
                    // terminator's own after it) stays on the line.
                    let last = i + 1 == comments.len();
                    let ends_line = match after {
                        After::Opener { .. } => Doc::NIL,
                        After::Token if last && comment.after.is_line_break() => {
                            d.indent(Doc::HARD_LINE)
                        }
                        After::Keyword if last && comment.after.is_line_break() => Doc::HARD_LINE,
                        After::Token | After::Keyword => Doc::NIL,
                    };
                    d.concat([Doc::LINE_SUFFIX_BOUNDARY, space, d.text(text), ends_line])
                }
                (CommentKind::Line, false) => {
                    let continuation = match after {
                        After::Opener { .. } | After::Token => d.indent(Doc::HARD_LINE),
                        After::Keyword => Doc::HARD_LINE,
                    };
                    d.concat([end_of_line(d, text), continuation])
                }
            }
        })
        .collect();
    d.concat(docs)
}

/// `comments` written before or inside a type's closing suffix (`[]`, `?`),
/// or after a prefix operator, which carries them past itself: a block
/// comment on one line in place after it, the first line comment at the end
/// of the line (or, when a comment is pending there already, on a line of
/// its own after it), and every comment after one that left the line on a
/// line of its own, in order — a block comment printed in place would
/// overtake it. None of them breaks the enclosing group — they were not
/// where they are now, and the layout decides without them, the same way on
/// every pass — so a block comment with lines of its own, which would, goes
/// on a line of its own too, and takes the rest of the run with it.
pub fn closing<'a>(d: Docs<'a>, tree: &'a FileTree, comments: &[SourceComment]) -> Doc<'a> {
    let mut left_the_line = false;
    d.concat(comments.iter().map(|&comment| {
        let text = comment.text(tree);
        match (comment.kind, left_the_line || text.contains('\n')) {
            (CommentKind::Line | CommentKind::Block, true) => {
                left_the_line = true;
                d.closing_line(text)
            }
            (CommentKind::Block, false) => {
                d.concat([Doc::LINE_SUFFIX_BOUNDARY, Doc::SPACE, d.text(text)])
            }
            (CommentKind::Line, false) => {
                left_the_line = true;
                d.closing_suffix(text)
            }
        }
    }))
}

/// What separates a leading comment from what follows it.
fn after(gap: Gap) -> Doc<'static> {
    match gap {
        Gap::Inline => Doc::SPACE,
        gap if gap.has_blank_line() => Doc::EMPTY_LINE,
        Gap::Edge | Gap::Lines(_) => Doc::HARD_LINE,
    }
}

/// A comment at the end of the line being printed — or, when a comment is
/// pending there already, on a line of its own after it, so that two
/// comments never merge into one and stay in the order written (see
/// [`Docs::closing_suffix`]). It breaks the enclosing group: nothing that
/// would have followed it on a flat line slips in before it.
fn end_of_line<'a>(d: Docs<'a>, text: &'a str) -> Doc<'a> {
    d.concat([d.closing_suffix(text), Doc::EXPAND_PARENT])
}
