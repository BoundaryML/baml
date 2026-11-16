//! Plain comments that belong to a node, read from the trivia around its
//! span. (`///` doc lines and `//baml:` directives are not trivia: a
//! declaration owns them as [`AnyDoc`](crate::nodes::AnyDoc) fields.)
//!
//! Trivia is never in a node's fields; a node *owns* significant tokens only.
//! The one rule relating a node to the comments around it:
//!
//! - its **leading trivia** is the trivia between the previous significant
//!   token and the node's first token, minus a comment on that previous
//!   token's own line (that is the previous token's trailing comment);
//! - its **trailing comment** is a comment after its last token on the same
//!   line.

use crate::{
    nodes::FileTree,
    token::{TokenIdx, TokenKind},
    tree::AnyNodeId,
};

/// The trivia tokens that lead `id`, in order (see the module doc).
pub fn leading_trivia(tree: &FileTree, id: AnyNodeId) -> impl Iterator<Item = TokenIdx> + '_ {
    let tokens = tree.tokens();
    let span = tree.arenas().span(id);
    let start = span.start();
    let from = match tokens.prev_significant(start) {
        _ if span.is_empty() => start,
        None => TokenIdx(0),
        // Everything on the previous token's line is its trailing comment:
        // the node's own trivia begins at the line break.
        Some(prev) => tokens
            .between(TokenIdx(prev.0 + 1)..start)
            .find(|&t| tokens.kind(t) == TokenKind::Newline)
            .unwrap_or(start),
    };
    tokens.between(from..start)
}

/// A comment on the same line after `id`'s last token.
pub fn trailing_comment(tree: &FileTree, id: AnyNodeId) -> Option<TokenIdx> {
    let tokens = tree.tokens();
    let span = tree.arenas().span(id);
    if span.is_empty() {
        return None;
    }
    let mut token = span.end();
    while let Some(kind) = tokens.get(token) {
        match kind {
            TokenKind::Whitespace => token = TokenIdx(token.0 + 1),
            TokenKind::LineComment
            | TokenKind::DocComment
            | TokenKind::DirectiveComment
            | TokenKind::HeaderComment
            | TokenKind::BlockComment
            | TokenKind::UnclosedBlockComment => return Some(token),
            _ => return None,
        }
    }
    None
}
