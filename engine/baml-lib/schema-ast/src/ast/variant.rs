use super::{
    traits::WithAttributes, Attribute, Comment, ConfigBlockProperty, Identifier, Serializer,
    SerializerFieldId, Span, WithDocumentation, WithIdentifier, WithSpan,
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

impl std::ops::Index<FieldId> for Variant {
    type Output = ConfigBlockProperty;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

/// An opaque identifier for a field in an AST model. Use the
/// `model[field_id]` syntax to resolve the id to an `ast::Field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SerializerId(pub(super) u32);

impl SerializerId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: SerializerId = SerializerId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: SerializerId = SerializerId(u32::MAX);
}

impl std::ops::Index<SerializerId> for Variant {
    type Output = Serializer;

    fn index(&self, index: SerializerId) -> &Self::Output {
        &self.serializers[index.0 as usize]
    }
}

/// A model declaration.
#[derive(Debug, Clone)]
pub struct Variant {
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

    pub serializers: Vec<Serializer>,

    pub(crate) variant_type: String,

    pub(crate) function_name: Identifier,

    /// The location of this model in the text representation.
    pub(crate) span: Span,
}

impl Variant {
    pub fn iter_fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (FieldId, &ConfigBlockProperty)> + Clone {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn iter_serializers(
        &self,
    ) -> impl ExactSizeIterator<Item = (SerializerId, &Serializer)> + Clone {
        self.serializers
            .iter()
            .enumerate()
            .map(|(idx, field)| (SerializerId(idx as u32), field))
    }

    pub fn is_llm(&self) -> bool {
        self.variant_type == "llm"
    }

    pub fn function_name(&self) -> &Identifier {
        &self.function_name
    }
}

impl WithIdentifier for Variant {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Variant {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Variant {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Variant {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
