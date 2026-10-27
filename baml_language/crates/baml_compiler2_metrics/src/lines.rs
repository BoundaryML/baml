//! Line counts read from the CST, where comments are distinct tokens.

use std::collections::HashSet;

use baml_base::line_index::LineIndex;
use baml_compiler_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, TextRange};

use crate::count;

/// The lines in `range` holding code: a token that is neither whitespace nor
/// a comment.
pub(crate) fn code_lines(
    root: &SyntaxNode,
    lines: &LineIndex<'_>,
    range: TextRange,
) -> HashSet<u32> {
    let node = match root.covering_element(range) {
        NodeOrToken::Node(node) => node,
        NodeOrToken::Token(token) => token.parent().unwrap_or_else(|| root.clone()),
    };
    let mut code = HashSet::new();
    for element in node.descendants_with_tokens() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        let token_range = token.text_range();
        if !range.contains_range(token_range)
            || token.kind().is_whitespace()
            || token.kind().is_comment()
        {
            continue;
        }
        let start = lines.line(token_range.start().into());
        let end = lines.line(u32::from(token_range.end()).saturating_sub(1).max(start));
        code.extend(start..=end);
    }
    code
}

/// `(lines, tests, test_lines)` for a whole file: code lines outside test
/// blocks, the number of `test` blocks, and code lines inside `test` and
/// `testset` blocks.
pub(crate) fn file_lines(root: &SyntaxNode, lines: &LineIndex<'_>) -> (u32, u32, u32) {
    let mut tests = 0;
    let mut test_lines = HashSet::new();
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::TEST_EXPR_DEF => tests += 1,
            SyntaxKind::TESTSET_DEF => {}
            _ => continue,
        }
        test_lines.extend(code_lines(root, lines, node.text_range()));
    }
    let all = code_lines(root, lines, root.text_range());
    let lines = all.difference(&test_lines).count();
    (count(lines), tests, count(test_lines.len()))
}
