use crate::ast::{Span, WithDocumentation, WithSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeBlockId(pub u32);

impl CodeBlockId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: CodeBlockId = CodeBlockId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: CodeBlockId = CodeBlockId(u32::MAX);
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub block: String,
    pub span: Span,
}

impl WithSpan for CodeBlock {
    fn span(&self) -> &Span {
        &self.span
    }
}

// impl WithIdentifier for CodeBlock {
//     fn identifier(&self) -> &Identifier {
//         &self.name
//     }
// }
