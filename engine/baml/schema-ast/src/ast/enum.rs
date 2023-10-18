use super::{
    traits::WithAttributes, Attribute, Comment, Identifier, Span, WithDocumentation,
    WithIdentifier, WithSpan,
};

/// An opaque identifier for a value in an AST enum. Use the
/// `r#enum[enum_value_id]` syntax to resolve the id to an `ast::EnumValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumValueId(pub(super) u32);

impl EnumValueId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: EnumValueId = EnumValueId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: EnumValueId = EnumValueId(u32::MAX);
}

impl std::ops::Index<EnumValueId> for Enum {
    type Output = EnumValue;

    fn index(&self, index: EnumValueId) -> &Self::Output {
        &self.values[index.0 as usize]
    }
}

/// An enum declaration. Enumeration can either be in the database schema, or completely a Prisma level concept.
///
/// PostgreSQL stores enums in a schema, while in MySQL the information is in
/// the table definition. On MongoDB the enumerations are handled in the Query
/// Engine.
#[derive(Debug, Clone)]
pub struct Enum {
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
    pub values: Vec<EnumValue>,

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
    /// The span of the inner contents.
    pub inner_span: Span,
}

impl Enum {
    pub fn iter_values(&self) -> impl ExactSizeIterator<Item = (EnumValueId, &EnumValue)> {
        self.values
            .iter()
            .enumerate()
            .map(|(idx, field)| (EnumValueId(idx as u32), field))
    }
}

impl WithIdentifier for Enum {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Enum {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Enum {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Enum {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

/// An enum value definition.
#[derive(Debug, Clone)]
pub struct EnumValue {
    /// The name of the enum value as it will be exposed by the api.
    pub name: Identifier,
    pub attributes: Vec<Attribute>,
    pub(crate) documentation: Option<Comment>,
    /// The location of this enum value in the text representation.
    pub span: Span,
}

impl WithIdentifier for EnumValue {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithAttributes for EnumValue {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithSpan for EnumValue {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithDocumentation for EnumValue {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
