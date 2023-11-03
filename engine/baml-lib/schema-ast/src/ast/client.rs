use super::{
    traits::WithAttributes, Attribute, Comment, ConfigBlockProperty, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
};

/// An opaque identifier for a field in an AST model. Use the
/// `model[field_id]` syntax to resolve the id to an `ast::Field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub(super) u32);

impl FieldId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: FieldId = FieldId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: FieldId = FieldId(u32::MAX);
}

impl std::ops::Index<FieldId> for Client {
    type Output = ConfigBlockProperty;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

/// A model declaration.
#[derive(Debug, Clone)]
pub struct Client {
    /// The name of the model.
    ///
    /// ```ignore
    /// model Foo { .. }
    ///       ^^^
    /// ```
    pub(crate) name: Identifier,
    /// The documentation for this model.
    ///
    /// ```ignore
    /// /// Lorem ipsum
    ///     ^^^^^^^^^^^
    /// model Foo {
    ///   id    Int    @id
    ///   field String
    /// }
    /// ```
    pub(crate) documentation: Option<Comment>,
    /// The attributes of this model.
    ///
    /// ```ignore
    /// model Foo {
    ///   id    Int    @id
    ///   field String
    ///
    ///   @@index([field])
    ///   ^^^^^^^^^^^^^^^^
    ///   @@map("Bar")
    ///   ^^^^^^^^^^^^
    /// }
    /// ```
    pub attributes: Vec<Attribute>,

    pub fields: Vec<ConfigBlockProperty>,

    pub client_type: String,

    /// The location of this model in the text representation.
    pub span: Span,
}

impl Client {
    pub fn iter_fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (FieldId, &ConfigBlockProperty)> + Clone {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn fields(&self) -> &[ConfigBlockProperty] {
        &self.fields
    }

    pub fn is_llm(&self) -> bool {
        self.client_type == "llm"
    }
}

impl WithIdentifier for Client {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Client {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Client {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Client {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
