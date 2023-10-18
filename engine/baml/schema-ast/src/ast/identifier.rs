use super::{Span, WithSpan};

/// An identifier.
#[derive(Debug, Clone, PartialEq)]
pub struct Identifier {
    pub path: Option<String>,
    /// The identifier contents.
    pub name: String,
    /// The span of the AST node.
    pub span: Span,
}

impl WithSpan for Identifier {
    fn span(&self) -> &Span {
        &self.span
    }
}
