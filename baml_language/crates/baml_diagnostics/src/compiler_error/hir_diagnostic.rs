// ============================================================================
// HIR Lowering Diagnostics
// ============================================================================

use baml_base::Span;

/// Diagnostics that occur during HIR lowering (per-file validation).
///
/// These are distinct from `NameError` which handles project-wide validation
/// like duplicate class/function names across files. `HirDiagnostic` handles
/// within-item validation like duplicate fields or attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirDiagnostic {
    /// Duplicate field in a class.
    DuplicateField {
        class_name: String,
        field_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate variant in an enum.
    DuplicateVariant {
        enum_name: String,
        variant_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate block attribute (@@) on an item.
    DuplicateBlockAttribute {
        item_kind: &'static str, // "class", "enum", "function"
        item_name: String,
        attr_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate field attribute (@) on a field/variant.
    DuplicateFieldAttribute {
        container_kind: &'static str, // "class", "enum"
        container_name: String,
        field_name: String,
        attr_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Unknown attribute.
    UnknownAttribute {
        attr_name: String,
        span: Span,
        valid_attributes: Vec<&'static str>,
    },

    /// Attribute used in wrong context.
    InvalidAttributeContext {
        attr_name: String,
        context: &'static str,          // "type alias", "test field"
        allowed_contexts: &'static str, // "class fields, enum variants"
        span: Span,
    },
}
