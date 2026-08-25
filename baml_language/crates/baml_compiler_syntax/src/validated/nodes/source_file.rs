use super::TopLevelDeclaration;
use crate::{
    SyntaxElement, SyntaxKind,
    validated::{FromCST, KnownKind, StrongAstError, SyntaxNodeIter},
};

#[derive(Debug)]
pub struct SourceFile {
    pub items: Vec<TopLevelDeclaration>,
}

impl FromCST for SourceFile {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::SOURCE_FILE)?;
        let items = SyntaxNodeIter::new(&node)
            .map(TopLevelDeclaration::from_cst)
            .collect::<Result<_, _>>()?;
        Ok(Self { items })
    }
}

impl KnownKind for SourceFile {
    fn kind() -> SyntaxKind {
        SyntaxKind::SOURCE_FILE
    }
}
