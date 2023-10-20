use super::{
    traits::WithAttributes, Attribute, Comment, Field, Identifier, Span, WithDocumentation,
    WithIdentifier, WithSpan,
};

/// An opaque identifier for a field in an AST model. Use the
/// `model[field_id]` syntax to resolve the id to an `ast::Field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u32);

impl FieldId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: FieldId = FieldId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: FieldId = FieldId(u32::MAX);
}

impl std::ops::Index<FieldId> for Class {
    type Output = Field;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

/// A model declaration.
#[derive(Debug, Clone)]
pub struct Class {
    /// The name of the model.
    ///
    /// ```ignore
    /// model Foo { .. }
    ///       ^^^
    /// ```
    pub(crate) name: Identifier,
    /// The fields of the model.
    ///
    /// ```ignore
    /// model Foo {
    ///   id    Int    @id
    ///   ^^^^^^^^^^^^^^^^
    ///   field String
    ///   ^^^^^^^^^^^^
    /// }
    /// ```
    pub(crate) fields: Vec<Field>,
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
    /// The location of this model in the text representation.
    pub(crate) span: Span,
}

impl Class {
    pub fn iter_fields(&self) -> impl ExactSizeIterator<Item = (FieldId, &Field)> + Clone {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
}

impl WithIdentifier for Class {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Class {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Class {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Class {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
