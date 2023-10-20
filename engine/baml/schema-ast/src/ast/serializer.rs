use super::{
    traits::WithAttributes, Attribute, Comment, Identifier, Span, WithDocumentation,
    WithIdentifier, WithSpan,
};

/// An opaque identifier for a value in an AST enum. Use the
/// `r#enum[enum_value_id]` syntax to resolve the id to an `ast::SerializerField`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SerializerFieldId(pub u32);

impl SerializerFieldId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: SerializerFieldId = SerializerFieldId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: SerializerFieldId = SerializerFieldId(u32::MAX);
}

impl std::ops::Index<SerializerFieldId> for Serializer {
    type Output = SerializerField;

    fn index(&self, index: SerializerFieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

/// An enum declaration. Enumeration can either be in the database schema, or completely a Prisma level concept.
///
/// PostgreSQL stores enums in a schema, while in MySQL the information is in
/// the table definition. On MongoDB the enumerations are handled in the Query
/// Engine.
#[derive(Debug, Clone)]
pub struct Serializer {
    pub name: Identifier,

    pub fields: Vec<SerializerField>,

    pub attributes: Vec<Attribute>,

    pub(crate) documentation: Option<Comment>,
    pub span: Span,
}

impl Serializer {
    pub fn iter_fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (SerializerFieldId, &SerializerField)> {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (SerializerFieldId(idx as u32), field))
    }
}

impl WithIdentifier for Serializer {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Serializer {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Serializer {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Serializer {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct SerializerField {
    pub name: Identifier,
    pub attributes: Vec<Attribute>,
    pub(crate) documentation: Option<Comment>,
    pub span: Span,
}

impl WithIdentifier for SerializerField {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithAttributes for SerializerField {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithSpan for SerializerField {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithDocumentation for SerializerField {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
