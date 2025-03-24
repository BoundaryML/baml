/// Types for the concrete syntax of compound expressions,
/// top-level assignments, and non-llm functions.

use baml_types::{TypeValue, UnresolvedValue};
use internal_baml_diagnostics::Diagnostics;

use crate::ast::{Expression, Identifier};
use crate::ast::Span;

use super::{ArgumentsList, BlockArgs, FieldType};

/// A lambda-calculus expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Basic expressions, such as literals and variables.
    Atom(Expression),
    /// Function abstraction.
    Lambda(ArgumentsList, Box<FunctionBody>),
    /// Function Application
    FnApp(Identifier, Vec<ExprWithSpan>),
}

#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub stmts: Vec<Stmt>,
    pub expr: ExprWithSpan,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub identifier: Identifier,
    pub body: FunctionBody,
    pub span: Span,
}

/// A function definition.
#[derive(Debug, Clone)]
pub struct ExprFn {
    pub name: Identifier,
    pub args: BlockArgs,
    pub return_type: Option<FieldType>,
    pub body: FunctionBody,
    pub span: Span,
}

/// A Constant Applicative Form (top-level variable-binding).
#[derive(Debug, Clone)]
pub struct TopLevelAssignment {
    pub stmt: Stmt
}

#[derive(Debug, Clone)]
pub struct ExprWithSpan {
    pub expr: Expr,
    pub span: Span,
}