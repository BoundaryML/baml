/// Re-exported unchanged: a generic parameter carries only a name and its
/// bounds (themselves `ast::TypeExpr`s, as everywhere else in the `ItemTree`),
/// so there is nothing for a mirror struct to strip.
pub use ast::GenericParam;
use baml_base::Name;
use baml_compiler2_ast::ast;

/// A span-free attribute for position-independent storage in the `ItemTree`.
/// Derived from `ast::RawAttribute` with all `TextRange`s stripped.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attribute {
    pub args: Vec<AttributeArg>,
    pub name: Name,
}

/// A span-free attribute argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributeArg {
    pub key: Option<Name>,
    pub value: String,
}

impl From<&ast::RawAttribute> for Attribute {
    fn from(raw: &ast::RawAttribute) -> Self {
        Self {
            name: raw.name.clone(),
            args: raw.args.iter().map(AttributeArg::from).collect(),
        }
    }
}

impl From<&ast::RawAttributeArg> for AttributeArg {
    fn from(raw: &ast::RawAttributeArg) -> Self {
        Self {
            key: raw.key.clone(),
            value: raw.value.clone(),
        }
    }
}
