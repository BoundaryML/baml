//! Code lenses for BAML files.
//!
//! Code lenses are actionable links that appear inline in the editor,
//! typically above function definitions. This module provides LSP-agnostic
//! code lens computation.

use std::path::PathBuf;

use baml_db::Span;

/// A code lens action kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeLensKind {
    /// Open the BAML playground/panel for a function.
    OpenPlayground {
        /// The function name to open.
        function_name: String,
        /// Whether to show tests panel.
        show_tests: bool,
    },
    /// Run a test case.
    RunTest {
        /// The test name to run.
        test_name: String,
        /// The function the test targets.
        function_name: String,
    },
}

/// An LSP-agnostic code lens.
#[derive(Debug, Clone)]
pub struct CodeLens {
    /// The span where the lens should appear.
    pub span: Span,
    /// The title to display.
    pub title: String,
    /// The action kind.
    pub kind: CodeLensKind,
    /// The file containing the lens.
    pub file_path: PathBuf,
}

impl CodeLens {
    /// Create a new code lens.
    pub fn new(
        span: Span,
        title: impl Into<String>,
        kind: CodeLensKind,
        file_path: PathBuf,
    ) -> Self {
        Self {
            span,
            title: title.into(),
            kind,
            file_path,
        }
    }
}
