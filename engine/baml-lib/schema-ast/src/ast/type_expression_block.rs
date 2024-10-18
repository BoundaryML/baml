use super::{
    traits::WithAttributes, Attribute, BlockArgs, Comment, Field, FieldType, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
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

impl std::ops::Index<FieldId> for TypeExpressionBlock {
    type Output = Field<FieldType>;

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

/// A class or enum declaration.
#[derive(Debug, Clone)]
pub struct TypeExpressionBlock {
    /// The name of the class or enum.
    ///
    /// ```ignore
    /// enum Foo { ... }
    ///      ^^^
    /// class Bar { ... }
    ///       ^^^
    /// ```
    pub name: Identifier,
    /// The values of the enum, or fields of the class.
    ///
    /// ```ignore
    /// enum Foo {
    ///   Value1
    ///   ^^^^^^
    ///   Value2
    ///   ^^^^^^
    /// }
    /// ```
    pub(crate) input: Option<BlockArgs>,
    ///
    pub fields: Vec<Field<FieldType>>, // needs to support field as well

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

impl TypeExpressionBlock {
    pub fn iter_fields(&self) -> impl ExactSizeIterator<Item = (FieldId, &Field<FieldType>)> {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn values(&self) -> &[Field<FieldType>] {
        &self.fields
    }

    pub fn input(&self) -> Option<&BlockArgs> {
        match &self.input {
            Some(input) => Some(input),
            None => None,
        }
    }
}

impl WithIdentifier for TypeExpressionBlock {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for TypeExpressionBlock {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for TypeExpressionBlock {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for TypeExpressionBlock {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
