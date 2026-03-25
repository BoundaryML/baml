//! Inlay hints for BAML files.
//!
//! NOTE: Stubbed — pending compiler2 LSP action reimplementation.

use baml_db::SourceFile;
use baml_project::ProjectDatabase;
use text_size::TextSize;

use crate::goto_definition::NavigationTarget;

/// The semantic kind of an inlay hint, mirroring the LSP `InlayHintKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayHintKind {
    /// A parameter-name hint, e.g. `name:` before a call argument.
    Parameter,
    /// A type hint, e.g. `: string` after a variable name.
    Type,
}

/// A single segment of an inlay hint label.
///
/// When `target` is set, the editor renders the segment as a hyperlink
/// that navigates to the target definition on click.
pub struct InlayHintLabelPart {
    /// The text to display for this segment.
    pub value: String,
    /// Optional navigation target; when set, the segment is a clickable link.
    pub target: Option<NavigationTarget>,
}

/// A text edit applied when the user double-clicks an inlay hint.
pub struct InlayHintTextEdit {
    /// Byte offset where the edit is inserted.
    pub offset: TextSize,
    /// The text to insert.
    pub new_text: String,
}

/// An inlay hint to display inline in the editor.
pub struct InlayHint {
    /// Byte offset where the hint is displayed.
    pub offset: TextSize,
    /// Label segments. Each segment may optionally carry a navigation target.
    pub label: Vec<InlayHintLabelPart>,
    /// Semantic kind used by the editor for styling/filtering.
    /// `None` means no specific kind, will fall back to a default.
    pub kind: Option<InlayHintKind>,
    /// Insert a thin space between the hint and the token to its left.
    pub padding_left: bool,
    /// Insert a thin space between the hint and the token to its right.
    pub padding_right: bool,
    /// Text edits applied when the user double-clicks the hint.
    pub text_edits: Vec<InlayHintTextEdit>,
}

/// Compute all inlay hints for the given file.
///
/// NOTE: Stubbed — returns empty. Will be reimplemented with compiler2 HIR.
#[allow(unused_variables)]
pub fn inlay_hints(
    db: &ProjectDatabase,
    file: SourceFile,
    project: baml_db::baml_workspace::Project,
) -> Vec<InlayHint> {
    Vec::new()
}
