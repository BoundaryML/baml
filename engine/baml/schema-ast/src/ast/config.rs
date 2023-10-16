use crate::ast::{
    Attribute, Comment, Expression, Identifier, Span, WithAttributes, WithDocumentation,
    WithIdentifier, WithSpan,
};

/// A named property in a config block.
///
/// ```ignore
/// datasource db {
///     url = env("URL")
///     ^^^^^^^^^^^^^^^^
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ConfigBlockProperty {
    /// The property name.
    ///
    /// ```ignore
    /// datasource db {
    ///     url = env("URL")
    ///     ^^^
    /// }
    /// ```
    pub name: Identifier,
    /// The property value.
    ///
    /// ```ignore
    /// datasource db {
    ///     url = env("URL")
    ///           ^^^^^^^^^^
    /// }
    /// ```
    pub value: Option<Expression>,

    pub(crate) documentation: Option<Comment>,
    /// The node span.
    pub span: Span,
}

impl WithIdentifier for ConfigBlockProperty {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for ConfigBlockProperty {
    fn span(&self) -> Span {
        self.span
    }
}

impl WithDocumentation for ConfigBlockProperty {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}
