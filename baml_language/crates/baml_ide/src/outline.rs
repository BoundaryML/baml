//! `file_outline` — per-file hierarchical symbol tree (Salsa tracked query).
//!
//! ## Design
//!
//! `file_outline` is the **one exception** to the "IDE features are plain
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
//! Top-level items come from `file_symbol_contributions` (which carries each
//! item's `name_span`); their full extents come from the matching PPIR
//! item-data source map (`class_source_map`, `function_source_map`, …).
//! Children (class fields, class methods, enum variants) come from the same
//! item-data queries. The HIR item tree records only the *name token* for
//! class fields and enum variants, so those children report the name span as
//! their full extent as well; methods are items of their own and carry a real
//! declaration span.

use baml_base::SourceFile;
use baml_compiler2_ast::LetOrigin;
use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    file_symbol_contributions,
    loc::{ClassLoc, EnumLoc},
};
use baml_compiler2_ppir::item_data::{
    class_data, class_source_map, client_source_map, enum_data, enum_source_map, function_data,
    function_source_map, interface_source_map, let_data, let_source_map, retry_policy_source_map,
    template_string_source_map, type_alias_source_map,
};
use text_size::TextRange;

// ── OutlineItem ───────────────────────────────────────────────────────────────

/// A single symbol in the file's outline, with optional children.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct OutlineItem {
    /// The symbol's name as it appears in source.
    pub name: String,
    /// Symbol kind (Class, Enum, Function, Field, Variant, …).
    pub kind: DefinitionKind,
    /// Byte range of the whole item — declaration head and body
    /// (`DocumentSymbol.range`). Always contains [`Self::name_span`]. Class
    /// fields and enum variants have only their name token recorded in the
    /// item tree, so for them this equals `name_span`.
    pub range: TextRange,
    /// Span of the name token (`DocumentSymbol.selectionRange`).
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
pub fn file_outline(db: &dyn baml_compiler2_ppir::Db, file: SourceFile) -> Vec<OutlineItem> {
    let contribs = file_symbol_contributions(db, file);

    let mut items: Vec<OutlineItem> = Vec::new();

    // ── Types: classes, enums, interfaces, type aliases ───────────────────────
    for (name, contrib) in &contribs.types {
        let children = match contrib.definition {
            Definition::Class(class_loc) => class_children(db, class_loc),
            Definition::Enum(enum_loc) => enum_children(db, enum_loc),
            // Interfaces and type aliases contribute no outline children.
            Definition::Interface(_) | Definition::TypeAlias(_) => Vec::new(),
            // Value-namespace kinds never appear in the type namespace.
            Definition::Function(_)
            | Definition::TemplateString(_)
            | Definition::Client(_)
            | Definition::RetryPolicy(_)
            | Definition::Let(_) => Vec::new(),
        };

        items.push(OutlineItem {
            name: name.to_string(),
            kind: contrib.definition.kind(),
            range: definition_full_span(db, contrib.definition),
            name_span: contrib.name_span,
            children,
        });
    }

    // ── Values: functions, template strings, clients, generators, tests, retry policies ──
    for (name, contrib) in &contribs.values {
        // `Definition::is_language_internal` reads the raw HIR item tree,
        // which re-runs on every keystroke; `function_data` carries the same
        // metadata bit through the PPIR firewall. Only functions can be
        // language-internal (mirrors the `Definition` impl).
        let is_internal = match contrib.definition {
            Definition::Function(loc) => function_data(db, loc).metadata.is_language_internal,
            Definition::Class(_)
            | Definition::Enum(_)
            | Definition::Interface(_)
            | Definition::TypeAlias(_)
            | Definition::TemplateString(_)
            | Definition::Client(_)
            | Definition::RetryPolicy(_)
            | Definition::Let(_) => false,
        };
        if is_internal {
            continue;
        }

        // Value-namespace items have no children in the outline. (Function
        // params / return types could be surfaced as children later.)
        //
        // For `Definition::Let`, use the `LetOrigin` to report the symbol
        // kind the user wrote (Client or RetryPolicy) rather than the generic
        // `Let` kind. `Definition::source_kind` computes the same mapping but
        // reads the raw item tree; `let_data` is the PPIR firewall query.
        let kind = match contrib.definition {
            Definition::Let(loc) => match let_data(db, loc).origin {
                LetOrigin::Client => DefinitionKind::Client,
                LetOrigin::RetryPolicy => DefinitionKind::RetryPolicy,
                LetOrigin::Source => DefinitionKind::Let,
            },
            Definition::Class(_)
            | Definition::Enum(_)
            | Definition::Interface(_)
            | Definition::TypeAlias(_)
            | Definition::Function(_)
            | Definition::TemplateString(_)
            | Definition::Client(_)
            | Definition::RetryPolicy(_) => contrib.definition.kind(),
        };
        items.push(OutlineItem {
            name: name.to_string(),
            kind,
            range: definition_full_span(db, contrib.definition),
            name_span: contrib.name_span,
            children: Vec::new(),
        });
    }

    items
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Full declaration span of a top-level definition, from its PPIR source map.
///
/// The whole-item counterpart of [`crate::syntax::definition_span`] (which
/// returns the *name* span): every item kind's `*_source_map` firewall query
/// records the whole declaration's extent.
fn definition_full_span<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    def: Definition<'db>,
) -> TextRange {
    match def {
        Definition::Class(loc) => class_source_map(db, loc).span,
        Definition::Enum(loc) => enum_source_map(db, loc).span,
        Definition::Interface(loc) => interface_source_map(db, loc).span,
        Definition::TypeAlias(loc) => type_alias_source_map(db, loc).span,
        Definition::Function(loc) => function_source_map(db, loc).span,
        Definition::TemplateString(loc) => template_string_source_map(db, loc).span,
        Definition::Client(loc) => client_source_map(db, loc).span,
        Definition::RetryPolicy(loc) => retry_policy_source_map(db, loc).span,
        Definition::Let(loc) => let_source_map(db, loc).span,
    }
}

