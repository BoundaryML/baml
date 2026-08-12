use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::ids::{
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, InterfaceMarker, LocalItemId,
    RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker,
};

/// Parallel source map for `ItemTree` — stores name spans that are
/// deliberately excluded from the semantic `ItemTree` to avoid polluting
/// Salsa's early-cutoff comparisons with position data.
///
/// Follows the same body/signature source-map pattern used by
/// `function_body` / `function_body_source_map`.
#[derive(Debug, Clone, Default)]
pub struct ItemTreeSourceMap {
    /// `name_span` for each class.
    pub class_name_spans: FxHashMap<LocalItemId<ClassMarker>, TextRange>,
    /// `name_span` for each class's fields, parallel to `Class::fields`.
    pub class_field_spans: FxHashMap<LocalItemId<ClassMarker>, Vec<TextRange>>,
    /// `name_span` for each enum.
    pub enum_name_spans: FxHashMap<LocalItemId<EnumMarker>, TextRange>,
    /// `name_span` for each enum's variants, parallel to `Enum::variants`.
    pub enum_variant_spans: FxHashMap<LocalItemId<EnumMarker>, Vec<TextRange>>,
    /// `name_span` for each function.
    pub function_name_spans: FxHashMap<LocalItemId<FunctionMarker>, TextRange>,
    /// `name_span` for each interface.
    pub interface_name_spans: FxHashMap<LocalItemId<InterfaceMarker>, TextRange>,
    /// `name_span` for each interface's fields, parallel to `Interface::fields`.
    pub interface_field_spans: FxHashMap<LocalItemId<InterfaceMarker>, Vec<TextRange>>,
    /// `name_span` for each interface's required methods, parallel to
    /// `Interface::required_methods`.
    pub interface_method_spans: FxHashMap<LocalItemId<InterfaceMarker>, Vec<TextRange>>,
    /// `name_span` for each type alias.
    pub type_alias_name_spans: FxHashMap<LocalItemId<TypeAliasMarker>, TextRange>,
    /// `name_span` for each template string.
    pub template_string_name_spans: FxHashMap<LocalItemId<TemplateStringMarker>, TextRange>,
    /// `(span, name_span)` for each client. The full span rides along because
    /// the semantic `Client` deliberately stores no span at all.
    pub client_spans: FxHashMap<LocalItemId<ClientMarker>, ItemSpans>,
    /// `(span, name_span)` for each test — same rationale as `client_spans`.
    pub test_spans: FxHashMap<LocalItemId<TestMarker>, ItemSpans>,
    /// `(span, name_span)` for each retry policy — same rationale as
    /// `client_spans`.
    pub retry_policy_spans: FxHashMap<LocalItemId<RetryPolicyMarker>, ItemSpans>,
}

/// Full declaration span plus name-token span, for item kinds whose semantic
/// `ItemTree` entry carries no span of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemSpans {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of the name token.
    pub name_span: TextRange,
}
