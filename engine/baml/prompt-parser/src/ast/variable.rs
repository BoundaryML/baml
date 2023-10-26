use crate::ast::{Span, WithDocumentation, WithSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub u32);

impl VariableId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: VariableId = VariableId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: VariableId = VariableId(u32::MAX);
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub text: String,
    pub path: Vec<String>,
    pub span: Span,
}

impl WithSpan for Variable {
    fn span(&self) -> &Span {
        &self.span
    }
}
