//! Code actions for BAML files.
//!
//! Code actions are quick fixes and refactorings that can be applied to code.
//! This module provides LSP-agnostic code action types.

/// The kind of a code action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeActionKind {
    /// Open the BAML playground in browser.
    OpenPlaygroundInBrowser {
        /// Optional function name to focus on.
        function_name: Option<String>,
    },
    /// Generate client code.
    GenerateClient,
    /// Other custom actions.
    Custom(String),
}

/// An LSP-agnostic code action.
#[derive(Debug, Clone)]
pub struct CodeAction {
    /// The title to display.
    pub title: String,
    /// The kind of action.
    pub kind: CodeActionKind,
    /// Whether this action is preferred/recommended.
    pub is_preferred: bool,
}

impl CodeAction {
    /// Create a new code action.
    pub fn new(title: impl Into<String>, kind: CodeActionKind) -> Self {
        Self {
            title: title.into(),
            kind,
            is_preferred: false,
        }
    }

    /// Mark this action as preferred.
    #[must_use]
    pub fn preferred(mut self) -> Self {
        self.is_preferred = true;
        self
    }
}
