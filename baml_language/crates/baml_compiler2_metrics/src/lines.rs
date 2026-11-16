//! Line counts read from the token array, where comments are distinct tokens.

use std::collections::HashSet;

use baml_base::line_index::LineIndex;
use baml_syntax::{
    nodes::{FileTree, TestDef},
    token::TokenKind,
};
use text_size::{TextRange, TextSize};

use crate::count;

/// The lines in `range` holding code: a token that is neither whitespace nor
/// a comment of any spelling.
pub(crate) fn code_lines(tree: &FileTree, lines: &LineIndex<'_>, range: TextRange) -> HashSet<u32> {
    let tokens = tree.tokens();
    let mut code = HashSet::new();
    for idx in tokens.iter() {
        let kind = tokens.kind(idx);
        let token_range = tokens.range(idx);
        if !range.contains_range(token_range)
            || kind.is_trivia()
            || kind.is_doc()
            || kind == TokenKind::HeaderComment
        {
            continue;
        }
        let start = lines.line(token_range.start().into());
        let end = lines.line(u32::from(token_range.end()).saturating_sub(1).max(start));
        if kind == TokenKind::TemplateChunk && end > start {
            // A chunk of template text spans lines; a blank line inside it
            // is not a line of code (it was not when each word was a token).
            for (offset, line) in tree.token_text(idx).split('\n').enumerate() {
                if !line.trim().is_empty() {
                    let offset = u32::try_from(offset)
                        .unwrap_or_else(|_| unreachable!("a file has fewer than 2^32 lines"));
                    code.insert(start + offset);
                }
            }
            continue;
        }
        code.extend(start..=end);
    }
    code
}

/// `(lines, tests, test_lines)` for a whole file: code lines outside test
/// blocks, the number of `test` blocks, and code lines inside `test` and
/// `testset` blocks.
pub(crate) fn file_lines(tree: &FileTree, lines: &LineIndex<'_>) -> (u32, u32, u32) {
    let tokens = tree.tokens();
    let mut tests = 0;
    let mut test_lines = HashSet::new();
    for id in tree.arenas().ids::<TestDef>() {
        if tokens.kind(tree.get(id).kw.idx) == TokenKind::Test {
            tests += 1;
        }
        test_lines.extend(code_lines(tree, lines, tokens.text_range(tree.span(id))));
    }
    let all = code_lines(tree, lines, TextRange::up_to(TextSize::of(tree.text())));
    let lines = all.difference(&test_lines).count();
    (count(lines), tests, count(test_lines.len()))
}
