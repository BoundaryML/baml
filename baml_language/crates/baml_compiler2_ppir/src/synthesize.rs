//! AST item synthesis for compiler2 stream_* definitions.
//!
//! Replaces compiler1's `expand_cst.rs` (`GreenNode` synthesis) with direct
//! construction of `ast::Item::Class` and `ast::Item::TypeAlias`.

use baml_base::Name;
use baml_compiler2_ast as ast;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use text_size::TextRange;

use crate::{
    desugar::{PpirDesugaredClass, PpirDesugaredField, PpirDesugaredTypeAlias, PpirStreamStartsAs},
    ty::{PpirTy, PpirTypeAttrs},
};

/// Synthesize `stream_*` AST items from desugared PPIR data.
///
/// For each desugared class, emits an `Item::Class` with name `stream_{original}`.
/// For each desugared type alias, emits an `Item::TypeAlias` with name `stream_{original}`.
pub fn synthesize_stream_items(
    classes: &[PpirDesugaredClass],
    type_aliases: &[PpirDesugaredTypeAlias],
    original_class_attrs: &FxHashMap<Name, Vec<ast::RawAttribute>>,
) -> Vec<ast::Item> {
    let mut items = Vec::new();

    for class in classes {
        items.push(ast::Item::Class(synthesize_stream_class(
            class,
            original_class_attrs,
        )));
    }

    for alias in type_aliases {
        items.push(ast::Item::TypeAlias(synthesize_stream_type_alias(alias)));
    }

    items
}

/// Synthesize a `stream_*` class definition from desugared field data.
fn synthesize_stream_class(
    class: &PpirDesugaredClass,
    original_class_attrs: &FxHashMap<Name, Vec<ast::RawAttribute>>,
) -> ast::ClassDef {
    let stream_name = SmolStr::new(format!("stream_{}", class.name));

    let mut fields = Vec::new();
    for field in &class.fields {
        let final_type = compute_final_field_type(field);

        // If final type is never, omit the field entirely
        if matches!(&final_type, PpirTy::Never { .. }) {
            continue;
        }

        let type_expr = final_type.to_type_expr();
        let mut field_attrs = Vec::new();

        // @sap.in_progress(never) if @stream.done was used
        if field.sap_in_progress_never {
            field_attrs.push(make_raw_attr("sap.in_progress", "never"));
        }

        // @sap.class_completed_field_missing / @sap.class_in_progress_field_missing
        add_sap_starts_as_attrs(&field.sap_starts_as, &mut field_attrs);

        fields.push(ast::FieldDef {
            name: field.name.clone(),
            type_expr: Some(ast::SpannedTypeExpr {
                expr: type_expr,
                span: TextRange::default(),
            }),
            attributes: field_attrs,
            span: TextRange::default(),
            name_span: TextRange::default(),
        });
    }

    // Class-level attributes
    let mut class_attrs = Vec::new();
    if let Some(orig_attrs) = original_class_attrs.get(&class.name) {
        // If original class had @@stream.done, add @@sap.in_progress(never)
        let has_stream_done = orig_attrs.iter().any(|a| a.name.as_str() == "stream.done");
        if has_stream_done {
            class_attrs.push(make_raw_attr("sap.in_progress", "never"));
        }
        // Copy non-stream block attributes (e.g., @@dynamic)
        for attr in orig_attrs {
            if !attr.name.starts_with("stream.") {
                class_attrs.push(attr.clone());
            }
        }
    }

    ast::ClassDef {
        name: stream_name,
        generic_params: Vec::new(),
        fields,
        methods: Vec::new(),
        attributes: class_attrs,
        span: TextRange::default(),
        name_span: TextRange::default(),
    }
}

/// Synthesize a `stream_*` type alias from desugared body.
fn synthesize_stream_type_alias(alias: &PpirDesugaredTypeAlias) -> ast::TypeAliasDef {
    let stream_name = SmolStr::new(format!("stream_{}", alias.name));

    ast::TypeAliasDef {
        name: stream_name,
        type_expr: Some(ast::SpannedTypeExpr {
            expr: alias.expanded_body.to_type_expr(),
            span: TextRange::default(),
        }),
        span: TextRange::default(),
        name_span: TextRange::default(),
    }
}

//
// ──────────────────────────────────────── FIELD TYPE COMPUTATION ─────
//

/// Compute final field type as `typeof(starts_as) | stream_type`.
///
/// No simplification is performed here — union simplification (flattening,
/// dedup, never-removal) is deliberately deferred to TIR. See lib.rs for
/// the rationale.
fn compute_final_field_type(field: &PpirDesugaredField) -> PpirTy {
    let s_ty = field.sap_starts_as.as_ty();
    let d_ty = &field.stream_type;

    match s_ty {
        Some(ref s) if matches!(s, PpirTy::Never { .. }) => {
            // starts_as = never → final = stream_type (never | T = T)
            d_ty.clone()
        }
        Some(s) => {
            // Unsimplified union of starts_as type and stream_type
            PpirTy::Union {
                variants: vec![s, d_ty.clone()],
                attrs: PpirTypeAttrs::default(),
            }
        }
        None => d_ty.clone(),
    }
}

//
// ──────────────────────────────────────── SAP ATTRIBUTE HELPERS ─────
//

/// Add `@sap.class_completed_field_missing` and `@sap.class_in_progress_field_missing`
/// attributes based on the `starts_as` value.
fn add_sap_starts_as_attrs(starts_as: &PpirStreamStartsAs, attrs: &mut Vec<ast::RawAttribute>) {
    match starts_as {
        PpirStreamStartsAs::Never => {
            // Field absent until streaming completes — no missing attrs needed
        }
        PpirStreamStartsAs::DefaultFor(ty) => {
            // Default starts-as value from stream_type category
            if let Some(text) = default_starts_as_text(ty) {
                attrs.push(make_raw_attr("sap.class_completed_field_missing", &text));
                attrs.push(make_raw_attr("sap.class_in_progress_field_missing", &text));
            }
        }
    }
}

/// Convert a default `starts_as` type to its text representation.
fn default_starts_as_text(ty: &PpirTy) -> Option<String> {
    match ty {
        PpirTy::Null { .. } => Some("null".to_string()),
        PpirTy::List { .. } => Some("[]".to_string()),
        PpirTy::Map { .. } => Some("{}".to_string()),
        _ => None,
    }
}

//
// ──────────────────────────────────────── HELPER ─────
//

fn make_raw_attr(name: &str, value: &str) -> ast::RawAttribute {
    ast::RawAttribute {
        name: SmolStr::new(name),
        args: vec![ast::RawAttributeArg {
            key: None,
            value: value.to_string(),
            span: TextRange::default(),
        }],
        span: TextRange::default(),
    }
}
