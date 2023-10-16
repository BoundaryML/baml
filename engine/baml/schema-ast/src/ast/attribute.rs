use super::{ArgumentsList, Identifier, Span, WithIdentifier, WithSpan};
use std::ops::Index;

/// An attribute (following `@` or `@@``) on a model, model field, enum, enum value or composite
/// type field.
#[derive(Debug, Clone)]
pub struct Attribute {
    /// The name of the attribute:
    ///
    /// ```ignore
    /// @@index([a, b, c])
    ///   ^^^^^
    /// ```
    pub name: Identifier,
    /// The arguments of the attribute.
    ///
    /// ```ignore
    /// @@index([a, b, c], map: "myidix")
    ///         ^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: ArgumentsList,
    /// The AST span of the node.
    pub span: Span,
}

impl Attribute {
    /// Try to find the argument and return its span.
    pub fn span_for_argument(&self, argument: &str) -> Option<Span> {
        self.arguments
            .iter()
            .find(|a| a.name.as_ref().map(|n| n.name.as_str()) == Some(argument))
            .map(|a| a.span)
    }
}

impl WithIdentifier for Attribute {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Attribute {
    fn span(&self) -> Span {
        self.span
    }
}

/// A node containing attributes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AttributeContainer {
    // ModelField(super::ModelId, super::FieldId),
    Enum(super::EnumId),
    EnumValue(super::EnumId, u32),
    // CompositeTypeField(super::CompositeTypeId, super::FieldId),
}

impl From<super::EnumId> for AttributeContainer {
    fn from(v: super::EnumId) -> Self {
        Self::Enum(v)
    }
}

impl From<(super::EnumId, u32)> for AttributeContainer {
    fn from((enm, val): (super::EnumId, u32)) -> Self {
        Self::EnumValue(enm, val)
    }
}

/// An attribute (@ or @@) node in the AST.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AttributeId(AttributeContainer, u32);

impl AttributeId {
    pub fn new_in_container(container: AttributeContainer, idx: usize) -> AttributeId {
        AttributeId(container, idx as u32)
    }
}

impl Index<AttributeContainer> for super::SchemaAst {
    type Output = [Attribute];

    fn index(&self, index: AttributeContainer) -> &Self::Output {
        match index {
            // AttributeContainer::Model(model_id) => &self[model_id].attributes,
            // AttributeContainer::ModelField(model_id, field_id) => &self[model_id][field_id].attributes,
            AttributeContainer::Enum(enum_id) => &self[enum_id].attributes,
            AttributeContainer::EnumValue(enum_id, value_idx) => &self[enum_id].values[value_idx as usize].attributes,
            // AttributeContainer::CompositeTypeField(ctid, field_id) => &self[ctid][field_id].attributes,
        }
    }
}

impl Index<AttributeId> for super::SchemaAst {
    type Output = Attribute;

    fn index(&self, index: AttributeId) -> &Self::Output {
        &self[index.0][index.1 as usize]
    }
}
