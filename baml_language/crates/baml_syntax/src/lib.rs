//! Syntax tree representation using Rowan.
//!
//! Provides a lossless, incremental syntax tree with parent pointers and full source fidelity.

pub mod syntax_kind;
pub mod syntax_node;

pub use syntax_kind::SyntaxKind;
pub use syntax_node::{BamlLanguage, SyntaxElement, SyntaxNode, SyntaxToken};
