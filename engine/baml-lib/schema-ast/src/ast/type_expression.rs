use super::{
    traits::WithAttributes, Attribute, Comment, Field, Identifier, Span, WithDocumentation,
    WithIdentifier, WithSpan,
};

/// An opaque identifier for a value in an AST enum. Use the
/// `r#enum[enum_value_id]` syntax to resolve the id to an `ast::EnumValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u32);

impl FieldId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: FieldId = FieldId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: FieldId = FieldId(u32::MAX);
}

impl std::ops::Index<FieldId> for TypeExpression {
    type Output = Field;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

#[derive(Debug, Clone)]
pub enum SubType {
    Enum,
    Class,
    Other(String),
}

/// An enum declaration. Enumeration can either be in the database schema, or completely a Prisma level concept.
///
/// PostgreSQL stores enums in a schema, while in MySQL the information is in
/// the table definition. On MongoDB the enumerations are handled in the Query
/// Engine.
#[derive(Debug, Clone)]
pub struct TypeExpression {
    /// The name of the enum.
    ///
    /// ```ignore
    /// enum Foo { ... }
    ///      ^^^
    /// ```
    pub name: Identifier,
    /// The values of the enum.
    ///
    /// ```ignore
    /// enum Foo {
    ///   Value1
    ///   ^^^^^^
    ///   Value2
    ///   ^^^^^^
    /// }
    /// ```
    pub fields: Vec<Field>, // needs to support field as well

    /// The attributes of this enum.
    ///
    /// ```ignore
    /// enum Foo {
    ///   Value1
    ///   Value2
    ///
    ///   @@map("1Foo")
    ///   ^^^^^^^^^^^^^
    /// }
    /// ```
    pub attributes: Vec<Attribute>,

    /// The comments for this enum.
    ///
    /// ```ignore
    /// /// Lorem ipsum
    ///     ^^^^^^^^^^^
    /// enum Foo {
    ///   Value1
    ///   Value2
    /// }
    /// ```
    pub(crate) documentation: Option<Comment>,
    /// The location of this enum in the text representation.
    pub span: Span,

    /// This is used to distinguish between enums and classes.
    pub sub_type: SubType,
}

impl TypeExpression {
    pub fn iter_values(&self) -> impl ExactSizeIterator<Item = (FieldId, &Field)> {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn values(&self) -> &[Field] {
        &self.fields
    }
}

impl WithIdentifier for TypeExpression {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for TypeExpression {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for TypeExpression {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for TypeExpression {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

#[derive(Debug, Clone)]
pub enum ValueUnion {
    Identifier(Identifier),
    Field(Field),
}

impl ValueUnion {
    pub fn name(&self) -> &Identifier {
        match self {
            ValueUnion::Identifier(name) => name,
            ValueUnion::Field(field) => field.identifier(),
        }
    }
}

/// An enum value definition.
#[derive(Debug, Clone)]
pub struct TypeValue {
    /// The name of the enum value as it will be exposed by the api.
    pub data: ValueUnion,
    pub attributes: Vec<Attribute>,
    pub(crate) documentation: Option<Comment>,
    /// The location of this enum value in the text representation.
    pub span: Span,
}

impl WithIdentifier for TypeValue {
    fn identifier(&self) -> &Identifier {
        &self.data.name()
    }
}

impl WithAttributes for TypeValue {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithSpan for TypeValue {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithDocumentation for TypeValue {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
