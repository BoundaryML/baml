//! Document symbols for BAML files.
//!
//! Document symbols provide an outline of a file's structure,
//! showing classes, functions, enums, etc. with their hierarchies.

use text_size::TextRange;

/// The kind of a document symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    File,
    Module,
    Class,
    Enum,
    Function,
    Field,
    EnumMember,
    Constant,
    Variable,
    TypeParameter,
}

/// A document symbol representing an item in the file outline.
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    /// The name of the symbol.
    pub name: String,
    /// Optional detail text (e.g., type signature).
    pub detail: Option<String>,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// The full range of the symbol definition.
    pub range: TextRange,
    /// The range of just the symbol name.
    pub selection_range: TextRange,
    /// Child symbols (e.g., fields in a class).
    pub children: Vec<DocumentSymbol>,
}

impl DocumentSymbol {
    /// Create a new document symbol.
    pub fn new(
        name: impl Into<String>,
        kind: SymbolKind,
        range: TextRange,
        selection_range: TextRange,
    ) -> Self {
        Self {
            name: name.into(),
            detail: None,
            kind,
            range,
            selection_range,
            children: Vec::new(),
        }
    }

    /// Add a detail string.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add children symbols.
    #[must_use]
    pub fn with_children(mut self, children: Vec<DocumentSymbol>) -> Self {
        self.children = children;
        self
    }
}
