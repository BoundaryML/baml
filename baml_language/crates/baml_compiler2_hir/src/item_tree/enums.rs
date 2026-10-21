use baml_base::Name;
use text_size::TextRange;

use crate::item_tree::{EnumAttrs, EnumVariantAttrs};

/// An enum variant stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Name,
    /// The variant's lowered `@` attributes.
    pub attrs: EnumVariantAttrs,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Name,
    /// Variants of the enum, in declaration order.
    pub variants: Vec<EnumVariant>,
    /// The enum's lowered `@@` attributes.
    pub attrs: EnumAttrs,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// Full source span of the enum declaration.
    pub span: TextRange,
}
