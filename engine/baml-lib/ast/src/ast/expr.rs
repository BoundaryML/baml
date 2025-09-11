use crate::ast::{BlockArgs, ExpressionBlock, FieldType, Header, Identifier, LetStmt, Span};

/// A function definition.
#[derive(Debug, Clone)]
pub struct ExprFn {
    pub name: Identifier,
    pub args: BlockArgs,
    pub return_type: Option<FieldType>,
    pub body: ExpressionBlock,
    pub span: Span,
    pub annotations: Vec<std::sync::Arc<Header>>,
}

/// A top-level binding.
/// E.g. (at top-level in source file) `let x = 1;`
#[derive(Debug, Clone)]
pub struct TopLevelAssignment {
    pub stmt: LetStmt,
}
