use super::{
    traits::WithAttributes, Attribute, Comment, Field, Identifier, Span, WithDocumentation,
    WithIdentifier, WithSpan,
};

/// An opaque identifier for a field in an AST model. Use the
/// `model[field_id]` syntax to resolve the id to an `ast::Field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub(super) u32);

/// A model declaration.
#[derive(Debug, Clone)]
pub struct Function {
    /// The name of the model.
    ///
    /// ```ignore
    /// function Foo { .. }
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
    pub(crate) input: Field,
    pub(crate) output: Field,
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

impl Function {
    pub fn input(&self) -> &Field {
        &self.input
    }
    pub fn output(&self) -> &Field {
        &self.output
    }
}

impl WithIdentifier for Function {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Function {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Function {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Function {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
