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
    match (boundary_token(node, false), boundary_token(node, true)) {
        (Some(first), Some(last)) => {
            TextRange::new(first.text_range().start(), last.text_range().end())
        }
        _ => node.text_range(),
    }
}

/// Search from either edge, skipping empty/trivia-only children without
/// visiting the interior once a significant token is found. Walk parent links
/// rather than recurse so deeply nested recovery trees don't consume the stack.
fn boundary_token(node: &SyntaxNode, from_end: bool) -> Option<SyntaxToken> {
    let edge = |node: &SyntaxNode| {
        if from_end {
            node.last_child_or_token()
        } else {
            node.first_child_or_token()
        }
    };
    let mut current = edge(node)?;
    loop {
        match &current {
            NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                return Some(token.clone());
            }
            NodeOrToken::Node(child) => {
                if let Some(child) = edge(child) {
                    current = child;
                    continue;
                }
            }
            _ => {}
        }
        loop {
            let sibling = if from_end {
                current.prev_sibling_or_token()
            } else {
                current.next_sibling_or_token()
            };
            if let Some(sibling) = sibling {
                current = sibling;
                break;
            }
            let parent = current.parent()?;
            if parent == *node {
                return None;
            }
            current = parent.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SyntaxTreeBuilder;

    // Original full descendant scan is the equivalence oracle.
    fn scanned_range(node: &SyntaxNode) -> TextRange {
        let mut tokens = node.non_trivia_tokens();
        match tokens.next() {
            Some(first) => {
                let last = tokens.last().unwrap_or_else(|| first.clone());
                TextRange::new(first.text_range().start(), last.text_range().end())
            }
            None => node.text_range(),
        }
    }

    fn tree(build: impl FnOnce(&mut SyntaxTreeBuilder)) -> SyntaxNode {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(SyntaxKind::SOURCE_FILE);
        build(&mut builder);
        builder.finish_node();
        let root = SyntaxNode::new_root(builder.finish());
        for node in root.descendants() {
            assert_eq!(node.span_range(), scanned_range(&node), "{node:?}");
        }
        root
    }

    #[test]
    fn trimmed_range_comments_and_nested_boundaries() {
        let root = tree(|b| {
            b.token(SyntaxKind::LINE_COMMENT, "// before");
            b.nl();
            b.start_node(SyntaxKind::TYPE_EXPR);
            b.ws("  ");
            b.token(SyntaxKind::WORD, "héllo");
            b.token(SyntaxKind::BLOCK_COMMENT, "/* inside */");
            b.token(SyntaxKind::WORD, "world");
            b.ws(" ");
            b.finish_node();
            b.token(SyntaxKind::BLOCK_COMMENT, "/* after */");
        });
        assert_eq!(root.span_range(), TextRange::new(12.into(), 35.into()));
    }

    #[test]
    fn trimmed_range_empty_and_trivia_only_subtrees() {
        let root = tree(|b| {
            b.token(SyntaxKind::WORD, "outside");
            b.start_node(SyntaxKind::ERROR);
            b.start_node(SyntaxKind::ERROR);
            b.finish_node();
            b.ws("   ");
            b.token(SyntaxKind::LINE_COMMENT, "// comment");
            b.nl();
            b.finish_node();
            b.token(SyntaxKind::WORD, "also_outside");
        });
        let child = root.first_child().unwrap();
        assert_eq!(child.span_range(), child.text_range());
        let empty = child.first_child().unwrap();
        assert_eq!(empty.span_range(), TextRange::empty(7.into()));
        let empty_root = tree(|_| {});
        assert_eq!(empty_root.span_range(), empty_root.text_range());
    }

    #[test]
    fn trimmed_range_recovery_and_zero_width_tokens() {
        let root = tree(|b| {
            // Empty recovery children and trivia-only siblings at both edges.
            for word in ["", "?"] {
                b.start_node(SyntaxKind::ERROR);
                b.start_node(SyntaxKind::ERROR);
                b.finish_node();
                b.token(SyntaxKind::ERROR_TOKEN, word);
                b.finish_node();
                b.start_node(SyntaxKind::ERROR);
                b.ws(" ");
                b.finish_node();
            }
            // HEADER_COMMENT is intentionally not is_trivia(): preserve that.
            b.token(SyntaxKind::HEADER_COMMENT, "//# heading");
            b.start_node(SyntaxKind::ERROR);
            b.finish_node();
        });
        assert_eq!(root.span_range(), root.text_range());
    }

    #[test]
    fn trimmed_range_matches_scan_across_tree_shapes() {
        fn populate(b: &mut SyntaxTreeBuilder, seed: &mut u32, depth: usize) {
            *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let count = (*seed >> 16) % 6;
            for _ in 0..count {
                *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                match (*seed >> 16) % 6 {
                    0 if depth > 0 => {
                        b.start_node(SyntaxKind::ERROR);
                        populate(b, seed, depth - 1);
                        b.finish_node();
                    }
                    1 => b.ws("  "),
                    2 => b.token(SyntaxKind::BLOCK_COMMENT, "/* c */"),
                    3 => b.token(SyntaxKind::WORD, "λ"),
                    4 => b.token(SyntaxKind::ERROR_TOKEN, ""),
                    _ => b.token(SyntaxKind::WORD, "word"),
                }
            }
        }
        for mut seed in 0..256 {
            tree(|b| populate(b, &mut seed, 6));
        }
    }
}
