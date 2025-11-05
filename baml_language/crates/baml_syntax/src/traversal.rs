//! Utilities for traversing syntax trees.

use crate::{SyntaxKind, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextRange, TextSize, ast::AstNode};

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
}

/// Find a specific node type at a text offset.
pub fn find_node_at_offset<N: AstNode<Language = crate::BamlLanguage>>(
    root: &SyntaxNode,
    offset: TextSize,
) -> Option<N> {
    root.token_at_offset(offset)
        .right_biased()
        .and_then(|token| token.parent_ancestors().find_map(N::cast))
}

/// Find all nodes of a specific type in the tree.
pub fn find_all_nodes<N: AstNode<Language = crate::BamlLanguage>>(root: &SyntaxNode) -> Vec<N> {
    root.descendants().filter_map(N::cast).collect()
}

/// Check if a node contains any errors.
pub fn has_errors(node: &SyntaxNode) -> bool {
    node.descendants().any(|n| n.kind() == SyntaxKind::ERROR)
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
