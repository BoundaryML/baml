use crate::ast::{Span, WithSpan};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommentBlockId(pub u32);

impl CommentBlockId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: CommentBlockId = CommentBlockId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: CommentBlockId = CommentBlockId(u32::MAX);
}

#[derive(Debug, Clone)]
pub struct CommentBlock {
    pub block: String,
    pub span: Span,
}

impl WithSpan for CommentBlock {
    fn span(&self) -> &Span {
        &self.span
    }
}

// impl WithIdentifier for CommentBlock {
//     fn identifier(&self) -> &Identifier {
//         &self.name
//     }
// }
