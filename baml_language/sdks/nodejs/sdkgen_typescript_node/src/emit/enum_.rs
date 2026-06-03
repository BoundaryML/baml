//! `NodeEnum` — TypeScript enum definition.
//!
//! Emits `export enum Foo { … }` with `<VARIANT> = "<value>"` members.

use baml_codegen_types::Name;

pub(crate) struct NodeEnum {
    pub(crate) name: String,
    pub(crate) source: Name,
    /// Enum variants in IR declaration order.
    pub(crate) variants: Vec<NodeEnumVariant>,
    /// Joined `///` doc-comment lines from the BAML enum declaration.
    pub(crate) docstring: Option<String>,
}

pub(crate) struct NodeEnumVariant {
    /// LHS identifier.
    pub(crate) ident: String,
    /// RHS string literal (IR's `EnumVariant.value`, verbatim).
    pub(crate) value: String,
    /// Joined `///` doc-comment lines preceding the variant.
    pub(crate) docstring: Option<String>,
}
