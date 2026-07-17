use baml_base::Name;
use text_size::TextRange;

use crate::item_tree::Attribute;

/// An enum variant stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Name,
    /// Field-level attributes (@description, @alias, @skip, etc.).
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Name,
    /// Variants of the enum, in declaration order.
    pub variants: Vec<EnumVariant>,
    /// Block-level attributes (@@description, @@alias, etc.).
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// Full source span of the enum declaration.
    pub span: TextRange,
}
