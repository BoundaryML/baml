use crate::ast::{Span, WithSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PromptTextId(pub u32);

impl PromptTextId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: PromptTextId = PromptTextId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: PromptTextId = PromptTextId(u32::MAX);
}

#[derive(Debug, Clone)]
pub struct PromptText {
    pub text: String,
    pub span: Span,
}

impl WithSpan for PromptText {
    fn span(&self) -> &Span {
        &self.span
    }
}
