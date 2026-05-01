//! `grep` — semantic code search for BAML files.
//!
//! The core `grep()` function routes between semantic and text search modes:
//! - If the pattern matches a known symbol name → delegate to `describe()`
//! - Otherwise → text search with semantic annotations on matches
//!
//! This is a regular function (not a Salsa query).

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::DefinitionKind;

use crate::{
    Db,
    describe::{SymbolDescription, describe},
    search::{SymbolInfo, search_symbols},
};

// ── Types ────────────────────────────────────────────────────────────────────

/// Result of a grep operation.
pub struct GrepResult {
    /// Whether the result came from semantic lookup or text search.
    pub mode: GrepMode,
    /// Symbol descriptions (populated in Semantic mode).
    pub descriptions: Vec<SymbolDescription>,
    /// Text search matches (populated in `TextSearch` mode).
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
    /// File path as a string, for display and JSON consumers.
    pub file_path: String,
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
    Reference {
        target_name: String,
        target_kind: DefinitionKind,
    },
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

/// Index of definition name spans for CST-based annotation.
struct OutlineIndex {
    /// Maps (file, offset) to (`symbol_name`, kind) for definition sites.
    def_spans: std::collections::HashMap<(SourceFile, u32), (String, DefinitionKind)>,
    /// Maps symbol name to kind for reference annotation.
    symbol_kinds: std::collections::HashMap<String, DefinitionKind>,
}

impl OutlineIndex {
    fn build(db: &dyn Db, files: &[SourceFile]) -> Self {
        let mut def_spans = std::collections::HashMap::new();
        let mut symbol_kinds = std::collections::HashMap::new();

        for &file in files {
            let outline = crate::outline::file_outline(db, file);
            for item in outline {
                // Record the name span range for definition detection.
                let start: u32 = item.name_span.start().into();
                let end: u32 = item.name_span.end().into();
                for offset in start..end {
                    def_spans.insert((file, offset), (item.name.clone(), item.kind));
                }
                symbol_kinds.insert(item.name.clone(), item.kind);

                // Also record children (fields, variants).
                for child in &item.children {
                    let child_start: u32 = child.name_span.start().into();
                    let child_end: u32 = child.name_span.end().into();
                    for offset in child_start..child_end {
                        def_spans.insert((file, offset), (child.name.clone(), child.kind));
                    }
                    symbol_kinds.insert(child.name.clone(), child.kind);
                }
            }
        }

        OutlineIndex {
            def_spans,
            symbol_kinds,
        }
    }

    fn is_definition(&self, file: SourceFile, offset: u32) -> Option<(String, DefinitionKind)> {
        self.def_spans.get(&(file, offset)).cloned()
    }

    fn get_kind(&self, name: &str) -> Option<DefinitionKind> {
        self.symbol_kinds.get(name).copied()
    }
}

/// Text search across all source files with semantic annotations.
fn text_search(db: &dyn Db, files: &[SourceFile], opts: &GrepOptions<'_>) -> Vec<TextMatch> {
    let pattern = if opts.ignore_case {
        opts.pattern.to_lowercase()
    } else {
        opts.pattern.to_string()
    };

    // Build an outline index for CST-based semantic annotation.
    let index = OutlineIndex::build(db, files);

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

        let mut line_start_offset = 0usize;
        for (line_idx, raw_line) in text.split_inclusive('\n').enumerate() {
            // Strip trailing newline chars for matching and display.
            let line = raw_line.trim_end_matches(['\n', '\r']);

            let line_to_check = if opts.ignore_case {
                line.to_lowercase()
            } else {
                line.to_string()
            };

            if let Some(match_col) = line_to_check.find(&pattern) {
                // Try to annotate the match semantically using CST-based lookup.
                let annotation =
                    annotate_match(file, line_start_offset, match_col, opts.pattern, &index);

                matches.push(TextMatch {
                    file_path: file.path(db).display().to_string(),
                    file,
                    line_number: line_idx + 1,
                    line_text: line.to_string(),
                    annotation,
                });
            }

            // Advance by the full raw slice length (includes \n or \r\n).
            line_start_offset += raw_line.len();
        }
    }

    matches
}

/// Try to annotate a match with semantic information using CST-based lookup.
fn annotate_match(
    file: SourceFile,
    line_start_offset: usize,
    match_col: usize,
    pattern: &str,
    index: &OutlineIndex,
) -> Option<MatchAnnotation> {
    #[allow(clippy::cast_possible_truncation)]
    let match_offset = (line_start_offset + match_col) as u32;

    // Check if this offset falls within a definition's name span.
    if let Some((name, kind)) = index.is_definition(file, match_offset) {
        // Verify the match is actually for this symbol (pattern matches the name).
        if name.contains(pattern) || pattern.contains(&name) || name.eq_ignore_ascii_case(pattern) {
            return Some(MatchAnnotation::Definition { kind });
        }
    }

    // Otherwise, if the pattern is a known symbol name, it's a reference.
    if let Some(kind) = index.get_kind(pattern) {
        return Some(MatchAnnotation::Reference {
            target_name: pattern.to_string(),
            target_kind: kind,
        });
    }

    None
}
