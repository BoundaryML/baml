//! Baml HIR.
//!
//! This file contains the definitions for all HIR items.

use baml_types::{type_meta::base::StreamingBehavior, Constraint};
use internal_baml_diagnostics::Span;

mod dump;
mod lowering;

/// High-level intermediate representation.
///
/// This is analogous to the HIR in Rust: https://rustc-dev-guide.rust-lang.org/hir.html
/// It carries just enough information to produce BAML bytecode. It differs from
/// baml-core IR in that it does not contain any type information. It has
/// limited metadata, for use in debugging, namely source spans.
///
/// See [`Hir::from_ast`] to see how BAML syntax is lowered into HIR.
///
/// Lowering from AST to HIR involves desugaring certain syntax forms.
///   - For loops become while loops.
///   - Class constructor spreads become regular class constructors with exhaustive fields.
///   - Implicit returns become explicit.
#[derive(Clone, Debug)]
pub struct Hir {
    pub expr_functions: Vec<ExprFunction>,
    pub llm_functions: Vec<LlmFunction>,
    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
}

pub type Type = TypeM<TypeMeta>;

#[derive(Clone, Debug)]
pub enum TypeM<M> {
    Int(M),
    String(M),
    Bool(M),
    Null(M),
    Array(Box<TypeM<M>>, M),
    Map(Box<TypeM<M>>, Box<TypeM<M>>, M),
    ClassName(String, M),
    EnumName(String, M),
    Union(Vec<TypeM<M>>, M),
}

#[derive(Clone, Debug)]
struct TypeMeta {
    span: Span,
    constraints: Vec<Constraint>,
    streaming_behavior: StreamingBehavior,
}

#[derive(Clone, Debug)]
pub struct ExprFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeM<TypeMeta>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct LlmFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeM<TypeMeta>,
    pub client: String,
    pub prompt: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub r#type: TypeM<TypeMeta>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub r#type: TypeM<TypeMeta>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
}

/// A single unit of execution within a block.
#[derive(Clone, Debug)]
pub enum Statement {
    /// Assign an immutable variable.
    Let {
        name: String,
        value: Expression,
        span: Span,
    },
    /// Declare a (mutable) reference.
    /// There is no span because it is never present in the source AST.
    /// This is a desugaring from `if` expressions.
    Declare { name: String, span: Span },
    /// Assign a mutable variable.
    Assign { name: String, value: Expression },
    /// Declare and assign a mutable reference in one statement.
    DeclareAndAssign {
        name: String,
        value: Expression,
        span: Span,
    },
    /// Return from a function.
    Return { expr: Expression, span: Span },
    /// Evaluate an expression as the final value of a block (without returning from function).
    Expression { expr: Expression, span: Span },
    While {
        condition: Box<Expression>,
        block: Block,
        span: Span,
    },
    ForLoop {
        identifier: String,
        iterator: Box<Expression>,
        block: Block,
        span: Span,
    },
}

/// Expressions
#[derive(Clone, Debug)]
pub enum Expression {
    ArrayAccess {
        base: Box<Expression>,
        index: String,
        span: Span,
    },
    FieldAccess {
        base: Box<Expression>,
        field: String,
        span: Span,
    },
    BoolValue(bool, Span),
    NumericValue(String, Span),
    Identifier(String, Span),
    StringValue(String, Span),
    RawStringValue(String, Span),
    If {
        condition: Box<Expression>,
        if_branch: Box<Expression>,
        else_branch: Option<Box<Expression>>,
        span: Span,
    },
    Array(Vec<Expression>, Span),
    Map(Vec<(Expression, Expression)>, Span),
    JinjaExpressionValue(String, Span),
    Call(String, Vec<Expression>, Span),
    // Lambda(ArgumentsList, Box<ExpressionBlock>, Span), // TODO.
    // MethodCall(Box<Expression>, String, Vec<Expression>), // TODO.
    ClassConstructor(ClassConstructor, Span),
    /// Expression block - has its own scope with statements and evaluates to a value
    ExpressionBlock(Box<Block>, Span),
}

// TODO: struct Expr {kind: ExprKind, span: Span}
impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Expression::ArrayAccess { span, .. } => span.clone(),
            Expression::FieldAccess { span, .. } => span.clone(),
            Expression::BoolValue(_, span) => span.clone(),
            Expression::NumericValue(_, span) => span.clone(),
            Expression::Identifier(_, span) => span.clone(),
            Expression::StringValue(_, span) => span.clone(),
            Expression::RawStringValue(_, span) => span.clone(),
            Expression::If { span, .. } => span.clone(),
            Expression::Array(_, span) => span.clone(),
            Expression::Map(_, span) => span.clone(),
            Expression::JinjaExpressionValue(_, span) => span.clone(),
            Expression::Call(_, _, span) => span.clone(),
            Expression::ClassConstructor(_, span) => span.clone(),
            Expression::ExpressionBlock(_, span) => span.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClassConstructor {
    pub class_name: String,
    pub fields: Vec<ClassConstructorField>,
}

#[derive(Clone, Debug)]
pub enum ClassConstructorField {
    Named { name: String, value: Expression },
    Spread { value: Expression },
}
