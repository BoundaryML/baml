//! `grep` — semantic code search for BAML files.
//!
//! The core `grep()` function routes between semantic and text search modes:
//! - If the pattern matches a known symbol name → delegate to `describe()`
//! - Otherwise → text search with semantic annotations on matches
//!
//! This is a regular function (not a Salsa query).

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::DefinitionKind;

use crate::Db;
use crate::describe::{SymbolDescription, describe};
use crate::search::{SymbolInfo, search_symbols};

// ── Types ────────────────────────────────────────────────────────────────────

/// Result of a grep operation.
pub struct GrepResult {
    /// Whether the result came from semantic lookup or text search.
    pub mode: GrepMode,
    /// Symbol descriptions (populated in Semantic mode).
    pub descriptions: Vec<SymbolDescription>,
    /// Text search matches (populated in TextSearch mode).
    pub text_matches: Vec<TextMatch>,
}

/// How the grep was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrepMode {
    /// Pattern matched a known symbol — results come from `describe()`.
    Semantic,
    /// Fallback to text matching with semantic annotations.
    TextSearch,
}

/// A single text search match with optional semantic annotation.
pub struct TextMatch {
    pub file: SourceFile,
    /// 1-based line number.
    pub line_number: usize,
    /// The full text of the matching line.
    pub line_text: String,
    /// What this match represents semantically, if resolvable.
    pub annotation: Option<MatchAnnotation>,
}

/// Semantic annotation on a text match.
#[derive(Debug, Clone)]
pub enum MatchAnnotation {
    /// This match is a definition site.
    Definition { kind: DefinitionKind },
    /// This match is a reference to a known symbol.
    Reference { target_name: String, target_kind: DefinitionKind },
}

// ── grep ─────────────────────────────────────────────────────────────────────

/// Options for grep.
pub struct GrepOptions<'a> {
    /// The search pattern.
    pub pattern: &'a str,
    /// Case-insensitive matching (for text search mode).
    pub ignore_case: bool,
    /// Filter results to specific symbol kinds.
    pub kind_filter: &'a [DefinitionKind],
}

/// Semantic grep: if pattern is a known symbol, describe it; else text search.
pub fn grep(db: &dyn Db, files: &[SourceFile], opts: &GrepOptions<'_>) -> GrepResult {
    // Try semantic lookup first.
    let descriptions = describe(db, files, opts.pattern);

    if !descriptions.is_empty() {
        let descriptions = if opts.kind_filter.is_empty() {
            descriptions
        } else {
            descriptions
                .into_iter()
                .filter(|d| opts.kind_filter.contains(&d.kind))
                .collect()
        };
        return GrepResult {
            mode: GrepMode::Semantic,
            descriptions,
            text_matches: Vec::new(),
        };
    }

    // Fall back to text search.
    let text_matches = text_search(db, files, opts);

    GrepResult {
        mode: GrepMode::TextSearch,
        descriptions: Vec::new(),
        text_matches,
    }
}

/// List all symbols in the project, optionally filtered by kind.
pub fn list_symbols(
    db: &dyn Db,
    files: &[SourceFile],
    kind_filter: &[DefinitionKind],
) -> Vec<SymbolInfo> {
    let all = search_symbols(db, files, "");
    if kind_filter.is_empty() {
        // Only top-level symbols for listing.
        all.into_iter()
            .filter(|s| s.container_name.is_none())
            .collect()
    } else {
        all.into_iter()
            .filter(|s| s.container_name.is_none() && kind_filter.contains(&s.kind))
            .collect()
    }
}

// ── Text search ──────────────────────────────────────────────────────────────

/// Text search across all source files with semantic annotations.
fn text_search(db: &dyn Db, files: &[SourceFile], opts: &GrepOptions<'_>) -> Vec<TextMatch> {
    let pattern = if opts.ignore_case {
        opts.pattern.to_lowercase()
    } else {
        opts.pattern.to_string()
    };

    // Build an outline lookup for semantic annotation.
    let mut symbol_names: std::collections::HashMap<String, DefinitionKind> =
        std::collections::HashMap::new();
    for &file in files {
        let outline = crate::outline::file_outline(db, file);
        for item in outline {
            symbol_names.insert(item.name.clone(), item.kind);
            for child in &item.children {
                symbol_names.insert(
                    format!("{}.{}", item.name, child.name),
                    child.kind,
                );
            }
        }
    }

    let mut matches = Vec::new();

    for &file in files {
        let text = file.text(db);

        // Pre-filter: skip files that don't contain the pattern.
        let text_to_check = if opts.ignore_case {
            text.to_lowercase()
        } else {
            text.clone()
        };
        if !text_to_check.contains(&pattern) {
            continue;
        }

        for (line_idx, line) in text.lines().enumerate() {
            let line_to_check = if opts.ignore_case {
                line.to_lowercase()
            } else {
                line.to_string()
            };

            if !line_to_check.contains(&pattern) {
                continue;
            }

            // Try to annotate the match semantically.
            let annotation = annotate_line(line, opts.pattern, &symbol_names);

            matches.push(TextMatch {
                file,
                line_number: line_idx + 1,
                line_text: line.to_string(),
                annotation,
            });
        }
    }

    matches
}

/// Try to annotate a matching line with semantic information.
fn annotate_line(
    line: &str,
    pattern: &str,
    symbol_names: &std::collections::HashMap<String, DefinitionKind>,
) -> Option<MatchAnnotation> {
    // Check if the pattern itself is a known symbol name.
    if let Some(&kind) = symbol_names.get(pattern) {
        // Heuristic: if the line looks like a definition (starts with a keyword
        // followed by the name), mark it as a definition.
        let trimmed = line.trim();
        let def_keywords = [
            "class ", "enum ", "function ", "client ", "client<",
            "test ", "type ", "retry_policy ", "template_string ",
            "generator ",
        ];
        if def_keywords.iter().any(|kw| trimmed.starts_with(kw)) {
            return Some(MatchAnnotation::Definition { kind });
        }
        return Some(MatchAnnotation::Reference {
            target_name: pattern.to_string(),
            target_kind: kind,
        });
    }

    None
}
