use super::{ArguementId, ArgumentsList, Identifier, Span, WithIdentifier, WithSpan};
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
    /// Whether the Attribute was closely associated to a type, via parentheses.
    /// This flag protects attributes from being lifted away from a union variant
    /// up to the level of the union.
    pub parenthesized: bool,
    /// The AST span of the node.
    pub span: Span,
}

impl Attribute {
    /// Try to find the argument and return its span.
    pub fn span_for_argument(&self, argument: ArguementId) -> Span {
        self.arguments[argument].span.clone()
    }

    pub fn assert_eq_up_to_span(&self, other: &Attribute) {
        assert_eq!(self.name.to_string(), other.name.to_string());
        assert_eq!(self.parenthesized, other.parenthesized);
        self.arguments
            .iter()
            .zip(other.arguments.iter())
            .for_each(|(arg1, arg2)| {
                assert_eq!(arg1.0, arg2.0);
                arg1.1.assert_eq_up_to_span(arg2.1);
            })
    }
}

impl WithIdentifier for Attribute {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Attribute {
    fn span(&self) -> &Span {
        &self.span
    }
}

/// A node containing attributes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AttributeContainer {
    Class(super::TypeExpId),
    ClassField(super::TypeExpId, super::FieldId),
    Enum(super::TypeExpId),
    EnumValue(super::TypeExpId, super::FieldId),
}

impl From<super::TypeExpId> for AttributeContainer {
    fn from(v: super::TypeExpId) -> Self {
        Self::Enum(v)
    }
}

impl From<(super::TypeExpId, super::FieldId)> for AttributeContainer {
    fn from((enm, val): (super::TypeExpId, super::FieldId)) -> Self {
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
            AttributeContainer::Class(model_id) => &self[model_id].attributes,
            AttributeContainer::ClassField(model_id, field_id) => {
                &self[model_id][field_id].attributes
            }
            AttributeContainer::Enum(enum_id) => &self[enum_id].attributes,
            AttributeContainer::EnumValue(enum_id, value_idx) => {
                &self[enum_id][value_idx].attributes
            }
        }
    }
}

impl Index<AttributeId> for super::SchemaAst {
    type Output = Attribute;

    fn index(&self, index: AttributeId) -> &Self::Output {
        &self[index.0][index.1 as usize]
    }
}
