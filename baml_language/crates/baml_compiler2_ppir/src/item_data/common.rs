//! Item-data pieces shared by more than one item kind.
//!
//! Mirrors `item_tree::common`. Interfaces reuse the class field shape and the
//! function parameter shape, exactly as they do in the `ItemTree`, so those live
//! here rather than in `classes`/`functions`.

use baml_base::Name;
use baml_compiler2_hir::{item_tree::Attribute, type_ref::TypeRefId};
use text_size::TextRange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParamData {
    pub name: Name,
    pub type_ref: Option<TypeRefId>,
    /// Whether a default expression was supplied. The expression itself lives in
    /// `signature::function_parameter_defaults`.
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldData {
    pub name: Name,
    pub type_ref: Option<TypeRefId>,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeBindingData {
    pub name: Name,
    pub type_ref: Option<TypeRefId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeBindingSourceMap {
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceFieldLinkData {
    pub interface_field: Name,
    pub class_field: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceFieldLinkSourceMap {
    pub span: TextRange,
    pub interface_field_span: TextRange,
    pub class_field_span: TextRange,
}
