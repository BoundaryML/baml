//! `file_outline` — per-file hierarchical symbol tree (Salsa tracked query).
//!
//! ## Design
//!
//! `file_outline` is the **one exception** to the "IDE functions are plain
//! functions" rule. It is a Salsa tracked query because:
//!
//! - Both `textDocument/documentSymbol` and `workspace/symbol` need it.
//! - It depends only on `file_symbol_contributions` + the PPIR item-data
//!   firewall queries, all of which are Salsa-cached per file revision.
//! - Workspace symbol search iterates all files — caching per-file outlines
//!   avoids redundant work.
//!
//! ## Structure
//!
//! Top-level items come from `file_symbol_contributions` (which carries the
//! `name_span` for each item). Children (class fields, enum variants, methods)
//! come from the corresponding PPIR item-data firewall query
//! (`class_data`/`class_source_map`, `enum_data`/`enum_source_map`, …).
//!
//! Note: `ClassField` and `EnumVariant` in the item tree do not carry a source
//! span of their own (Risk #1 from the plan). For Phase 2, children use a
//! zero-width range at offset 0 as a placeholder. A future phase can add spans
//! to `ClassField` / `EnumVariant` in the HIR item tree.

use baml_base::SourceFile;
use baml_compiler2_hir::{contributions::DefinitionKind, file_symbol_contributions};
use baml_compiler2_ppir::item_data::{
    class_data, class_source_map, enum_data, enum_source_map, function_data, function_source_map,
    let_data,
};
use text_size::TextRange;

use crate::Db;

// ── OutlineItem ───────────────────────────────────────────────────────────────

/// A single symbol in the file's outline, with optional children.
///
/// Top-level items carry a non-empty `name_span` from the HIR contributions.
/// Child items (fields, variants) use a zero-width placeholder range until the
/// HIR item tree tracks their source spans.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct OutlineItem {
    /// The symbol's name as it appears in source.
    pub name: String,
    /// Symbol kind (Class, Enum, Function, Field, Variant, …).
    pub kind: DefinitionKind,
    /// Span of the name token (not the full item body).
    pub name_span: TextRange,
    /// Nested symbols: class fields + methods, enum variants.
    pub children: Vec<OutlineItem>,
}

// ── file_outline ──────────────────────────────────────────────────────────────

/// Hierarchical symbol outline for a single file.
///
/// Salsa tracked query — cached per file revision. Both `file_symbol_contributions`
/// and the PPIR item-data firewall queries are Salsa-cached, so this query is
/// cheap to re-evaluate when the file hasn't changed.
///
/// Returns `Vec<OutlineItem>` in the order contributions appear (types first,
/// then values, preserving declaration order within each group).
#[salsa::tracked(returns(ref))]
pub fn file_outline(db: &dyn Db, file: SourceFile) -> Vec<OutlineItem> {
    let contribs = file_symbol_contributions(db, file);

    let mut items: Vec<OutlineItem> = Vec::new();

    // ── Types: classes, enums, type aliases ───────────────────────────────────
    for (name, contrib) in &contribs.types {
        use baml_compiler2_hir::contributions::Definition;

        let children = match contrib.definition {
            Definition::Class(class_loc) => {
                let class = class_data(db, class_loc);
                let field_name_spans = &class_source_map(db, class_loc).field_name_spans;

                let mut child_items: Vec<OutlineItem> = Vec::new();

                // Class fields — use real name spans from the source map.
                for (i, field) in class.fields.iter().enumerate() {
                    child_items.push(OutlineItem {
                        name: field.name.to_string(),
                        kind: DefinitionKind::Field,
                        name_span: field_name_spans
                            .get(i)
                            .copied()
                            .unwrap_or_else(|| TextRange::empty(TextRange::default().start())),
                        children: Vec::new(),
                    });
                }

                // Methods — resolve each `FunctionLoc` via the firewall.
                for method_loc in &class.methods {
                    let method = function_data(db, *method_loc);
                    if method.metadata.is_language_internal {
                        continue;
                    }
                    child_items.push(OutlineItem {
                        name: method.name.to_string(),
                        kind: DefinitionKind::Method,
                        name_span: TextRange::empty(
                            function_source_map(db, *method_loc).span.start(),
                        ),
                        children: Vec::new(),
                    });
                }

                child_items
            }

            Definition::Enum(enum_loc) => {
                let enum_def = enum_data(db, enum_loc);
                let variant_name_spans = &enum_source_map(db, enum_loc).variant_name_spans;

                enum_def
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| OutlineItem {
                        name: v.name.to_string(),
                        kind: DefinitionKind::Variant,
                        name_span: variant_name_spans
                            .get(i)
                            .copied()
                            .unwrap_or_else(|| TextRange::empty(TextRange::default().start())),
                        children: Vec::new(),
                    })
                    .collect()
            }

            // TypeAlias has no children.
            _ => Vec::new(),
        };

        items.push(OutlineItem {
            name: name.to_string(),
            kind: contrib.definition.kind(),
            name_span: contrib.name_span,
            children,
        });
    }

    // ── Values: functions, template strings, clients, generators, tests, retry policies ──
    for (name, contrib) in &contribs.values {
        if contrib.definition.is_language_internal(db) {
            continue;
        }
        // Value-namespace items have no children in the outline for Phase 2.
        // (Function params/return type could be added in a future phase.)
        //
        // For `Definition::Let`, use the `LetOrigin` to report the correct symbol kind
        // (Client or RetryPolicy) rather than the generic `Let` kind.
        let kind = match contrib.definition {
            baml_compiler2_hir::contributions::Definition::Let(loc) => {
                match let_data(db, loc).origin {
                    baml_compiler2_ast::ast::LetOrigin::Client => DefinitionKind::Client,
                    baml_compiler2_ast::ast::LetOrigin::RetryPolicy => DefinitionKind::RetryPolicy,
                    baml_compiler2_ast::ast::LetOrigin::Source => DefinitionKind::Let,
                }
            }
            other => other.kind(),
        };
        items.push(OutlineItem {
            name: name.to_string(),
            kind,
            name_span: contrib.name_span,
            children: Vec::new(),
        });
    }

    items
}

// ── salsa::Update impl ────────────────────────────────────────────────────────
//
// `Vec<OutlineItem>` satisfies `salsa::Update` automatically because `Vec<T>`
// implements it when `T: salsa::Update`, and we derived `salsa::Update` on
// `OutlineItem` above. The `DefinitionKind` and `TextRange` fields are `Copy +
// PartialEq`, so the derive works without manual impls.
