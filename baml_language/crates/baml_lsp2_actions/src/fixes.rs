//! `fixes_at` — quick-fixes and code actions available at a source range.
//!
//! This is a regular function (not a Salsa query). It returns the set of
//! `Fix` values that are applicable at or overlapping the given `range` in the
//! file.
//!
//! ## Current scope (Phase 9)
//!
//! Initially minimal: one "Open in Playground" action is surfaced regardless
//! of what is under the cursor. This mirrors the behaviour of the inline v1
//! handler, which always returned a single `OpenBamlPanel` code action.
//!
//! Future phases can grow this to include proper quick-fixes (add missing
//! field, fix type annotation, etc.) driven by diagnostics in the range.
//!
//! ## Why `range` instead of `offset`?
//!
//! Code actions are always requested for a range (the current selection or the
//! diagnostic span). We keep the parameter consistent with the LSP spec even
//! though the initial implementation does not filter on it.

use baml_base::SourceFile;
use text_size::TextRange;

use crate::Db;

// ── Fix ───────────────────────────────────────────────────────────────────────

/// The kind of fix / code action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixKind {
    /// Open the BAML Playground. Carries an optional function name to focus on.
    OpenInPlayground { function_name: Option<String> },
}

/// A single quick-fix or refactor applicable at a source range.
///
/// The caller (request.rs) is responsible for converting this to the
/// `lsp_types::CodeAction` or `lsp_types::Command` the LSP layer expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// Human-readable title shown in the editor UI.
    pub title: String,
    /// The action to perform.
    pub kind: FixKind,
}

// ── fixes_at ──────────────────────────────────────────────────────────────────

/// Return quick-fixes and code actions applicable at `range` in `file`.
///
/// Regular function (not cached). Currently returns a single "Open in
/// Playground" action unconditionally; future phases will filter by diagnostics
/// that overlap `range` and add proper quick-fixes.
pub fn fixes_at(db: &dyn Db, file: SourceFile, _range: TextRange) -> Vec<Fix> {
    // Suppress unused-parameter warnings — `file` and `_range` will be used
    // once we implement diagnostic-driven fixes.
    let _ = db;
    let _ = file;

    vec![Fix {
        title: "Open in Playground".to_string(),
        kind: FixKind::OpenInPlayground {
            function_name: None,
        },
    }]
}
