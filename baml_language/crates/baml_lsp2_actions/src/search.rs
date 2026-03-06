//! `search_symbols` — cross-file symbol search (workspace/symbol).
//!
//! This is a regular function (not a Salsa query). Caching happens at the
//! `file_outline` layer — which is Salsa-tracked per file. Workspace symbol
//! search iterates all files and filters by the query string.
//!
//! ## Usage
//!
//! Called by `bex_project` to handle `workspace/symbol` requests.
//! Pass the user-visible source files (not builtin stubs) and the query string.

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::DefinitionKind;
use text_size::TextRange;

use crate::{Db, outline::file_outline};

// ── SymbolInfo ────────────────────────────────────────────────────────────────

/// A symbol result from workspace-wide search.
///
/// Contains all the information needed by the LSP layer to build a
/// `WorkspaceSymbol` response: name, kind, source file, and name span.
#[derive(Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    /// The symbol's display name.
    pub name: String,
    /// Symbol kind (Class, Enum, Function, Field, Variant, …).
    pub kind: DefinitionKind,
    /// The file where this symbol is defined.
    pub file: SourceFile,
    /// Byte range of the name token in the source file.
    pub name_span: TextRange,
    /// The container name, if this is a child symbol (e.g. field in a class).
    pub container_name: Option<String>,
}

// ── search_symbols ────────────────────────────────────────────────────────────

/// Search for symbols across a set of files, filtered by a query string.
///
/// Regular function (not cached) — calls `file_outline(db, file)` for each
/// file, which IS Salsa-cached. So repeat calls for unchanged files are free.
///
/// ## Matching
///
/// The query is matched case-insensitively as a substring of the symbol name.
/// An empty query matches all symbols (for workspace symbol browsing).
///
/// ## Files
///
/// Pass only user source files, not builtin stubs. The `ProjectDatabase`
/// provides `get_source_files()` for this purpose.
pub fn search_symbols(db: &dyn Db, files: &[SourceFile], query: &str) -> Vec<SymbolInfo> {
    let query_lower = query.to_lowercase();
    let mut results: Vec<SymbolInfo> = Vec::new();

    for &file in files {
        let outline = file_outline(db, file);

        for item in outline {
            // Check the top-level item itself.
            if query_matches(&item.name, &query_lower) {
                results.push(SymbolInfo {
                    name: item.name.clone(),
                    kind: item.kind,
                    file,
                    name_span: item.name_span,
                    container_name: None,
                });
            }

            // Check children (class fields, enum variants, methods).
            for child in &item.children {
                if query_matches(&child.name, &query_lower) {
                    results.push(SymbolInfo {
                        name: child.name.clone(),
                        kind: child.kind,
                        file,
                        name_span: child.name_span,
                        container_name: Some(item.name.clone()),
                    });
                }
            }
        }
    }

    results
}

/// Returns `true` if `name` (lowercased) contains `query_lower` as a substring.
///
/// An empty `query_lower` always matches (browse all symbols).
#[inline]
fn query_matches(name: &str, query_lower: &str) -> bool {
    query_lower.is_empty() || name.to_lowercase().contains(query_lower)
}
