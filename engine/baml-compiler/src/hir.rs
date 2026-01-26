//! Baml HIR.
//!
//! This file contains the definitions for all HIR items.

use baml_types::ir_type::TypeIR;
use internal_baml_diagnostics::Span;

use crate::watch::{WatchSpec, WatchWhen};

pub mod dump;
pub mod lowering;

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
    pub global_assignments: baml_types::BamlMap<String, GlobalAssignment>,
}

impl Hir {
    pub fn empty() -> Self {
        Hir {
            expr_functions: vec![],
            llm_functions: vec![],
            classes: crate::builtin::builtin_classes(),
            enums: crate::builtin::builtin_enums(),
            global_assignments: baml_types::BamlMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExprFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeIR,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct LlmFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeIR,
    pub client: String,
    pub prompt: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub fields: Vec<Field>,
    // TODO: Allow LLM functions here.
    pub methods: Vec<ExprFunction>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub r#type: TypeIR,
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
    /// Always true after mut keyword removal
    pub is_mutable: bool,
    pub r#type: TypeIR,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    /// List of statements.
    pub statements: Vec<Statement>,

    /// Final expression in the block without semicolon (used as return).
    pub trailing_expr: Option<Box<Expression>>,
}

#[derive(Clone, Debug)]
pub struct HeaderContext {
    pub level: u8,
    pub title: String,
    pub span: Span,
}

/// A single unit of execution within a block.
#[derive(Clone, Debug)]
pub enum Statement {
    HeaderContextEnter(HeaderContext),
    /// Assign an immutable variable.
    Let {
        name: String,
        value: Expression,
        annotated_type: Option<TypeIR>,
        watch: Option<WatchSpec>,
        span: Span,
    },
    /// Declare a (mutable) reference.
    /// There is no span because it is never present in the source AST.
    /// This is a desugaring from `if` expressions.
    Declare {
        name: String,
        span: Span,
    },
    /// Assign a mutable variable.
    Assign {
        left: Expression,
        value: Expression,
        span: Span,
    },
    AssignOp {
        left: Expression,
        assign_op: AssignOp,
        value: Expression,
        span: Span,
    },
    /// Declare and assign a mutable reference in one statement.
    DeclareAndAssign {
        name: String,
        value: Expression,
        annotated_type: Option<TypeIR>,
        watch: Option<WatchSpec>,
        span: Span,
    },
    /// Return from a function.
    Return {
        expr: Expression,
        span: Span,
    },
    /// Evaluate an expression as the final value of a block (without returning from function).
    Expression {
        expr: Expression,
        span: Span,
    },
    Semicolon {
        expr: Expression,
        span: Span,
    },
    While {
        condition: Expression,
        block: Block,
        span: Span,
    },
    ForLoop {
        identifier: String,
        iterator: Box<Expression>,
        block: Block,
        span: Span,
    },
    /// C-like for-loop that can't be directly mapped to `while` because it has either no condition or has after statement
    CForLoop {
        condition: Option<Expression>,
        after: Option<Box<Statement>>,
        block: Block,
    },
    Break(Span),
    Continue(Span),

    Assert {
        condition: Expression,
        span: Span,
    },

    /// Configure watch options for a watched variable.
    /// Syntax: `variable.$watch.options( baml.WatchOptions { channel: "channel", when: FilterFunc } )`
    WatchOptions {
        variable: String,
        channel: Option<String>,
        when: Option<WatchWhen>,
        span: Span,
    },
    /// Manually notify watchers of a variable.
    /// Syntax: `variable.$watch.notify()`
    WatchNotify {
        variable: String,
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct GlobalAssignment {
    pub value: Expression,
    pub annotated_type: Option<TypeIR>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum AssignOp {
    /// The `+=` operator (addition)
    AddAssign,
    /// The `-=` operator (subtraction)
    SubAssign,
    /// The `*=` operator (multiplication)
    MulAssign,
    /// The `/=` operator (division)
    DivAssign,
    /// The `%=` operator (modulus)
    ModAssign,
    /// The `^=` operator (bitwise xor)
    BitXorAssign,
    /// The `&=` operator (bitwise and)
    BitAndAssign,
    /// The `|=` operator (bitwise or)
    BitOrAssign,
    /// The `<<=` operator (shift left)
    ShlAssign,
    /// The `>>=` operator (shift right)
    ShrAssign,
}

/// Expressions
#[derive(Clone, Debug)]
pub enum Expression {
    ArrayAccess {
        base: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    FieldAccess {
        base: Box<Expression>,
        field: String,
        span: Span,
    },
    MethodCall {
        receiver: Box<Expression>,
        method: String,
        type_args: Vec<TypeArg>,
        args: Vec<Expression>,
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
    Call {
        function: Box<Expression>,
        type_args: Vec<TypeArg>,
        args: Vec<Expression>,
        span: Span,
    },
    // Lambda(ArgumentsList, Box<ExpressionBlock>, Span), // TODO.
    // MethodCall(Box<Expression>, String, Vec<Expression>), // TODO.
    ClassConstructor(ClassConstructor, Span),
    /// Expression block - has its own scope with statements and evaluates to a value
    Block(Block, Span),
    BinaryOperation {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        span: Span,
    },
    UnaryOperation {
        operator: UnaryOperator,
        expr: Box<Expression>,
        span: Span,
    },
    Paren(Box<Expression>, Span),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BinaryOperator {
    /// The `==` operator (equal).
    Eq,
    /// The `!=` operator (not equal).
    Neq,
    /// The `<` operator (less than).
    Lt,
    /// The `<=` operator (less than or equal).
    LtEq,
    /// The `>` operator (greater than).
    Gt,
    /// The `>=` operator (greater than or equal).
    GtEq,
    /// The `+` operator (addition).
    Add,
    /// The `-` operator (subtraction).
    Sub,
    /// The `*` operator (multiplication).
    Mul,
    /// The `/` operator (division).
    Div,
    /// The `%` operator (modulus).
    Mod,
    /// The `&` operator (bitwise and).
    BitAnd,
    /// The `|` operator (bitwise or).
    BitOr,
    /// The `^` operator (bitwise xor).
    BitXor,
    /// The `<<` operator (shift left).
    Shl,
    /// The `>>` operator (shift right).
    Shr,
    /// The `&&` operator (logical and).
    And,
    /// The `||` operator (logical or).
    Or,
    /// The `instanceof` operator (instance of).
    InstanceOf,
}

impl BinaryOperator {
    pub fn is_arithmetic(&self) -> bool {
        matches!(
            self,
            BinaryOperator::Add
                | BinaryOperator::Sub
                | BinaryOperator::Mul
                | BinaryOperator::Div
                | BinaryOperator::Mod
        )
    }

    pub fn is_bitwise(&self) -> bool {
        matches!(
            self,
            BinaryOperator::BitAnd
                | BinaryOperator::BitOr
                | BinaryOperator::BitXor
                | BinaryOperator::Shl
                | BinaryOperator::Shr
        )
    }

    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinaryOperator::Eq
                | BinaryOperator::Neq
                | BinaryOperator::Lt
                | BinaryOperator::LtEq
                | BinaryOperator::Gt
                | BinaryOperator::GtEq
        )
    }

    pub fn is_logical(&self) -> bool {
        matches!(self, BinaryOperator::And | BinaryOperator::Or)
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum UnaryOperator {
    Not,
    Neg,
}

/// A type argument to a generic function call.
///
/// baml.fetch_value<int>(...) == TypeArg::Type(int),
/// baml.fetch_value<T>(...) == TypeArg::TypeName("T")
#[derive(Clone, Debug)]
pub enum TypeArg {
    Type(TypeIR),
    TypeName(String),
}

// TODO: struct Expr {kind: ExprKind, span: Span}
impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Expression::ArrayAccess { span, .. } => span.clone(),
            Expression::FieldAccess { span, .. } => span.clone(),
            Expression::MethodCall { span, .. } => span.clone(),
            Expression::BoolValue(_, span) => span.clone(),
            Expression::NumericValue(_, span) => span.clone(),
            Expression::Identifier(_, span) => span.clone(),
            Expression::StringValue(_, span) => span.clone(),
            Expression::RawStringValue(_, span) => span.clone(),
            Expression::If { span, .. } => span.clone(),
            Expression::Array(_, span) => span.clone(),
            Expression::Map(_, span) => span.clone(),
            Expression::JinjaExpressionValue(_, span) => span.clone(),
            Expression::Call { span, .. } => span.clone(),
            Expression::ClassConstructor(_, span) => span.clone(),
            Expression::Block(_, span) => span.clone(),
            Expression::BinaryOperation { span, .. } => span.clone(),
            Expression::UnaryOperation { span, .. } => span.clone(),
            Expression::Paren(_, span) => span.clone(),
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
