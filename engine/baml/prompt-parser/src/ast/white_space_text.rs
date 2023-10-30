use crate::ast::{Span, WithSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WhiteSpaceId(pub u32);

impl WhiteSpaceId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: WhiteSpaceId = WhiteSpaceId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: WhiteSpaceId = WhiteSpaceId(u32::MAX);
}

#[derive(Debug, Clone)]
pub struct WhiteSpace {
    pub text: String,
    pub span: Span,
}

impl WithSpan for WhiteSpace {
    fn span(&self) -> &Span {
        &self.span
    }
}