/// Children of a class outline item: fields first, then methods.
fn class_children<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class_loc: ClassLoc<'db>,
) -> Vec<OutlineItem> {
    let class = class_data(db, class_loc);
    let source_map = class_source_map(db, class_loc);
    debug_assert_eq!(
        class.fields.len(),
        source_map.field_name_spans.len(),
        "field name spans are parallel to fields"
    );

    // Class fields — the item tree records only the field's name token, so
    // the full range and the name span coincide.
    let mut children: Vec<OutlineItem> = class
        .fields
        .iter()
        .zip(&source_map.field_name_spans)
        .map(|(field, &name_span)| OutlineItem {
            name: field.name.to_string(),
            kind: DefinitionKind::Field,
            range: name_span,
            name_span,
            children: Vec::new(),
        })
        .collect();

    // Methods — resolve each `FunctionLoc` via the firewall. `ClassData`
    // methods always live in the class's own file, so their spans are valid
    // ranges within this outline's document.
    for &method_loc in &class.methods {
        let method = function_data(db, method_loc);
        if method.metadata.is_language_internal {
            continue;
        }
        let method_map = function_source_map(db, method_loc);
        children.push(OutlineItem {
            name: method.name.to_string(),
            kind: DefinitionKind::Method,
            range: method_map.span,
            name_span: method_map.name_span,
            children: Vec::new(),
        });
    }

    children
}

/// Children of an enum outline item: its variants.
fn enum_children<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    enum_loc: EnumLoc<'db>,
) -> Vec<OutlineItem> {
    let enum_def = enum_data(db, enum_loc);
    let source_map = enum_source_map(db, enum_loc);
    debug_assert_eq!(
        enum_def.variants.len(),
        source_map.variant_name_spans.len(),
        "variant name spans are parallel to variants"
    );

    // Enum variants — like class fields, only the name token is recorded.
    enum_def
        .variants
        .iter()
        .zip(&source_map.variant_name_spans)
        .map(|(variant, &name_span)| OutlineItem {
            name: variant.name.to_string(),
            kind: DefinitionKind::Variant,
            range: name_span,
            name_span,
            children: Vec::new(),
        })
        .collect()
}

