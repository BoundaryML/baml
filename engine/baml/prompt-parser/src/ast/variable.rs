use crate::ast::{Span, WithSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub u32);

impl VariableId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: VariableId = VariableId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: VariableId = VariableId(u32::MAX);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Entire unparsed text of the variable.  (input.something.bar)
    pub text: String,
    /// [input, something, bar]
    pub path: Vec<String>,
    pub span: Span,
}

impl WithSpan for Variable {
    fn span(&self) -> &Span {
        &self.span
    }
}
