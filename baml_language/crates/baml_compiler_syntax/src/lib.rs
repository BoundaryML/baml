//! Syntax tree representation using Rowan.
//!
//! Provides a lossless, incremental syntax tree with parent pointers and full source fidelity.

pub mod ast;
pub mod builder;
pub mod syntax_kind;
pub mod syntax_node;
pub mod traversal;
pub mod validated;

#[cfg(test)]
mod tests;

pub use ast::*;
pub use builder::SyntaxTreeBuilder;
// Re-export useful rowan types
pub use rowan::{GreenNode, NodeOrToken, TextRange, TextSize, TokenAtOffset, WalkEvent};
pub use syntax_kind::SyntaxKind;
pub use syntax_node::{BamlLanguage, SyntaxElement, SyntaxNode, SyntaxToken};
pub use traversal::*;
pub use validated::{
    FromCST, KnownKind, StrongAstError, SyntaxNodeIter, ValidatedToken, is_word_like,
};
