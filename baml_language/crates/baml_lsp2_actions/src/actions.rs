//! `file_actions` — code lenses for a file (Run in Playground, Run Test).
//!
//! This is a regular function (not a Salsa query). It uses
//! `file_symbol_contributions` to find all functions and tests defined in the
//! file, then produces one `FileAction` per item with an appropriate action
//! kind.
//!
//! ## Design
//!
//! Code lenses are purely structural — they don't need type inference. We only
//! need to know *where* each function and test is in the file, which
//! `file_symbol_contributions` already gives us via `name_span`.
//!
//! ## Action kinds
//!
//! - **`RunInPlayground`**: shown on every `function` item. Clicking it opens
//!   the BAML Playground for that function.
//! - **`RunTest`**: shown on every `test` item. Clicking it runs the test in
//!   the Playground.

use baml_base::SourceFile;
use baml_compiler2_hir::{contributions::Definition, file_symbol_contributions};
use text_size::TextRange;

use crate::Db;

// ── FileActionKind ────────────────────────────────────────────────────────────

/// The kind of action represented by a `FileAction`.
///
/// Maps to the LSP command that the caller (request.rs) will attach to the
/// `CodeLens` or `CodeAction` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileActionKind {
    /// Open the BAML Playground focused on this function.
    RunInPlayground,
    /// Run this test case in the BAML Playground.
    RunTest,
}

// ── FileAction ────────────────────────────────────────────────────────────────

/// A single code-lens action attached to a named item in the file.
///
/// The `name_span` gives the byte range of the item's name token — this is
/// what the LSP uses to position the code lens above the declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAction {
    /// The item's name as it appears in source.
    pub name: String,
    /// Byte range of the name token (used to position the code lens).
    pub name_span: TextRange,
    /// What kind of action this is.
    pub kind: FileActionKind,
}

// ── file_actions ──────────────────────────────────────────────────────────────

/// Return all code-lens actions for a file.
///
/// Regular function (not cached). Internally calls `file_symbol_contributions`,
/// which is Salsa-cached per file revision.
///
/// Returns one action per function (Run in Playground) and one per test (Run
/// Test), in the order they appear in the contributions list.
pub fn file_actions(db: &dyn Db, file: SourceFile) -> Vec<FileAction> {
    let contribs = file_symbol_contributions(db, file);
    let mut actions = Vec::new();

    // Iterate value-namespace contributions: functions, tests, template strings,
    // clients, generators, retry policies all live here.
    for (_name, contrib) in &contribs.values {
        match contrib.definition {
            Definition::Function(_) => {
                actions.push(FileAction {
                    name: _name.to_string(),
                    name_span: contrib.name_span,
                    kind: FileActionKind::RunInPlayground,
                });
            }
            Definition::Test(_) => {
                actions.push(FileAction {
                    name: _name.to_string(),
                    name_span: contrib.name_span,
                    kind: FileActionKind::RunTest,
                });
            }
            // Other value-namespace items (client, generator, template_string,
            // retry_policy) don't get code lenses.
            _ => {}
        }
    }

    actions
}
