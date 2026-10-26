//! Token-level metrics read from the CST: Halstead counts and the physical
//! line counts.
//!
//! Halstead operators and operands follow rust-code-analysis: keywords and
//! punctuation are operators (an opening bracket counts, its closer does not);
//! identifiers and literals are operands. A string literal, and each prose
//! run of a template, is one operand. Operators are told apart by kind,
//! operands by spelling.

use std::collections::{HashMap, HashSet};

use baml_base::line_index::LineIndex;
use baml_compiler_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, TextRange, WalkEvent};

use crate::{Halstead, Loc, count};

/// Halstead counts for the code in `range`.
pub(crate) fn halstead(root: &SyntaxNode, range: TextRange) -> Halstead {
    let mut operators: HashMap<SyntaxKind, u32> = HashMap::new();
    let mut operands: HashMap<String, u32> = HashMap::new();

    let mut events = covering_node(root, range).preorder_with_tokens();
    while let Some(event) = events.next() {
        let WalkEvent::Enter(element) = event else {
            continue;
        };
        if !range.contains_range(element.text_range()) {
            continue;
        }
        match element {
            NodeOrToken::Node(node) => match node.kind() {
                SyntaxKind::STRING_LITERAL
                | SyntaxKind::RAW_STRING_LITERAL
                | SyntaxKind::BYTE_STRING_LITERAL
                | SyntaxKind::DEPRECATED_HASH_STRING_LITERAL
                | SyntaxKind::UNQUOTED_STRING
                | SyntaxKind::STRING_LITERAL_TYPE
                | SyntaxKind::BACKTICK_TEXT => {
                    *operands.entry(node.text().to_string()).or_default() += 1;
                    events.skip_subtree();
                }
                SyntaxKind::BACKTICK_STRING_LITERAL => {
                    *operators.entry(node.kind()).or_default() += 1;
                }
                _ => {}
            },
            NodeOrToken::Token(token) => match classify(token.kind()) {
                Role::Operator => *operators.entry(token.kind()).or_default() += 1,
                Role::Operand => *operands.entry(token.text().to_owned()).or_default() += 1,
                Role::Ignored => {}
            },
        }
    }

    Halstead {
        distinct_operators: count(operators.len()),
        operators: operators.values().sum(),
        distinct_operands: count(operands.len()),
        operands: operands.values().sum(),
    }
}

/// Physical line counts for `range`. `lloc` is left at zero: logical lines
/// are statements, which the AST walk counts.
pub(crate) fn loc(root: &SyntaxNode, lines: &LineIndex<'_>, range: TextRange) -> Loc {
    let first = lines.line(range.start().into());
    let last = lines.line(
        u32::from(range.end())
            .saturating_sub(1)
            .max(range.start().into()),
    );

    let mut code = HashSet::new();
    let mut comments = HashSet::new();
    for element in covering_node(root, range).descendants_with_tokens() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        let token_range = token.text_range();
        if !range.contains_range(token_range) || token.kind().is_whitespace() {
            continue;
        }
        let start = lines.line(token_range.start().into());
        let end = lines.line(u32::from(token_range.end()).saturating_sub(1).max(start));
        let set = if token.kind().is_comment() {
            &mut comments
        } else {
            &mut code
        };
        set.extend(start..=end);
    }

    let sloc = last - first + 1;
    let occupied = count(code.union(&comments).count());
    Loc {
        sloc,
        ploc: count(code.len()),
        lloc: 0,
        cloc: count(comments.len()),
        blank: sloc.saturating_sub(occupied),
    }
}

enum Role {
    Operator,
    Operand,
    Ignored,
}

fn classify(kind: SyntaxKind) -> Role {
    match kind {
        SyntaxKind::WORD
        | SyntaxKind::INTEGER_LITERAL
        | SyntaxKind::FLOAT_LITERAL
        | SyntaxKind::BIGINT_LITERAL
        | SyntaxKind::KW_TRUE
        | SyntaxKind::KW_FALSE
        | SyntaxKind::KW_NULL => Role::Operand,
        SyntaxKind::R_PAREN
        | SyntaxKind::R_BRACE
        | SyntaxKind::R_BRACKET
        | SyntaxKind::BACKTICK
        | SyntaxKind::QUOTE
        | SyntaxKind::HASH
        | SyntaxKind::ERROR_TOKEN => Role::Ignored,
        kind if kind.is_trivia() => Role::Ignored,
        _ => Role::Operator,
    }
}

fn covering_node(root: &SyntaxNode, range: TextRange) -> SyntaxNode {
    match root.covering_element(range) {
        NodeOrToken::Node(node) => node,
        NodeOrToken::Token(token) => token.parent().unwrap_or_else(|| root.clone()),
    }
}
