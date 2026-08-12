//! Utilities for traversing syntax trees.

use rowan::{NodeOrToken, TextRange};

use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Extension trait for syntax nodes.
pub trait SyntaxNodeExt {
    /// Find the first ancestor node of the given kind.
    fn ancestor_of_kind(&self, kind: SyntaxKind) -> Option<SyntaxNode>;

    /// Find all descendant nodes of the given kind.
    fn descendants_of_kind(&self, kind: SyntaxKind) -> Vec<SyntaxNode>;

    /// Find the first descendant node of the given kind.
    fn first_descendant_of_kind(&self, kind: SyntaxKind) -> Option<SyntaxNode>;

    /// Find the first child token of the given kind.
    fn first_child_token_of_kind(&self, kind: SyntaxKind) -> Option<SyntaxToken>;

    /// Get all tokens in this subtree.
    fn tokens(&self) -> impl Iterator<Item = SyntaxToken>;

    /// Get all non-trivia tokens in this subtree.
    fn non_trivia_tokens(&self) -> impl Iterator<Item = SyntaxToken>;

    /// The text range of this node for use as a diagnostic / editor span,
    /// excluding leading and trailing trivia (whitespace, newlines, comments).
    ///
    /// Rowan attaches trivia as child tokens, so a node's raw `text_range()`
    /// can start on the inter-token whitespace before its first real token
    /// (e.g. the space after `->` in a return type). Spans must tightly cover
    /// the construct, so build them with this instead of `text_range()`.
    /// See [`trimmed_range`].
    fn span_range(&self) -> TextRange;
}

impl SyntaxNodeExt for SyntaxNode {
    fn ancestor_of_kind(&self, kind: SyntaxKind) -> Option<SyntaxNode> {
        self.ancestors().find(|node| node.kind() == kind)
    }

    fn descendants_of_kind(&self, kind: SyntaxKind) -> Vec<SyntaxNode> {
        self.descendants()
            .filter(|node| node.kind() == kind)
            .collect()
    }

    fn first_descendant_of_kind(&self, kind: SyntaxKind) -> Option<SyntaxNode> {
        self.descendants().find(|node| node.kind() == kind)
    }

    fn first_child_token_of_kind(&self, kind: SyntaxKind) -> Option<SyntaxToken> {
        self.children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| token.kind() == kind)
    }

    fn tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        self.descendants_with_tokens()
            .filter_map(|element| match element {
                NodeOrToken::Token(token) => Some(token),
                NodeOrToken::Node(_) => None,
            })
    }

    fn non_trivia_tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        self.tokens().filter(|token| !token.kind().is_trivia())
    }

    fn span_range(&self) -> TextRange {
        trimmed_range(self)
    }
}

/// Get the text range of a node, excluding leading/trailing trivia.
pub fn trimmed_range(node: &SyntaxNode) -> TextRange {
    let first_non_trivia = node.descendants_with_tokens().find(|element| {
        element
            .as_token()
            .map(|t| !t.kind().is_trivia())
            .unwrap_or(false)
    });

    let last_non_trivia = node
        .descendants_with_tokens()
        .filter(|element| {
            element
                .as_token()
                .map(|t| !t.kind().is_trivia())
                .unwrap_or(false)
        })
        .last();

    match (first_non_trivia, last_non_trivia) {
        (Some(first), Some(last)) => {
            TextRange::new(first.text_range().start(), last.text_range().end())
        }
        _ => node.text_range(),
    }
}
