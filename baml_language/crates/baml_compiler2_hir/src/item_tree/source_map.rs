use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::ids::{
    ClassMarker, EnumMarker, FunctionMarker, InterfaceMarker, LocalItemId, TypeAliasMarker,
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
}
