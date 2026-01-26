//! Rowan syntax node types for BAML.

use crate::SyntaxKind;

/// BAML language definition for Rowan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BamlLanguage;

impl rowan::Language for BamlLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind::from(raw)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Syntax node in the Rowan tree.
pub type SyntaxNode = rowan::SyntaxNode<BamlLanguage>;

/// Syntax token (leaf node) in the Rowan tree.
pub type SyntaxToken = rowan::SyntaxToken<BamlLanguage>;

/// Either a node or token.
pub type SyntaxElement = rowan::SyntaxElement<BamlLanguage>;

#[cfg(test)]
mod tests {
    use rowan::GreenNodeBuilder;

    use super::*;

    #[test]
    fn test_syntax_tree_construction() {
        let mut builder = GreenNodeBuilder::new();

        builder.start_node(SyntaxKind::SOURCE_FILE.into());
        builder.token(SyntaxKind::WORD.into(), "function");
        builder.finish_node();

        let green = builder.finish();
        let root = SyntaxNode::new_root(green);

        assert_eq!(root.kind(), SyntaxKind::SOURCE_FILE);
    }
}
