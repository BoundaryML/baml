//! `file_outline` — per-file hierarchical symbol tree (Salsa tracked query).
//!
//! ## Design
//!
//! `file_outline` is the **one exception** to the "IDE functions are plain
//! functions" rule. It is a Salsa tracked query because:
//!
//! - Both `textDocument/documentSymbol` and `workspace/symbol` need it.
//! - It depends only on `file_symbol_contributions` + `file_item_tree`,
//!   both of which are Salsa-cached per file revision.
//! - Workspace symbol search iterates all files — caching per-file outlines
//!   avoids redundant work.
//!
//! ## Structure
//!
//! Top-level items come from `file_symbol_contributions` (which carries the
//! `name_span` for each item). Children (class fields, enum variants, methods)
//! come from the corresponding entry in `file_item_tree`.
//!
//! Note: `ClassField` and `EnumVariant` in the item tree do not carry a source
//! span of their own (Risk #1 from the plan). For Phase 2, children use a
//! zero-width range at offset 0 as a placeholder. A future phase can add spans
//! to `ClassField` / `EnumVariant` in the HIR item tree.

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::DefinitionKind;
use baml_compiler2_ppir::{file_item_tree, file_symbol_contributions};
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
/// and `file_item_tree` are Salsa-cached, so this query is cheap to re-evaluate
/// when the file hasn't changed.
///
/// Returns `Vec<OutlineItem>` in the order contributions appear (types first,
/// then values, preserving declaration order within each group).
#[salsa::tracked(returns(ref))]
pub fn file_outline(db: &dyn Db, file: SourceFile) -> Vec<OutlineItem> {
    let contribs = file_symbol_contributions(db, file);
    let item_tree = file_item_tree(db, file);

    let mut items: Vec<OutlineItem> = Vec::new();

    // ── Types: classes, enums, type aliases ───────────────────────────────────
    for (name, contrib) in &contribs.types {
        use baml_compiler2_hir::contributions::Definition;

        let children = match contrib.definition {
            Definition::Class(class_loc) => {
                let class = &item_tree[class_loc.id(db)];

                let mut child_items: Vec<OutlineItem> = Vec::new();

                // Class fields — no per-field span in the item tree yet.
                for field in &class.fields {
                    child_items.push(OutlineItem {
                        name: field.name.to_string(),
                        kind: DefinitionKind::Field,
                        name_span: TextRange::empty(TextRange::default().start()),
                        children: Vec::new(),
                    });
                }

                // Methods — look up in item_tree via their LocalItemId.
                for method_id in &class.methods {
                    let method = &item_tree[*method_id];
                    child_items.push(OutlineItem {
                        name: method.name.to_string(),
                        kind: DefinitionKind::Method,
                        name_span: TextRange::empty(method.span.start()),
                        children: Vec::new(),
                    });
                }

                child_items
            }

            Definition::Enum(enum_loc) => {
                let enum_def = &item_tree[enum_loc.id(db)];

                enum_def
                    .variants
                    .iter()
                    .map(|v| OutlineItem {
                        name: v.name.to_string(),
                        kind: DefinitionKind::Variant,
                        name_span: TextRange::empty(TextRange::default().start()),
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
        // Value-namespace items have no children in the outline for Phase 2.
        // (Function params/return type could be added in a future phase.)
        items.push(OutlineItem {
            name: name.to_string(),
            kind: contrib.definition.kind(),
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
