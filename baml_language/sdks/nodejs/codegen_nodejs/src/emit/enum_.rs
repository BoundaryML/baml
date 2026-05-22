//! `NodeEnum` — TypeScript enum definition.
//!
//! Phase 2 carries the minimum needed to render a placeholder. Variants
//! stay on the struct so Phase 4 can render real members without
//! re-touching scaffolding.

use baml_codegen_types::Name;

pub(crate) struct NodeEnum {
    pub(crate) name: String,
    pub(crate) source: Name,
    #[allow(dead_code)]
    pub(crate) variants: Vec<NodeEnumVariant>,
    #[allow(dead_code)]
    pub(crate) docstring: Option<String>,
}

#[allow(dead_code)]
pub(crate) struct NodeEnumVariant {
    pub(crate) ident: String,
    pub(crate) value: String,
    pub(crate) docstring: Option<String>,
}
