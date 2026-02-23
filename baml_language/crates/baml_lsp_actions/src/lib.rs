//! IDE features for the BAML language.
//!
//! This crate provides language intelligence features like hover, goto-definition,
//! code lenses, and more. It is designed to be used by LSP servers and test
//! infrastructure.
//!
//! ## LSP-Agnostic Design
//!
//! All types in this crate are LSP-agnostic. The language server is responsible
//! for converting these types to LSP types (e.g., `CodeLens` → `lsp_types::CodeLens`).
//!
//! ## Features
//!
//! - **Hover**: Show information about a symbol on hover
//! - **Goto Definition**: Navigate to symbol definitions
//! - **Code Lens**: Inline actions like "Run" buttons
//! - **Code Actions**: Quick fixes and refactorings
//! - **Document Symbols**: File outline/structure
//! - **Completion**: Context-aware autocomplete suggestions

pub mod code_action;
pub mod code_lens;
pub mod completion;
pub mod document_symbols;
pub mod find_references;
pub mod goto_definition;
pub mod hover;
pub mod inlay_hints;

pub use code_action::{CodeAction, CodeActionKind};
pub use code_lens::{CodeLens, CodeLensKind};
pub use completion::{CompletionContext, CompletionItem, CompletionKind, complete};
pub use document_symbols::{DocumentSymbol, SymbolKind};
pub use find_references::{Reference, find_all_references};
pub use goto_definition::{NavigationTarget, find_word_at_offset, goto_definition};
pub use hover::{Hover, HoverContent, hover};

/// Markup format for hover content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    /// Plain text without formatting.
    PlainText,
    /// Markdown formatted text.
    Markdown,
}

/// A value associated with a text range.
#[derive(Debug, Clone)]
pub struct RangedValue<T> {
    /// The text range this value applies to.
    pub range: text_size::TextRange,
    /// The value.
    pub value: T,
}

impl<T> RangedValue<T> {
    /// Create a new ranged value.
    pub fn new(range: text_size::TextRange, value: T) -> Self {
        Self { range, value }
    }
}

impl<T> std::ops::Deref for RangedValue<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(test)]
pub mod testing;

#[cfg(test)]
mod goto_definition_tests;

#[cfg(test)]
mod find_references_tests;
