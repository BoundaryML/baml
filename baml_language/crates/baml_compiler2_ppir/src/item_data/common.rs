//! Item-data pieces shared by more than one item kind.
//!
//! Mirrors `item_tree::common`. Interfaces reuse the class field shape and the
//! function parameter shape, exactly as they do in the `ItemTree`, so those live
//! here rather than in `classes`/`functions`.

use baml_base::Name;
use baml_compiler2_hir::{
    item_tree::{Attribute, GenericParam},
    type_ref::{TypeRefBuilder, TypeRefId},
};
use text_size::TextRange;

/// A generic parameter on a function, class, interface, interface method
/// signature, or out-of-body `implements` block, paired with its set of
/// `&`-separated bounds. Mirrors
/// [`item_tree::GenericParam`](baml_compiler2_hir::item_tree::GenericParam)
/// with the bounds lowered into the owning item's type-ref arena.
///
/// The bound set is a **conjunction**: an argument for this parameter must
/// satisfy every entry. Pairing the name with its bounds makes a length
/// mismatch between the two unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParamData {
    pub name: Name,
    pub bounds: Vec<TypeRefId>,
}

/// Lower a declaration's generic parameters into `type_refs`. Bounds are
/// allocated in declaration order, so ids stay a pure function of the item's
/// shape.
pub(crate) fn lower_generic_params(
    params: &[GenericParam],
    type_refs: &mut TypeRefBuilder,
) -> Vec<GenericParamData> {
    params
        .iter()
        .map(|param| GenericParamData {
            name: param.name.clone(),
            bounds: param
                .bounds
                .iter()
                .map(|bound| type_refs.lower(bound))
                .collect(),
        })
        .collect()
}

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
    /// Always present. A field written without a type is reported by the parser and
    /// recovers as [`TypeRefKind::Error`](baml_compiler2_hir::type_ref::TypeRefKind::Error),
    /// which suppresses follow-on diagnostics while the rest of the declaration still
    /// type-checks. "No type" is not a kind of type, so it is not representable here —
    /// otherwise every consumer has to invent its own stand-in, and they disagree.
    pub type_ref: TypeRefId,
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
