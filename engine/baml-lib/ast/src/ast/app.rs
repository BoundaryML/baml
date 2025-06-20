use super::{Expression, FieldType, Identifier, Span};

/// Function application.
#[derive(Debug, Clone)]
pub struct App {
    /// The name of the function.
    pub name: Identifier,
    /// Generic types.
    pub type_args: Vec<FieldType>,
    /// Function arguments.
    pub args: Vec<Expression>,
    /// Span of the entire function call.
    pub span: Span,
}

impl App {
    pub fn span(&self) -> &Span {
        &self.span
    }
}
