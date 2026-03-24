//! Hover information for BAML symbols.
//!
//! NOTE: Stubbed — pending compiler2 LSP action reimplementation.

use baml_db::SourceFile;
use text_size::TextSize;

use crate::{MarkupKind, RangedValue};

/// Hover information for a symbol.
#[derive(Debug, Clone)]
pub struct Hover {
    contents: Vec<HoverContent>,
}

/// Content within a hover popup.
#[derive(Debug, Clone)]
pub enum HoverContent {
    /// A code signature (function, class, etc.).
    Signature(String),
    /// Documentation text.
    Docstring(String),
}

impl Hover {
    /// Create a new hover with signature content.
    pub fn with_signature(signature: String) -> Self {
        Self {
            contents: vec![HoverContent::Signature(signature)],
        }
    }

    /// Check if hover has any content.
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    /// Format the hover for display.
    pub fn display(&self, kind: MarkupKind) -> String {
        let mut result = String::new();
        for content in &self.contents {
            match content {
                HoverContent::Signature(sig) => match kind {
                    MarkupKind::PlainText => result.push_str(sig),
                    MarkupKind::Markdown => {
                        result.push_str("```baml\n");
                        result.push_str(sig);
                        result.push_str("\n```");
                    }
                },
                HoverContent::Docstring(doc) => {
                    if !result.is_empty() {
                        result.push_str("\n\n");
                    }
                    result.push_str(doc);
                }
            }
        }
        result
    }
}

/// Get hover information at the given position.
///
/// NOTE: Stubbed — returns None. Will be reimplemented with compiler2 HIR.
#[allow(unused_variables)]
pub fn hover(
    db: &dyn baml_db::baml_compiler2_hir::Db,
    file: SourceFile,
    project: baml_db::baml_workspace::Project,
    offset: TextSize,
) -> Option<RangedValue<Hover>> {
    None
}
