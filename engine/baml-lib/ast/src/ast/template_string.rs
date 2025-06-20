use super::{
    traits::WithAttributes, Attribute, BlockArgs, Comment, Expression, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
};

/// A model declaration.
#[derive(Debug, Clone)]
pub struct TemplateString {
    /// The name of the variable.
    ///
    /// ```ignore
    /// function Foo { .. }
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
    pub(crate) input: Option<BlockArgs>,
    pub attributes: Vec<Attribute>,
    /// The location of this model in the text representation.
    pub span: Span,
    pub value: Expression,
}

impl TemplateString {
    pub fn value(&self) -> &Expression {
        &self.value
    }

    pub fn input(&self) -> Option<&BlockArgs> {
        self.input.as_ref()
    }
}

impl WithIdentifier for TemplateString {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for TemplateString {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for TemplateString {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for TemplateString {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