// `Vec<OutlineItem>` satisfies `salsa::Update` because `Vec<T>` implements it
// when `T: salsa::Update`, and `OutlineItem` derives it above.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ProjectTest;

    fn span_text(text: &str, range: TextRange) -> &str {
        &text[range]
    }

    fn make_impl_method_project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source(
            "types.baml",
            r#"
interface Animal {
    function speak(self) -> string
}

class Dog {}

implements Animal for Dog {
    function speak(self) -> string { return "woof" }
}
"#,
        );
        builder.build()
    }

    #[test]
    fn top_level_items_carry_full_ranges_and_name_spans() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "types.baml",
            r#"
class Point {
    x int
    y int
}

enum Color {
    Red
    Green
}

function zero() -> int {
    return 0;
}
"#,
        );
        let project = builder.build();
        let file = project.files[0];
        let text = file.text(&project.db);
        let outline = file_outline(&project.db, file);

        let names: Vec<&str> = outline.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, ["Point", "Color", "zero"]);

        let point = &outline[0];
        assert_eq!(point.kind, DefinitionKind::Class);
        assert_eq!(span_text(text, point.name_span), "Point");
        assert!(point.range.contains_range(point.name_span));
        assert!(span_text(text, point.range).starts_with("class Point"));
        assert!(span_text(text, point.range).ends_with('}'));

        let field_names: Vec<&str> = point
            .children
            .iter()
            .map(|child| child.name.as_str())
            .collect();
        assert_eq!(field_names, ["x", "y"]);
        for child in &point.children {
            assert_eq!(child.kind, DefinitionKind::Field);
            assert_eq!(span_text(text, child.name_span), child.name);
            // Only the name token is recorded for fields.
            assert_eq!(child.range, child.name_span);
        }

        let color = &outline[1];
        assert_eq!(color.kind, DefinitionKind::Enum);
        assert!(color.range.contains_range(color.name_span));
        let variant_names: Vec<&str> = color
            .children
            .iter()
            .map(|child| child.name.as_str())
            .collect();
        assert_eq!(variant_names, ["Red", "Green"]);
        for child in &color.children {
            assert_eq!(child.kind, DefinitionKind::Variant);
            assert_eq!(span_text(text, child.name_span), child.name);
            assert_eq!(child.range, child.name_span);
        }

        let zero = &outline[2];
        assert_eq!(zero.kind, DefinitionKind::Function);
        assert_eq!(span_text(text, zero.name_span), "zero");
        assert!(zero.range.contains_range(zero.name_span));
        assert!(span_text(text, zero.range).starts_with("function zero"));
        assert!(span_text(text, zero.range).contains("return 0"));
    }

    #[test]
    fn out_of_body_implements_method_appears_in_class_outline() {
        let project = make_impl_method_project();

        let outline = file_outline(&project.db, project.files[0]);
        let dog = outline
            .iter()
            .find(|item| item.name == "Dog")
            .expect("Dog should appear in outline");

        assert!(
            dog.children
                .iter()
                .any(|child| child.name == "speak" && child.kind == DefinitionKind::Method),
            "expected out-of-body impl method to be visible as a Dog outline child, got: {:?}",
            dog.children
        );
    }

    #[test]
    fn method_name_span_is_name_token_and_range_covers_declaration() {
        let project = make_impl_method_project();
        let file = project.files[0];
        let text = file.text(&project.db);

        let outline = file_outline(&project.db, file);
        let dog = outline
            .iter()
            .find(|item| item.name == "Dog")
            .expect("Dog should appear in outline");
        let speak = dog
            .children
            .iter()
            .find(|child| child.name == "speak" && child.kind == DefinitionKind::Method)
            .expect("speak method should appear as a Dog outline child");

        // The name span is the actual `speak` token of the definition inside
        // the implements block — not a zero-width marker at the declaration
        // start, and not the interface's signature.
        assert!(!speak.name_span.is_empty());
        assert_eq!(span_text(text, speak.name_span), "speak");
        let impl_start = text
            .find("implements Animal")
            .expect("fixture contains an implements block");
        assert!(usize::from(speak.name_span.start()) > impl_start);

        // The full range covers the whole method declaration, body included.
        assert!(speak.range.contains_range(speak.name_span));
        let method_text = span_text(text, speak.range);
        assert!(method_text.starts_with("function speak"));
        assert!(method_text.contains("woof"));
    }
}
