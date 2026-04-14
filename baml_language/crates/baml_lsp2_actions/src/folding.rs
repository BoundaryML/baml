//! Folding ranges for BAML files.
//!
//! Returns collapsible regions for:
//! - Function bodies
//! - Class bodies
//! - Enum bodies
//! - Block expressions (if/match/for/etc.)

use baml_base::SourceFile;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, traversal::trimmed_range};
use text_size::TextRange;

use crate::Db;

/// The kind of folding range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldingRangeKind {
    /// A region (default).
    Region,
    /// A comment block.
    Comment,
}

/// A single folding range in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingRange {
    /// The span to fold.
    pub range: TextRange,
    /// The kind of folding range.
    pub kind: FoldingRangeKind,
}

/// Compute folding ranges for a file.
///
/// Returns ranges for top-level item bodies (functions, classes, enums) and
/// nested block expressions.
pub fn folding_ranges(db: &dyn Db, file: SourceFile) -> Vec<FoldingRange> {
    let tree = baml_compiler_parser::syntax_tree(db, file);

    let mut ranges = Vec::new();
    collect_foldable_nodes(&tree, &mut ranges);

    // Sort by start position for consistent ordering.
    ranges.sort_by_key(|r| r.range.start());

    ranges
}

/// Recursively collect foldable nodes from the CST.
fn collect_foldable_nodes(node: &SyntaxNode, ranges: &mut Vec<FoldingRange>) {
    match node.kind() {
        // Top-level items
        SyntaxKind::FUNCTION_DEF
        | SyntaxKind::CLASS_DEF
        | SyntaxKind::ENUM_DEF
        | SyntaxKind::CLIENT_DEF
        | SyntaxKind::GENERATOR_DEF
        | SyntaxKind::TEMPLATE_STRING_DEF
        | SyntaxKind::RETRY_POLICY_DEF
        | SyntaxKind::TEST_DEF
        // Control flow
        | SyntaxKind::IF_EXPR
        | SyntaxKind::MATCH_EXPR
        | SyntaxKind::FOR_EXPR
        | SyntaxKind::WHILE_STMT
        | SyntaxKind::LAMBDA_EXPR
        // Blocks and parameter lists (for nested folding)
        | SyntaxKind::BLOCK_EXPR
        | SyntaxKind::PARAMETER_LIST => {
            ranges.push(FoldingRange {
                range: trimmed_range(node),
                kind: FoldingRangeKind::Region,
            });
        }
        _ => {}
    }

    // Recurse into children
    for child in node.children() {
        collect_foldable_nodes(&child, ranges);
    }
}
