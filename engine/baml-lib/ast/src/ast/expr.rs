/// Types for the concrete syntax of compound expressions,
/// top-level assignments, and non-llm functions.
use baml_types::{TypeValue, UnresolvedValue};
use internal_baml_diagnostics::Diagnostics;

use crate::ast::{
    ArgumentsList, BlockArgs, Expression, ExpressionBlock, FieldType, Header, Identifier, LetStmt,
    Span, Stmt,
};

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
