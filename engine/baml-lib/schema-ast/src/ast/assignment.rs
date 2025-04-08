//! Assignment expressions.
//!
//! As of right now the only supported "assignments" are type aliases.

use super::{FieldType, Identifier, Span, WithIdentifier, WithSpan};

/// Assignment expression. `left = right`.
#[derive(Debug, Clone)]
pub struct Assignment {
    /// Left side of the assignment.
    ///
    /// For now this can only be an identifier, but if we end up needing to
    /// support stuff like destructuring then change it to some sort of
    /// expression.
    pub identifier: Identifier,

    /// Right side of the assignment.
    ///
    /// Since for now it's only used for type aliases then it's just a type.
    pub value: FieldType,

    /// Span of the entire assignment.
    pub span: Span,
}

impl WithSpan for Assignment {
    fn span(&self) -> &Span {
        &self.span
    }
}

// TODO: Right now the left side is always an identifier, but if it ends up
// being an expression we'll have to refactor this somehow.
impl WithIdentifier for Assignment {
    fn identifier(&self) -> &Identifier {
        &self.identifier
    }
}
