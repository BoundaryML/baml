//! Autocomplete/completion support for BAML.
//!
//! This module provides context-aware completions for the BAML language.
//! It detects the cursor context and provides appropriate suggestions.

mod context;
mod providers;

use baml_db::{SourceFile, baml_compiler_hir::Db, baml_compiler_parser, baml_workspace::Project};
pub use context::CompletionContext;
use text_size::TextSize;

/// A completion item to suggest to the user.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// The label shown in the completion list.
    pub label: String,
    /// The kind of completion item.
    pub kind: CompletionKind,
    /// Optional detail text shown alongside the label.
    pub detail: Option<String>,
    /// Text to insert when the completion is accepted.
    /// If None, the label is used.
    pub insert_text: Option<String>,
    /// Sort key for ordering completions.
    pub sort_text: Option<String>,
    /// Documentation for the completion item.
    pub documentation: Option<String>,
}

impl CompletionItem {
    /// Create a new completion item with just a label and kind.
    pub fn new(label: impl Into<String>, kind: CompletionKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            insert_text: None,
            sort_text: None,
            documentation: None,
        }
    }

    /// Set the detail text.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Set the insert text.
    #[must_use]
    pub fn with_insert_text(mut self, text: impl Into<String>) -> Self {
        self.insert_text = Some(text.into());
        self
    }

    /// Set the sort text.
    #[must_use]
    pub fn with_sort_text(mut self, text: impl Into<String>) -> Self {
        self.sort_text = Some(text.into());
        self
    }

    /// Set the documentation.
    #[must_use]
    pub fn with_documentation(mut self, doc: impl Into<String>) -> Self {
        self.documentation = Some(doc.into());
        self
    }
}

/// The kind of completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// A keyword (function, class, enum, etc.)
    Keyword,
    /// A function definition
    Function,
    /// A class definition
    Class,
    /// An enum definition
    Enum,
    /// An enum variant
    EnumVariant,
    /// A class field
    Field,
    /// A client definition
    Client,
    /// A type alias
    TypeAlias,
    /// A property (for ctx.*, etc.)
    Property,
    /// A code snippet
    Snippet,
    /// A generator
    Generator,
    /// A test
    Test,
    /// A template string
    TemplateString,
    /// A type (primitive or user-defined)
    Type,
}

/// Get completions at the given position in the file.
///
/// This is the main entry point for autocomplete.
pub fn complete(
    db: &dyn Db,
    file: SourceFile,
    project: Project,
    offset: TextSize,
) -> Vec<CompletionItem> {
    let text = file.text(db);

    // Get the syntax tree from the parser
    let root = baml_compiler_parser::syntax_tree(db, file);

    // Detect the completion context
    let ctx = context::detect_context(&root, offset, text);

    // Generate completions based on context
    match ctx {
        CompletionContext::TopLevel => providers::complete_top_level(),

        CompletionContext::TypeAnnotation { partial } => {
            providers::complete_types(db, project, partial.as_deref())
        }

        CompletionContext::FieldAccess { base_text } => {
            providers::complete_field_access(db, project, &base_text)
        }

        CompletionContext::FieldAttribute { partial } => {
            providers::complete_field_attributes(partial.as_deref())
        }

        CompletionContext::BlockAttribute { partial } => {
            providers::complete_block_attributes(partial.as_deref())
        }

        CompletionContext::PromptUnderscore => providers::complete_prompt_underscore(),

        CompletionContext::PromptContext { partial_path } => {
            providers::complete_prompt_ctx(&partial_path)
        }

        CompletionContext::PromptTemplate { in_interpolation } => {
            if in_interpolation {
                // Inside {{ }} - suggest variables and expressions
                providers::complete_expression_context(db, project)
            } else {
                // Outside {{ }} - suggest template helpers
                providers::complete_prompt_helpers()
            }
        }

        CompletionContext::ConfigBlock { block_type } => {
            providers::complete_config_block(&block_type)
        }

        CompletionContext::Expression => providers::complete_expression_context(db, project),

        CompletionContext::Unknown => {
            // Fallback: provide top-level and symbol completions
            let mut items = providers::complete_top_level();
            items.extend(providers::complete_symbols(db, project));
            items
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_item_builder() {
        let item = CompletionItem::new("function", CompletionKind::Keyword)
            .with_detail("Define a function")
            .with_sort_text("0function");

        assert_eq!(item.label, "function");
        assert_eq!(item.kind, CompletionKind::Keyword);
        assert_eq!(item.detail, Some("Define a function".to_string()));
        assert_eq!(item.sort_text, Some("0function".to_string()));
    }
}
