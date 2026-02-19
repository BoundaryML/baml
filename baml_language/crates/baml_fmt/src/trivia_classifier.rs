use std::collections::HashMap;

use baml_db::baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use ouroboros::self_referencing;
use rowan::{TextRange, TextSize};

use crate::printer::Printable;

/// Represents all the trivia attached to tokens or EOF.
pub struct TriviaInfo {
    inner: TriviaInfoInner,
}

#[self_referencing]
struct TriviaInfoInner {
    trivia: Vec<EmittableTrivia>,
    #[borrows(trivia)]
    #[covariant]
    token_trivia: HashMap<TextRange, &'this [EmittableTrivia]>,
    #[borrows(trivia)]
    eof_trivia: &'this [EmittableTrivia],
}

impl TriviaInfo {
    /// We walk through the syntax tree and classify trivia tokens.
    ///
    /// The output will always be sorted with regard to the range they are attached to (with EOF being later than everything else),
    /// then ordered by the location of the order the trivia should be emitted (based on the order in the input).
    #[must_use]
    pub fn classify_trivia(root: &SyntaxNode) -> Self {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum AttachTriviaToNext {
            LineComment(TextRange),
            BlockComment(TextRange),
            Newline,
        }
        let mut found_trivia = Vec::new();

        let mut prev_non_trivia_on_line: Option<TextRange> = None;
        let mut has_comment_on_line = false;
        let mut trivia_to_attach_next = Vec::new();

        let mut next = root.first_token();
        while let Some(token) = next {
            next = token.next_token();
            match token.kind() {
                SyntaxKind::NEWLINE => {
                    if !has_comment_on_line && prev_non_trivia_on_line.is_none() {
                        // terminated line is empty except for maybe whitespace
                        if !trivia_to_attach_next.ends_with(&[AttachTriviaToNext::Newline]) {
                            trivia_to_attach_next.push(AttachTriviaToNext::Newline);
                        }
                    }
                    has_comment_on_line = false;
                    prev_non_trivia_on_line = None;
                }
                SyntaxKind::LINE_COMMENT => {
                    if let Some(prev) = prev_non_trivia_on_line {
                        debug_assert!(
                            next.is_none()
                                || next
                                    .as_ref()
                                    .is_some_and(|next| next.kind() == SyntaxKind::NEWLINE),
                            "We expect a newline after a line comment",
                        );
                        found_trivia.push(EmittableTrivia::TrailingLineComment {
                            comment: token.text_range(),
                            after: prev,
                        });
                    } else {
                        trivia_to_attach_next
                            .push(AttachTriviaToNext::LineComment(token.text_range()));
                    }
                    has_comment_on_line = true;
                }
                SyntaxKind::BLOCK_COMMENT => {
                    if let Some(prev) = prev_non_trivia_on_line {
                        found_trivia.push(EmittableTrivia::TrailingBlockComment {
                            comment: token.text_range(),
                            after: prev,
                        });
                    } else {
                        trivia_to_attach_next
                            .push(AttachTriviaToNext::BlockComment(token.text_range()));
                    }
                    has_comment_on_line = true;
                }
                SyntaxKind::WHITESPACE => {}
                kind => {
                    debug_assert!(
                        !kind.is_trivia(),
                        "Unexpected trivia token kind {kind:?} in the catch-all non-trivia branch. This means a new trivia token kind was added without updating this match statement."
                    );
                    prev_non_trivia_on_line = Some(token.text_range());
                    for trivia in trivia_to_attach_next.drain(..) {
                        match trivia {
                            AttachTriviaToNext::LineComment(comment) => {
                                found_trivia.push(EmittableTrivia::LeadingLineComment {
                                    comment,
                                    before: token.text_range(),
                                });
                            }
                            AttachTriviaToNext::BlockComment(comment) => {
                                found_trivia.push(EmittableTrivia::LeadingBlockComment {
                                    comment,
                                    before: token.text_range(),
                                });
                            }
                            AttachTriviaToNext::Newline => {
                                found_trivia.push(EmittableTrivia::EmptyLine {
                                    before: token.text_range(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // trivia at end:
        for trivia in trivia_to_attach_next {
            match trivia {
                AttachTriviaToNext::LineComment(comment)
                | AttachTriviaToNext::BlockComment(comment) => {
                    found_trivia.push(EmittableTrivia::CommentBeforeEOF { comment });
                }
                AttachTriviaToNext::Newline => {
                    found_trivia.push(EmittableTrivia::EmptyLineBeforeEOF);
                }
            }
        }

        debug_assert!(found_trivia.is_sorted_by_key(|trivia| trivia.attached_to().start()));

        let found_trivia = TriviaInfoInner::new(
            found_trivia,
            |found_trivia| {
                let mut token_trivia = HashMap::new();

                let mut it = found_trivia.iter().enumerate().peekable();
                'outer_loop: while let Some((start_idx, trivia)) = it.next() {
                    let range = trivia.attached_to();
                    if range.start() == TextSize::new(u32::MAX) {
                        // EOF trivia, there are no more token-attached trivia
                        break;
                    }
                    while let Some(&(idx, trivia)) = it.peek() {
                        if trivia.attached_to() == range {
                            it.next();
                        } else {
                            // found something not attached to the same range, so we can stop
                            token_trivia.insert(range, &found_trivia[start_idx..idx]);
                            continue 'outer_loop;
                        }
                    }
                    // we reached the end of trivia without finding a token not attached to the same range
                    // so we can add the entire rest of the trivia to the token trivia
                    token_trivia.insert(range, &found_trivia[start_idx..]);
                }

                token_trivia
            },
            |found_trivia| {
                if let Some((idx, _)) = found_trivia
                    .iter()
                    .enumerate()
                    .rfind(|(_, trivia)| !trivia.is_at_eof())
                {
                    &found_trivia[(idx + 1)..]
                } else {
                    // all trivia is attached to EOF (or there is not trivia)
                    found_trivia
                }
            },
        );

        TriviaInfo {
            inner: found_trivia,
        }
    }

    #[must_use]
    pub fn all_trivia(&self) -> &[EmittableTrivia] {
        self.inner.borrow_trivia()
    }

    /// Returns all trivia attached to the token at the given range.
    #[must_use]
    pub fn get_for_range(&self, range: TextRange) -> &[EmittableTrivia] {
        self.inner
            .borrow_token_trivia()
            .get(&range)
            .copied()
            .unwrap_or(&[])
    }

    /// Returns all trivia attached to the token at the given range, split into leading and trailing trivia.
    #[must_use]
    pub fn get_for_range_split(
        &self,
        range: TextRange,
    ) -> (&[EmittableTrivia], &[EmittableTrivia]) {
        let trivia = self.get_for_range(range);
        debug_assert!(trivia.iter().all(|t| t.attached_to() == range));
        let split_idx = trivia
            .iter()
            .enumerate()
            .find(|(_, t)| !t.is_leading())
            .map_or(trivia.len(), |(idx, _)| idx);
        let leading = &trivia[..split_idx];
        let trailing = &trivia[split_idx..];
        debug_assert!(
            leading.iter().all(EmittableTrivia::is_leading),
            "{leading:?}"
        );
        debug_assert!(trailing.iter().all(|t| !t.is_leading()), "{trailing:?}");
        (leading, trailing)
    }

    /// Returns the leading trivia for [`Printable::leftmost_token`] and the trailing trivia for [`Printable::rightmost_token`].
    ///
    /// This can be more efficient than calling [`Self::get_for_range_split`] on each separately,
    /// since it will only split the trivia for the element once if has only one token (leftmost == rightmost).
    /// It is also often cleaner.
    #[must_use]
    pub fn get_for_element(
        &self,
        printable: &impl Printable,
    ) -> (&[EmittableTrivia], &[EmittableTrivia]) {
        let leftmost = printable.leftmost_token();
        let rightmost = printable.rightmost_token();
        if leftmost == rightmost {
            self.get_for_range_split(leftmost)
        } else {
            let (leading, _) = self.get_for_range_split(leftmost);
            let (_, trailing) = self.get_for_range_split(rightmost);
            (leading, trailing)
        }
    }

    /// Returns all trivia attached to EOF.
    #[must_use]
    pub fn get_for_eof(&self) -> &[EmittableTrivia] {
        self.inner.borrow_eof_trivia()
    }
}

/// Represents a trivia token that can be emitted by the formatter printer.
/// Includes information about what non-trivia token it should be placed relative to (or relative to EOF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmittableTrivia {
    /// After some token, can have other tokens after it on the same line
    ///
    /// Placed after the preceding non-trivia token.
    TrailingBlockComment {
        comment: TextRange,
        /// The input location of the non-trivia token that this is after
        after: TextRange,
    },
    /// At the end of a line, must stay at the end of a line.
    ///
    /// Placed after the preceding non-trivia token.
    TrailingLineComment {
        comment: TextRange,
        /// The input location of the non-trivia token that this is after
        after: TextRange,
    },
    /// There are no other non-trivia tokens before this token on the same line.
    /// However, it may have tokens after it on the same line.
    ///
    /// Placed before the following non-trivia token (or before EOF).
    LeadingBlockComment {
        comment: TextRange,
        /// The input location of the non-trivia token that this precedes
        before: TextRange,
    },
    /// There are no other non-trivia tokens before this token on the same line.
    /// It may not have tokens after it on the same line.
    ///
    /// Placed before the following non-trivia token (or before EOF).
    /// Since it cannot have tokens after it on the same line, this means it is always on its own line.
    LeadingLineComment {
        comment: TextRange,
        /// The input location of the non-trivia token that this precedes
        before: TextRange,
    },
    /// There is a comment (either line or block) with no other non-trivia tokens on the same line,
    /// and no non-trivia tokens after it in the file.
    ///
    /// Will be placed in its own line at the end of the file.
    CommentBeforeEOF { comment: TextRange },
    /// There is a newline with no other non-whitespace tokens on the same line.
    /// While this may not be emitted in all contexts (depending on formatting rules), it may result in one empty line.
    ///
    /// This is the primary way we retain whether two lines have an empty line between them:
    /// ```baml
    /// let a = 1;
    ///
    /// a += 2;
    /// ```
    /// vs.
    /// ```baml
    /// let a = 1;
    /// a += 2;
    /// ```
    ///
    /// Attached to the following non-trivia token
    /// (this is important because we want to retain information on whether there is a newline between two elements or not).
    EmptyLine { before: TextRange },
    /// There is a newline with no other non-whitespace tokens on the same line,
    /// and no non-trivia tokens after it in the file.
    /// While this may not be emitted in all contexts (depending on formatting rules), it may result in one empty line.
    ///
    /// While this is typically overwritten by the empty line at the end of the file,
    /// it may be relevant if there are comments at the end of the file, such as
    /// ```baml
    /// function MyFunction() {
    ///     ...
    /// } // this the end of some block
    ///
    /// // this is another comment, with an empty line before it
    ///
    /// ```
    EmptyLineBeforeEOF,
}

impl EmittableTrivia {
    /// `true` is the trivia represents a comment.
    /// `false` if it represents an empty line.
    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(
            self,
            EmittableTrivia::CommentBeforeEOF { .. }
                | EmittableTrivia::LeadingBlockComment { .. }
                | EmittableTrivia::LeadingLineComment { .. }
                | EmittableTrivia::TrailingBlockComment { .. }
                | EmittableTrivia::TrailingLineComment { .. }
        )
    }
    /// `true` if the trivia is attached to the following non-trivia token (or EOF),
    /// `false` if it is attached to the previous non-trivia token (on same line).
    #[must_use]
    pub fn is_leading(&self) -> bool {
        matches!(
            self,
            EmittableTrivia::EmptyLine { .. }
                | EmittableTrivia::EmptyLineBeforeEOF
                | EmittableTrivia::CommentBeforeEOF { .. }
                | EmittableTrivia::LeadingBlockComment { .. }
                | EmittableTrivia::LeadingLineComment { .. }
        )
    }
    /// Anything at EOF has `TextRange` with `u32::MAX` as start and end.
    #[must_use]
    pub fn attached_to(&self) -> TextRange {
        match self {
            Self::EmptyLine { before } => *before,
            Self::EmptyLineBeforeEOF | Self::CommentBeforeEOF { .. } => {
                TextRange::new(TextSize::new(u32::MAX), TextSize::new(u32::MAX))
            }
            Self::LeadingBlockComment { before, .. } | Self::LeadingLineComment { before, .. } => {
                *before
            }
            Self::TrailingBlockComment { after, .. } | Self::TrailingLineComment { after, .. } => {
                *after
            }
        }
    }
    /// `true` if the trivia is attached to EOF.
    /// `false` if it is attached to some other token.
    #[must_use]
    pub fn is_at_eof(&self) -> bool {
        matches!(
            self,
            EmittableTrivia::CommentBeforeEOF { .. } | EmittableTrivia::EmptyLineBeforeEOF
        )
    }

    /// Returns the length of the trivia when trying to print its parent as a single line.
    ///
    /// - Block comments return their length if they contain no newlines.
    /// - Line comments (and block comments with newlines) return `None` as they cannot be included in a single line.
    /// - Empty lines return `Some(0)` as they get ignored when printing in a single line.
    /// - EOF comments return `None`. Generally, they should not be passed into this function as they are not between any tokens.
    ///
    /// `input` is needed to determine whether a block comment contains newlines.
    #[must_use]
    pub fn single_line_len(&self, input: &str) -> Option<usize> {
        match self {
            Self::TrailingBlockComment { comment, .. }
            | Self::LeadingBlockComment { comment, .. } => {
                if input[*comment].contains('\n') {
                    None
                } else {
                    Some(comment.len().into())
                }
            }
            Self::EmptyLine { .. } => Some(0),
            Self::TrailingLineComment { .. }
            | Self::LeadingLineComment { .. }
            | Self::CommentBeforeEOF { .. }
            | Self::EmptyLineBeforeEOF => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_db::{baml_compiler_lexer, baml_compiler_parser, baml_compiler_syntax::SyntaxNode};
    use baml_project::ProjectDatabase;

    use super::*;

    #[test]
    fn test_classify_trivia() {
        let source = "\
// leading comment1
/* leading comment2 */
// leading comment3
function MyFunction() -> int {

    // leading comment4

    let x = 1; // trailing comment1
    /* leading comment5 */
    let y = /* trailing comment2 */ 2;
    y
}
// comment before eof

";
        let mut db = ProjectDatabase::new();
        let source_file = db.add_file("file.baml", source);
        let tokens = baml_compiler_lexer::lex_file(&db, source_file);
        let (parsed, errors) = baml_compiler_parser::parse_file(&tokens);
        assert!(errors.is_empty());
        let ast = SyntaxNode::new_root(parsed);
        let trivia = TriviaInfo::classify_trivia(&ast);
        let trivia = trivia.all_trivia();

        assert_eq!(
            trivia,
            vec![
                EmittableTrivia::LeadingLineComment {
                    // leading comment1
                    comment: TextRange::new(0.into(), 19.into()),
                    before: TextRange::new(63.into(), 71.into()),
                },
                EmittableTrivia::LeadingBlockComment {
                    // leading comment2
                    comment: TextRange::new(20.into(), 42.into()),
                    before: TextRange::new(63.into(), 71.into()),
                },
                EmittableTrivia::LeadingLineComment {
                    // leading comment3
                    comment: TextRange::new(43.into(), 62.into()),
                    before: TextRange::new(63.into(), 71.into()),
                },
                EmittableTrivia::EmptyLine {
                    before: TextRange::new(124.into(), 127.into()),
                },
                EmittableTrivia::LeadingLineComment {
                    // leading comment4
                    comment: TextRange::new(99.into(), 118.into()),
                    before: TextRange::new(124.into(), 127.into()),
                },
                EmittableTrivia::EmptyLine {
                    before: TextRange::new(124.into(), 127.into()),
                },
                EmittableTrivia::TrailingLineComment {
                    // trailing comment1
                    comment: TextRange::new(135.into(), 155.into()),
                    after: TextRange::new(133.into(), 134.into()),
                },
                EmittableTrivia::LeadingBlockComment {
                    // leading comment5
                    comment: TextRange::new(160.into(), 182.into()),
                    before: TextRange::new(187.into(), 190.into()),
                },
                EmittableTrivia::TrailingBlockComment {
                    // trailing comment2
                    comment: TextRange::new(195.into(), 218.into()),
                    after: TextRange::new(193.into(), 194.into()),
                },
                EmittableTrivia::CommentBeforeEOF {
                    // comment before eof
                    comment: TextRange::new(230.into(), 251.into())
                },
                EmittableTrivia::EmptyLineBeforeEOF,
            ]
        );
    }
}
