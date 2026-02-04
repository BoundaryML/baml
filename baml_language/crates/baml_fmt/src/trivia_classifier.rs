use baml_compiler_syntax::{SourceFile, SyntaxKind};
use rowan::{TextRange, ast::AstNode};

/// We walk through the syntax tree and classify trivia tokens.
///
/// Note that this will also handle header comments, despite them not being considered trivia in [`SyntaxKind::is_trivia`].
/// This is okay because we maintain ordering, so header comments will not be separated from their correct relative location.
///
/// The output will always be sorted with regard to the range they are attached to (with EOF being later than everything else),
/// then ordered by the location of the order the trivia should be emitted (based on the order in the input).
pub fn classify_trivia(root: &SourceFile) -> Vec<EmittableTrivia> {
    let mut found_trivia = Vec::new();

    let mut prev_non_trivia_on_line: Option<TextRange> = None;
    let mut has_comment_on_line = false;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AttachTriviaToNext {
        LineComment(TextRange),
        BlockComment(TextRange),
        Newline,
    }
    let mut trivia_to_attach_next = Vec::new();

    let mut next = root.syntax().first_token();
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
            SyntaxKind::LINE_COMMENT | SyntaxKind::HEADER_COMMENT => {
                if let Some(prev) = prev_non_trivia_on_line {
                    debug_assert!(
                        next.is_none()
                            || next
                                .as_ref()
                                .is_some_and(|next| next.kind() == SyntaxKind::NEWLINE),
                        "We expect a newline after a line/header comment",
                    );
                    found_trivia.push(EmittableTrivia::TrailingLineComment {
                        comment: token.text_range(),
                        after: prev,
                    });
                } else {
                    trivia_to_attach_next.push(AttachTriviaToNext::LineComment(token.text_range()));
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

    found_trivia
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
    ///
    /// E.g. a `//` or `//#` comment
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
    ///
    /// E.g. a `//` or `//#` comment
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
    /// Attached to the following non-trivia token (this is important because )
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

#[cfg(test)]
mod tests {
    use baml_compiler_syntax::SyntaxNode;
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
        let tokens = baml_compiler_lexer::lex_file(&mut db, source_file);
        let (parsed, errors) = baml_compiler_parser::parse_file(&tokens);
        assert!(errors.is_empty());
        let ast = SyntaxNode::new_root(parsed);
        let trivia = classify_trivia(&SourceFile::cast(ast).unwrap());

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
        )
    }
}
